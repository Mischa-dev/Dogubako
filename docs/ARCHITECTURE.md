# Architecture

```
                ┌──────────────── web/ (React + TS + Vite) ────────────────┐
                │ one UI codebase · Backend interface: HTTP | Tauri invoke │
                └───────────────┬──────────────────────────┬───────────────┘
                                │ HTTP/SSE                 │ IPC (no server)
   agents ── MCP (HTTP) ──┐     │                          │
                          ▼     ▼                          ▼
              crates/server (axum)                  apps/desktop (Tauri 2)
              auth · authz · jobs · storage
              SQLite · MCP Streamable HTTP
                          │                                │
  agents ── MCP (stdio) ──┤  crates/cli (clap, generated   │
                          │  per-tool commands)            │
                          ▼                                ▼
                    crates/core — Engine::run (the only execution path)
     registry · specs → JSON Schema · validation · permissions · deps · models
     safe process runner (argv, timeouts, rlimits, bwrap) · SSRF-guarded net
                          │
          crates/tools → tools-image · tools-media · tools-docs · tools-files
                         tools-text · tools-net · tools-system · TOML extensions
                          │
        FFmpeg · ImageMagick · libvips · ExifTool · qpdf · Poppler · Ghostscript
        Tesseract · whisper.cpp · Piper · LibreOffice · Pandoc · 7-Zip · rembg …
```

## Crates

| Crate | Responsibility |
|---|---|
| `dogubako-core` | `ToolSpec`/`ParamSpec` model, `Registry` + search, validation & coercion, JSON Schema generation, `Engine` (authorization hook, permissions, platform/dependency/model checks, scratch dirs, timeouts, cancellation, artifact containment), `Exec` process runner, `FsPolicy`/`NetPolicy`/`ExecLimits`, dependency detection, model downloads with checksum verification, declarative extension manifests + trust store, test harness |
| `dogubako-tools-*` | Builtin tool implementations, one crate per domain (feature-gated in `dogubako-tools`) |
| `dogubako-tools` | Aggregates builtins + trusted extensions into a registry |
| `dogubako-mcp` | Transport-independent MCP JSON-RPC server (`McpBackend` trait), meta-tools, API view types |
| `dogubako-server` | axum app: `/api/v1`, `/mcp`, embedded UI, SQLite (users, sessions, tokens, files, jobs, audit), job manager with bounded concurrency, storage with ownership/quotas/retention |
| `dogubako-cli` | `dogubako` binary: generated tool commands, `server`, `mcp serve` (stdio), admin commands |
| `apps/desktop` | Tauri 2 shell; calls the engine in-process via commands (no network server) |
| `web` | React UI shared by browser and desktop |

## One implementation, every interface

A tool is a `ToolSpec` plus a `run` function. From the spec:

* the UI renders a form (or a specialized component chosen by `spec.ui`);
* the CLI builds `dogubako <category> <tool>` with typed flags;
* the server exposes `POST /api/v1/tools/{id}/run` and an OpenAPI schema;
* MCP lists `{category}_{name}` with a JSON input schema and annotations.

All of them call `Engine::run(RunRequest)`. Interfaces differ only in how they
**resolve file references** (`FileResolver`: local paths vs. owned file ids),
which **permissions** and **authorization predicate** they pass, and where
**outputs** go.

## Execution flow (server)

1. Request authenticated (session + CSRF, or bearer token) → `Principal` (policy ∩ token scope).
2. `JobManager::submit` checks the tool is permitted and quota is available, persists a queued job.
3. Job acquires the principal's semaphore and the global semaphore.
4. `Engine::run`: authorize → platform → capability permissions → dependencies/models → validate & coerce params → resolve file ids (ownership) → scratch dir → tool with progress + cancellation + timeout.
5. Artifacts are moved into storage under new random ids owned by the principal; job result persisted; SSE subscribers notified; audit entry written.

## Performance notes

* Startup builds the registry from static specs (no I/O); dependency detection is lazy and cached per dependency; models load only when used.
* Native image/text/data tools run in-process on the blocking pool; heavy media work is delegated to FFmpeg & co. with bounded concurrency.
* Uploads and downloads are streamed; artifacts are moved, not copied; Range requests support media seeking.
* The desktop app runs no HTTP server and no background services.
