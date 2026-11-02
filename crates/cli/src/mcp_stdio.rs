//! MCP over stdio for local agents. File parameters take local paths;
//! outputs are written to an output directory and returned as paths and
//! `dogubako://files/<id>` resources.

use crate::render::Out;
use clap::ArgMatches;
use dogubako_core::engine::{Engine, RunRequest, ToolInfo};
use dogubako_core::error::{ToolError, ToolResult};
use dogubako_core::params::{FileResolver, InputFile, LocalFileResolver};
use dogubako_core::policy::{FsPolicy, PermissionSet};
use dogubako_core::spec::{Permission, ToolSpec};
use dogubako_core::tool::ProgressFn;
use dogubako_mcp::{ArtifactView, DiscoveryMode, FileView, JobStatus, JobView, McpBackend, McpServer, ResultView};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

struct LocalBackend {
    engine: Arc<Engine>,
    allow: Vec<String>,
    deny: Vec<String>,
    permissions: PermissionSet,
    fs: FsPolicy,
    output_dir: PathBuf,
    jobs: Mutex<HashMap<String, (watch::Sender<JobView>, CancellationToken)>>,
    files: Arc<Mutex<HashMap<String, (FileView, PathBuf)>>>,
}

fn matches(p: &str, spec: &ToolSpec) -> bool {
    p == "*" || p.strip_suffix(".*").map(|pre| spec.id.starts_with(&format!("{pre}."))).unwrap_or(false) || p == spec.id
}

impl LocalBackend {
    fn permitted(&self, spec: &ToolSpec) -> bool {
        self.allow.iter().any(|p| matches(p, spec)) && !self.deny.iter().any(|p| matches(p, spec)) && self.permissions.missing(&spec.permissions).is_empty()
    }
}

/// Accepts local paths, or ids of files produced/uploaded in this session.
struct SessionResolver {
    local: LocalFileResolver,
    files: Arc<Mutex<HashMap<String, (FileView, PathBuf)>>>,
}

impl FileResolver for SessionResolver {
    fn resolve(&self, param: &str, value: &str) -> ToolResult<InputFile> {
        let id = value.trim_start_matches("dogubako://files/");
        if let Some((v, p)) = self.files.lock().unwrap().get(id) {
            return Ok(InputFile { path: p.clone(), name: v.name.clone() });
        }
        self.local.resolve(param, value)
    }
}

#[async_trait::async_trait]
impl McpBackend for LocalBackend {
    fn tools(&self) -> Vec<ToolInfo> {
        self.engine.registry.specs().filter(|s| self.permitted(s)).map(|s| self.engine.info(s)).collect()
    }

    fn file_mode(&self) -> &'static str {
        "path"
    }

    async fn run_tool(&self, tool_id: &str, args: Map<String, Value>, progress: Option<ProgressFn>, wait: Duration) -> Result<JobView, ToolError> {
        let spec = self.engine.registry.find(tool_id).ok_or_else(|| ToolError::NotFound { id: tool_id.into() })?.spec().clone();
        if !self.permitted(&spec) {
            return Err(ToolError::denied(format!("'{}' is not permitted for this MCP server", spec.id)));
        }
        let id = format!("j_{}", dogubako_core::util::random_id(8));
        let view = JobView {
            id: id.clone(),
            tool_id: spec.id.clone(),
            status: JobStatus::Running,
            progress: None,
            message: None,
            result: None,
            error: None,
            created_at: now(),
            finished_at: None,
            params: Value::Null,
            owner: None,
        };
        let (tx, mut rx) = watch::channel(view);
        let cancel = CancellationToken::new();
        self.jobs.lock().unwrap().insert(id.clone(), (tx.clone(), cancel.clone()));
        let engine = self.engine.clone();
        let out_dir = self.output_dir.join(&id);
        let permissions = self.permissions.clone();
        let fs = self.fs.clone();
        let files = self.files.clone();
        let allow = self.allow.clone();
        let deny = self.deny.clone();
        let tx2 = tx.clone();
        tokio::spawn(async move {
            let txp = tx2.clone();
            let progress_fn: ProgressFn = Arc::new(move |ev: dogubako_core::tool::ProgressEvent| {
                txp.send_modify(|v| {
                    if ev.fraction.is_some() {
                        v.progress = ev.fraction;
                    }
                    if ev.message.is_some() {
                        v.message = ev.message.clone();
                    }
                });
                if let Some(p) = &progress {
                    p(ev);
                }
            });
            let resolver = SessionResolver { local: LocalFileResolver { policy: fs.clone() }, files: files.clone() };
            let authorize = move |s: &ToolSpec| allow.iter().any(|p| matches(p, s)) && !deny.iter().any(|p| matches(p, s));
            let res = engine
                .run(RunRequest {
                    tool_id: &spec.id,
                    params: args,
                    resolver: &resolver,
                    out_dir,
                    permissions,
                    authorize: Some(&authorize),
                    progress: Some(progress_fn),
                    cancel,
                    timeout: None,
                    fs: Some(fs),
                })
                .await;
            let mut v = tx2.borrow().clone();
            v.finished_at = Some(now());
            match res {
                Ok((out, stats)) => {
                    let mut arts = vec![];
                    for a in &out.artifacts {
                        let fid = format!("f_{}", dogubako_core::util::random_id(16));
                        let fv = FileView { id: fid.clone(), name: a.name.clone(), mime: a.mime.clone(), size: a.size, created_at: now(), expires_at: None, source: "artifact".into(), job_id: Some(v.id.clone()), url: None };
                        files.lock().unwrap().insert(fid.clone(), (fv, a.path.clone()));
                        arts.push(ArtifactView { id: Some(fid), name: a.name.clone(), mime: a.mime.clone(), size: a.size, label: a.label.clone(), url: None, path: Some(a.path.to_string_lossy().to_string()) });
                    }
                    v.status = JobStatus::Succeeded;
                    v.progress = Some(1.0);
                    v.result = Some(ResultView { text: out.text, text_mime: out.text_mime, data: out.data, artifacts: arts, warnings: out.warnings, duration_ms: stats.duration_ms });
                }
                Err(e) => {
                    v.status = if matches!(e, ToolError::Cancelled) { JobStatus::Cancelled } else { JobStatus::Failed };
                    v.error = Some(e);
                }
            }
            tx2.send_replace(v);
        });
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if rx.borrow().status.is_terminal() {
                break;
            }
            match tokio::time::timeout_at(deadline, rx.changed()).await {
                Ok(Ok(())) => continue,
                _ => break,
            }
        }
        let v = rx.borrow().clone();
        Ok(v)
    }

    async fn job(&self, job_id: &str, wait: Duration) -> Result<JobView, ToolError> {
        let mut rx = self.jobs.lock().unwrap().get(job_id).map(|(tx, _)| tx.subscribe()).ok_or_else(|| ToolError::NotFound { id: job_id.into() })?;
        let deadline = tokio::time::Instant::now() + wait;
        while !rx.borrow().status.is_terminal() {
            match tokio::time::timeout_at(deadline, rx.changed()).await {
                Ok(Ok(())) => continue,
                _ => break,
            }
        }
        let v = rx.borrow().clone();
        Ok(v)
    }

    async fn cancel_job(&self, job_id: &str) -> Result<JobView, ToolError> {
        if let Some((_, c)) = self.jobs.lock().unwrap().get(job_id) {
            c.cancel();
        }
        self.job(job_id, Duration::from_secs(5)).await
    }

    async fn upload(&self, name: &str, bytes: Vec<u8>) -> Result<FileView, ToolError> {
        let dir = self.output_dir.join("uploads");
        std::fs::create_dir_all(&dir)?;
        let id = format!("f_{}", dogubako_core::util::random_id(16));
        let clean = dogubako_core::policy::sanitize_filename(name);
        let path = dir.join(format!("{id}-{clean}"));
        std::fs::write(&path, &bytes)?;
        let fv = FileView { id: id.clone(), name: clean, mime: dogubako_core::util::mime_for_path(&path), size: bytes.len() as u64, created_at: now(), expires_at: None, source: "upload".into(), job_id: None, url: None };
        self.files.lock().unwrap().insert(id, (fv.clone(), path));
        Ok(fv)
    }

    async fn list_files(&self) -> Result<Vec<FileView>, ToolError> {
        Ok(self.files.lock().unwrap().values().map(|(v, _)| v.clone()).collect())
    }

    async fn read_file(&self, file_id: &str) -> Result<(FileView, Vec<u8>), ToolError> {
        let (v, p) = self.files.lock().unwrap().get(file_id).cloned().ok_or_else(|| ToolError::NotFound { id: file_id.into() })?;
        Ok((v, std::fs::read(p)?))
    }
}

pub async fn serve(engine: Engine, m: &ArgMatches) -> i32 {
    let out = Out { json: false, quiet: false };
    let allow: Vec<String> = m.get_many::<String>("allow").map(|v| v.cloned().collect()).unwrap_or_else(|| vec!["*".into()]);
    let deny: Vec<String> = m.get_many::<String>("deny").map(|v| v.cloned().collect()).unwrap_or_default();
    let permissions = match m.get_one::<String>("permissions") {
        None => PermissionSet::all(),
        Some(s) => {
            let mut set = vec![];
            for p in s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty() && *x != "none") {
                match Permission::parse(p) {
                    Some(p) => set.push(p),
                    None => return out.error(&format!("unknown permission '{p}'"), 2),
                }
            }
            PermissionSet::from_iter(set)
        }
    };
    let fs = match m.get_many::<String>("fs-root") {
        Some(r) => FsPolicy::roots(r.map(PathBuf::from).collect()),
        None => FsPolicy::unrestricted(),
    };
    let output_dir = m
        .get_one::<String>("output-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| dogubako_core::paths::AppDirs::default_local().data.join("mcp-output"));
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        return out.error(&format!("cannot create output dir: {e}"), 1);
    }
    let mode = DiscoveryMode::parse(m.get_one::<String>("discovery").map(|s| s.as_str()).unwrap_or("full")).unwrap_or(DiscoveryMode::Full);
    let backend = Arc::new(LocalBackend {
        engine: Arc::new(engine),
        allow,
        deny,
        permissions,
        fs,
        output_dir,
        jobs: Mutex::new(HashMap::new()),
        files: Arc::new(Mutex::new(HashMap::new())),
    });
    let server = Arc::new(McpServer::new(backend, mode, "Dogubako (local)"));
    let stdout = Arc::new(tokio::sync::Mutex::new(tokio::io::stdout()));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    tracing::info!("dogubako MCP stdio server ready");
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}});
                write_line(&stdout, &err).await;
                continue;
            }
        };
        let server = server.clone();
        let stdout = stdout.clone();
        tokio::spawn(async move {
            let so = stdout.clone();
            let rt = tokio::runtime::Handle::current();
            let notifier: dogubako_mcp::Notifier = Arc::new(move |v: Value| {
                let so = so.clone();
                rt.spawn(async move { write_line(&so, &v).await });
            });
            if let Some(resp) = server.handle(msg, Some(notifier)).await {
                write_line(&stdout, &resp).await;
            }
        });
    }
    0
}

async fn write_line(out: &Arc<tokio::sync::Mutex<tokio::io::Stdout>>, v: &Value) {
    let mut s = serde_json::to_string(v).unwrap_or_default();
    s.push('\n');
    let mut o = out.lock().await;
    let _ = o.write_all(s.as_bytes()).await;
    let _ = o.flush().await;
}
