//! # dogubako-mcp
//!
//! Model Context Protocol server logic, independent of transport. The stdio
//! transport (CLI) and the Streamable HTTP transport (server) both feed
//! JSON-RPC messages into [`McpServer::handle`].
//!
//! Discovery is designed for large catalogs: in `full` mode every permitted
//! tool is listed (paginated); in `compact` mode only a handful of meta-tools
//! are listed (`dogubako_search_tools`, `dogubako_describe_tool`,
//! `dogubako_run_tool`, ...) so agents can search instead of loading hundreds
//! of schemas. Authorization is enforced on every invocation by the backend
//! and the engine, independently of what was listed.

pub mod types;

use base64::Engine as _;
use dogubako_core::engine::ToolInfo;
use dogubako_core::error::ToolError;
use dogubako_core::schema;
use dogubako_core::spec::{Category, OutputKind, Permission};
use dogubako_core::tool::ProgressFn;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use std::time::Duration;
pub use types::*;

pub const SUPPORTED_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
pub const LATEST_VERSION: &str = "2025-11-25";
const PAGE_SIZE: usize = 200;
const MAX_INLINE_RESOURCE: u64 = 16 * 1024 * 1024;

/// Read at most `max` bytes of `path`, so an oversized file is detected
/// without loading it (and stays bounded if it grows meanwhile).
pub async fn read_capped(path: impl AsRef<std::path::Path>, max: u64) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let f = tokio::fs::File::open(path.as_ref()).await?;
    let mut buf = Vec::new();
    f.take(max).read_to_end(&mut buf).await?;
    Ok(buf)
}

/// How tools are exposed in `tools/list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryMode {
    /// Meta-tools plus every permitted tool.
    Full,
    /// Meta-tools only (search-oriented discovery).
    Compact,
}

impl DiscoveryMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "full" => Some(DiscoveryMode::Full),
            "compact" | "search" => Some(DiscoveryMode::Compact),
            _ => None,
        }
    }
}

/// Default largest file accepted by the `upload_file` meta-tool.
pub const DEFAULT_MAX_UPLOAD: u64 = 50 * 1024 * 1024;

/// What the MCP layer needs from its host (local CLI or hosted server).
#[async_trait::async_trait]
pub trait McpBackend: Send + Sync {
    /// Tools visible to this caller (already filtered by authorization).
    fn tools(&self) -> Vec<ToolInfo>;
    /// "path" (local paths) or "id" (stored file ids).
    fn file_mode(&self) -> &'static str;
    /// Start a tool run; wait up to `wait` for completion.
    async fn run_tool(
        &self,
        tool_id: &str,
        args: Map<String, Value>,
        progress: Option<ProgressFn>,
        wait: Duration,
    ) -> Result<JobView, ToolError>;
    async fn job(&self, job_id: &str, wait: Duration) -> Result<JobView, ToolError>;
    async fn cancel_job(&self, job_id: &str) -> Result<JobView, ToolError>;
    async fn upload(&self, name: &str, bytes: Vec<u8>) -> Result<FileView, ToolError>;
    async fn list_files(&self) -> Result<Vec<FileView>, ToolError>;
    /// A stored file's metadata and at most `max` bytes of its contents
    /// (see [`read_capped`]); callers compare the length with `max` to
    /// detect a file that is too large without loading all of it.
    async fn read_file(&self, file_id: &str, max: u64) -> Result<(FileView, Vec<u8>), ToolError>;
    /// Maximum accepted upload size through MCP (bytes).
    fn max_upload(&self) -> u64 {
        DEFAULT_MAX_UPLOAD
    }
    /// Default time a direct tool call waits before returning a job handle.
    fn default_wait(&self) -> Duration {
        Duration::from_secs(120)
    }
}

pub struct McpServer {
    backend: Arc<dyn McpBackend>,
    mode: DiscoveryMode,
    instance_name: String,
}

/// Notification sink (progress, logging) for the current request.
pub type Notifier = Arc<dyn Fn(Value) + Send + Sync>;

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

#[allow(dead_code)]
const META_TOOLS: &[&str] = &[
    "dogubako_search_tools",
    "dogubako_describe_tool",
    "dogubako_run_tool",
    "dogubako_job_status",
    "dogubako_job_cancel",
    "dogubako_list_categories",
    "dogubako_upload_file",
    "dogubako_list_files",
];

impl McpServer {
    pub fn new(
        backend: Arc<dyn McpBackend>,
        mode: DiscoveryMode,
        instance_name: impl Into<String>,
    ) -> Self {
        McpServer {
            backend,
            mode,
            instance_name: instance_name.into(),
        }
    }

    /// Handle one parsed JSON-RPC message (request, notification or batch).
    /// Returns the response (None for notifications).
    pub async fn handle(&self, msg: Value, notify: Option<Notifier>) -> Option<Value> {
        if let Value::Array(items) = msg {
            let mut out = vec![];
            for m in items {
                if let Some(r) = Box::pin(self.handle(m, notify.clone())).await {
                    out.push(r);
                }
            }
            return if out.is_empty() {
                None
            } else {
                Some(Value::Array(out))
            };
        }
        let id = msg.get("id").cloned();
        let method = msg
            .get("method")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());
        let Some(method) = method else {
            // A response from the client (we never send requests) – ignore.
            return id.map(|id| rpc_error(&id, -32600, "invalid request: missing method"));
        };
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let Some(id) = id else {
            // Notification: initialized, cancelled, ... nothing to return.
            tracing::debug!(%method, "mcp notification");
            return None;
        };
        let res = match method.as_str() {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => self.tools_list(&params),
            "tools/call" => Ok(self.tools_call(&params, notify).await),
            "resources/list" => self.resources_list().await,
            "resources/templates/list" => Ok(json!({"resourceTemplates": [{
                "uriTemplate": "dogubako://files/{file_id}",
                "name": "Dogubako file",
                "description": "A stored input file or tool output artifact",
            }]})),
            "resources/read" => self.resources_read(&params).await,
            "prompts/list" => Ok(json!({"prompts": []})),
            "logging/setLevel" => Ok(json!({})),
            "completion/complete" => Ok(json!({"completion": {"values": [], "hasMore": false}})),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match res {
            Ok(r) => rpc_result(&id, r),
            Err((code, m)) => rpc_error(&id, code, m),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let requested = params
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or(LATEST_VERSION);
        let version = if SUPPORTED_VERSIONS.contains(&requested) {
            requested
        } else {
            LATEST_VERSION
        };
        let n = self.backend.tools().len();
        let mode_hint = match self.mode {
            DiscoveryMode::Full => "All permitted tools are listed directly; meta-tools are also available.",
            DiscoveryMode::Compact => "Only meta-tools are listed. Use dogubako_search_tools to find tools, dogubako_describe_tool for a schema, and dogubako_run_tool to invoke.",
        };
        let files = if self.backend.file_mode() == "path" {
            "File parameters take absolute local paths; outputs are written to the server's output directory and returned as paths and dogubako:// resources."
        } else {
            "File parameters take file ids: upload with dogubako_upload_file (base64) or the HTTP API (POST /api/v1/files), or reuse artifact ids from previous results. Outputs are returned as dogubako://files/<id> resource links (readable via resources/read) plus HTTP download URLs."
        };
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": {"listChanged": false},
                "resources": {"subscribe": false, "listChanged": false},
                "logging": {}
            },
            "serverInfo": {"name": "dogubako", "title": self.instance_name, "version": env!("CARGO_PKG_VERSION")},
            "instructions": format!(
                "Dogubako is a local-first utility toolbox ({n} tools available to you: images, video, audio, speech, PDF, files, text, developer, data, network, design, system). {mode_hint} {files} Long-running calls return a job_id when they exceed the wait time; poll with dogubako_job_status."
            )
        })
    }

    fn tool_entry(&self, info: &ToolInfo) -> Value {
        let s = &info.spec;
        let mut desc = s.description.clone();
        if !info.availability.available {
            desc = format!(
                "{desc} [UNAVAILABLE on this host: {}]",
                info.availability.reason.clone().unwrap_or_default()
            );
        }
        if s.output.kind == OutputKind::Files {
            desc.push_str(" Returns output files as resource links.");
        }
        let read_only =
            s.permissions.iter().all(|p| *p != Permission::HostFsWrite) && !s.destructive;
        json!({
            "name": s.mcp_name(),
            "title": s.name,
            "description": desc,
            "inputSchema": schema::input_schema(s, self.backend.file_mode()),
            "outputSchema": schema::output_schema(s),
            "annotations": {
                "title": s.name,
                "readOnlyHint": read_only,
                "destructiveHint": s.destructive,
                "idempotentHint": read_only,
                "openWorldHint": s.permissions.contains(&Permission::Network),
            },
            "_meta": {"dogubako/id": s.id, "dogubako/category": s.category.id(), "dogubako/available": info.availability.available}
        })
    }

    fn meta_tools(&self) -> Vec<Value> {
        let file_mode = self.backend.file_mode();
        let mut v = vec![
            json!({
                "name": "dogubako_search_tools",
                "title": "Search Dogubako tools",
                "description": "Search the tools you are permitted to use by keyword, capability or category. Returns ids, descriptions and availability. Use before dogubako_describe_tool / dogubako_run_tool.",
                "inputSchema": {"type": "object", "properties": {
                    "query": {"type": "string", "description": "Keywords, e.g. 'compress video' or 'pdf merge'. Empty lists everything."},
                    "category": {"type": "string", "enum": Category::ALL.iter().map(|c| c.id()).collect::<Vec<_>>()},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 25}
                }, "additionalProperties": false},
                "annotations": {"readOnlyHint": true, "openWorldHint": false}
            }),
            json!({
                "name": "dogubako_describe_tool",
                "title": "Describe a Dogubako tool",
                "description": "Full description of one tool: input JSON schema, output kind, dependencies, availability on this host and required permissions.",
                "inputSchema": {"type": "object", "properties": {
                    "tool": {"type": "string", "description": "Tool id (e.g. 'video.compress') or MCP name ('video_compress')"}
                }, "required": ["tool"], "additionalProperties": false},
                "annotations": {"readOnlyHint": true, "openWorldHint": false}
            }),
            json!({
                "name": "dogubako_run_tool",
                "title": "Run a Dogubako tool",
                "description": "Invoke any permitted tool by id with arguments matching its input schema. Waits up to wait_seconds; if the job is still running, returns its job_id for polling with dogubako_job_status.",
                "inputSchema": {"type": "object", "properties": {
                    "tool": {"type": "string"},
                    "arguments": {"type": "object", "description": "Tool arguments (see dogubako_describe_tool)"},
                    "wait_seconds": {"type": "integer", "minimum": 0, "maximum": 600, "default": 60}
                }, "required": ["tool"], "additionalProperties": false},
                "annotations": {"readOnlyHint": false, "openWorldHint": true}
            }),
            json!({
                "name": "dogubako_job_status",
                "title": "Get job status / result",
                "description": "Status, progress and (when finished) the result of a job started earlier. Optionally waits for completion.",
                "inputSchema": {"type": "object", "properties": {
                    "job_id": {"type": "string"},
                    "wait_seconds": {"type": "integer", "minimum": 0, "maximum": 600, "default": 0}
                }, "required": ["job_id"], "additionalProperties": false},
                "annotations": {"readOnlyHint": true, "openWorldHint": false}
            }),
            json!({
                "name": "dogubako_job_cancel",
                "title": "Cancel a job",
                "description": "Cancel a running job.",
                "inputSchema": {"type": "object", "properties": {"job_id": {"type": "string"}}, "required": ["job_id"], "additionalProperties": false},
                "annotations": {"readOnlyHint": false, "destructiveHint": false, "openWorldHint": false}
            }),
            json!({
                "name": "dogubako_list_categories",
                "title": "List tool categories",
                "description": "Categories with the number of tools you may use in each.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
                "annotations": {"readOnlyHint": true, "openWorldHint": false}
            }),
        ];
        if file_mode == "id" {
            v.push(json!({
                "name": "dogubako_upload_file",
                "title": "Upload a file",
                "description": format!("Upload a file (base64, max {} MB) to use as a tool input. Returns a file_id. For large files prefer the HTTP upload endpoint.", self.backend.max_upload() / (1024 * 1024)),
                "inputSchema": {"type": "object", "properties": {
                    "name": {"type": "string", "description": "File name including extension"},
                    "content_base64": {"type": "string"}
                }, "required": ["name", "content_base64"], "additionalProperties": false},
                "annotations": {"readOnlyHint": false, "openWorldHint": false}
            }));
        }
        v.push(json!({
            "name": "dogubako_list_files",
            "title": "List my files",
            "description": "Files and output artifacts you can use as inputs.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false},
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        }));
        v
    }

    fn tools_list(&self, params: &Value) -> Result<Value, (i64, String)> {
        let mut all = self.meta_tools();
        if self.mode == DiscoveryMode::Full {
            let mut infos = self.backend.tools();
            infos.sort_by(|a, b| a.spec.id.cmp(&b.spec.id));
            all.extend(infos.iter().map(|i| self.tool_entry(i)));
        }
        let start = match params.get("cursor").and_then(|c| c.as_str()) {
            Some(c) => c
                .parse::<usize>()
                .map_err(|_| (-32602, "invalid cursor".to_string()))?,
            None => 0,
        };
        let end = (start + PAGE_SIZE).min(all.len());
        let page: Vec<Value> = all.get(start..end).map(|s| s.to_vec()).unwrap_or_default();
        let mut res = json!({"tools": page});
        if end < all.len() {
            res["nextCursor"] = json!(end.to_string());
        }
        Ok(res)
    }

    async fn tools_call(&self, params: &Value, notify: Option<Notifier>) -> Value {
        let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let args = params
            .get("arguments")
            .and_then(|a| a.as_object())
            .cloned()
            .unwrap_or_default();
        let progress_token = params
            .get("_meta")
            .and_then(|m| m.get("progressToken"))
            .cloned();
        let progress: Option<ProgressFn> = match (progress_token, notify) {
            (Some(tok), Some(n)) => {
                let last = std::sync::Mutex::new(None::<f64>);
                Some(Arc::new(move |ev: dogubako_core::tool::ProgressEvent| {
                    let progress =
                        next_progress(&mut last.lock().unwrap(), ev.fraction.map(f64::from));
                    let mut p = json!({"progressToken": tok, "progress": progress, "total": 100.0});
                    if let Some(m) = ev.message {
                        p["message"] = json!(m);
                    }
                    n(json!({"jsonrpc": "2.0", "method": "notifications/progress", "params": p}));
                }))
            }
            _ => None,
        };
        match self.call_inner(name, args, progress).await {
            Ok(v) => v,
            Err(e) => error_result(&e),
        }
    }

    async fn call_inner(
        &self,
        name: &str,
        args: Map<String, Value>,
        progress: Option<ProgressFn>,
    ) -> Result<Value, ToolError> {
        let wait_arg = |a: &Map<String, Value>, d: u64| {
            Duration::from_secs(
                a.get("wait_seconds")
                    .and_then(|w| w.as_u64())
                    .unwrap_or(d)
                    .min(600),
            )
        };
        match name {
            "dogubako_search_tools" => {
                let q = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let cat = args.get("category").and_then(|v| v.as_str());
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(25)
                    .clamp(1, 500) as usize;
                let tools = self.backend.tools();
                let mut scored: Vec<(usize, &ToolInfo)> = vec![];
                let terms: Vec<&str> = q.split_whitespace().collect();
                for t in &tools {
                    if let Some(c) = cat {
                        if t.spec.category.id() != c {
                            continue;
                        }
                    }
                    let hay = format!(
                        "{} {} {} {} {}",
                        t.spec.id,
                        t.spec.name,
                        t.spec.description,
                        t.spec.keywords.join(" "),
                        t.spec.category.label()
                    )
                    .to_lowercase();
                    if terms.iter().all(|term| hay.contains(term)) {
                        let name_l = t.spec.name.to_lowercase();
                        let score = terms
                            .iter()
                            .map(|term| {
                                if name_l.contains(term) || t.spec.id.contains(term) {
                                    3
                                } else {
                                    1
                                }
                            })
                            .sum::<usize>();
                        scored.push((score, t));
                    }
                }
                scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.spec.id.cmp(&b.1.spec.id)));
                let results: Vec<Value> = scored
                    .iter()
                    .take(limit)
                    .map(|(_, t)| {
                        json!({
                            "id": t.spec.id, "mcp_name": t.spec.mcp_name(), "name": t.spec.name,
                            "category": t.spec.category.id(), "description": t.spec.description,
                            "available": t.availability.available,
                            "unavailable_reason": t.availability.reason,
                        })
                    })
                    .collect();
                let text = if results.is_empty() {
                    "No matching tools.".to_string()
                } else {
                    results
                        .iter()
                        .map(|r| {
                            format!(
                                "- {} — {}{}",
                                r["id"].as_str().unwrap_or(""),
                                r["description"].as_str().unwrap_or(""),
                                if r["available"] == json!(false) {
                                    " (unavailable)"
                                } else {
                                    ""
                                }
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(ok_result(
                    text,
                    json!({"tools": results, "total": scored.len()}),
                ))
            }
            "dogubako_describe_tool" => {
                let id = args.get("tool").and_then(|v| v.as_str()).unwrap_or("");
                let info = self.find(id)?;
                let s = &info.spec;
                let v = json!({
                    "id": s.id, "mcp_name": s.mcp_name(), "name": s.name, "description": s.description,
                    "category": s.category.id(), "version": s.version,
                    "input_schema": schema::input_schema(s, self.backend.file_mode()),
                    "output": s.output, "requires": s.requires, "requires_model": s.requires_model,
                    "permissions": s.permissions, "timeout_secs": s.timeout_secs, "destructive": s.destructive,
                    "platforms": s.platforms, "availability": info.availability, "permitted": true,
                });
                Ok(ok_result(
                    serde_json::to_string_pretty(&v).unwrap_or_default(),
                    v,
                ))
            }
            "dogubako_run_tool" => {
                let id = args
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let tool_args = args
                    .get("arguments")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                let info = self.find(&id)?;
                let job = self
                    .backend
                    .run_tool(&info.spec.id, tool_args, progress, wait_arg(&args, 60))
                    .await?;
                Ok(job_result(&job))
            }
            "dogubako_job_status" => {
                let jid = args.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
                let job = self.backend.job(jid, wait_arg(&args, 0)).await?;
                Ok(job_result(&job))
            }
            "dogubako_job_cancel" => {
                let jid = args.get("job_id").and_then(|v| v.as_str()).unwrap_or("");
                let job = self.backend.cancel_job(jid).await?;
                Ok(job_result(&job))
            }
            "dogubako_list_categories" => {
                let tools = self.backend.tools();
                let cats: Vec<Value> = Category::ALL
                    .iter()
                    .map(|c| {
                        let n = tools.iter().filter(|t| t.spec.category == *c).count();
                        json!({"id": c.id(), "label": c.label(), "description": c.description(), "tools": n})
                    })
                    .filter(|v| v["tools"].as_u64().unwrap_or(0) > 0)
                    .collect();
                let text = cats
                    .iter()
                    .map(|c| {
                        format!(
                            "- {} ({} tools): {}",
                            c["id"].as_str().unwrap_or(""),
                            c["tools"],
                            c["description"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(ok_result(text, json!({"categories": cats})))
            }
            "dogubako_upload_file" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("upload.bin");
                let b64 = args
                    .get("content_base64")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if (b64.len() as u64) * 3 / 4 > self.backend.max_upload() {
                    return Err(ToolError::limit(
                        "file exceeds the MCP upload limit; use the HTTP upload endpoint",
                    ));
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.trim())
                    .map_err(|e| {
                        ToolError::invalid("content_base64", format!("invalid base64: {e}"))
                    })?;
                let f = self.backend.upload(name, bytes).await?;
                Ok(ok_result(
                    format!(
                        "Uploaded '{}' as {} ({} bytes). Use this id for file parameters.",
                        f.name, f.id, f.size
                    ),
                    serde_json::to_value(&f)?,
                ))
            }
            "dogubako_list_files" => {
                let files = self.backend.list_files().await?;
                let text = if files.is_empty() {
                    "No files.".into()
                } else {
                    files
                        .iter()
                        .map(|f| format!("- {} {} ({} bytes, {})", f.id, f.name, f.size, f.mime))
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Ok(ok_result(text, json!({"files": files})))
            }
            other => {
                // Direct invocation by MCP name. Works in compact mode too:
                // listing is about context size, not authorization.
                let info = self.find(other)?;
                let job = self
                    .backend
                    .run_tool(&info.spec.id, args, progress, self.backend.default_wait())
                    .await?;
                Ok(job_result(&job))
            }
        }
    }

    fn find(&self, name: &str) -> Result<ToolInfo, ToolError> {
        self.backend
            .tools()
            .into_iter()
            .find(|t| t.spec.id == name || t.spec.mcp_name() == name)
            .ok_or_else(|| ToolError::NotFound {
                id: format!("{name} (unknown or not permitted)"),
            })
    }

    async fn resources_list(&self) -> Result<Value, (i64, String)> {
        let files = self
            .backend
            .list_files()
            .await
            .map_err(|e| (-32603, e.to_string()))?;
        let res: Vec<Value> = files
            .iter()
            .take(500)
            .map(|f| json!({"uri": f.uri(), "name": f.name, "mimeType": f.mime, "size": f.size}))
            .collect();
        Ok(json!({"resources": res}))
    }

    async fn resources_read(&self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        let id = uri
            .strip_prefix("dogubako://files/")
            .ok_or((-32602, format!("unknown resource uri: {uri}")))?;
        let (f, bytes) = self
            .backend
            .read_file(id, MAX_INLINE_RESOURCE + 1)
            .await
            .map_err(|e| (-32002, e.to_string()))?;
        if f.size > MAX_INLINE_RESOURCE || bytes.len() as u64 > MAX_INLINE_RESOURCE {
            return Err((
                -32603,
                format!(
                    "resource too large for inline transfer ({} bytes); download it from {}",
                    f.size.max(bytes.len() as u64),
                    f.url.clone().unwrap_or_default()
                ),
            ));
        }
        let is_text = f.mime.starts_with("text/")
            || matches!(
                f.mime.as_str(),
                "application/json" | "application/xml" | "image/svg+xml" | "application/x-subrip"
            );
        let content = if is_text {
            json!({"uri": uri, "mimeType": f.mime, "text": String::from_utf8_lossy(&bytes)})
        } else {
            json!({"uri": uri, "mimeType": f.mime, "blob": base64::engine::general_purpose::STANDARD.encode(&bytes)})
        };
        Ok(json!({"contents": [content]}))
    }
}

/// MCP requires each progress notification to report a higher value than the
/// last. Message-only events (no fraction) and late, lower fractions nudge the
/// previous value up by the smallest step so their messages still go out.
fn next_progress(last: &mut Option<f64>, fraction: Option<f64>) -> f64 {
    let wanted = fraction
        .filter(|f| f.is_finite())
        .map(|f| (f * 100.0).clamp(0.0, 100.0));
    let v = match (*last, wanted) {
        (None, w) => w.unwrap_or(0.0),
        (Some(l), Some(w)) if w > l => w,
        (Some(l), _) => l.next_up(),
    };
    *last = Some(v);
    v
}

fn ok_result(text: String, structured: Value) -> Value {
    json!({"content": [{"type": "text", "text": text}], "structuredContent": structured, "isError": false})
}

fn error_result(e: &ToolError) -> Value {
    json!({
        "content": [{"type": "text", "text": format!("Error ({}): {e}", e.code())}],
        "structuredContent": {"error": e},
        "isError": true
    })
}

/// Convert a job into an MCP tool result.
pub fn job_result(job: &JobView) -> Value {
    match job.status {
        JobStatus::Succeeded => {
            let r = job.result.clone().unwrap_or_default();
            let mut content = vec![];
            let mut summary = String::new();
            if let Some(t) = &r.text {
                let shown: String = if t.len() > 200_000 {
                    format!(
                        "{}… [truncated, {} bytes total]",
                        &t[..t
                            .char_indices()
                            .nth(200_000)
                            .map(|(i, _)| i)
                            .unwrap_or(t.len())],
                        t.len()
                    )
                } else {
                    t.clone()
                };
                summary.push_str(&shown);
            } else if !r.data.is_null() && r.data != json!({}) {
                summary.push_str(&serde_json::to_string_pretty(&r.data).unwrap_or_default());
            }
            if !r.artifacts.is_empty() {
                if !summary.is_empty() {
                    summary.push_str("\n\n");
                }
                summary.push_str("Output files:\n");
                for a in &r.artifacts {
                    summary.push_str(&format!(
                        "- {} ({}, {} bytes){}{}\n",
                        a.name,
                        a.mime,
                        a.size,
                        a.id.as_ref()
                            .map(|i| format!(" id={i}"))
                            .unwrap_or_default(),
                        a.path
                            .as_ref()
                            .map(|p| format!(" path={p}"))
                            .or_else(|| a.url.as_ref().map(|u| format!(" url={u}")))
                            .unwrap_or_default()
                    ));
                }
            }
            for w in &r.warnings {
                summary.push_str(&format!("\nWarning: {w}"));
            }
            if summary.is_empty() {
                summary = "Done.".into();
            }
            content.push(json!({"type": "text", "text": summary}));
            for a in &r.artifacts {
                if let Some(id) = &a.id {
                    content.push(json!({"type": "resource_link", "uri": format!("dogubako://files/{id}"), "name": a.name, "mimeType": a.mime, "size": a.size}));
                }
            }
            json!({"content": content, "structuredContent": r, "isError": false})
        }
        JobStatus::Failed | JobStatus::Cancelled => {
            let e = job.error.clone().unwrap_or(ToolError::Cancelled);
            let mut v = error_result(&e);
            v["structuredContent"]["job_id"] = json!(job.id);
            v
        }
        JobStatus::Queued | JobStatus::Running => {
            let pct = job
                .progress
                .map(|p| format!(" ({:.0}%)", p * 100.0))
                .unwrap_or_default();
            json!({
                "content": [{"type": "text", "text": format!("Job {} is {}{pct}. Call dogubako_job_status with job_id=\"{}\" (optionally wait_seconds) to get the result.", job.id, job.status.as_str(), job.id)}],
                "structuredContent": {"job_id": job.id, "status": job.status.as_str(), "progress": job.progress, "message": job.message},
                "isError": false
            })
        }
    }
}

#[cfg(test)]
mod progress_tests {
    use super::{next_progress, read_capped};

    #[tokio::test]
    async fn read_capped_stops_at_the_limit() {
        let dir = std::env::temp_dir().join(format!("dogubako-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.bin");
        std::fs::write(&path, vec![7u8; 1000]).unwrap();
        assert_eq!(read_capped(&path, 101).await.unwrap().len(), 101);
        assert_eq!(read_capped(&path, 5000).await.unwrap().len(), 1000);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn progress_strictly_increases() {
        let mut last = None;
        let seq = [
            Some(0.5),
            None,
            Some(0.2),
            Some(0.8),
            None,
            Some(f64::NAN),
            Some(1.0),
        ];
        let mut prev = f64::NEG_INFINITY;
        for f in seq {
            let v = next_progress(&mut last, f);
            assert!(v > prev, "{v} after {prev}");
            prev = v;
        }
        assert_eq!(prev, 100.0);
    }
}
