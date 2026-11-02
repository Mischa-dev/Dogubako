# Engines: what the app ships, downloads and pins

Dogubako drives mature external programs (FFmpeg, qpdf, Poppler, …). Instead
of asking people to install them, the desktop app manages them itself:

* **Bundled** engines ship inside `Dogubako.app` and work right away.
* **On-demand** engines are downloaded the first time a tool needs them, with
  one click in the tool's error or in **Settings → Dependencies**. They go into
  the app's own folder (`~/Library/Application Support/Dogubako/engines`), not
  system-wide.
* Every engine is pinned: exact version, source, SHA-256. The app only uses
  its own copies of the engines it manages, so what runs is what was tested.
  An app update that bumps an engine downloads the new pinned version and
  deletes the old one.

The homelab Docker image bakes the same engines in at pinned versions (see
[Homelab](#homelab)).

## Tiers

Sizes are real numbers from the current lock (macOS, Apple silicon).

| Engine | Tier | Version | Download | On disk | Why |
|---|---|---|---:|---:|---|
| FFmpeg + FFprobe | bundled | 9.0.2 | | | 38 video/audio tools |
| qpdf | bundled | 12.4.2 | | | 25 PDF tools, small |
| Poppler (pdftoppm, pdftotext, …) | bundled | 26.10.0 | | | 15 PDF/text tools |
| ExifTool | bundled | 13.55 | | | 11 metadata tools, tiny |
| 7-Zip | bundled | 26.05 | | | 5 archive tools, one small binary |
| **Bundled total** | | | **36 MB** (in the DMG) | **97 MB** | |
| ImageMagick | on-demand | 7.1.2-35 | 9 MB | 29 MB | HEIC/PSD/exotic formats only; native code covers the rest |
| Tesseract (eng + osd) | on-demand | 5.5.4 | 10 MB | 23 MB | 8 OCR tools |
| Ghostscript | on-demand | 10.08.0 | 20 MB | 46 MB | PDF compress, PostScript |
| Real-ESRGAN (+ models) | on-demand | 0.2.5.0 | 51 MB | 73 MB | 1 upscaling tool |
| Pandoc | on-demand | 3.13 | 63 MB | 281 MB | 6 document tools; one huge binary |
| whisper.cpp | on-demand | v1.7.6 | ~3 MB | ~5 MB | Speech tools need a model download first anyway |
| LibreOffice | system | | | ~700 MB | Too big for now; detected if installed |
| Piper | system | | | | Last native release's macOS build is broken (below) |
| rembg | system | | | | Python app, not relocatable; better replaced by an in-process model |
| libvips | dropped | | | | No tool uses it |

Everything Dogubako implements natively (most image, text, data, network and
developer tools) needs no engine at all.

## How it works

```
packaging/engines/
  engines.toml          the source of truth: tier, source, license, Debian package per engine
  engines.lock.json     generated: every Homebrew bottle in each engine's closure, pinned by SHA-256
  lock.py               (re)generates the lock; --check fails CI if it's stale
  fetch.py              downloads the locked sources, verifies SHA-256, unpacks to build/engines/stage
  build-from-source.sh  engines without a usable prebuilt binary (whisper.cpp)
  assemble.py           makes relocatable folders: bundled + one tar.gz pack per on-demand engine
  smoke.sh              runs every engine on a Mac with a clean environment and Homebrew hidden
  docker-lock.py        pins the homelab image's Debian packages (docker-packages*.txt)
```

**Sources.** macOS engines come from Homebrew's prebuilt bottles. Bottles are
content-addressed on ghcr.io, so a pinned SHA-256 stays downloadable after
Homebrew moves on. Real-ESRGAN comes from its upstream release; whisper.cpp is
built from a pinned tag.

**Relocation.** `assemble.py` copies each engine's binaries, follows their
linked libraries (`otool -L`) and copies only those into one `lib/`, rewrites
every Homebrew install name to `@rpath/…` with an `@loader_path`-relative
rpath, and ad-hoc signs each changed file (Apple silicon refuses to run
unsigned code; this is not Developer ID signing). Data files go to `share/`;
the variables an engine needs to find them (`TESSDATA_PREFIX`, `GS_LIB`,
`MAGICK_*`) are recorded in the manifest and set by the app for that engine's
processes only. The script refuses to finish if any Homebrew path survives in
a load command.

**At runtime** (`crates/core/src/engines.rs`). The desktop app reads
`Contents/Resources/engines/manifest.json`. For a dependency the manifest
covers, the app uses only its bundled copy or its downloaded pack and never
looks on PATH or in Homebrew. A missing on-demand engine reports
`missing_dependency` with "Download … in Settings → Dependencies", and the
Dependencies page shows a **Download (size)** button. Downloads are verified
against the SHA-256 in the manifest, unpacked with path and link checks, and
stored per version. Dependencies the manifest doesn't cover (LibreOffice,
Piper, rembg) are still found on the system as before.

`DOGUBAKO_ENGINES=system` turns managed engines off (handy for testing a newer
engine), `prefer` uses managed copies first and then the system for
everything. Without a manifest (CLI, server, development builds) nothing
changes.

## Building the engines

On a Mac (Apple silicon), from the repo root:

```sh
python3 -I packaging/engines/fetch.py
packaging/engines/build-from-source.sh whisper
python3 -I packaging/engines/assemble.py
packaging/engines/smoke.sh apps/desktop/src-tauri/engines build/engines/packs/*/
cd apps/desktop/src-tauri && cargo tauri build --target aarch64-apple-darwin --config tauri.engines.conf.json
```

`tauri.engines.conf.json` adds the engines folder to the bundle and sets the
minimum macOS to 15.0. The on-demand packs (`build/engines/packs/*.tar.gz`)
are uploaded to a GitHub release named `engines-<lock id>`; the manifest
already points there. `DOGUBAKO_ENGINE_PACK_BASE_URL` overrides that for a
test server.

The [Engines workflow](../.github/workflows/engines.yml) does all of this on a
macOS runner whenever `packaging/engines/` changes and keeps the result as a
build artifact. The [release workflow](../.github/workflows/release.yml) does
it for tagged releases: Apple-silicon DMG with the bundled engines, packs
uploaded to the same (draft) release, Developer ID signing and notarization
when the `APPLE_*` secrets are set (`DOGUBAKO_ENGINE_SIGN_CMD` re-signs every
engine before packing) and an unsigned DMG when they aren't.

`assemble.py --bundle-all` puts the on-demand engines in the app too, for test
builds that should work without downloads; the
[alpha test workflow](../.github/workflows/alpha-test.yml) uses it (see
[ALPHA_TESTING.md](ALPHA_TESTING.md)).

Every engine folder and pack carries `licenses/<component>/` with the license
texts of each component shipped in it (the engine and every library it
links). Texts a package doesn't include are kept in
`packaging/engines/licenses/`; anything still missing gets a
`LICENSE-INFO.txt` with the SPDX id and source link, and assemble.py warns.

`fetch.py` and `assemble.py` also run on Linux (with `llvm-otool` and
`llvm-install-name-tool`), which is useful for checking the tree, but the
output is unsigned there and only runs after signing on a Mac.

## Updating an engine

1. `python3 -I packaging/engines/lock.py` re-pins every managed engine to
   Homebrew's current bottles (edit `engines.toml` first to change tiers or
   sources).
2. Run the build steps above, then the app's tool tests against the new
   engines.
3. Commit `engines.lock.json`. The next app release ships the new bundled
   engines, and its manifest points at new packs, which each app downloads
   when needed.

## Homelab

The Docker image bakes **every** engine in, installed from a fixed Debian
snapshot at exact versions (`docker-packages.txt`), plus whisper.cpp (pinned
tag) and Piper (checksum-pinned release). LibreOffice is opt-in
(`DOGUBAKO_WITH_OFFICE=1`, about 500 MB). The same engines as the desktop, from
Debian's builds.

Why not the on-demand flow on a server:

* The compose file runs the container read-only with `no-new-privileges`.
  Downloading executables into `/data` and running them would undo that.
* A server should work offline and come back identical after a rebuild. The
  image tag is the engine set.
* Size matters much less on a server than in a download people wait for.
* The desktop packs are macOS binaries; Debian already builds, patches and
  ships these engines for Linux.

Bump the snapshot in `engines.toml`, run `docker-lock.py`, rebuild, check
`docker run --rm dogubako deps`, commit. Pinning trades automatic security
updates for reproducibility, so bump it regularly (monthly is a good default).

Plain-words mode needs no engine. Its server settings
(`DOGUBAKO_ASSISTANT_BASE_URL`, `_MODEL`, `_API_KEY`, `_ALLOW_PRIVATE`) are in
`docker-compose.yml` and read from `.env` (see `.env.example`); the compose file
also has an optional Ollama service (`--profile local-model`).

Versions differ between platforms (for example FFmpeg 9.0 on macOS, 7.1 in
Debian 13). Tools already handle version differences (see "Known engine
issues" in [DEPENDENCIES.md](DEPENDENCIES.md)); test both when bumping.

## Licenses

All managed engines allow redistribution alongside an AGPL app. They run as
separate programs, so bundling them is aggregation, not linking. Notes:

* **GPL engines** (FFmpeg as built by Homebrew, Poppler, Pandoc, Piper via
  espeak-ng) and **AGPL Ghostscript**: compatible with Dogubako's AGPL. Each
  release must offer the corresponding source; `source_url` per engine in
  `engines.toml` lists where. Mirroring the exact source tarballs next to the
  engine packs is the safest way to satisfy this.
* **7-Zip**: LGPL plus the unRAR restriction (the RAR code is freeware: it may
  not be used to recreate a RAR compressor). We only extract, and shipping it
  as a separate program is fine.
* **ImageMagick** (Apache-2.0 style), **qpdf / Tesseract** (Apache-2.0),
  **ExifTool** (Perl terms), **Real-ESRGAN** (BSD-3), **whisper.cpp** (MIT):
  no constraints beyond keeping notices.

No engine is blocked by its license. The blockers are technical (below).

## Open issues

* **Intel Macs.** Homebrew no longer publishes Intel bottles, so managed
  engines are Apple silicon only. Build the engines DMG for
  `aarch64-apple-darwin`; a universal build would carry arm64-only engines.
  Intel would need engines built from source or the system-engine fallback.
* **macOS 15+.** The oldest bottles available are for macOS 15 (Sequoia), so
  the engines DMG requires macOS 15.
* **Piper.** The last native release's macOS archive is missing its
  libraries, so it can't run. Needs our own build or a different TTS engine.
* **LibreOffice.** Could become on-demand by unpacking the official DMG into
  the engines folder; deferred because of size.
* **ExifTool** uses the `/usr/bin/perl` that macOS still ships.
* **Developer ID signing and notarization** come later. Then every bundled
  engine binary must be signed with the Developer ID and hardened runtime, and
  the packs signed the same way before upload.
