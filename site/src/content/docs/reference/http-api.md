---
title: HTTP API
description: Dogubako's REST API for running tools, uploading files and following jobs, with every endpoint.
---

The API is versioned under `/api/v1`. A complete **OpenAPI 3.1** document,
with a typed request schema for every tool, is generated from the live
catalog:

- `GET /api/v1/openapi.json` on a running server (also linked from **Admin →
  API** and **Settings → About**);
- `dogubako api openapi > openapi.json` offline.

Feed it to your favorite client generator, or to an HTTP client like
Bruno, Insomnia or Postman.

## Authentication

- **Scripts and agents:** `Authorization: Bearer dgb_…`. Create a token in
  **Account → API tokens**, or with `dogubako tokens create`. See
  [API tokens](/self-hosting/accounts-and-agents/#api-tokens).
- **Browsers:** a session cookie from `POST /api/v1/auth/login`. Every
  non-GET request with a session must send the returned `csrf_token` as the
  `X-CSRF-Token` header.

Discovery endpoints only return what the caller is permitted to use, and
every run is checked against the caller's policy.

## Typical flow

```sh
H="Authorization: Bearer $TOKEN"

# 1. Upload: multipart (any field name, many files) or a raw body
curl -H "$H" -F file=@photo.jpg https://host/api/v1/files
curl -H "$H" -T photo.jpg "https://host/api/v1/files/raw?name=photo.jpg"     # → {"id":"f_…", …}

# 2. Run: waits up to wait_seconds (default 30); 200 when finished, 202 with the job if still running
curl -H "$H" -H 'Content-Type: application/json' \
     -d '{"params":{"input":["f_…"],"format":"webp","quality":80},"wait_seconds":60}' \
     https://host/api/v1/tools/image.convert/run

# 3. Poll, stream or cancel
curl -H "$H" "https://host/api/v1/jobs/j_…?wait_seconds=30"
curl -N -H "$H" https://host/api/v1/jobs/j_…/events          # server-sent events
curl -X POST -H "$H" https://host/api/v1/jobs/j_…/cancel

# 4. Download outputs
curl -H "$H" -o out.webp https://host/api/v1/files/f_…/content
curl -H "$H" -o all.zip  https://host/api/v1/jobs/j_…/artifacts.zip
```

File parameters take **file ids** (`f_…`). A parameter that accepts several
files takes an array of ids.

## Running a tool

`POST /api/v1/tools/{id}/run`

```json
{
  "params": { "input": ["f_…"], "format": "webp" },
  "wait_seconds": 60,
  "ephemeral": false
}
```

| Field | Meaning |
|---|---|
| `params` | The tool's parameters. The [tool catalog](/tools/) and the OpenAPI document list them |
| `wait_seconds` | How long to wait for the result before returning `202` with the running job. `0` returns immediately |
| `ephemeral` | `true` for throwaway runs (such as live previews): executed and audited, but hidden from history |

## The job object

```json
{
  "id": "j_…",
  "tool_id": "image.convert",
  "status": "succeeded",
  "progress": 1.0,
  "message": null,
  "params": { "input": ["f_…"], "format": "webp" },
  "result": {
    "text": null,
    "text_mime": null,
    "data": {},
    "artifacts": [
      { "id": "f_…", "name": "photo.webp", "mime": "image/webp", "size": 48213, "url": "/api/v1/files/f_…/content" }
    ],
    "warnings": [],
    "duration_ms": 412
  },
  "error": null,
  "created_at": 1767225600,
  "finished_at": 1767225601
}
```

`status` is `queued`, `running`, `succeeded`, `failed` or `cancelled`.
`progress` runs from 0 to 1 when the tool reports it. A failed job carries a
[tagged error](/reference/errors/) in `error`.

### Following progress

`GET /api/v1/jobs/{id}/events` is a server-sent event stream. Each event is
named `job` and its data is the job object as JSON. The stream ends after a
terminal status. A common pattern is to run with `wait_seconds: 0` and then
follow the events.

## Errors

```json
{
  "error": {
    "code": "invalid_input",
    "message": "invalid input for 'quality': must be at most 100",
    "detail": { "code": "invalid_input", "param": "quality", "message": "must be at most 100" }
  }
}
```

All codes and HTTP statuses are in [Errors](/reference/errors/).

## Endpoints

### Public

| Method and path | Purpose |
|---|---|
| `GET /status` | Instance name, version, auth mode, whether setup is required, whether MCP is on, tool count, max upload |
| `POST /setup` | First-run owner creation with the setup token: `{setup_token, username, password}` |
| `GET /healthz` | Liveness (outside `/api/v1`) |

### Session

| Method and path | Purpose |
|---|---|
| `POST /auth/login` | `{username, password}` → sets the session cookie, returns `{csrf_token, user}` |
| `POST /auth/logout` | End the session |
| `GET /auth/me` | The current user, permissions, policy, storage use and CSRF token |
| `POST /auth/password` | `{current_password, new_password}` |

### Tools

| Method and path | Purpose |
|---|---|
| `GET /tools[?q=&category=]` | Permitted tools with their specs, availability and input schemas |
| `GET /tools/{id}` | One tool |
| `GET /categories` | Categories with tool counts |
| `POST /tools/{id}/run` | Run a tool (see above) |

### Jobs

| Method and path | Purpose |
|---|---|
| `GET /jobs[?limit=]` | Your job history |
| `GET /jobs/{id}[?wait_seconds=]` | One job, optionally long-polling until it finishes |
| `GET /jobs/{id}/events` | Server-sent events for one job |
| `POST /jobs/{id}/cancel` | Cancel |
| `DELETE /jobs/{id}` | Delete the job and its outputs |
| `GET /jobs/{id}/artifacts.zip` | All outputs as a zip |

### Files

| Method and path | Purpose |
|---|---|
| `GET /files` | Your files, with storage used and quota |
| `POST /files` | Multipart upload, any field name, many files → `{files: [...]}` |
| `PUT /files/raw?name=` | Raw-body upload of one file → the file object |
| `GET /files/{id}` | File metadata |
| `GET /files/{id}/content[?inline=1]` | Download. Supports `Range`. `inline=1` previews passive types (images, audio, video, PDF, plain text) in a sandbox |
| `DELETE /files/{id}` | Delete |

### Tokens

| Method and path | Purpose |
|---|---|
| `GET /tokens` | Your tokens |
| `POST /tokens` | `{name, expires_days?, scope?}` → `{id, token, prefix}`; the token is shown once |
| `DELETE /tokens/{id}` | Revoke |

### Engines, models and extensions

| Method and path | Purpose |
|---|---|
| `GET /deps` | Engine status with install commands for each package manager |
| `POST /deps/refresh` | Detect engines again |
| `GET /models` | Models and download progress |
| `POST /models/{id}/download` | Download a model (admin) |
| `DELETE /models/{id}` | Remove a model (admin) |
| `GET /extensions` | Extension status (admin) |

### Administration (admin only)

| Method and path | Purpose |
|---|---|
| `GET /admin/users`, `POST /admin/users` | List, or create `{username, password?, role, policy?, description}`. Agents have no password |
| `PATCH /admin/users/{id}` | Change `{role?, policy?, disabled?, description?}` |
| `DELETE /admin/users/{id}` | Delete |
| `POST /admin/users/{id}/password` | Set a password |
| `GET /admin/tokens`, `POST /admin/tokens` | Everyone's tokens; create one for any account: `{user_id, name, expires_days?, scope?}` |
| `GET /admin/audit?limit=&before=` | Audit log, newest first |
| `GET /admin/config` | Effective configuration, running jobs, uptime and the permission list |

### Other

| Method and path | Purpose |
|---|---|
| `GET /openapi.json` | The OpenAPI document |
| `POST /mcp`, `DELETE /mcp` | MCP Streamable HTTP, outside `/api/v1`. See [MCP](/reference/mcp/) |

A `scope` or `policy` object has the shape described in
[Permissions and policies](/reference/permissions-and-policies/#policies).
