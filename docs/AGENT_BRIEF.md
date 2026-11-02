# Tool implementation brief (shared)

You are implementing a slice of the builtin tool catalog for **Dogubako**, an
open-source, local-first utility suite (Rust core + React UI + CLI + HTTP API +
MCP). The core engine already exists and works. Your job: implement many
**genuinely functional** tools in your assigned crate(s), with tests.

## Read first
- `crates/core/src/spec.rs` – `ToolSpec` / `ParamSpec` builders (the declarative tool model)
- `crates/core/src/params.rs` – `Params` accessors (`p.text()`, `p.str()`, `p.opt_int()`, `p.file()`, `p.files()`, `p.list()`, `p.bool()`, `p.json()`, `p.path()`...)
- `crates/core/src/tool.rs` – `ToolContext` (output_path, artifact, write_output, command, command_sibling, blocking, progress, check_cancelled, http_client, temp_path/temp_dir), `ToolOutput`, `sync_tool`, `async_tool`
- `crates/core/src/process.rs` – `Exec` builder for external programs (argv only, timeouts, cancellation, rlimits, `on_stdout_line`/`on_stderr_line` for progress, `run()` / `run_unchecked()`, `stdin()`)
- `crates/core/src/error.rs` – `ToolError` + `Context` trait (`.ctx("what")`, `.bad_input("param")`)
- `crates/core/src/deps.rs` – known external dependency ids: ffmpeg, ffprobe, imagemagick, vips, exiftool, qpdf, poppler (pdftoppm; use `ctx.command_sibling("poppler","pdftotext")` for siblings), ghostscript, tesseract, whisper, piper, rembg, realesrgan, libreoffice, pandoc, 7zip, bwrap
- `crates/core/src/models.rs` – optional model registry (`ctx.models().path("seeta-face")`, `installed_in_group("whisper")`), groups: whisper, whisper-vad, face-detection, piper
- `crates/core/src/policy.rs`, `crates/core/src/net.rs` – FsPolicy, NetPolicy, `net::check_url`, `net::resolve_checked`
- `crates/core/src/testing.rs` – `TestHarness` to run tools through the real engine in tests
- Example: `crates/tools-text/src/text.rs` (one tool + test)

Each tool crate exposes `pub fn tools() -> Vec<Arc<dyn Tool>>`, aggregated by `crates/tools`.

## Rules
1. **Only edit files inside your assigned crate directories.** Do not edit `crates/core` or other crates. If core lacks a helper, write it locally in your crate and mention it in your report. Add dependencies to your crate with `cargo add --package <crate-name> <dep>`.
2. **No fake tools, placeholders, or fabricated results.** Every registered tool must really work. If you can't make something work properly, don't register it — list it under limitations.
3. **Reuse mature libraries/programs.** Prefer well-maintained crates (pure Rust or small `cc`-built C) and external programs via `ctx.command("<dep-id>")`. Declare external deps with `.requires(&["ffmpeg"])` so the engine reports missing deps; don't hard-fail at registration. Do NOT add heavy native deps needing system libraries (no OpenCV, no onnxruntime/ort, no GTK, no libtorch). Licenses must be compatible with AGPL-3.0-or-later (MIT/Apache/BSD/ISC/Zlib/MPL/LGPL/GPL OK; no non-commercial/proprietary).
4. **Security.** Never use a shell. Pass paths as separate argv elements; when a user-controlled value could be mistaken for an option by an external program, guard it (e.g. prefix `./`, use `--`, or validate). Only read files given as params. Tools that touch arbitrary host paths must use `ParamSpec::path(..)` params and declare `.permission(Permission::HostFsRead/HostFsWrite)`; destructive operations default to dry-run/preview and need an explicit `apply`/`confirm` boolean, and should be marked `.destructive()`. Archive extraction must reject path traversal/absolute paths/symlinks escaping, and enforce `ctx.limits().max_extract_bytes/max_extract_entries`. Decoding images must respect `ctx.limits().max_image_pixels`. Network tools must use `ctx.http_client()` (SSRF-guarded) or `dogubako_core::net::resolve_checked` and declare `.permission(Permission::Network)`.
5. **Outputs.** Write files only via `ctx.output_path(name)` (or `ctx.write_output`) and return `ctx.artifact(path)`. Derive names from inputs (`dogubako_core::util::suffixed(&file.name, "compressed", Some("webp"))`). Text results: `ToolOutput::text(..)` / `text_as(.., "application/json")`. Structured results: `.with_data(json!(..))`.
6. **Long work.** Report `ctx.progress(fraction, msg)` and call `ctx.check_cancelled()?` in loops; mark spec `.progress()`; set a sensible `.timeout(secs)`. CPU-heavy native code: `sync_tool` (already on the blocking pool) or `ctx.blocking(..)` inside `async_tool`. FFmpeg: use `-progress pipe:1 -nostats` with `on_stdout_line` parsing `out_time_us=`/`out_time_ms=` against the probed duration.
7. **Batch.** Prefer `ParamSpec::files("input")` (multiple) for operations that make sense in bulk (convert/resize/compress...), producing one output per input, with progress per file.
8. **Spec quality.** IDs: `<category>.<snake_name>` (category prefix matches `Category`), params snake_case. Clear `describe(..)` sentence, useful `keywords(..)`, labels, defaults, `.advanced()` for rare options, `.primary()` on the main input(s) (become positional CLI args), `.accept(&["image/*", ".pdf"])` for file params, `.show_if(..)` for conditional options. Set `.text_output(mime)` / `.output(OutputKind::Files|Data)`.
9. **Frontend rendering conventions for `data`** (the UI renders these automatically):
   - `data.table = {"columns": [..], "rows": [[..], ..]}` → table view
   - `data.colors = [{"hex": "#rrggbb", "name": ".."}]` → color swatches
   - `data.diff = [{"tag": "equal"|"insert"|"delete", "text": ".."}]` → inline diff
   - `data.regions = [{"x","y","width","height","label"?,"score"?}]` (pixels, relative to the first input image) → box overlay
   - `data.matches = [{"start","end","text","groups":[..]}]` → regex highlight over the input text
   - Any other object → key/value list with JSON tree for nested values.
   - Text with mime `text/markdown`, `text/html`, `image/svg+xml`, `application/json` gets previews.
   - `ui` hints you may set: `"live"` (fast pure tool: UI re-runs as the user types — use for instant text/dev tools), `"face-redact"`, `"crop"`, `"regex"`, `"qr"`, `"color"`, `"system"` (auto-refreshing dashboard), `"diff"`.
10. **Tests.** Every crate needs tests using `TestHarness` covering representative tools, malformed input, and edge cases. Generate fixtures programmatically (image crate, `ffmpeg -f lavfi -i testsrc`, qpdf/lopdf-generated PDFs, etc.). Tests depending on external programs must skip gracefully: `if !h.available("video.compress") { return; }`. Installed here: ffmpeg/ffprobe 6.1, ImageMagick 6 (`convert`), libvips, exiftool, qpdf, poppler-utils, ghostscript, tesseract (eng), 7z. Not installed: whisper.cpp, piper, rembg, realesrgan, libreoffice, pandoc. Network: crates.io, npm and GitHub work; huggingface.co is blocked.
11. **Build etiquette.** Other agents build concurrently in the same workspace/target dir; "Blocking waiting for file lock" is normal — just wait. Build/test only your crate: `cargo test -p <crate>`. Never run `cargo clean`, never format/modify other crates (`cargo fmt -p <crate>` is fine). Do not commit to git. Keep warnings at zero in your crate.
12. **Final report** (≤ 70 lines): tool ids implemented (one line each), dependencies added (crate + license), external programs used, test results, known limitations/anything not done.
