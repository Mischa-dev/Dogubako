//! OpenAPI 3.1 document generated from the live tool registry.

use dogubako_core::engine::Engine;
use dogubako_core::schema;
use serde_json::{json, Map, Value};

fn op(summary: &str, tag: &str, extra: Value) -> Value {
    let mut o = json!({"summary": summary, "tags": [tag], "responses": {"200": {"description": "OK"}, "401": {"$ref": "#/components/responses/Error"}, "403": {"$ref": "#/components/responses/Error"}}});
    if let (Some(o), Value::Object(e)) = (o.as_object_mut(), extra) {
        for (k, v) in e {
            o.insert(k, v);
        }
    }
    o
}

fn id_param(name: &str) -> Value {
    json!({"name": name, "in": "path", "required": true, "schema": {"type": "string"}})
}

pub fn spec(engine: &Engine) -> Value {
    let mut paths = Map::new();
    let mut add = |path: &str, method: &str, v: Value| {
        let entry = paths.entry(path.to_string()).or_insert_with(|| json!({}));
        entry[method] = v;
    };
    add("/api/v1/status", "get", op("Instance status (public)", "meta", json!({"security": []})));
    add("/api/v1/setup", "post", op("Create the owner account with the setup token (first run only)", "auth", json!({"security": []})));
    add("/api/v1/auth/login", "post", op("Log in (sets a session cookie, returns a CSRF token)", "auth", json!({"security": [], "requestBody": {"content": {"application/json": {"schema": {"type": "object", "required": ["username", "password"], "properties": {"username": {"type": "string"}, "password": {"type": "string"}}}}}}})));
    add("/api/v1/auth/logout", "post", op("Log out", "auth", json!({})));
    add("/api/v1/auth/me", "get", op("Current principal, permissions and storage usage", "auth", json!({})));
    add("/api/v1/tools", "get", op("List permitted tools with availability", "tools", json!({"parameters": [
        {"name": "q", "in": "query", "schema": {"type": "string"}},
        {"name": "category", "in": "query", "schema": {"type": "string"}}
    ]})));
    add("/api/v1/tools/{id}", "get", op("Describe a tool", "tools", json!({"parameters": [id_param("id")]})));
    add("/api/v1/categories", "get", op("Categories with tool counts", "tools", json!({})));
    add("/api/v1/jobs", "get", op("List my jobs", "jobs", json!({})));
    add("/api/v1/jobs/{id}", "get", op("Job status/result (optionally wait_seconds)", "jobs", json!({"parameters": [id_param("id"), {"name": "wait_seconds", "in": "query", "schema": {"type": "integer"}}]})));
    add("/api/v1/jobs/{id}", "delete", op("Delete a finished job and its outputs", "jobs", json!({"parameters": [id_param("id")]})));
    add("/api/v1/jobs/{id}/events", "get", op("Server-sent events with job progress", "jobs", json!({"parameters": [id_param("id")]})));
    add("/api/v1/jobs/{id}/cancel", "post", op("Cancel a job", "jobs", json!({"parameters": [id_param("id")]})));
    add("/api/v1/jobs/{id}/artifacts.zip", "get", op("Download all outputs of a job as ZIP", "jobs", json!({"parameters": [id_param("id")]})));
    add("/api/v1/files", "get", op("List my files", "files", json!({})));
    add("/api/v1/files", "post", op("Upload files (multipart/form-data, any field name)", "files", json!({"requestBody": {"content": {"multipart/form-data": {"schema": {"type": "object", "properties": {"file": {"type": "string", "format": "binary"}}}}}}})));
    add("/api/v1/files/raw", "put", op("Upload a file as the raw request body", "files", json!({"parameters": [{"name": "name", "in": "query", "required": true, "schema": {"type": "string"}}], "requestBody": {"content": {"application/octet-stream": {"schema": {"type": "string", "format": "binary"}}}}})));
    add("/api/v1/files/{id}", "get", op("File metadata", "files", json!({"parameters": [id_param("id")]})));
    add("/api/v1/files/{id}", "delete", op("Delete a file", "files", json!({"parameters": [id_param("id")]})));
    add("/api/v1/files/{id}/content", "get", op("Download file content (supports Range; inline=1 for safe previews)", "files", json!({"parameters": [id_param("id"), {"name": "inline", "in": "query", "schema": {"type": "boolean"}}]})));
    add("/api/v1/tokens", "get", op("My API tokens", "tokens", json!({})));
    add("/api/v1/tokens", "post", op("Create an API token for myself (optionally narrower scope)", "tokens", json!({})));
    add("/api/v1/tokens/{id}", "delete", op("Revoke a token", "tokens", json!({"parameters": [id_param("id")]})));
    add("/api/v1/deps", "get", op("External dependency status", "system", json!({})));
    add("/api/v1/models", "get", op("Optional models and download state", "system", json!({})));
    add("/api/v1/models/{id}/download", "post", op("Download a model (admin)", "system", json!({"parameters": [id_param("id")]})));
    add("/api/v1/admin/users", "get", op("List users and agents (admin)", "admin", json!({})));
    add("/api/v1/admin/users", "post", op("Create a user or agent (admin)", "admin", json!({})));
    add("/api/v1/admin/users/{id}", "patch", op("Update role, policy, disabled state (admin)", "admin", json!({"parameters": [id_param("id")]})));
    add("/api/v1/admin/users/{id}", "delete", op("Delete user/agent and their files (admin)", "admin", json!({"parameters": [id_param("id")]})));
    add("/api/v1/admin/tokens", "get", op("All tokens (admin)", "admin", json!({})));
    add("/api/v1/admin/tokens", "post", op("Create a token for any user/agent (admin)", "admin", json!({})));
    add("/api/v1/admin/audit", "get", op("Security & execution audit log (admin)", "admin", json!({})));
    add("/mcp", "post", op("MCP Streamable HTTP endpoint (JSON-RPC 2.0)", "mcp", json!({})));

    let mut tool_paths = Map::new();
    for spec in engine.registry.specs() {
        let body = json!({
            "type": "object",
            "properties": {
                "params": schema::input_schema(spec, "id"),
                "wait_seconds": {"type": "integer", "minimum": 0, "maximum": 600, "default": 30}
            },
            "required": ["params"]
        });
        tool_paths.insert(
            format!("/api/v1/tools/{}/run", spec.id),
            json!({"post": {
                "summary": spec.name,
                "description": spec.description,
                "operationId": format!("run_{}", spec.mcp_name()),
                "tags": [format!("tool:{}", spec.category.id())],
                "requestBody": {"required": true, "content": {"application/json": {"schema": body}}},
                "responses": {
                    "200": {"description": "Job finished", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Job"}}}},
                    "202": {"description": "Job still running; poll /api/v1/jobs/{id}", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Job"}}}},
                    "400": {"$ref": "#/components/responses/Error"},
                    "403": {"$ref": "#/components/responses/Error"}
                }
            }}),
        );
    }
    for (k, v) in tool_paths {
        paths.insert(k, v);
    }

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Dogubako API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Local-first utility toolbox. Upload inputs to /api/v1/files, run tools with POST /api/v1/tools/{id}/run, download outputs from artifact URLs. Authenticate with `Authorization: Bearer dgb_…` API tokens (or a browser session + X-CSRF-Token header).",
            "license": {"name": "AGPL-3.0-or-later"}
        },
        "servers": [{"url": "/"}],
        "security": [{"bearer": []}, {"session": []}],
        "paths": paths,
        "components": {
            "securitySchemes": {
                "bearer": {"type": "http", "scheme": "bearer", "description": "API token (dgb_…)"},
                "session": {"type": "apiKey", "in": "cookie", "name": "dgb_session"}
            },
            "responses": {"Error": {"description": "Error", "content": {"application/json": {"schema": {"$ref": "#/components/schemas/Error"}}}}},
            "schemas": {
                "Error": {"type": "object", "properties": {"error": {"type": "object", "properties": {"code": {"type": "string"}, "message": {"type": "string"}, "detail": {}}}}},
                "Artifact": {"type": "object", "properties": {"id": {"type": "string"}, "name": {"type": "string"}, "mime": {"type": "string"}, "size": {"type": "integer"}, "url": {"type": "string"}, "label": {"type": "string"}}},
                "Job": {"type": "object", "properties": {
                    "id": {"type": "string"}, "tool_id": {"type": "string"},
                    "status": {"type": "string", "enum": ["queued", "running", "succeeded", "failed", "cancelled"]},
                    "progress": {"type": "number"}, "message": {"type": "string"},
                    "result": {"type": "object", "properties": {"text": {"type": "string"}, "text_mime": {"type": "string"}, "data": {}, "artifacts": {"type": "array", "items": {"$ref": "#/components/schemas/Artifact"}}, "warnings": {"type": "array", "items": {"type": "string"}}, "duration_ms": {"type": "integer"}}},
                    "error": {"type": "object"}, "created_at": {"type": "integer"}, "finished_at": {"type": "integer"}
                }}
            }
        }
    })
}
