---
title: Architecture
description: How Dogubako is put together, from crates to the single execution path every interface shares.
sidebar:
  order: 1
---

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
| `dogubako-core` | The `ToolSpec` and `ParamSpec` model, the registry and search, validation and coercion, JSON Schema generation, and the `Engine`: authorization hook, permissions, platform, dependency and model checks, scratch folders, timeouts, cancellation and artifact containment. Also the `Exec` process runner, `FsPolicy`, `NetPolicy` and `ExecLimits`, dependency detection, model downloads with checksum verification, extension manifests with the trust store, and a test harness |
| `dogubako-tools-*` | Built-in tools, one crate per domain: image, media, docs, files, text, net, system. Feature-gated in `dogubako-tools` |
| `dogubako-tools` | Aggregates the built-ins and trusted extensions into a registry |
| `dogubako-mcp` | A transport-independent MCP JSON-RPC server (`McpBackend` trait), the meta-tools, and API view types |
| `dogubako-server` | The axum app: `/api/v1`, `/mcp`, the embedded UI, SQLite (users, sessions, tokens, files, jobs, audit), a job manager with bounded concurrency, and storage with ownership, quotas and retention |
| `dogubako-cli` | The `dogubako` binary: generated tool commands, `server`, `mcp serve` (stdio) and admin commands |
| `apps/desktop` | The Tauri 2 shell, which calls the engine in-process with no network server |
| `web` | The React UI shared by the browser and the desktop app |

## One implementation, every interface

A tool is a `ToolSpec` plus a `run` function. From the spec:

- the UI renders a form, or a specialized component chosen by `spec.ui`;
- the CLI builds `dogubako <category> <tool>` with typed flags;
- the server exposes `POST /api/v1/tools/{id}/run` and an OpenAPI schema;
- MCP lists `{category}_{name}` with a JSON input schema and annotations.

All of them call `Engine::run(RunRequest)`. Interfaces differ only in:

- how they **resolve file references** (`FileResolver`: local paths, or owned
  file ids on a server);
- which **permissions** and **authorization predicate** they pass;
- where **outputs** go.

## Execution flow on a server

1. The request is authenticated (session plus CSRF token, or a bearer token)
   into a `Principal`: the account's policy intersected with the token's
   scope.
2. `JobManager::submit` checks the tool is permitted and quota is available,
   then persists a queued job.
3. The job acquires the principal's semaphore and the global semaphore.
4. `Engine::run`: authorize → platform → capability permissions →
   dependencies and models → validate and coerce parameters → resolve file
   ids (checking ownership) → create a scratch folder → run the tool with
   progress, cancellation and a timeout.
5. Artifacts are moved into storage under new random ids owned by the
   principal. The result is persisted, SSE subscribers are notified, and an
   audit entry is written.

## Performance choices

- Startup builds the registry from static specs with no I/O. Dependency
  detection is lazy and cached per dependency. Models load only when used.
- Native image, text and data tools run in-process on the blocking pool.
  Heavy media work goes to FFmpeg and friends with bounded concurrency.
- Uploads and downloads stream. Artifacts are moved, not copied. HTTP Range
  requests support media seeking.
- The desktop app runs no HTTP server and no background services.

See [Performance](/project/performance/) for measured numbers.

## Repository layout

```
crates/core          engine: specs, registry, validation, safe exec, policies, models, extensions
crates/tools-*       tool implementations (image, media, docs, files, text, net, system)
crates/tools         aggregates built-ins + trusted extensions
crates/mcp           MCP protocol (transport-agnostic)
crates/server        self-hosted service (axum, SQLite, auth, jobs, storage, MCP HTTP)
crates/cli           `dogubako` binary
apps/desktop         Tauri 2 desktop app
web                  React + TypeScript UI (shared by desktop and web)
extensions           example TOML extension
deploy               compose overlays, systemd unit, example config
docs                 Markdown docs kept with the code
site                 this documentation site (Astro Starlight)
```
