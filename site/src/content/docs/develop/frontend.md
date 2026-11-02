---
title: The web app
description: How the React app in web/ runs in a browser and in the desktop app, and the backend contract it relies on.
sidebar:
  order: 5
---

The React and TypeScript app in `web/` runs in two hosts with one codebase:

1. **Server mode**: a browser talking to `dogubako server start` over the
   HTTP API.
2. **Desktop mode**: the Tauri 2 webview, calling the engine through Tauri
   commands with no HTTP server. Detected with
   `'__TAURI_INTERNALS__' in window`.

Both implement the same `Backend` interface (`web/src/lib/backend/`), so pages
and tools never care which one they run on. Tauri packages are imported
dynamically, so the web build never needs them at runtime.

## Structure

| Path | Holds |
|---|---|
| `src/Shell.tsx` | Layout, routing, global shortcuts, sidebar, Tools menu, search |
| `src/pages/` | Home, the drop screen (`Intent.tsx`), History, Files, Settings, Account, Admin |
| `src/tools/` | The tool page: top row, stage, generic form tool, run bar, and `special/` editors (face redaction, crop, regex, QR, color, diff, system dashboard) |
| `src/results/` | Result views: artifacts, data tables, swatches, diff, JSON tree, text output, errors |
| `src/form/` | Form fields generated from parameter specs |
| `src/lib/` | Backends, the router, stores, search, the intent ranking for dropped files, the bench |
| `src/styles/` | Design tokens (`tokens.css`) and component styles |

Routes are hash-based: `#/tool/<id>`, `#/category/<id>`, `#/open`,
`#/history`, `#/settings/<section>`, `#/admin/<section>` and so on.

## Design tokens

The "paper with frosted panels" look lives in `src/styles/tokens.css`: warm
paper backgrounds, ink for actions, frosted glass only on floating surfaces,
and the brand vermilion (`#e0533d`) only in the logo's latch. Instrument Sans
and DM Mono are bundled. Frosted glass falls back to solid panels when
`backdrop-filter` isn't supported, when the user asks for reduced
transparency, or when **Frosted panels** is turned off.

## Server mode essentials

- `GET /api/v1/status` (public) tells the app the instance name, version, auth
  mode and whether setup is required.
- Sign-in returns a `csrf_token`; **every non-GET request with a session must
  send `X-CSRF-Token`**.
- `GET /api/v1/tools` returns tool specs flattened with `availability`,
  `permitted`, `missing_permissions` and `input_schema`.
- Files upload as multipart to `POST /api/v1/files` (with XHR, for progress).
  File parameters take ids (`f_…`).
- Runs use `POST /api/v1/tools/{id}/run` with `wait_seconds: 0`, then follow
  `GET /api/v1/jobs/{id}/events`. Live and auto-refresh runs pass
  `ephemeral: true` so they stay out of history.

The full API is in the [HTTP API reference](/reference/http-api/).

## Desktop mode commands

Invoked with `invoke` from `@tauri-apps/api/core`:

| Command | Does |
|---|---|
| `app_info()` | `{version, platform, tools, output_dir, data_dir}` |
| `list_tools()` | Tool specs, same shape as the server (`permitted` is always true) |
| `run_tool({runId, toolId, params})` | Runs and returns the final job. File parameters take absolute local paths; artifacts have `path` instead of `url`. Progress arrives as `dogubako://progress` events |
| `cancel_run({runId})` | Cancel |
| `stage_file({name, bytes})` | Save a browser `File` (paste, in-page drag) to a temporary path |
| `file_info({path})` | `{path, name, size, mime}` |
| `save_artifact({src, dest})` | Copy an output to a destination chosen with the save dialog |
| `reveal_path({path})` | Show in the file manager |
| `deps_status()`, `models_list()` | Same shapes as the server |
| `model_download({id})`, `model_remove({id})` | Progress arrives as `dogubako://model-progress` events |

Native file picking uses `@tauri-apps/plugin-dialog`, OS drag and drop uses
`getCurrentWebview().onDragDropEvent`, and previews use `convertFileSrc`
(the asset protocol is enabled for the output folder and user-picked files).
There are no accounts, admin pages or Files page in desktop mode.

## Developing

```sh
cd web
npm ci
npm run dev        # http://localhost:5173, proxies /api and /mcp to 127.0.0.1:8420
npm test
npm run build
```

Run a server on port 8420 next to it; see
[Building](/develop/building/#web-ui).
