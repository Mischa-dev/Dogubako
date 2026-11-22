//! MCP Streamable HTTP transport (`/mcp`).

use crate::authz::Principal;
use crate::error::ApiError;
use crate::state::{AppState, McpSession};
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use dogubako_core::engine::ToolInfo;
use dogubako_core::error::ToolError;
use dogubako_core::extensions::ExtensionReport;
use dogubako_core::spec::{Category, Permission};
use dogubako_core::tool::ProgressFn;
use dogubako_mcp::{
    DiscoveryMode, ExtensionView, FileView, JobStatus, JobView, McpBackend, McpServer, Notifier,
};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct ServerBackend {
    pub state: AppState,
    pub principal: Principal,
}

impl ServerBackend {
    fn usable(&self, spec: &dogubako_core::spec::ToolSpec) -> bool {
        let perms = self.principal.permissions(self.state.host_fs_enabled());
        self.principal.can_use(spec) && perms.missing(&spec.permissions).is_empty()
    }

    fn author_allowed(&self) -> Result<(), ToolError> {
        if !self.can_author() {
            return Err(dogubako_mcp::author_denied());
        }
        if !self.state.cfg.allow_extension_install {
            return Err(ToolError::denied(
                "installing extensions over HTTP is off on this server; an admin can set \
                 allow_extension_install (DOGUBAKO_ALLOW_EXTENSION_INSTALL=true)",
            ));
        }
        Ok(())
    }

    /// The caller's own dev tools and the trusted extensions it may use.
    fn visible_extensions(&self, id: Option<&str>) -> Vec<ExtensionView> {
        dogubako_mcp::extension_views(&self.state.engine, id, |st, spec| {
            if st.dev {
                self.principal.is_author_key(st.author.as_deref())
            } else {
                spec.is_some_and(|s| self.usable(s))
            }
        })
    }

    fn audit(
        &self,
        action: &str,
        r: &Result<ExtensionReport, ToolError>,
        target: Option<&str>,
        detail: &str,
    ) {
        let (id, outcome) = match r {
            Ok(r) if r.passed => (r.id.as_str(), "ok"),
            Ok(r) => (r.id.as_str(), "tests_failed"),
            Err(_) => (target.unwrap_or(""), "error"),
        };
        self.state
            .db
            .audit(&self.principal.username, action, id, outcome, "", detail);
    }
}

#[async_trait::async_trait]
impl McpBackend for ServerBackend {
    fn tools(&self) -> Vec<ToolInfo> {
        let perms = self.principal.permissions(self.state.host_fs_enabled());
        self.state
            .engine
            .registry()
            .specs()
            .filter(|s| self.principal.can_use(s) && perms.missing(&s.permissions).is_empty())
            .map(|s| self.state.engine.info(s))
            .collect()
    }

    fn not_installed(&self, name: &str) -> Option<ToolError> {
        let perms = self.principal.permissions(self.state.host_fs_enabled());
        let tools = self.state.engine.snapshot();
        let spec = tools.catalog.find(name)?.spec();
        if !self.principal.can_use(spec) || !perms.missing(&spec.permissions).is_empty() {
            return None;
        }
        tools.not_installed(name)
    }
    fn available_tools(&self) -> Vec<dogubako_core::store::AvailableTool> {
        let perms = self.principal.permissions(self.state.host_fs_enabled());
        let index =
            self.state.cfg.store_url().and_then(|u| {
                dogubako_core::store::cached_index(&u, &self.state.cfg.extensions_dir())
            });
        dogubako_core::store::available(&self.state.engine, index.as_ref(), |s| {
            self.principal.can_use(s) && perms.missing(&s.permissions).is_empty()
        })
    }
    fn file_mode(&self) -> &'static str {
        "id"
    }

    fn can_author(&self) -> bool {
        self.principal
            .permissions(self.state.host_fs_enabled())
            .has(Permission::ExtensionsAuthor)
    }

    async fn extension_install(
        &self,
        manifest: &str,
        replace: bool,
    ) -> Result<ExtensionReport, ToolError> {
        use dogubako_core::extensions as ext;
        self.author_allowed()?;
        let p = &self.principal;
        // Its tests run with every permission: install only a tool the
        // caller could run itself. A manifest that doesn't parse gets
        // install_dev's size and parse errors.
        if let Ok(tool) = dogubako_core::manifest::parse_manifest(manifest, "dev") {
            let mut spec = dogubako_core::tool::Tool::spec(&tool).clone();
            spec.dev = true;
            spec.author = Some(p.author_key());
            if !self.usable(&spec) {
                return Err(ToolError::denied(format!(
                    "your policy does not allow '{}' or its permissions ({}); ask an admin to extend it",
                    spec.id,
                    spec.permissions.iter().map(|p| p.id()).collect::<Vec<_>>().join(", ")
                )));
            }
        }
        let r = ext::install_dev(
            &self.state.engine,
            &self.state.cfg.extensions_dir(),
            manifest,
            &p.author_key(),
            ext::InstallOptions { replace },
        )
        .await;
        self.audit("extension.install", &r, None, "dev mcp");
        r
    }

    async fn extension_test(&self, id: &str) -> Result<ExtensionReport, ToolError> {
        self.author_allowed()?;
        if self.visible_extensions(Some(id)).is_empty() {
            return Err(ToolError::NotFound {
                id: format!("{id} (no extension you can see)"),
            });
        }
        dogubako_core::extensions::test(&self.state.engine, &self.state.cfg.extensions_dir(), id)
            .await
    }

    async fn extension_status(&self, id: Option<&str>) -> Result<Vec<ExtensionView>, ToolError> {
        Ok(self.visible_extensions(id))
    }

    async fn store_install(&self, id: &str) -> Result<ExtensionReport, ToolError> {
        use dogubako_core::store;
        self.author_allowed()?;
        let url = self
            .state
            .cfg
            .store_url()
            .ok_or_else(store::disabled_error)?;
        let dir = self.state.cfg.extensions_dir();
        let index = store::load_index(&url, &dir, &self.state.engine.config.net).await?;
        let e = index
            .index
            .tools
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| ToolError::NotFound {
                id: format!("{id} (not in the tool store)"),
            })?;
        // Installed store tools are visible to everyone the policy allows;
        // an author may only add one it could run itself.
        let category = Category::parse(&e.category).unwrap_or(Category::Dev);
        let mut spec = dogubako_core::spec::ToolSpec::new(&e.id, &e.name, category);
        for p in &e.permissions {
            match Permission::parse(p).filter(Permission::is_tool_capability) {
                Some(p) => spec = spec.permission(p),
                None => {
                    return Err(ToolError::invalid(
                        "id",
                        format!("store entry '{id}' needs unknown permission '{p}'"),
                    ))
                }
            }
        }
        if !self.usable(&spec) {
            return Err(ToolError::denied(format!(
                "your policy does not allow '{id}' or its permissions; ask an admin to extend it"
            )));
        }
        let r = store::install(&self.state.engine, &dir, &index, id, false).await;
        self.audit("store.install", &r, Some(id), "mcp");
        r
    }

    async fn run_tool(
        &self,
        tool_id: &str,
        args: Map<String, Value>,
        progress: Option<ProgressFn>,
        wait: Duration,
    ) -> Result<JobView, ToolError> {
        let job = self
            .state
            .jobs
            .submit(
                &self.principal,
                tool_id,
                args,
                "mcp",
                self.state.cfg.user_quota_mb,
            )
            .map_err(|e| match e.status {
                StatusCode::FORBIDDEN => ToolError::denied(e.message),
                StatusCode::NOT_FOUND => self
                    .not_installed(tool_id)
                    .unwrap_or_else(|| ToolError::NotFound { id: tool_id.into() }),
                StatusCode::PAYLOAD_TOO_LARGE => ToolError::limit(e.message),
                _ => ToolError::failed(e.message),
            })?;
        // Forward job progress to the MCP client while waiting.
        let forward = progress.zip(self.state.jobs.subscribe(&self.principal.user_id, &job.id));
        let fwd_task = forward.map(|(p, rx)| {
            tokio::spawn(async move {
                let mut s = tokio_stream::wrappers::WatchStream::new(rx);
                while let Some(v) = s.next().await {
                    if v.progress.is_some() || v.message.is_some() {
                        p(dogubako_core::tool::ProgressEvent {
                            fraction: v.progress,
                            message: v.message.clone(),
                        });
                    }
                    if v.status.is_terminal() {
                        break;
                    }
                }
            })
        });
        let v = self
            .state
            .jobs
            .wait(&self.principal.user_id, &job.id, wait)
            .await
            .unwrap_or(job);
        if let Some(t) = fwd_task {
            t.abort();
        }
        Ok(self.absolutize(v))
    }

    async fn job(&self, job_id: &str, wait: Duration) -> Result<JobView, ToolError> {
        self.state
            .jobs
            .wait(&self.principal.user_id, job_id, wait)
            .await
            .map(|v| self.absolutize(v))
            .ok_or_else(|| ToolError::NotFound { id: job_id.into() })
    }

    async fn cancel_job(&self, job_id: &str) -> Result<JobView, ToolError> {
        self.state
            .jobs
            .cancel(&self.principal.user_id, job_id)
            .ok_or_else(|| ToolError::NotFound { id: job_id.into() })?;
        let v = self
            .state
            .jobs
            .wait(&self.principal.user_id, job_id, Duration::from_secs(5))
            .await
            .ok_or_else(|| ToolError::NotFound { id: job_id.into() })?;
        Ok(v)
    }

    async fn upload(&self, name: &str, bytes: Vec<u8>) -> Result<FileView, ToolError> {
        let max = self
            .principal
            .max_upload_bytes(self.state.cfg.max_upload_mb)
            .min(self.max_upload());
        if bytes.len() as u64 > max {
            return Err(ToolError::limit("file exceeds the upload limit"));
        }
        if self.state.db.storage_used(&self.principal.user_id) + bytes.len() as u64
            > self.principal.quota_bytes(self.state.cfg.user_quota_mb)
        {
            return Err(ToolError::limit("storage quota exceeded"));
        }
        let tmp = tempfile::NamedTempFile::new_in(self.state.cfg.tmp_dir())?;
        std::fs::write(tmp.path(), &bytes)?;
        let path = tmp
            .into_temp_path()
            .keep()
            .map_err(|e| ToolError::failed(e.to_string()))?;
        let quota = self.principal.quota_bytes(self.state.cfg.user_quota_mb);
        let stored = self.state.storage.with_quota(
            &self.principal.user_id,
            quota,
            bytes.len() as u64,
            || {
                self.state
                    .storage
                    .store(&self.principal.user_id, name, &path, "upload", None)
            },
        );
        let mut v = match stored {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(match e {
                    crate::storage::QuotaError::Exceeded { .. } => ToolError::limit(e.to_string()),
                    crate::storage::QuotaError::Io(io) => io.into(),
                });
            }
        };
        v.url = v.url.map(|u| self.state.absolute(&u));
        self.state.db.audit(
            &self.principal.username,
            "file.upload",
            &v.id,
            "ok",
            "",
            "mcp",
        );
        Ok(v)
    }

    async fn list_files(&self) -> Result<Vec<FileView>, ToolError> {
        Ok(self
            .state
            .storage
            .list(&self.principal.user_id)
            .into_iter()
            .map(|mut f| {
                f.url = f.url.map(|u| self.state.absolute(&u));
                f
            })
            .collect())
    }

    async fn read_file(&self, file_id: &str, max: u64) -> Result<(FileView, Vec<u8>), ToolError> {
        let f = self
            .state
            .storage
            .get(&self.principal.user_id, file_id)
            .ok_or_else(|| ToolError::NotFound { id: file_id.into() })?;
        if f.view.size > max {
            // Metadata already says it is too large: don't touch the file.
            return Ok((f.view, Vec::new()));
        }
        let bytes = dogubako_mcp::read_capped(&f.path, max).await?;
        Ok((f.view, bytes))
    }
}

impl ServerBackend {
    fn absolutize(&self, mut v: JobView) -> JobView {
        if let Some(r) = v.result.as_mut() {
            for a in &mut r.artifacts {
                a.url = a.url.as_ref().map(|u| self.state.absolute(u));
            }
        }
        v
    }
}

#[derive(Deserialize)]
pub struct McpQuery {
    discovery: Option<String>,
}

fn check_origin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return Ok(());
    };
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let origin_host = origin.split("://").nth(1).unwrap_or("");
    if origin_host == host
        || state.cfg.allowed_origins.iter().any(|o| o == origin)
        || state.cfg.public_url.as_deref() == Some(origin)
    {
        Ok(())
    } else {
        Err(ApiError::forbidden("origin not allowed"))
    }
}

pub async fn mcp_post(
    State(state): State<AppState>,
    Query(q): Query<McpQuery>,
    principal: Principal,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    if !state.cfg.mcp_enabled {
        return Err(ApiError::not_found("MCP is disabled on this instance"));
    }
    check_origin(&state, &headers)?;
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return Ok((StatusCode::BAD_REQUEST, Json(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}})))
                .into_response())
        }
    };
    let items: Vec<&Value> = match &msg {
        Value::Array(a) => a.iter().collect(),
        v => vec![v],
    };
    let is_init = items
        .iter()
        .any(|m| m.get("method").and_then(|x| x.as_str()) == Some("initialize"));
    let has_requests = items
        .iter()
        .any(|m| m.get("method").is_some() && m.get("id").is_some());

    // Session handling: issued on initialize, bound to the principal.
    let mut new_session = None;
    if is_init {
        let sid = crate::auth::random_token(16);
        let mut sessions = state.mcp_sessions.lock().unwrap();
        if sessions.len() > 10_000 {
            let cutoff = Instant::now() - Duration::from_secs(24 * 3600);
            sessions.retain(|_, s| s.last_used > cutoff);
        }
        sessions.insert(
            sid.clone(),
            McpSession {
                user_id: principal.user_id.clone(),
                token_id: principal.token_id.clone(),
                created: Instant::now(),
                last_used: Instant::now(),
                closed: Default::default(),
            },
        );
        new_session = Some(sid);
    } else if let Some(sid) = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) {
        let mut sessions = state.mcp_sessions.lock().unwrap();
        match sessions.get_mut(sid) {
            Some(s) if s.user_id == principal.user_id && s.token_id == principal.token_id => {
                s.last_used = Instant::now()
            }
            _ => return Err(ApiError::not_found("unknown MCP session; re-initialize")),
        }
    }
    if !has_requests {
        return Ok(StatusCode::ACCEPTED.into_response());
    }

    let mode = q
        .discovery
        .as_deref()
        .or(principal.mcp_discovery())
        .and_then(DiscoveryMode::parse)
        .or_else(|| DiscoveryMode::parse(&state.cfg.mcp_discovery))
        .unwrap_or(DiscoveryMode::Full);
    let username = principal.username.clone();
    let backend = Arc::new(ServerBackend {
        state: state.clone(),
        principal,
    });
    let server = Arc::new(McpServer::new(
        backend,
        mode,
        state.cfg.instance_name.clone(),
    ));

    let accepts_sse = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/event-stream"))
        .unwrap_or(false);
    let wants_progress = items.iter().any(|m| {
        m.get("method").and_then(|x| x.as_str()) == Some("tools/call")
            && m.pointer("/params/_meta/progressToken").is_some()
    });
    let _ = username;

    let mut resp = if accepts_sse && wants_progress {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        let tx2 = tx.clone();
        let notifier: Notifier = Arc::new(move |v: Value| {
            let _ = tx2.send(v);
        });
        tokio::spawn(async move {
            if let Some(r) = server.handle(msg, Some(notifier)).await {
                let _ = tx.send(r);
            }
        });
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx).map(|v| {
            Ok::<_, std::convert::Infallible>(Event::default().event("message").data(v.to_string()))
        });
        Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
            .into_response()
    } else {
        match server.handle(msg, None).await {
            Some(r) => Json(r).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        }
    };
    if let Some(sid) = new_session {
        resp.headers_mut()
            .insert("mcp-session-id", HeaderValue::from_str(&sid).unwrap());
    }
    Ok(resp)
}

/// The standalone server-initiated stream for an initialized session: sends
/// `notifications/tools/list_changed` after a reload that changed the tools.
pub async fn mcp_get(
    State(state): State<AppState>,
    principal: Principal,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let accepts_sse = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/event-stream"))
        .unwrap_or(false);
    let sid = headers.get("mcp-session-id").and_then(|v| v.to_str().ok());
    let (true, Some(sid)) = (accepts_sse, sid) else {
        return Ok((
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "POST, DELETE")],
        )
            .into_response());
    };
    let closed = {
        let mut sessions = state.mcp_sessions.lock().unwrap();
        match sessions.get_mut(sid) {
            Some(s) if s.user_id == principal.user_id && s.token_id == principal.token_id => {
                s.last_used = Instant::now();
                s.closed.clone()
            }
            _ => return Err(ApiError::not_found("unknown MCP session; re-initialize")),
        }
    };
    let rx = state.engine.subscribe();
    let stream = tokio_stream::wrappers::WatchStream::from_changes(rx)
        .take_until(closed.cancelled_owned())
        .take_until(state.shutdown.clone().cancelled_owned())
        .map(|_| {
            Ok::<_, std::convert::Infallible>(
                Event::default()
                    .event("message")
                    .data(McpServer::tools_list_changed().to_string()),
            )
        });
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response())
}

pub async fn mcp_delete(
    State(state): State<AppState>,
    principal: Principal,
    headers: HeaderMap,
) -> Response {
    if let Some(sid) = headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) {
        let mut s = state.mcp_sessions.lock().unwrap();
        if s.get(sid)
            .map(|x| x.user_id == principal.user_id)
            .unwrap_or(false)
        {
            if let Some(gone) = s.remove(sid) {
                gone.closed.cancel();
            }
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

#[allow(dead_code)]
fn _status(_: JobStatus) {}
