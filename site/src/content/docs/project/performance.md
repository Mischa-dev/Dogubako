---
title: Performance
description: Measured startup, processing and server numbers, and the design choices behind them.
---

Measured on the development container (4 vCPU, 15 GiB RAM, Linux 6.18,
release build, `lto = "thin"`), wall-clock, warm filesystem cache. Reproduce
with the commands shown; numbers vary by hardware.

| Operation | Command | Time |
|---|---|---|
| CLI startup | `dogubako --version` | 15–20 ms |
| Pure tool end-to-end (process start → output) | `echo '{"a":1}' \| dogubako dev json` | ~21 ms |
| List 249 tools incl. dependency probing | `dogubako tools list --json` | ~530 ms (probes each external program once) |
| Resize 12 MP PNG → 1600 px (native) | `dogubako image resize big.png --width 1600` | 1.1 s |
| Convert 12 MP PNG → WebP (native libwebp) | `dogubako image convert big.png --format webp` | 1.7 s |
| Lossless-optimize 12 MP PNG (oxipng) | `dogubako image compress big.png` | 5.9 s |
| Compress 20 s 1080p30 H.264 (FFmpeg libx264, balanced) | `dogubako video compress hd.mp4` | 14.1 s |
| SHA-256 + BLAKE3 of a 4.6 MB file | `dogubako files hash hd.mp4 --algorithms sha256,blake3` | 0.7 s |
| Server idle memory (RSS) | `dogubako server start` after 2 s | 29 MiB |
| `GET /api/v1/tools` (first call, includes dependency detection) | `curl …/api/v1/tools` | 58 ms |

## Measured improvement

Originally every CLI invocation computed tool availability while building the
generated help, which spawned each external program (`ffmpeg -version`,
`qpdf --version`, …). Startup was **~430 ms** for `dogubako --version`. Making
dependency detection lazy (only `tools list/info`, the API and actual runs
probe, and results are cached per process) brought it to **~17 ms** — a ~25×
improvement, measured before/after with the same release binary flags.

## Design choices that keep things fast

* Registry is built from static specs at startup: no I/O, no plugin scanning beyond the extensions folder.
* Dependency detection is lazy and cached per dependency (`DepRegistry`).
* Models (whisper, face detection, voices) load only inside the tool that uses them.
* Native tools run in-process on Tokio's blocking pool; external engines run with bounded global and per-user concurrency.
* Uploads/downloads stream to/from disk; artifacts are moved, not copied; HTTP Range is supported for media.
* The desktop app runs no HTTP server or background services.

Binary size: the all-tools release binary is ~62 MiB (stripped) because it
embeds image codecs, barcode engines, a jq implementation, DNS/TLS stacks,
spreadsheet readers and the web UI. Category features (`dogubako-tools`
features `image`, `media`, `docs`, `files`, `text`, `net`, `system`) allow
smaller builds.
