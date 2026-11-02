# HTTP API

Versioned under `/api/v1`. The complete OpenAPI 3.1 document — including a
typed request schema for every tool — is generated from the live registry:

* `GET /api/v1/openapi.json` on a running server (also linked from the UI's API page)
* `dogubako api openapi > openapi.json`

## Authentication

* Scripts and agents: `Authorization: Bearer dgb_…` (create in the UI under your
  profile → API tokens, or `dogubako tokens create`).
* Browsers: session cookie from `POST /api/v1/auth/login`; send the returned
  `csrf_token` as `X-CSRF-Token` on non-GET requests.

## Typical flow

```sh
H="Authorization: Bearer $TOKEN"
# 1. upload (multipart or raw body)
curl -H "$H" -F file=@photo.jpg https://host/api/v1/files
curl -H "$H" -T photo.jpg "https://host/api/v1/files/raw?name=photo.jpg"      # → {"id":"f_…"}

# 2. run (waits up to wait_seconds, default 30; 202 + job if still running)
curl -H "$H" -H 'Content-Type: application/json' \
     -d '{"params":{"input":["f_…"],"format":"webp","quality":80},"wait_seconds":60}' \
     https://host/api/v1/tools/image.convert/run

# 3. poll / stream / cancel
curl -H "$H" "https://host/api/v1/jobs/j_…?wait_seconds=30"
curl -N -H "$H" https://host/api/v1/jobs/j_…/events        # Server-Sent Events
curl -X POST -H "$H" https://host/api/v1/jobs/j_…/cancel

# 4. download outputs
curl -H "$H" -o out.webp https://host/api/v1/files/f_…/content
curl -H "$H" -o all.zip https://host/api/v1/jobs/j_…/artifacts.zip
```

Job object:

```json
{
  "id": "j_…", "tool_id": "image.convert", "status": "succeeded",
  "progress": 1.0,
  "result": {
    "text": null, "data": {…},
    "artifacts": [{"id": "f_…", "name": "photo.webp", "mime": "image/webp", "size": 48213, "url": "/api/v1/files/f_…/content"}],
    "warnings": [], "duration_ms": 412
  },
  "created_at": 1767225600, "finished_at": 1767225601
}
```

Errors: `{"error": {"code": "invalid_input", "message": "…", "detail": {"code": "invalid_input", "param": "quality", "message": "must be at most 100"}}}`.
Job failures carry the same tagged error in `job.error`. Codes: `invalid_input`,
`not_found`, `missing_dependency`, `missing_model`, `unsupported`,
`permission_denied`, `cancelled`, `timeout`, `limit_exceeded`,
`external_failure`, `failed`.

## Endpoints

| Method & path | Purpose |
|---|---|
| `GET /status` | Public instance info (auth mode, setup required, version) |
| `POST /setup` | First-run owner creation with setup token |
| `POST /auth/login`, `POST /auth/logout`, `GET /auth/me`, `POST /auth/password` | Sessions |
| `GET /tools[?q=&category=]`, `GET /tools/{id}`, `GET /categories` | Discovery (only permitted tools) |
| `POST /tools/{id}/run` | Run a tool |
| `GET /jobs`, `GET /jobs/{id}[?wait_seconds=]`, `GET /jobs/{id}/events`, `POST /jobs/{id}/cancel`, `DELETE /jobs/{id}`, `GET /jobs/{id}/artifacts.zip` | Jobs |
| `GET /files`, `POST /files`, `PUT /files/raw?name=`, `GET /files/{id}`, `GET /files/{id}/content[?inline=1]` (Range supported), `DELETE /files/{id}` | Files |
| `GET /tokens`, `POST /tokens`, `DELETE /tokens/{id}` | Own API tokens |
| `GET /deps`, `POST /deps/refresh` | External dependency status |
| `GET /models`, `POST /models/{id}/download`, `DELETE /models/{id}` | Models (admin for changes) |
| `GET /extensions` | Extension status (admin) |
| `GET/POST /admin/users`, `PATCH/DELETE /admin/users/{id}`, `POST /admin/users/{id}/password` | Users & agents (admin) |
| `GET/POST /admin/tokens` | Tokens for anyone (admin) |
| `GET /admin/audit?limit=&before=` | Audit log (admin) |
| `GET /admin/config` | Effective config and runtime stats (admin) |
| `POST/DELETE /mcp` | MCP Streamable HTTP (see [MCP.md](MCP.md)) |
| `GET /healthz` | Liveness |
