# Frontend ↔ backend contract

The React app in `web/` runs in two hosts with one codebase:

1. **Server mode** (browser → `dogubako server start`): HTTP API at `/api/v1` (see `crates/server/src/api.rs` for exact routes/payloads; types in `crates/mcp/src/types.rs`; tool spec types in `crates/core/src/spec.rs` and `crates/core/src/engine.rs` (`ToolInfo` = spec fields flattened + `availability`)).
2. **Desktop mode** (Tauri 2 webview, no HTTP server): Tauri commands below. Detect with `'__TAURI_INTERNALS__' in window`. Import `@tauri-apps/api` / plugins **dynamically** so the web build never requires them at runtime.

## Server mode essentials
- `GET /api/v1/status` (public) → `{instance_name, version, auth: "required"|"none", setup_required, mcp_enabled, tools, mode: "server", max_upload_mb}`
- `POST /api/v1/setup {setup_token, username, password}` → sets cookie, returns `{csrf_token, user}`
- `POST /api/v1/auth/login {username, password}` → sets `dgb_session` cookie, returns `{csrf_token, user}`; `POST /auth/logout`; `GET /auth/me` → `{user:{id,username,role,is_admin,via}, csrf_token, permissions, policy, storage:{used,quota}, max_upload}`; `POST /auth/password {current_password,new_password}`
- **Every non-GET request with a session must send header `X-CSRF-Token: <csrf_token>`** (get it from login/setup/me).
- `GET /api/v1/tools` → `{tools: ToolInfo[] & {permitted: bool, missing_permissions?: string[], input_schema}}`; `GET /api/v1/categories` → `{categories:[{id,label,description,count}]}`
- Files: `POST /api/v1/files` multipart (any field name, many files) → `{files: FileView[]}` (use XHR for upload progress); `GET /api/v1/files` → `{files, used, quota}`; `DELETE /api/v1/files/{id}`; content: `GET /api/v1/files/{id}/content` (`?inline=1` for previews of safe types: images, audio, video, pdf, text/plain, svg, json/csv/vtt as text). File params take **file ids** (`f_…`).
- Run: `POST /api/v1/tools/{id}/run {params, wait_seconds, ephemeral?}` (`ephemeral: true` for live/auto-refresh runs: executed and audited but hidden from history) → `JobView` (200 if finished, 202 if still running). Use `wait_seconds: 0` then follow `GET /api/v1/jobs/{id}/events` (SSE, event name `job`, data = JobView JSON; stream ends after terminal status). `POST /jobs/{id}/cancel`. `GET /jobs?limit=` history, `GET /jobs/{id}[?wait_seconds=]`. `DELETE /jobs/{id}`. `GET /jobs/{id}/artifacts.zip` download all.
- `JobView = {id, tool_id, status: queued|running|succeeded|failed|cancelled, progress?: 0..1, message?, result?: {text?, text_mime?, data, artifacts:[{id,name,mime,size,label?,url}], warnings, duration_ms}, error?: {code, message?, param?, ...}, created_at, finished_at?, params}` — error objects are tagged by `code` (see `crates/core/src/error.rs`: invalid_input has `param` + `message`; missing_dependency has `dependency` + `hint`; external_failure has `program`, `exit_code`, `stderr`...).
- Tokens: `GET/POST /api/v1/tokens` (`{name, expires_days?, scope?: Policy}` → `{id, token, prefix}` shown once), `DELETE /tokens/{id}`.
- Deps/models: `GET /api/v1/deps` → `{dependencies:[{id,name,purpose,found,version,license,install_hint,install:{apt,dnf,pacman,brew,winget,url},used_by:[]}], platform}`; `GET /models` → `{models:[{id,group,name,description,size_bytes,license,installed,verified}], downloads:[{model,done,total,status,error}]}`; `POST /models/{id}/download` (admin), `DELETE /models/{id}` (admin).
- Admin: `GET/POST /admin/users` (`{username, password?, role: owner|admin|user|agent, policy?, description}`; agents have no password), `PATCH /admin/users/{id} {role?, policy?, disabled?, description?}`, `DELETE /admin/users/{id}`, `POST /admin/users/{id}/password {password}`, `GET/POST /admin/tokens` (`{user_id, name, expires_days?, scope?}`), `GET /admin/audit?limit=&before=`, `GET /admin/config` → `{config, running_jobs, uptime_secs, permissions:[{id,description}]}`, `GET /extensions`.
- `Policy = {allow: string[] (patterns "*", "image.*", "video.compress", "category:pdf"), deny: string[], permissions: ("network"|"host_fs_read"|"host_fs_write"|"system_info"|"process_info")[], max_concurrent_jobs?, quota_mb?, max_upload_mb?, mcp_discovery?: "full"|"compact"}`
- `GET /api/v1/openapi.json` – generated OpenAPI document.

## Desktop mode (Tauri commands; `invoke` from `@tauri-apps/api/core`)
- `app_info()` → `{version, platform, tools, output_dir, data_dir}`
- `list_tools()` → `ToolInfo[]` (same shape as server, `permitted` always true, includes `input_schema`)
- `run_tool({runId, toolId, params})` → final `JobView` (artifacts have `path` instead of `url`/`id`). File params take **absolute local paths**. Progress: `listen('dogubako://progress', e => e.payload = {run_id, progress?, message?})`.
- `cancel_run({runId})`
- `stage_file({name, bytes: number[]})` → `{path, name, size}` — for browser `File` objects (paste, in-page drag) that have no path.
- `file_info({path})` → `{path, name, size, mime}`
- `save_artifact({src, dest})` — copy an output to a user-chosen destination (dest from `@tauri-apps/plugin-dialog` `save()`).
- `reveal_path({path})` — show in file manager.
- `deps_status()` → same shape as server `dependencies`; `models_list()` → same as server `models`; `model_download({id})` (progress event `dogubako://model-progress` `{id, done, total}`), `model_remove({id})`.
- Native file picking: `@tauri-apps/plugin-dialog` `open({multiple, filters})` → paths. OS drag & drop: `getCurrentWebview().onDragDropEvent(e => e.payload.type === 'drop' && e.payload.paths)` from `@tauri-apps/api/webview`.
- Previews: `convertFileSrc(path)` from `@tauri-apps/api/core` (asset protocol is enabled for the output dir and user-picked files).
- No accounts, no admin pages, no Files page in desktop mode.
