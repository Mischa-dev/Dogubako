---
title: Introduction
description: What Dogubako is, who it is for, and the ways you can use it.
sidebar:
  order: 1
---

Dogubako (道具箱, "toolbox") replaces the dozens of single-purpose utility
websites and small apps you reach for every week: the image converter, the PDF
merger, the video compressor, the JSON formatter, the subnet calculator. Open
it, find the tool, drop in a file, and you are done.

It is free and open-source software under the
[GNU AGPL v3.0 or later](/project/licensing/). There are no accounts for
desktop use, no telemetry and no paywalls.

## What it can do

Dogubako ships 249 tools in twelve categories:

| Category | Tools | Highlights |
|---|---:|---|
| [Images](/tools/image/) | 30 | Convert (HEIC, AVIF, WebP…), batch resize and compress, crop, watermark, metadata, compare, icons and favicons, face blur with manual correction, redaction, OCR, background removal, upscaling |
| [Video](/tools/video/) | 27 | Convert and compress (including to a target size and with hardware encoders), trim, cut, merge, crop, rotate, GIF ⇄ video, stabilize, frames, subtitles |
| [Audio](/tools/audio/) | 21 | Convert, EBU R128 loudness normalization, trim, split, merge, silence removal, waveforms, spectrograms, tags, denoise, EQ |
| [Speech & Voice](/tools/speech/) | 9 | Offline transcription with whisper.cpp to TXT, SRT, VTT, JSON or Markdown, language detection, speaker turns, text to speech with Piper |
| [PDF & Documents](/tools/pdf/) | 27 | Merge, split, reorder, rotate, compress, OCR to searchable PDF, extract text and images, watermark, page numbers, encrypt, forms, Office and Markdown conversion |
| [Files](/tools/files/) | 23 | Hashes and checksum checks, archives with bomb and traversal protection, duplicates, large files, folder compare, batch rename with preview |
| [Text](/tools/text/) | 27 | Case, counts, sort and dedupe, diff, regex replace, encodings, escaping, Markdown ⇄ HTML, text extraction from Office, EPUB and PDF |
| [Developer](/tools/dev/) | 34 | JSON, YAML, TOML and XML format and convert, Base64, URL and hex, UUIDs, passphrases, hashes and HMAC, timestamps, regex tester, JSONPath and jq, JWT, SQL format, cron, QR and barcodes |
| [Data](/tools/data/) | 12 | CSV view, filter, sort, stats, dedupe and clean, spreadsheets, JSON ⇄ CSV, schema validation, export to XLSX, SQL or Markdown |
| [Network & Web](/tools/network/) | 18 | DNS, RDAP and WHOIS, HTTP header and security audit, TLS certificates, URL tools, CIDR and subnet planning, connectivity |
| [Design](/tools/design/) | 9 | Palettes and palette extraction, WCAG contrast, gradients, canvas resize, QR styling |
| [System](/tools/system/) | 12 | Overview dashboard, CPU, memory, disks, battery health, hardware, processes, temperatures |

The [tool catalog](/tools/) lists every tool with its options, the engines it
needs and the platforms it runs on.

## Five ways to use it

All five share one engine and one catalog, so a tool behaves the same
everywhere.

1. **[Desktop app](/app/desktop/)**, released for macOS on Apple silicon
   (it builds on Windows and Linux too). Everything runs in-process on your
   machine. There is no server and no network port.
2. **[Self-hosted web app](/self-hosting/overview/)**. Run one server with all
   the engines installed and use it from any browser on your network,
   including phones.
3. **[Command line](/cli/overview/)**. Every tool is a `dogubako` command, for
   scripts and quick one-off jobs.
4. **[HTTP API](/reference/http-api/)**, with an OpenAPI 3.1 document
   generated from the live catalog.
5. **[MCP server](/agents/overview/)** for AI agents, either locally over stdio
   or shared over HTTP with a scoped token per agent.

## What makes it different

- **Built on mature open-source engines.** FFmpeg, ImageMagick,
  ExifTool, qpdf, Poppler, Ghostscript, Tesseract, whisper.cpp, Piper,
  LibreOffice, Pandoc and 7-Zip do the heavy lifting. About half the catalog
  needs none of them and runs as fast native Rust code. The macOS app and the
  Docker image bring the engines with them.
- **Plain words, if you want them.** Type "make these photos smaller" and a
  model you choose runs the right tools, asking before anything that touches
  the network or your folders. Off by default. See [Plain words](/app/plain-words/).
- **One implementation, every interface.** Each tool is a declarative spec and
  one function. Forms, CLI flags, API schemas and MCP tool definitions are
  generated from the spec. See [Architecture](/develop/architecture/).
- **Agent-ready, not agent-first.** A shared server can give an image bot
  permission to resize and blur, and a media bot permission to transcode and
  transcribe, without either touching the other's files or your system tools.
- **Secure by construction.** No shell is ever invoked. External programs run
  with timeouts, resource limits and optional sandboxing. Files are addressed
  by random ids and checked for ownership. Network tools refuse private
  addresses. Everything is audit-logged. See [Security](/project/security/).

## Next steps

- [Install Dogubako](/start/install/), or [try the alpha](/start/alpha/)
- [Quick start](/start/quick-start/): your first tool in each interface
- [Key concepts](/start/concepts/): tools, engines, jobs and artifacts
