# Dependencies and licensing

Dogubako is licensed under the **GNU Affero General Public License v3.0 or
later** (see [`LICENSE`](../LICENSE)). AGPL was chosen so that the hosted,
multi-user service remains free software for its users: anyone offering a
modified Dogubako over a network must provide its source. It is also compatible
with the GPL-licensed engines Dogubako integrates (Ghostscript is AGPL,
Poppler and some FFmpeg builds are GPL).

## Rust crates

All 765 third-party crates in `Cargo.lock` carry OSI-approved licenses
compatible with AGPL-3.0: predominantly MIT and/or Apache-2.0, plus BSD-2/3,
ISC, Zlib, Unicode-3.0, MPL-2.0, BSL-1.0, CC0, Unlicense and CDLA-Permissive-2.0
(Mozilla root certificate data via `webpki-roots`). No crate is unlicensed. To
regenerate the inventory:

```sh
cargo metadata --format-version 1 --locked | jq -r '.packages[] | "\(.name) \(.version) \(.license)"'
```

Notable components bundled into the binary:

| Component | License | Used for |
|---|---|---|
| `image`, `imageproc`, `resvg` | MIT/Apache-2.0 | Native image processing, SVG rendering |
| `oxipng` | MIT | Lossless PNG optimization |
| libwebp (via `libwebp-sys`) | BSD-3-Clause | WebP encoding |
| `rustface` (SeetaFace engine) | BSD-2-Clause | Face detection |
| `rxing`, `rqrr`, `qrcode` | Apache-2.0 / MIT | Barcodes and QR codes |
| `lopdf` | MIT | Native PDF editing |
| WebRTC VAD (via `webrtc-vad`) | BSD-3-Clause (WebRTC) / MIT | Voice activity detection |
| `calamine`, `rust_xlsxwriter` | MIT / MIT-Apache | Spreadsheets |
| `hickory-resolver`, `rustls`, `x509-parser` | MIT/Apache-2.0/ISC | DNS, TLS inspection |
| `sysinfo`, `starship-battery` | MIT / ISC | System and battery info |
| `jaq`, `jsonschema`, `serde_json_path` | MIT | Data querying/validation |
| `axum`, `tokio`, `rusqlite` (bundled SQLite: public domain) | MIT | Server |
| Tauri 2 | MIT/Apache-2.0 | Desktop shell |
| React, lucide-react | MIT / ISC | Web UI |

Bundled data files:

| File | License |
|---|---|
| `crates/tools-image/assets/DejaVuSans.ttf` | Bitstream Vera / DejaVu (permissive; `DejaVuSans-LICENSE.txt` alongside) |
| `crates/tools-text/data/eff_large_wordlist.txt` | EFF Large Wordlist, CC BY 3.0 US (attribution shown with generated passphrases) |

## External programs (not bundled with the desktop app or CLI)

Dogubako **invokes** these as separate processes when installed; it never links
them. Users install them through their package manager (`dogubako deps` shows
the commands). The Docker image installs Debian's packages, whose license terms
and source availability follow Debian's.

| Program | License | Notes |
|---|---|---|
| FFmpeg / FFprobe | LGPL-2.1+ or GPL-2.0+ (build-dependent) | Debian's build enables GPL components; combined distribution in the Docker image is under GPL-compatible terms with source available from Debian |
| ImageMagick | ImageMagick License (Apache-2.0 style) | |
| libvips | LGPL-2.1+ | |
| ExifTool | Artistic / GPL-1.0+ | |
| qpdf | Apache-2.0 | |
| Poppler utils | GPL-2.0+ | |
| Ghostscript | AGPL-3.0 | Compatible with Dogubako's AGPL |
| Tesseract | Apache-2.0 | Language data: Apache-2.0 |
| whisper.cpp | MIT | Optional; compiled into the `WITH_WHISPER=1` image |
| Piper | MIT | Voices have individual licenses (shown in the model list) |
| rembg | MIT | Downloads U²-Net models (Apache-2.0) itself |
| Real-ESRGAN ncnn | BSD-3-Clause | |
| LibreOffice | MPL-2.0 | |
| Pandoc | GPL-2.0+ | |
| 7-Zip | LGPL-2.1+ with unRAR restriction | RAR extraction code is freeware, not open source; RAR *creation* is never used |
| bubblewrap | LGPL-2.0+ | Optional sandbox |

### Known engine issues

* **FFmpeg 9 `anlmdn`:** the non-local-means audio denoiser crashes
  intermittently (SIGSEGV/SIGTRAP in 30–60 % of runs with Homebrew FFmpeg
  9.0.2 on Apple-silicon macOS, with or without SIMD). `audio.denoise` refuses
  `method: anlmdn` on FFmpeg ≥ 9 with an `unsupported` error and points to the
  FFT method (`afftdn`), which was not affected. Other FFmpeg versions run it.
* Homebrew's FFmpeg is built without libvorbis, libass (`subtitles` filter),
  drawtext and animated-WebP encoding; tools that need them report
  `unsupported` instead of failing obscurely.

## Optional models (downloaded on request, never bundled)

| Model | License | Verification |
|---|---|---|
| Whisper ggml (tiny … large-v3-turbo, tinydiarize) | MIT (OpenAI Whisper weights) | SHA-1 published by whisper.cpp |
| Silero VAD (ggml) | MIT | No upstream checksum; local SHA-256 recorded |
| SeetaFace frontal detector | BSD-2-Clause | SHA-256 pinned |
| Piper voices | Per voice (CC0 / CC BY / CC BY-SA …) | No upstream checksum; local SHA-256 recorded |

## Trademarks

Dogubako does not use proprietary branding, source code or visual assets from
other products. Its logo is original.
