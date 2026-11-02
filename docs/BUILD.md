# Building

## Prerequisites

* Rust ≥ 1.88 (`rustup`), a C compiler (some crates build bundled C: SQLite, libwebp, WebRTC VAD)
* Node.js ≥ 20 and npm (web UI)
* Desktop only: Tauri 2 system dependencies —
  Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev`;
  macOS: Xcode command-line tools; Windows: WebView2 (preinstalled on Windows 11) + MSVC build tools.

## Web UI

```sh
cd web
npm ci
npm run dev        # http://localhost:5173, proxies /api and /mcp to 127.0.0.1:8420
npm test           # vitest
npm run build      # → web/dist (embedded into the server binary)
```

## CLI + server

```sh
cargo build --release -p dogubako-cli        # target/release/dogubako
./target/release/dogubako server start       # http://127.0.0.1:8420
```

The server embeds `web/dist` at compile time in release builds (debug builds
read it from disk), so build the web UI first. Without it the server still
works and serves a short notice at `/`.

Feature flags trim the tool catalog for smaller binaries:

```sh
cargo build --release -p dogubako-cli --no-default-features --features dogubako-tools/text,dogubako-tools/image,dogubako-server/all-tools
```

## Desktop app (Tauri 2)

```sh
cargo install tauri-cli --version '^2'
cd apps/desktop/src-tauri
cargo tauri dev        # runs Vite dev server + app
cargo tauri build      # .dmg/.app, .msi/.exe, .deb/.rpm/.AppImage
```

The desktop crate is a separate Cargo workspace so that building the server
never requires GTK/WebKit.

## Tests

```sh
cargo test --workspace                       # ~210 Rust tests; tests needing missing programs skip
cargo clippy --workspace --all-targets -- -D warnings
cd web && npm test
```

Integration tests use FFmpeg, ImageMagick, libvips, ExifTool, qpdf, Poppler,
Ghostscript, Tesseract and 7-Zip when installed (CI installs them on Linux).
Optional: `DOGUBAKO_TEST_MODELS=<dir>` with model files enables model-based tests.

## Docker

```sh
docker build -t dogubako .
docker build --build-arg WITH_WHISPER=1 -t dogubako:whisper .
```
