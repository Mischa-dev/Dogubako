//! The execution engine: the single code path through which every interface
//! (desktop, web, CLI, HTTP API, MCP) runs tools.
//!
//! It enforces, in order: tool lookup, caller authorization, platform
//! support, permission grants, dependency availability, parameter
//! validation, isolated scratch directories, timeouts and cancellation.

use crate::deps::{DepRegistry, Unmet};
use crate::error::{ToolError, ToolResult};
use crate::models::ModelRegistry;
use crate::params::{validate, FileResolver, Params};
use crate::policy::{ExecLimits, FsPolicy, NetPolicy, PermissionSet};
use crate::registry::Registry;
use crate::spec::{ParamKind, Requirement, ToolSpec};
use crate::tool::{ContextBuilder, Environment, ProgressFn, Tool, ToolContext, ToolOutput};

/// Upper bound for a tool's pre-run [`Tool::check`].
const CHECK_TIMEOUT_SECS: u64 = 30;
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub limits: ExecLimits,
    pub fs: FsPolicy,
    pub net: NetPolicy,
    /// Where per-run scratch directories are created.
    pub temp_root: PathBuf,
    pub models_dir: PathBuf,
    pub env: Environment,
    /// Executable that handles `__dogubako-worker <name>` (normally the host
    /// binary itself); see [`crate::tool::ToolContext::worker`].
    pub worker_exe: Option<PathBuf>,
}

impl EngineConfig {
    pub fn local(env: Environment) -> Self {
        let dirs = crate::paths::AppDirs::default_local();
        EngineConfig {
            limits: ExecLimits::default(),
            fs: FsPolicy::unrestricted(),
            net: NetPolicy::default(),
            temp_root: std::env::temp_dir(),
            models_dir: dirs.models,
            env,
            worker_exe: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingDep {
    pub id: String,
    pub name: String,
    pub hint: String,
    /// Set when the app can download this program itself: the engine id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_bytes: Option<u64>,
}

/// The model to offer when a tool's model group has none installed.
#[derive(Debug, Clone, Serialize)]
pub struct ModelOffer {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub available: bool,
    pub platform_supported: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<MissingDep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_model: Option<String>,
    /// With `missing_model`: the model group the tool needs and the one to install.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_model: Option<ModelOffer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Spec plus host availability, as listed to clients.
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    #[serde(flatten)]
    pub spec: ToolSpec,
    pub availability: Availability,
}

pub struct RunRequest<'a> {
    pub tool_id: &'a str,
    pub params: Map<String, Value>,
    pub resolver: &'a dyn FileResolver,
    /// Directory receiving output artifacts (must exist).
    pub out_dir: PathBuf,
    pub permissions: PermissionSet,
    /// Caller-specific authorization (per-tool / per-category ACLs).
    pub authorize: Option<&'a (dyn Fn(&ToolSpec) -> bool + Send + Sync)>,
    pub progress: Option<ProgressFn>,
    pub cancel: CancellationToken,
    /// Optional override of the timeout (capped by the tool & limits).
    pub timeout: Option<Duration>,
    /// Optional per-caller filesystem policy override (e.g. stricter roots).
    pub fs: Option<FsPolicy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunStats {
    pub duration_ms: u64,
}

pub struct Engine {
    pub registry: Arc<Registry>,
    pub deps: Arc<DepRegistry>,
    pub models: Arc<ModelRegistry>,
    pub config: EngineConfig,
}

impl Engine {
    pub fn new(registry: Registry, config: EngineConfig, extra_bin_dirs: Vec<PathBuf>) -> Self {
        Self::with_deps(registry, config, DepRegistry::new(extra_bin_dirs))
    }

    /// With a prepared dependency registry, e.g. one that uses the app's own
    /// engines ([`DepRegistry::with_managed`]).
    pub fn with_deps(registry: Registry, config: EngineConfig, deps: DepRegistry) -> Self {
        Engine {
            registry: Arc::new(registry),
            deps: Arc::new(deps),
            models: Arc::new(ModelRegistry::new(config.models_dir.clone())),
            config,
        }
    }

    pub fn availability(&self, spec: &ToolSpec) -> Availability {
        let platform_supported = spec.supports_current_platform();
        let missing: Vec<MissingDep> = spec
            .requires
            .iter()
            .map(|d| self.deps.status(d))
            .filter(|s| !s.found)
            .map(|s| MissingDep {
                id: s.id,
                name: s.name,
                hint: s.install_hint,
                downloadable: s.downloadable,
                download_bytes: s.download_bytes,
            })
            .collect();
        let model_group = spec
            .requires_model
            .clone()
            .filter(|g| !self.models.group_available(g));
        let missing_model = model_group.as_ref().map(|g| {
            format!("No '{g}' model installed. Download one in Settings → Models or with `dogubako models download <id>`.")
        });
        let recommended_model = model_group
            .as_deref()
            .and_then(crate::models::recommended_model)
            .map(|m| ModelOffer {
                id: m.id.to_string(),
                name: m.name.to_string(),
                size_bytes: m.size_bytes,
            });
        let reason = if !platform_supported {
            Some(format!(
                "Not supported on {:?}",
                crate::spec::Platform::current()
            ))
        } else if let Some(m) = missing.first() {
            Some(format!("Requires {}", m.name))
        } else {
            missing_model.clone()
        };
        Availability {
            available: platform_supported && missing.is_empty() && missing_model.is_none(),
            platform_supported,
            missing,
            missing_model,
            model_group,
            recommended_model,
            reason,
        }
    }

    pub fn info(&self, spec: &ToolSpec) -> ToolInfo {
        let mut spec = spec.clone();
        for p in &mut spec.params {
            p.unavailable = p
                .requires
                .as_ref()
                .and_then(|r| self.deps.check(r).err())
                .map(|u| u.message());
            if let ParamKind::Enum { options, .. } = &mut p.kind {
                for o in options {
                    o.unavailable = o
                        .requires
                        .as_ref()
                        .and_then(|r| self.deps.check(r).err())
                        .map(|u| u.message());
                }
            }
        }
        ToolInfo {
            availability: self.availability(&spec),
            spec,
        }
    }

    /// Reject chosen options this install cannot do, before the tool runs.
    fn check_requirements(&self, spec: &ToolSpec, params: &Params) -> ToolResult<()> {
        let unmet = |param: &str, req: &Requirement| match self.deps.check(req) {
            Ok(()) => Ok(()),
            Err(Unmet::MissingDependency {
                dependency, hint, ..
            }) => Err(ToolError::MissingDependency { dependency, hint }),
            Err(Unmet::Unsupported(m)) => Err(ToolError::invalid(param, m)),
        };
        for p in &spec.params {
            let Some(v) = params.get(&p.name) else {
                continue;
            };
            if let Some(cond) = &p.show_if {
                let cur = params.get(&cond.param);
                let shown = cond.values.iter().any(|want| match cur {
                    Some(Value::Array(a)) => a.iter().any(|c| loose_eq(c, want)),
                    Some(c) => loose_eq(c, want),
                    None => false,
                });
                if !shown {
                    continue;
                }
            }
            if let Some(req) = &p.requires {
                if !matches!(v, Value::Bool(false)) && v.as_str() != Some("") {
                    unmet(&p.name, req)?;
                }
            }
            if let ParamKind::Enum { options, .. } = &p.kind {
                let chosen: Vec<&str> = match v {
                    Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect(),
                    other => other.as_str().into_iter().collect(),
                };
                for o in options
                    .iter()
                    .filter(|o| chosen.contains(&o.value.as_str()))
                {
                    if let Some(req) = &o.requires {
                        unmet(&p.name, req)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        self.registry.specs().map(|s| self.info(s)).collect()
    }

    /// Checks shared by [`Engine::run`] and [`Engine::check`]: the tool,
    /// authorization, platform, permissions, dependencies and params.
    fn prepare(&self, req: &RunRequest<'_>) -> ToolResult<(Arc<dyn Tool>, Params, FsPolicy)> {
        let tool = self
            .registry
            .find(req.tool_id)
            .ok_or_else(|| ToolError::NotFound {
                id: req.tool_id.to_string(),
            })?
            .clone();
        let spec = tool.spec().clone();

        if let Some(auth) = req.authorize {
            if !auth(&spec) {
                return Err(ToolError::denied(format!(
                    "not authorized to use '{}'",
                    spec.id
                )));
            }
        }
        if !spec.supports_current_platform() {
            return Err(ToolError::unsupported(format!(
                "'{}' is not supported on this platform",
                spec.id
            )));
        }
        let missing_perms = req.permissions.missing(&spec.permissions);
        if !missing_perms.is_empty() {
            let names: Vec<&str> = missing_perms.iter().map(|p| p.id()).collect();
            return Err(ToolError::denied(format!(
                "'{}' requires permission(s): {}",
                spec.id,
                names.join(", ")
            )));
        }
        let avail = self.availability(&spec);
        if let Some(m) = avail.missing.first() {
            return Err(ToolError::MissingDependency {
                dependency: m.id.clone(),
                hint: m.hint.clone(),
            });
        }
        if let Some(m) = avail.missing_model {
            return Err(ToolError::MissingModel {
                message: m,
                model: avail.recommended_model.map(|r| r.id),
            });
        }

        let fs = req.fs.clone().unwrap_or_else(|| self.config.fs.clone());
        let params = validate(&spec, &req.params, req.resolver, &fs)?;
        self.check_requirements(&spec, &params)?;
        Ok((tool, params, fs))
    }

    fn context(
        &self,
        req: &RunRequest<'_>,
        work_dir: PathBuf,
        out_dir: PathBuf,
        fs: FsPolicy,
        params: &Params,
    ) -> ToolContext {
        ContextBuilder {
            work_dir,
            out_dir,
            models_dir: self.config.models_dir.clone(),
            cancel: req.cancel.clone(),
            progress: req.progress.clone(),
            deps: self.deps.clone(),
            models: self.models.clone(),
            limits: self.config.limits.clone(),
            fs,
            net: self.config.net.clone(),
            env: self.config.env,
            permissions: req.permissions.clone(),
            input_paths: params.all_files().map(|f| f.path.clone()).collect(),
            worker_exe: self.config.worker_exe.clone(),
        }
        .build()
    }

    /// Check a tool's inputs before running it: everything [`Engine::run`]
    /// rejects up front, plus the tool's own [`Tool::check`]. Writes nothing
    /// but a scratch directory, removed afterwards.
    pub async fn check(&self, req: RunRequest<'_>) -> ToolResult<()> {
        let (tool, params, fs) = self.prepare(&req)?;
        if !tool.spec().prechecks {
            return Ok(());
        }
        std::fs::create_dir_all(&self.config.temp_root)?;
        let work = tempfile::Builder::new()
            .prefix("dgb-check-")
            .tempdir_in(&self.config.temp_root)?;
        let dir = work.path().to_path_buf();
        let ctx = self.context(&req, dir.clone(), dir, fs, &params);
        let timeout =
            Duration::from_secs(CHECK_TIMEOUT_SECS).min(req.timeout.unwrap_or(Duration::MAX));
        tokio::select! {
            biased;
            _ = req.cancel.cancelled() => Err(ToolError::Cancelled),
            r = tool.check(ctx, params) => r,
            _ = tokio::time::sleep(timeout) => Err(ToolError::Timeout { seconds: timeout.as_secs() }),
        }
    }

    /// Run a tool. All checks happen here, regardless of the interface.
    pub async fn run(&self, req: RunRequest<'_>) -> ToolResult<(ToolOutput, RunStats)> {
        let started = Instant::now();
        let (tool, params, fs) = self.prepare(&req)?;
        let spec = tool.spec().clone();

        std::fs::create_dir_all(&self.config.temp_root)?;
        let work = tempfile::Builder::new()
            .prefix("dgb-run-")
            .tempdir_in(&self.config.temp_root)?;
        std::fs::create_dir_all(&req.out_dir)?;
        let out_dir = std::fs::canonicalize(&req.out_dir)?;
        let ctx = self.context(
            &req,
            work.path().to_path_buf(),
            out_dir.clone(),
            fs,
            &params,
        );

        let mut timeout =
            Duration::from_secs(spec.timeout_secs.min(self.config.limits.max_timeout_secs));
        if let Some(t) = req.timeout {
            timeout = timeout.min(t);
        }
        let cancel = req.cancel.clone();
        let blocking = ctx.inner.blocking.clone();
        let mut fut = Box::pin(tool.run(ctx, params));
        // Cancellation is checked first, so a run cancelled before it
        // started never does its work (even a tool that finishes on its
        // first poll).
        let interrupted = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(ToolError::Cancelled),
            r = &mut fut => Ok(r),
            _ = tokio::time::sleep(timeout) => Err(ToolError::Timeout { seconds: timeout.as_secs() }),
        };
        let result = match interrupted {
            Ok(r) => r,
            Err(e) => {
                // Signal the tool and drop its future (this stops async work
                // and kills child processes), then wait for blocking workers,
                // which cannot be aborted. Until they finish, the scratch
                // directory and the caller's concurrency slot stay in use.
                cancel.cancel();
                drop(fut);
                blocking.close();
                blocking.wait().await;
                Err(e)
            }
        };
        let mut output = result?;

        // Artifacts must live inside the output directory.
        for a in &mut output.artifacts {
            let canon = std::fs::canonicalize(&a.path)
                .map_err(|e| ToolError::failed(format!("artifact '{}' missing: {e}", a.name)))?;
            if !canon.starts_with(&out_dir) {
                return Err(ToolError::failed(format!(
                    "tool returned an artifact outside its output directory: {}",
                    a.name
                )));
            }
            a.path = canon;
            a.size = std::fs::metadata(&a.path)
                .map(|m| m.len())
                .unwrap_or(a.size);
        }
        if output.data.is_null() {
            output.data = Value::Object(Map::new());
        }
        drop(work);
        Ok((
            output,
            RunStats {
                duration_ms: started.elapsed().as_millis() as u64,
            },
        ))
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    a == b
        || match (a, b) {
            (Value::String(x), y) | (y, Value::String(x)) => match y {
                Value::Number(n) => x.parse::<f64>().ok() == n.as_f64(),
                Value::Bool(v) => x.parse::<bool>().ok() == Some(*v),
                _ => false,
            },
            _ => false,
        }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::testing::TestHarness;

    static STUBBORN_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn tools() -> Vec<Arc<dyn Tool>> {
        vec![
            async_tool(
                ToolSpec::new("dev.slow_async", "Slow async", Category::Dev)
                    .describe("async work that never checks cancellation")
                    .timeout(1),
                |_c, _p| async move {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    Ok(ToolOutput::text("never"))
                },
            ),
            sync_tool(
                ToolSpec::new("dev.stubborn", "Stubborn", Category::Dev)
                    .describe("blocking work that ignores cancellation")
                    .timeout(1),
                |_c, _p| {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    STUBBORN_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(ToolOutput::text("late"))
                },
            ),
            async_tool(
                ToolSpec::new("dev.slow", "Slow", Category::Dev)
                    .describe("sleeps")
                    .timeout(1),
                |ctx, _p| async move {
                    #[cfg(unix)]
                    ctx.command_path("sleep".into(), "sleep")
                        .arg("30")
                        .run()
                        .await?;
                    #[cfg(not(unix))]
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    let _ = ctx;
                    Ok(ToolOutput::text("never"))
                },
            ),
            sync_tool(
                ToolSpec::new("dev.escape", "Escape", Category::Dev)
                    .describe("returns artifact outside out dir"),
                |ctx, _p| {
                    let p = ctx.work_dir().join("x.txt");
                    std::fs::write(&p, "x")?;
                    Ok(ToolOutput::files(vec![ctx.artifact(&p)?]))
                },
            ),
            sync_tool(
                ToolSpec::new("network.fake", "Net", Category::Network)
                    .describe("needs network")
                    .permission(Permission::Network),
                |_c, _p| Ok(ToolOutput::text("ok")),
            ),
            sync_tool(
                ToolSpec::new("video.fake", "Needs dep", Category::Video)
                    .describe("needs a missing dep")
                    .requires(&["definitely-not-a-dep"]),
                |_c, _p| Ok(ToolOutput::text("ok")),
            ),
            sync_tool(
                ToolSpec::new("dev.caps", "Caps", Category::Dev)
                    .describe("options that need missing engines")
                    .param(
                        ParamSpec::choice_of("mode", &["a", "b"])
                            .default_str("a")
                            .option_needs("b", Requirement::program("definitely-not-a-dep", "x")),
                    )
                    .param(
                        ParamSpec::boolean("extra")
                            .show_if("mode", &["a"])
                            .needs(Requirement::program("definitely-not-a-dep", "y")),
                    ),
                |_c, p| Ok(ToolOutput::text(p.str("mode")?)),
            ),
            sync_tool(
                ToolSpec::new("dev.capped", "Capped", Category::Dev)
                    .describe("per-mode maximum")
                    .param(ParamSpec::choice_of("mode", &["a", "b"]).default_str("a"))
                    .param(ParamSpec::integer("q").range(0.0, 9.0))
                    .param(
                        ParamSpec::integer("n")
                            .range(1.0, 640.0)
                            .max_if("mode", &["a"], 320.0)
                            .max_unless_set("q"),
                    ),
                |_c, p| Ok(ToolOutput::data(json!({"n": p.opt_int("n")}))),
            ),
            sync_tool(
                ToolSpec::new("dev.types", "Types", Category::Dev)
                    .describe("coercion")
                    .param(ParamSpec::integer("n").range(1.0, 10.0).required())
                    .param(ParamSpec::boolean("flag"))
                    .param(ParamSpec::choice_of("mode", &["a", "b"]).default_str("a"))
                    .param(ParamSpec::color("c").default_str("#abc")),
                |_c, p| {
                    Ok(ToolOutput::data(
                        json!({"n": p.int("n")?, "flag": p.bool("flag"), "mode": p.str("mode")?, "c": p.str("c")?}),
                    ))
                },
            ),
        ]
    }

    #[tokio::test]
    async fn options_needing_missing_engines_are_marked_and_rejected() {
        let h = TestHarness::new(tools());
        let spec = h.engine.registry.get("dev.caps").unwrap().spec().clone();
        let info = h.engine.info(&spec);
        assert!(info.availability.available);
        let mode = info.spec.params.iter().find(|p| p.name == "mode").unwrap();
        let crate::spec::ParamKind::Enum { options, .. } = &mode.kind else {
            panic!("mode is an enum");
        };
        assert_eq!(options[0].unavailable, None);
        assert_eq!(
            options[1].unavailable.as_deref(),
            Some("Requires definitely-not-a-dep")
        );
        let extra = info.spec.params.iter().find(|p| p.name == "extra").unwrap();
        assert!(extra.unavailable.is_some());

        let ok = h.run("dev.caps", json!({"mode": "a"})).await.unwrap();
        assert_eq!(ok.text.as_deref(), Some("a"));
        let e = h.run("dev.caps", json!({"mode": "b"})).await.unwrap_err();
        assert_eq!(e.code(), "missing_dependency");
        let e = h
            .run("dev.caps", json!({"mode": "a", "extra": true}))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "missing_dependency");
        // Turned off, or hidden by show_if, it doesn't block the run.
        h.run("dev.caps", json!({"mode": "a", "extra": false}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn max_if_caps_a_param_while_another_has_a_value() {
        let h = TestHarness::new(tools());
        let e = h.run("dev.capped", json!({"n": 321})).await.unwrap_err();
        assert_eq!(e.code(), "invalid_input");
        assert!(e.to_string().contains("at most 320 when mode is a"), "{e}");
        let e = h
            .run("dev.capped", json!({"mode": "a", "n": "640"}))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "invalid_input");
        h.run("dev.capped", json!({"n": 320})).await.unwrap();
        h.run("dev.capped", json!({"mode": "b", "n": 640}))
            .await
            .unwrap();
        let e = h
            .run("dev.capped", json!({"mode": "b", "n": 641}))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("at most 640"), "{e}");
        h.run("dev.capped", json!({})).await.unwrap();
        // Setting the lifting param removes the cap; null doesn't count as set.
        h.run("dev.capped", json!({"n": 640, "q": 2}))
            .await
            .unwrap();
        let e = h
            .run("dev.capped", json!({"n": 640, "q": null}))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("unless q is set"), "{e}");
    }

    #[tokio::test]
    async fn run_cancelled_before_start_does_not_run() {
        let h = TestHarness::new(tools());
        let resolver = crate::params::LocalFileResolver::unrestricted();
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        for _ in 0..20 {
            let r = h
                .engine
                .run(crate::engine::RunRequest {
                    tool_id: "network.fake",
                    params: Default::default(),
                    resolver: &resolver,
                    out_dir: h.path("o"),
                    permissions: crate::policy::PermissionSet::all(),
                    authorize: None,
                    progress: None,
                    cancel: cancel.clone(),
                    timeout: None,
                    fs: None,
                })
                .await;
            assert_eq!(r.unwrap_err().code(), "cancelled");
        }
    }

    #[tokio::test]
    async fn timeout_kills_tool() {
        let h = TestHarness::new(tools());
        let t = std::time::Instant::now();
        let e = h.run("dev.slow", json!({})).await.unwrap_err();
        assert_eq!(e.code(), "timeout");
        assert!(t.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn interrupted_async_work_is_dropped() {
        let h = TestHarness::new(tools());
        let t = std::time::Instant::now();
        let e = h.run("dev.slow_async", json!({})).await.unwrap_err();
        assert_eq!(e.code(), "timeout");
        assert!(t.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn interrupted_blocking_work_is_awaited() {
        let h = TestHarness::new(tools());
        let e = h.run("dev.stubborn", json!({})).await.unwrap_err();
        assert_eq!(e.code(), "timeout");
        // The engine only returns once the blocking worker has stopped, so
        // its scratch dir and the caller's concurrency slot stay accounted.
        assert!(STUBBORN_DONE.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn artifacts_must_stay_in_out_dir() {
        let h = TestHarness::new(tools());
        let e = h.run("dev.escape", json!({})).await.unwrap_err();
        assert!(e.to_string().contains("outside"));
    }

    #[tokio::test]
    async fn permissions_and_dependencies_enforced() {
        let h = TestHarness::new(tools());
        let resolver = crate::params::LocalFileResolver::unrestricted();
        let r = h
            .engine
            .run(crate::engine::RunRequest {
                tool_id: "network.fake",
                params: Default::default(),
                resolver: &resolver,
                out_dir: h.path("o"),
                permissions: crate::policy::PermissionSet::none(),
                authorize: None,
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await;
        assert_eq!(r.unwrap_err().code(), "permission_denied");
        let deny = |_: &ToolSpec| false;
        let r = h
            .engine
            .run(crate::engine::RunRequest {
                tool_id: "dev.types",
                params: Default::default(),
                resolver: &resolver,
                out_dir: h.path("o"),
                permissions: crate::policy::PermissionSet::all(),
                authorize: Some(&deny),
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await;
        assert_eq!(r.unwrap_err().code(), "permission_denied");
        assert_eq!(
            h.run("video.fake", json!({})).await.unwrap_err().code(),
            "missing_dependency"
        );
        assert_eq!(
            h.run("nope.nope", json!({})).await.unwrap_err().code(),
            "not_found"
        );
    }

    #[test]
    fn missing_model_offers_the_recommended_one() {
        use super::{Engine, EngineConfig};
        use crate::registry::Registry;
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.models_dir = dir.path().to_path_buf();
        let engine = Engine::new(Registry::new(), cfg, vec![]);
        let spec = ToolSpec::new("speech.fake", "Fake", Category::Audio)
            .describe("needs a whisper model")
            .requires(&["definitely-not-a-dep"])
            .requires_model("whisper");
        let a = engine.availability(&spec);
        assert!(!a.available);
        assert_eq!(a.model_group.as_deref(), Some("whisper"));
        let m = a.recommended_model.expect("recommended model");
        assert_eq!(m.id, "whisper-base");
        assert_eq!(m.size_bytes, 147_951_465);
        assert_eq!(a.missing[0].downloadable, None);
        let v = serde_json::to_value(engine.info(&spec).availability).unwrap();
        assert_eq!(v["recommended_model"]["name"], "Whisper base");
        assert!(v["missing"][0].get("downloadable").is_none());

        let e = ToolError::MissingModel {
            message: "x".into(),
            model: Some(m.id.clone()),
        };
        assert_eq!(serde_json::to_value(&e).unwrap()["model"], "whisper-base");

        let ok = ToolSpec::new("text.fake", "Fine", Category::Text).describe("needs nothing");
        let v = serde_json::to_value(engine.availability(&ok)).unwrap();
        assert!(v.get("model_group").is_none() && v.get("recommended_model").is_none());
    }

    #[tokio::test]
    async fn validation_and_coercion() {
        let h = TestHarness::new(tools());
        let o = h
            .run("dev.types", json!({"n": "5", "flag": "yes", "mode": "B"}))
            .await
            .unwrap();
        assert_eq!(
            o.data,
            json!({"n": 5, "flag": true, "mode": "b", "c": "#aabbcc"})
        );
        for bad in [
            json!({"n": 11}),
            json!({"n": "x"}),
            json!({}),
            json!({"n": 1, "mode": "z"}),
            json!({"n": 1, "c": "red"}),
            json!({"n": 1, "extra": 1}),
        ] {
            assert_eq!(
                h.run("dev.types", bad.clone()).await.unwrap_err().code(),
                "invalid_input",
                "{bad}"
            );
        }
        let json_schema =
            crate::schema::input_schema(h.engine.registry.get("dev.types").unwrap().spec(), "path");
        assert_eq!(json_schema["required"], json!(["n"]));
        assert_eq!(json_schema["properties"]["mode"]["enum"], json!(["a", "b"]));
    }
}
