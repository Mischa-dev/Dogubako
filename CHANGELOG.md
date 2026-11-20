# Changelog

All notable changes to Dogubako are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Alpha releases may change anything.

## [Unreleased]

## [0.1.0-alpha.2] - unreleased

The second alpha. Full notes: [docs/releases/v0.1.0-alpha.2.md](docs/releases/v0.1.0-alpha.2.md).
This entry covers what is on `main` so far; the fixes from the Linux test
campaign are added here as they merge.

### Added

- Podman support: the compose file labels `./data` for SELinux (`:z`) and
  declares the health check itself, and `deploy/dogubako.container` is a
  Quadlet unit with the same hardening, so the container starts at boot. New
  Podman page in the self-hosting docs.
- Optional background removal in the Docker image: build with
  `DOGUBAKO_WITH_REMBG=1` to include rembg and its pinned dependencies.
  rembg's six models are in the model registry (SHA-256 pinned); once a model
  is downloaded, Remove Background runs without the network.

### Changed

- In the Docker image, **Settings → Dependencies** and `dogubako deps` name the
  `.env` build option that adds a missing engine (for example
  `DOGUBAKO_WITH_OFFICE=1`) instead of offering apt, brew or winget commands.
  `/api/v1/deps` reports `docker_image`.
- The engine lock records the exact upstream source of every bottle, and the
  release's engine-sources tar is built only from those pins, with each
  download checked against its SHA-256 or git commit.
- Website: a command palette, a pause for the hero demo, a docs link back to
  the home page, and versioned CSS and script links so a deploy never pairs
  new pages with cached styles.

### Fixed

- The Docker image builds on arm64: its Debian packages are pinned per
  architecture. (Releases still publish linux/amd64 only.)
- The install guide no longer lets `tailscale serve` replace a service that
  already uses HTTPS port 443 on the host.
- Website tools search no longer shows rows it hid.
- `speech.transcribe` and `speech.subtitles` with translate on no longer start
  the output with an unpaired `"` that Whisper adds, and cue and word times on
  short clips stay within the media duration instead of ending at 0:30.
- `speech.transcript_convert` keeps WebVTT cue settings (`align:`, `line:`,
  `position:` ...) on WebVTT output and drops inline `<b>`/`<i>`/`<u>` markup
  from text, JSON and Markdown output.
- Downloading the `whisper-small.en-tdrz` model (speaker turns) no longer
  fails with 404: it now comes from `akashmjn/tinydiarize-whisper.cpp` on
  Hugging Face, and its pinned size is corrected.
- Detect Faces and Blur Faces reject a threshold below 0.1 with an
  `invalid_input` error instead of panicking at 0.
- The Linux x86-64 CLI (`dogubako-x86_64-unknown-linux-gnu`) is built against
  glibc 2.28, so it starts on current stable distros again. 0.1.0-alpha.1
  needed glibc 2.43 and failed on Debian 13. Minimum: glibc 2.28, which means
  Debian 10+, Ubuntu 20.04+, RHEL / AlmaLinux / Rocky 8+ and Fedora 29+.
  Musl distros such as Alpine are not supported; use the Docker image there.
- Convert Audio to Ogg Vorbis no longer fails on default settings: with a
  fixed bitrate, the bitrate (and if needed the sample rate) is fitted to what
  libvorbis accepts for the input, with a warning when it changes.
- Convert Markup to ODT works when Pandoc runs with `--sandbox`.
- Tool forms show validation errors before Run, disable Run until they are
  fixed, mark a result as stale once its inputs change, and always offer Reset.
- PDF to Images with a transparent PNG background works: it renders with
  `pdftocairo`, since `pdftoppm` has no `-transp` flag.
- Add Subtitles rejects WebM output for sources WebM can't hold as-is (such
  as h264/aac) before running FFmpeg, instead of failing with `Error: external`.
- Convert Audio rejects a sample rate the target encoder can't use (MP3 tops
  out at 48 kHz) as invalid input on `sample_rate`.
- Create Archive and Compress check the compression level against the format
  (zip level 0, gzip above 9) and reject an out-of-range level instead of
  failing at run time or clamping it silently.
- Extract Subtitles rejects a `track` past the last subtitle track instead of
  succeeding with no output.
- Regex Tester's Pattern field accepts multi-line verbose patterns, and an
  empty pattern or test text hides the previous matches.
- Convert Document rejects a target the input's LibreOffice app can't write
  (for example XLSX from a text document) and lists the formats it can; when
  LibreOffice still fails, the error shows only its `Error` lines.
- Compare Text's whitespace option now applies at word and character
  granularity, not only per line.
- Failed engine runs show the engine's reason (the first failure line of its
  output) instead of "Error: external" in the GUI.
- Compress Audio keeps M4A levels distinct on low-rate audio by scaling them
  under the AAC encoder's ceiling (with a warning), fixes the Voice label and
  caps MP3 at 320 kbit/s.
- Number fields keep typed decimals and a minus sign on WebKitGTK (Linux
  desktop), and reject non-decimal text such as `0x3`.
- Live system dashboards mark the last reading out of date when a setting is
  invalid, instead of staying on Loading.
- Same-named tools have distinct titles (Reduce Image Noise / Reduce Audio
  Noise, Extract Audio / Extract Audio from Video), and tool search shows
  categories.
- Multi-file results have Save all on the desktop, and archive extraction
  keeps subfolders on disk and in the server's `artifacts.zip`.
- Settings that rarely change the output explain when they matter; a lone
  `fps_from` or `fps_to` in Shift Subtitles is invalid input; behaviour that
  changed silently (format-changing compress, subtitle file over `stream`)
  now warns.
- TSV and NDJSON results save as `.tsv` / `.ndjson`, audio bit depth is read
  from the output, color names use CIELAB distance, and default subtitle
  names no longer repeat the language code.
- Live system dashboards stop animating between polls (much lower CPU on
  WebKitGTK), run one poll at a time, and Pause stops polling.
- Option buttons wrap instead of being clipped, format dropdowns support
  type-ahead, and the ICC drop zone ignores clicks that started elsewhere.
- Text fields no longer truncate silently at their length limit; the field
  shows the error instead.
- Watermark Video clamps the margin so the watermark stays on the frame.
- TIFF, TGA, QOI and PPM outputs preview in the GUI, ASS and QOI get proper
  MIME types, and the audio preview explains files it can't play.
- QR Code Generator and Styled QR Code check the content against the chosen
  error correction level's capacity before Run.
- Extract Frames in scene mode reports "no frames matched" for JPEG/WebP
  instead of an FFmpeg encoder error.
- Video to GIF accepts 2 colours, the field's minimum, instead of failing in
  FFmpeg's palette generator.

## [0.1.0-alpha.1] - 2026-11-02

The first public alpha. Full notes: [docs/releases/v0.1.0-alpha.1.md](docs/releases/v0.1.0-alpha.1.md).

### Added

- 249 tools in 12 categories: images, video, audio, speech, PDF and documents,
  files, text, developer, data, network, design and system.
- One engine behind five ways in: the desktop app, a self-hosted web service,
  the `dogubako` CLI, an HTTP API with a generated OpenAPI document, and an MCP
  server for AI agents (stdio and HTTP).
- Desktop app for macOS on Apple silicon (macOS 15+), built with Tauri 2.
- App-managed engines on macOS: FFmpeg, qpdf, Poppler, ExifTool and 7-Zip ship
  inside the app; ImageMagick, Tesseract, Ghostscript, Pandoc, whisper.cpp and
  Real-ESRGAN download into the app's own folder on first use, at pinned,
  checksum-verified versions.
- Homelab Docker image with every engine installed from a pinned Debian 13
  snapshot.
- Web UI redesign ("Paper with frosted panels"): a home bench for recent files,
  a drop screen that suggests the likeliest tools, ⌘K search, light and dark
  themes, and phone layouts.
- Plain-words mode (opt-in, off by default): describe what you want in the
  search box and a model you choose (OpenAI, Anthropic, OpenRouter, Ollama,
  LM Studio or any compatible endpoint) runs or opens the right tools. Steps
  that use the network, write to folders or change files ask first.
- Self-hosting: accounts, per-agent scoped tokens, audit log, SSRF-guarded
  network tools, optional bubblewrap sandboxing, Tailscale and systemd examples.
- Documentation site (docs.dogubako.app), website (dogubako.app) and an alpha
  trailer.
- `THIRD_PARTY_NOTICES.md`, generated by `packaging/release/notices.py`.

[Unreleased]: https://github.com/Mischa-dev/Dogubako/compare/v0.1.0-alpha.2...HEAD
[0.1.0-alpha.2]: https://github.com/Mischa-dev/Dogubako/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[0.1.0-alpha.1]: https://github.com/Mischa-dev/Dogubako/releases/tag/v0.1.0-alpha.1
