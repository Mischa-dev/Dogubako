//! The execution engine: the single code path through which every interface
//! (desktop, web, CLI, HTTP API, MCP) runs tools.
//!
//! It enforces, in order: tool lookup, caller authorization, platform
//! support, permission grants, dependency availability, parameter
//! validation, isolated scratch directories, timeouts and cancellation.

use crate::deps::DepRegistry;
use crate::error::{ToolError, ToolResult};
use crate::models::ModelRegistry;
use crate::params::{validate, FileResolver};
use crate::policy::{ExecLimits, FsPolicy, NetPolicy, PermissionSet};
use crate::registry::Registry;
use crate::spec::ToolSpec;
use crate::tool::{ContextBuilder, Environment, ProgressFn, ToolOutput};
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
}

#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub available: bool,
    pub platform_supported: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<MissingDep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_model: Option<String>,
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
        Engine {
            registry: Arc::new(registry),
            deps: Arc::new(DepRegistry::new(extra_bin_dirs)),
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
            })
            .collect();
        let missing_model = spec.requires_model.as_ref().filter(|g| !self.models.group_available(g)).map(|g| {
            format!("No '{g}' model installed. Download one in Settings → Models or with `dogubako models download <id>`.")
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
            reason,
        }
    }

    pub fn info(&self, spec: &ToolSpec) -> ToolInfo {
        ToolInfo {
            spec: spec.clone(),
            availability: self.availability(spec),
        }
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        self.registry.specs().map(|s| self.info(s)).collect()
    }

    /// Run a tool. All checks happen here, regardless of the interface.
    pub async fn run(&self, req: RunRequest<'_>) -> ToolResult<(ToolOutput, RunStats)> {
        let started = Instant::now();
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
            return Err(ToolError::MissingModel { message: m });
        }

        let fs = req.fs.clone().unwrap_or_else(|| self.config.fs.clone());
        let params = validate(&spec, &req.params, req.resolver, &fs)?;

        std::fs::create_dir_all(&self.config.temp_root)?;
        let work = tempfile::Builder::new()
            .prefix("dgb-run-")
            .tempdir_in(&self.config.temp_root)?;
        std::fs::create_dir_all(&req.out_dir)?;
        let out_dir = std::fs::canonicalize(&req.out_dir)?;

        let ctx = ContextBuilder {
            work_dir: work.path().to_path_buf(),
            out_dir: out_dir.clone(),
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
        .build();

        let mut timeout =
            Duration::from_secs(spec.timeout_secs.min(self.config.limits.max_timeout_secs));
        if let Some(t) = req.timeout {
            timeout = timeout.min(t);
        }
        let cancel = req.cancel.clone();
        let blocking = ctx.inner.blocking.clone();
        let mut fut = Box::pin(tool.run(ctx, params));
        let interrupted = tokio::select! {
            r = &mut fut => Ok(r),
            _ = cancel.cancelled() => Err(ToolError::Cancelled),
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
