//! The tool execution interface.

use crate::deps::DepRegistry;
use crate::error::{ToolError, ToolResult};
use crate::models::ModelRegistry;
use crate::params::Params;
use crate::policy::{sanitize_filename, ExecLimits, FsPolicy, NetPolicy, PermissionSet};
use crate::process::Exec;
use crate::spec::{Permission, ToolSpec};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Where the engine is running. Tools may adapt (e.g. system tools note
/// they report the *server's* hardware when hosted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Desktop,
    Cli,
    Server,
    Mcp,
}

/// Every tool implements this.
#[async_trait::async_trait]
pub trait Tool: Send + Sync + 'static {
    fn spec(&self) -> &ToolSpec;
    async fn run(&self, ctx: ToolContext, params: Params) -> ToolResult<ToolOutput>;
}

/// An output file produced by a tool.
#[derive(Debug, Clone, Serialize)]
pub struct Artifact {
    /// Absolute path inside the run's output directory.
    pub path: PathBuf,
    /// File name presented to users.
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// Optional short label ("compressed", "page 3", "preview").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Result of a tool run.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ToolOutput {
    /// Primary textual result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// MIME type of `text` (text/plain, application/json, text/markdown...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_mime: Option<String>,
    /// Structured result.
    pub data: Value,
    pub artifacts: Vec<Artifact>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl ToolOutput {
    pub fn new() -> Self {
        ToolOutput {
            data: Value::Null,
            ..Default::default()
        }
    }
    pub fn text(s: impl Into<String>) -> Self {
        ToolOutput {
            text: Some(s.into()),
            text_mime: Some("text/plain".into()),
            data: Value::Null,
            ..Default::default()
        }
    }
    pub fn text_as(s: impl Into<String>, mime: &str) -> Self {
        ToolOutput {
            text: Some(s.into()),
            text_mime: Some(mime.into()),
            data: Value::Null,
            ..Default::default()
        }
    }
    pub fn data(v: Value) -> Self {
        ToolOutput {
            data: v,
            ..Default::default()
        }
    }
    pub fn files(a: Vec<Artifact>) -> Self {
        ToolOutput {
            data: Value::Null,
            artifacts: a,
            ..Default::default()
        }
    }
    pub fn with_text(mut self, s: impl Into<String>, mime: &str) -> Self {
        self.text = Some(s.into());
        self.text_mime = Some(mime.into());
        self
    }
    pub fn with_data(mut self, v: Value) -> Self {
        self.data = v;
        self
    }
    pub fn with_artifact(mut self, a: Artifact) -> Self {
        self.artifacts.push(a);
        self
    }
    pub fn with_artifacts(mut self, a: Vec<Artifact>) -> Self {
        self.artifacts.extend(a);
        self
    }
    pub fn warn(mut self, w: impl Into<String>) -> Self {
        self.warnings.push(w.into());
        self
    }
}

pub type ProgressFn = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    /// 0.0 ..= 1.0 when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fraction: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub(crate) struct CtxInner {
    pub work_dir: PathBuf,
    pub out_dir: PathBuf,
    pub models_dir: PathBuf,
    pub cancel: CancellationToken,
    pub progress: Option<ProgressFn>,
    pub deps: Arc<DepRegistry>,
    pub models: Arc<ModelRegistry>,
    pub limits: ExecLimits,
    pub fs: FsPolicy,
    pub net: NetPolicy,
    pub env: Environment,
    pub permissions: PermissionSet,
    pub input_paths: Vec<PathBuf>,
    pub used_names: Mutex<HashSet<String>>,
}

/// Per-run execution context. Cheap to clone.
#[derive(Clone)]
pub struct ToolContext {
    pub(crate) inner: Arc<CtxInner>,
}

impl ToolContext {
    pub fn work_dir(&self) -> &Path {
        &self.inner.work_dir
    }
    pub fn out_dir(&self) -> &Path {
        &self.inner.out_dir
    }
    pub fn models_dir(&self) -> &Path {
        &self.inner.models_dir
    }
    pub fn deps(&self) -> &DepRegistry {
        &self.inner.deps
    }
    pub fn models(&self) -> &ModelRegistry {
        &self.inner.models
    }
    pub fn limits(&self) -> &ExecLimits {
        &self.inner.limits
    }
    pub fn fs_policy(&self) -> &FsPolicy {
        &self.inner.fs
    }
    pub fn net_policy(&self) -> &NetPolicy {
        &self.inner.net
    }
    pub fn environment(&self) -> Environment {
        self.inner.env
    }
    pub fn has_permission(&self, p: Permission) -> bool {
        self.inner.permissions.has(p)
    }
    pub(crate) fn input_paths(&self) -> &[PathBuf] {
        &self.inner.input_paths
    }
    pub fn cancel_token(&self) -> CancellationToken {
        self.inner.cancel.clone()
    }
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancel.is_cancelled()
    }
    pub fn check_cancelled(&self) -> ToolResult<()> {
        if self.is_cancelled() {
            Err(ToolError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Report progress (fraction 0..=1).
    pub fn progress(&self, fraction: f32, message: impl Into<String>) {
        if let Some(p) = &self.inner.progress {
            let m: String = message.into();
            p(ProgressEvent {
                fraction: Some(fraction.clamp(0.0, 1.0)),
                message: if m.is_empty() { None } else { Some(m) },
            });
        }
    }

    /// Report a status message without a fraction.
    pub fn message(&self, message: impl Into<String>) {
        if let Some(p) = &self.inner.progress {
            p(ProgressEvent {
                fraction: None,
                message: Some(message.into()),
            });
        }
    }

    /// A unique, sanitized path in the output directory for `name`.
    pub fn output_path(&self, name: &str) -> PathBuf {
        let clean = sanitize_filename(name);
        let mut used = self.inner.used_names.lock().unwrap();
        let p = Path::new(&clean);
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let ext = p
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let mut candidate = clean.clone();
        let mut i = 2;
        while used.contains(&candidate.to_lowercase())
            || self.inner.out_dir.join(&candidate).exists()
        {
            candidate = format!("{stem}-{i}{ext}");
            i += 1;
        }
        used.insert(candidate.to_lowercase());
        self.inner.out_dir.join(candidate)
    }

    /// A path in the private scratch directory.
    pub fn temp_path(&self, name: &str) -> PathBuf {
        self.inner.work_dir.join(sanitize_filename(name))
    }

    /// Create a scratch subdirectory.
    pub fn temp_dir(&self, name: &str) -> ToolResult<PathBuf> {
        let d = self.temp_path(name);
        std::fs::create_dir_all(&d)?;
        Ok(d)
    }

    /// Describe a produced file as an artifact.
    pub fn artifact(&self, path: impl AsRef<Path>) -> ToolResult<Artifact> {
        let path = path.as_ref();
        let meta = std::fs::metadata(path).map_err(|e| {
            ToolError::failed(format!("output file missing ({}): {e}", path.display()))
        })?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".into());
        Ok(Artifact {
            path: path.to_path_buf(),
            mime: crate::util::mime_for_path(path),
            name,
            size: meta.len(),
            label: None,
        })
    }

    pub fn artifact_labeled(&self, path: impl AsRef<Path>, label: &str) -> ToolResult<Artifact> {
        let mut a = self.artifact(path)?;
        a.label = Some(label.to_string());
        Ok(a)
    }

    /// Write bytes to an output file and return the artifact.
    pub fn write_output(&self, name: &str, bytes: &[u8]) -> ToolResult<Artifact> {
        let p = self.output_path(name);
        std::fs::write(&p, bytes)?;
        self.artifact(&p)
    }

    /// Start building an external command for a known dependency id.
    pub fn command(&self, dep_id: &str) -> ToolResult<Exec> {
        let status = self.inner.deps.status(dep_id);
        match status.path {
            Some(p) => Ok(Exec::new(self.clone(), p, status.name)),
            None => Err(ToolError::MissingDependency {
                dependency: dep_id.to_string(),
                hint: status.install_hint,
            }),
        }
    }

    /// Command for a specific sibling binary of a dependency (e.g. `pdftotext`
    /// next to `pdftoppm` for Poppler, `ffprobe` next to `ffmpeg`).
    pub fn command_sibling(&self, dep_id: &str, binary: &str) -> ToolResult<Exec> {
        let status = self.inner.deps.status(dep_id);
        let Some(p) = status.path else {
            return Err(ToolError::MissingDependency {
                dependency: dep_id.to_string(),
                hint: status.install_hint,
            });
        };
        let exe = if cfg!(windows) {
            format!("{binary}.exe")
        } else {
            binary.to_string()
        };
        let sib = p.parent().map(|d| d.join(&exe)).filter(|c| c.is_file());
        let path = sib.or_else(|| which::which(binary).ok()).ok_or_else(|| {
            ToolError::MissingDependency {
                dependency: binary.to_string(),
                hint: status.install_hint.clone(),
            }
        })?;
        Ok(Exec::new(self.clone(), path, binary.to_string()))
    }

    /// Command for an arbitrary program path (used by trusted extension manifests).
    pub fn command_path(&self, program: PathBuf, display: &str) -> Exec {
        Exec::new(self.clone(), program, display.to_string())
    }

    /// Run CPU-bound work on the blocking pool.
    pub async fn blocking<T, F>(&self, f: F) -> ToolResult<T>
    where
        T: Send + 'static,
        F: FnOnce() -> ToolResult<T> + Send + 'static,
    {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| ToolError::failed(format!("worker panicked: {e}")))?
    }

    /// HTTP client that enforces the network policy (SSRF guard).
    pub fn http_client(&self) -> ToolResult<reqwest::Client> {
        if !self.has_permission(Permission::Network) {
            return Err(ToolError::denied("network access was not granted"));
        }
        crate::net::guarded_client(self.inner.net.clone())
    }
}

/// Builder for contexts (used by the engine and by tests).
pub struct ContextBuilder {
    pub work_dir: PathBuf,
    pub out_dir: PathBuf,
    pub models_dir: PathBuf,
    pub cancel: CancellationToken,
    pub progress: Option<ProgressFn>,
    pub deps: Arc<DepRegistry>,
    pub models: Arc<ModelRegistry>,
    pub limits: ExecLimits,
    pub fs: FsPolicy,
    pub net: NetPolicy,
    pub env: Environment,
    pub permissions: PermissionSet,
    pub input_paths: Vec<PathBuf>,
}

impl ContextBuilder {
    pub fn build(self) -> ToolContext {
        ToolContext {
            inner: Arc::new(CtxInner {
                work_dir: self.work_dir,
                out_dir: self.out_dir,
                models_dir: self.models_dir,
                cancel: self.cancel,
                progress: self.progress,
                deps: self.deps,
                models: self.models,
                limits: self.limits,
                fs: self.fs,
                net: self.net,
                env: self.env,
                permissions: self.permissions,
                input_paths: self.input_paths,
                used_names: Mutex::new(HashSet::new()),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Closure-based tools, so most tools are a spec + a function.

type SyncFn = dyn Fn(&ToolContext, &Params) -> ToolResult<ToolOutput> + Send + Sync;
type AsyncFn = dyn Fn(ToolContext, Params) -> Pin<Box<dyn Future<Output = ToolResult<ToolOutput>> + Send>>
    + Send
    + Sync;

/// A tool whose body is a synchronous function, run on the blocking pool.
pub struct SyncTool {
    spec: ToolSpec,
    f: Arc<SyncFn>,
}

#[async_trait::async_trait]
impl Tool for SyncTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    async fn run(&self, ctx: ToolContext, params: Params) -> ToolResult<ToolOutput> {
        let f = self.f.clone();
        let c = ctx.clone();
        ctx.blocking(move || f(&c, &params)).await
    }
}

/// A tool whose body is an async function.
pub struct AsyncTool {
    spec: ToolSpec,
    f: Box<AsyncFn>,
}

#[async_trait::async_trait]
impl Tool for AsyncTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    async fn run(&self, ctx: ToolContext, params: Params) -> ToolResult<ToolOutput> {
        (self.f)(ctx, params).await
    }
}

/// Create a tool from a synchronous function.
pub fn sync_tool<F>(spec: ToolSpec, f: F) -> Arc<dyn Tool>
where
    F: Fn(&ToolContext, &Params) -> ToolResult<ToolOutput> + Send + Sync + 'static,
{
    Arc::new(SyncTool {
        spec,
        f: Arc::new(f),
    })
}

/// Create a tool from an async function.
pub fn async_tool<F, Fut>(spec: ToolSpec, f: F) -> Arc<dyn Tool>
where
    F: Fn(ToolContext, Params) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ToolResult<ToolOutput>> + Send + 'static,
{
    Arc::new(AsyncTool {
        spec,
        f: Box::new(move |c, p| Box::pin(f(c, p))),
    })
}
