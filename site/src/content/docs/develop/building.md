---
title: Building
description: Build Dogubako's web UI, CLI and server, desktop app and Docker image, and run the tests.
sidebar:
  order: 2
---

## Prerequisites

- Rust 1.95 or newer (`rustup`) and a C compiler. Some crates build bundled C
  code: SQLite, libwebp and WebRTC VAD.
- Node.js 20 or newer with npm, for the web UI.
- For the desktop app, Tauri 2's system dependencies:
  - **Linux:** `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev`
  - **macOS:** the Xcode command-line tools
  - **Windows:** WebView2 (preinstalled on Windows 11) and the MSVC build tools

## Web UI

```sh
cd web
npm ci
npm run dev        # http://localhost:5173, proxies /api and /mcp to 127.0.0.1:8420
npm test           # vitest
npm run build      # → web/dist, embedded into the server binary
```

To work on the UI against real tools, run a debug server next to it:

```sh
cargo build -p dogubako-cli
./target/debug/dogubako server start --no-auth --data-dir /tmp/dgb
```

A debug build serves `web/dist` from disk, so `npm run build` and a reload is
enough to see changes on port 8420.

## CLI and server

```sh
cargo build --release -p dogubako-cli        # → target/release/dogubako
./target/release/dogubako server start       # http://127.0.0.1:8420
```

Release builds embed `web/dist` at compile time, so build the web UI first.
Without it the server still works and shows a short notice at `/`.

### Smaller binaries

The full binary is about 62 MiB stripped, because it embeds image codecs,
barcode engines, a jq implementation, DNS and TLS stacks, spreadsheet readers
and the web UI. Category features trim it:

```sh
cargo build --release -p dogubako-cli --no-default-features \
  --features dogubako-tools/text,dogubako-tools/image,dogubako-server/all-tools
```

The `dogubako-tools` features are `image`, `media`, `docs`, `files`, `text`,
`net` and `system`.

## Desktop app (Tauri 2)

```sh
cargo install tauri-cli --version '^2'
cd apps/desktop/src-tauri
cargo tauri dev        # runs the Vite dev server and the app
cargo tauri build      # .dmg/.app, .msi/.exe, .deb/.rpm/.AppImage
```

The desktop crate is a separate Cargo workspace, so building the server never
needs GTK or WebKit.

## Docker

```sh
docker build -t dogubako .
docker build --build-arg WITH_WHISPER=1 -t dogubako:whisper .
```

See [Docker](/self-hosting/docker/#build-options) for every build argument.

## Tests

```sh
cargo test --workspace                       # Rust tests; tests needing missing programs skip
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cd web && npm test
```

Integration tests use FFmpeg, ImageMagick, libvips, ExifTool, qpdf, Poppler,
Ghostscript, Tesseract and 7-Zip when installed; CI installs them on Linux.
Set `DOGUBAKO_TEST_MODELS=<dir>` with model files to enable model-based
tests.

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request:

- **Web UI:** `npm ci`, tests and build.
- **Rust** on Linux, macOS and Windows: build, workspace tests, clippy (on
  Linux) and a CLI smoke test.
- **Desktop app** and **Docker** builds.

Tagging `v*` runs `.github/workflows/release.yml`, which builds draft desktop
releases for all three platforms and CLI binaries.

The docs site has its own workflow; see [The docs site](/develop/docs-site/).
