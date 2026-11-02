# Capability matrix

Generated from the tool registry by `dogubako tools matrix` (Dogubako 0.1.0). 249 tools.

Legend — Engine: **native** = built into Dogubako (no external programs), **external** = invokes the listed program(s), **model** = needs a downloadable local model, **network** = makes outbound requests. Platforms: L = Linux, M = macOS, W = Windows.

| Category | Tools | Native | External | Model | Network |
|---|---:|---:|---:|---:|---:|
| Images | 30 | 24 | 4 | 2 | 0 |
| Video | 27 | 0 | 27 | 0 | 0 |
| Audio | 21 | 0 | 21 | 0 | 0 |
| Speech & Voice | 9 | 2 | 3 | 4 | 0 |
| PDF & Documents | 27 | 5 | 22 | 0 | 0 |
| Files | 23 | 23 | 0 | 0 | 0 |
| Text | 27 | 27 | 0 | 0 | 0 |
| Developer | 34 | 34 | 0 | 0 | 0 |
| Data | 12 | 12 | 0 | 0 | 0 |
| Network & Web | 18 | 8 | 0 | 0 | 10 |
| Design | 9 | 9 | 0 | 0 | 0 |
| System | 12 | 12 | 0 | 0 | 0 |

## Images

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Convert Images | `image.convert` | native | — | L M W | — |
| Resize Images | `image.resize` | native | — | L M W | — |
| Create Thumbnails | `image.thumbnail` | native | — | L M W | — |
| Compress Images | `image.compress` | native | — | L M W | — |
| Image Info | `image.info` | native | — | L M W | — |
| Convert Color Space | `image.colorspace` | external | imagemagick | L M W | — |
| Extract Animation Frames | `image.gif_frames` | native | — | L M W | — |
| Generate Icons | `image.icons` | native | — | L M W | — |
| Favicon Generator | `image.favicon` | native | — | L M W | — |
| Crop Image | `image.crop` | native | — | L M W | — |
| Rotate & Flip | `image.rotate_flip` | native | — | L M W | — |
| Adjust & Filter | `image.adjust` | native | — | L M W | — |
| Watermark Images | `image.watermark` | native | — | L M W | — |
| Combine Images | `image.combine` | native | — | L M W | — |
| Split Image into Tiles | `image.split` | native | — | L M W | — |
| Compare Images | `image.compare` | native | — | L M W | — |
| Contact Sheet | `image.contact_sheet` | native | — | L M W | — |
| Sprite Sheet | `image.sprite_sheet` | native | — | L M W | — |
| Redact Regions | `image.redact` | native | — | L M W | — |
| Reduce Noise | `image.denoise` | native | — | L M W | — |
| View Image Metadata | `image.metadata` | native | — | L M W | — |
| Remove Image Metadata | `image.strip_metadata` | native | — | L M W | — |
| SVG to PNG/JPEG/WebP | `image.svg_to_raster` | native | — | L M W | — |
| Optimize SVG | `image.svg_optimize` | native | — | L M W | — |
| Detect Faces | `image.face_detect` | model | model: face-detection | L M W | — |
| Blur Faces | `image.face_blur` | model | model: face-detection | L M W | — |
| Extract Text (OCR) | `image.ocr` | external | tesseract | L M W | — |
| Remove Background | `image.remove_background` | external | rembg | L M W | — |
| AI Upscale | `image.upscale` | external | realesrgan | L M W | — |
| Dominant Colors | `image.dominant_colors` | native | — | L M W | — |

## Video

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Convert Video | `video.convert` | external | ffmpeg, ffprobe | L M W | — |
| Compress Video | `video.compress` | external | ffmpeg, ffprobe | L M W | — |
| Resize Video | `video.resize` | external | ffmpeg, ffprobe | L M W | — |
| Change Frame Rate | `video.fps` | external | ffmpeg, ffprobe | L M W | — |
| Trim Video | `video.trim` | external | ffmpeg, ffprobe | L M W | — |
| Cut / Split Video | `video.cut` | external | ffmpeg, ffprobe | L M W | — |
| Crop Video | `video.crop` | external | ffmpeg, ffprobe | L M W | — |
| Rotate / Flip Video | `video.rotate` | external | ffmpeg, ffprobe | L M W | — |
| Merge Videos | `video.merge` | external | ffmpeg, ffprobe | L M W | — |
| Extract Audio from Video | `video.extract_audio` | external | ffmpeg, ffprobe | L M W | — |
| Remove Audio | `video.remove_audio` | external | ffmpeg, ffprobe | L M W | — |
| Replace Audio | `video.replace_audio` | external | ffmpeg, ffprobe | L M W | — |
| Extract Frames | `video.extract_frames` | external | ffmpeg, ffprobe | L M W | — |
| Video Thumbnail | `video.thumbnail` | external | ffmpeg, ffprobe | L M W | — |
| Video to GIF | `video.to_gif` | external | ffmpeg, ffprobe | L M W | — |
| GIF to Video | `video.from_gif` | external | ffmpeg, ffprobe | L M W | — |
| Change Video Speed | `video.speed` | external | ffmpeg, ffprobe | L M W | — |
| Stabilize Video | `video.stabilize` | external | ffmpeg, ffprobe | L M W | — |
| Video Info | `video.info` | external | ffmpeg, ffprobe | L M W | — |
| Video Preview Clip | `video.preview` | external | ffmpeg, ffprobe | L M W | — |
| Hardware Encoders | `video.hw_encoders` | external | ffmpeg | L M W | — |
| Watermark Video | `video.watermark` | external | ffmpeg, ffprobe | L M W | — |
| Video Contact Sheet | `video.contact_sheet` | external | ffmpeg, ffprobe | L M W | — |
| Add Subtitles | `video.subtitles_add` | external | ffmpeg, ffprobe | L M W | — |
| Remove Subtitles | `video.subtitles_remove` | external | ffmpeg, ffprobe | L M W | — |
| Burn In Subtitles | `video.subtitles_burn` | external | ffmpeg, ffprobe | L M W | — |
| Extract Subtitles | `video.subtitles_extract` | external | ffmpeg, ffprobe | L M W | — |

## Audio

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Convert Audio | `audio.convert` | external | ffmpeg, ffprobe | L M W | — |
| Compress Audio | `audio.compress` | external | ffmpeg, ffprobe | L M W | — |
| Trim Audio | `audio.trim` | external | ffmpeg, ffprobe | L M W | — |
| Split Audio | `audio.split` | external | ffmpeg, ffprobe | L M W | — |
| Merge Audio | `audio.merge` | external | ffmpeg, ffprobe | L M W | — |
| Normalize Audio | `audio.normalize` | external | ffmpeg, ffprobe | L M W | — |
| Change Volume | `audio.volume` | external | ffmpeg, ffprobe | L M W | — |
| Change Sample Rate | `audio.sample_rate` | external | ffmpeg, ffprobe | L M W | — |
| Audio Channels | `audio.channels` | external | ffmpeg, ffprobe | L M W | — |
| Remove Silence | `audio.remove_silence` | external | ffmpeg, ffprobe | L M W | — |
| Detect Silence | `audio.detect_silence` | external | ffmpeg, ffprobe | L M W | — |
| Audio Waveform | `audio.waveform` | external | ffmpeg, ffprobe | L M W | — |
| Audio Spectrogram | `audio.spectrogram` | external | ffmpeg, ffprobe | L M W | — |
| Audio Info | `audio.info` | external | ffmpeg, ffprobe | L M W | — |
| Audio Tags | `audio.tags` | external | ffmpeg, ffprobe | L M W | — |
| Reduce Noise | `audio.denoise` | external | ffmpeg, ffprobe | L M W | — |
| Change Audio Speed / Pitch | `audio.speed` | external | ffmpeg, ffprobe | L M W | — |
| Equalizer | `audio.equalizer` | external | ffmpeg, ffprobe | L M W | — |
| Fade In / Out | `audio.fade` | external | ffmpeg, ffprobe | L M W | — |
| Reverse Audio | `audio.reverse` | external | ffmpeg, ffprobe | L M W | — |
| Extract Audio from Video | `audio.extract_from_video` | external | ffmpeg, ffprobe | L M W | — |

## Speech & Voice

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Convert Transcript / Subtitles | `speech.transcript_convert` | native | — | L M W | — |
| Shift Subtitle Timing | `speech.subtitle_shift` | native | — | L M W | — |
| Transcribe Audio / Video | `speech.transcribe` | model | whisper, ffmpeg, ffprobe, model: whisper | L M W | — |
| Detect Spoken Language | `speech.detect_language` | model | whisper, ffmpeg, ffprobe, model: whisper | L M W | — |
| Generate Subtitles | `speech.subtitles` | model | whisper, ffmpeg, ffprobe, model: whisper | L M W | — |
| Text to Speech | `speech.tts` | model | piper, model: piper | L M W | — |
| Voice Activity Detection | `speech.vad` | external | ffmpeg, ffprobe | L M W | — |
| Extract Speech Segments | `speech.extract_segments` | external | ffmpeg, ffprobe | L M W | — |
| Split at Pauses | `speech.split_by_speech` | external | ffmpeg, ffprobe | L M W | — |

## PDF & Documents

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Merge PDFs | `pdf.merge` | external | qpdf | L M W | — |
| Split PDF | `pdf.split` | external | qpdf | L M W | — |
| Extract Pages | `pdf.extract_pages` | external | qpdf | L M W | — |
| Reorder Pages | `pdf.reorder` | external | qpdf | L M W | — |
| Rotate Pages | `pdf.rotate` | external | qpdf | L M W | — |
| Delete Pages | `pdf.delete_pages` | external | qpdf | L M W | — |
| Repair PDF | `pdf.repair` | external | qpdf | L M W | — |
| Optimize for Web | `pdf.linearize` | external | qpdf | L M W | — |
| Flatten Forms & Annotations | `pdf.flatten` | external | qpdf | L M W | — |
| Protect PDF | `pdf.protect` | external | qpdf | L M W | — |
| Remove PDF Password | `pdf.unlock` | external | qpdf | L M W | — |
| Compress PDF | `pdf.compress` | external | qpdf, ghostscript | L M W | — |
| Extract Text | `pdf.extract_text` | external | qpdf, poppler | L M W | — |
| Extract Images | `pdf.extract_images` | external | qpdf, poppler | L M W | — |
| PDF to Images | `pdf.to_images` | external | qpdf, poppler | L M W | — |
| PDF Info | `pdf.info` | external | qpdf, poppler | L M W | — |
| Compare PDF Text | `pdf.compare_text` | external | qpdf, poppler | L M W | — |
| Images to PDF | `pdf.from_images` | native | — | L M W | — |
| OCR PDF | `pdf.ocr` | external | qpdf, poppler, tesseract | L M W | — |
| Watermark PDF | `pdf.watermark` | native | — | L M W | — |
| Add Page Numbers | `pdf.page_numbers` | native | — | L M W | — |
| Edit PDF Metadata | `pdf.metadata_edit` | native | — | L M W | — |
| List Form Fields | `pdf.form_fields` | native | — | L M W | — |
| Fill PDF Form | `pdf.form_fill` | external | qpdf | L M W | — |
| Convert Office Documents | `pdf.convert_document` | external | libreoffice | L M W | — |
| Convert Markup Documents | `pdf.convert_markup` | external | pandoc | L M W | — |
| Markdown to PDF | `pdf.markup_to_pdf` | external | pandoc, libreoffice | L M W | — |

## Files

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| File Hash | `files.hash` | native | — | L M W | — |
| Verify Checksums | `files.checksum_verify` | native | — | L M W | — |
| Checksum Manifest | `files.checksum_manifest` | native | — | L M W | host_fs_read |
| Detect File Type | `files.type_detect` | native | — | L M W | — |
| Executable Info | `files.binary_info` | native | — | L M W | — |
| Hex Viewer | `files.hexdump` | native | — | L M W | — |
| MIME Type Lookup | `files.mime_types` | native | — | L M W | — |
| Compare Files | `files.compare` | native | — | L M W | — |
| Compare Folders | `files.compare_dirs` | native | — | L M W | host_fs_read |
| List Archive Contents | `files.archive_list` | native | — | L M W | — |
| Extract Archive | `files.archive_extract` | native | — | L M W | — |
| Create Archive | `files.archive_create` | native | — | L M W | — |
| Compress File | `files.compress` | native | — | L M W | — |
| Decompress File | `files.decompress` | native | — | L M W | — |
| Folder Size | `files.dir_size` | native | — | L M W | host_fs_read |
| Find Large Files | `files.large_files` | native | — | L M W | host_fs_read |
| Find Duplicate Files | `files.duplicates` | native | — | L M W | host_fs_read |
| Find Duplicates Among Files | `files.duplicates_uploaded` | native | — | L M W | — |
| Find Empty Files & Folders | `files.empty` | native | — | L M W | host_fs_read |
| Remove Empty Files & Folders | `files.remove_empty` | native | — | L M W | host_fs_read, host_fs_write |
| Search Files | `files.search` | native | — | L M W | host_fs_read |
| File Manifest | `files.manifest` | native | — | L M W | host_fs_read |
| Batch Rename | `files.batch_rename` | native | — | L M W | host_fs_read, host_fs_write |

## Text

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Change Case | `text.case` | native | — | L M W | — |
| Word & Character Count | `text.count` | native | — | L M W | — |
| Sort Lines | `text.sort_lines` | native | — | L M W | — |
| Remove Duplicate Lines | `text.dedupe_lines` | native | — | L M W | — |
| Trim & Remove Whitespace | `text.trim` | native | — | L M W | — |
| Line Endings | `text.line_endings` | native | — | L M W | — |
| Find & Replace | `text.replace` | native | — | L M W | — |
| Regex Replace | `text.regex_replace` | native | — | L M W | — |
| Compare Text (Diff) | `text.diff` | native | — | L M W | — |
| Escape / Unescape | `text.escape` | native | — | L M W | — |
| Convert Text Encoding | `text.encoding` | native | — | L M W | — |
| Markdown Preview | `text.markdown_preview` | native | — | L M W | — |
| Markdown to HTML | `text.markdown_to_html` | native | — | L M W | — |
| HTML to Plain Text | `text.html_to_text` | native | — | L M W | — |
| HTML to Markdown | `text.html_to_markdown` | native | — | L M W | — |
| Clean Up Text | `text.cleanup` | native | — | L M W | — |
| Slugify | `text.slugify` | native | — | L M W | — |
| Reverse Text | `text.reverse` | native | — | L M W | — |
| Lorem Ipsum Generator | `text.lorem` | native | — | L M W | — |
| Wrap Text | `text.wrap` | native | — | L M W | — |
| Extract Columns | `text.column_extract` | native | — | L M W | — |
| Number Lines | `text.number_lines` | native | — | L M W | — |
| Add Prefix / Suffix | `text.prefix_suffix` | native | — | L M W | — |
| Filter Lines | `text.filter_lines` | native | — | L M W | — |
| Unicode Inspector | `text.unicode_inspect` | native | — | L M W | — |
| Word Frequency | `text.word_frequency` | native | — | L M W | — |
| Extract Text from Document | `text.extract` | native | — | L M W | — |

## Developer

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| QR Code Generator | `dev.qr_generate` | native | — | L M W | — |
| QR Code Reader | `dev.qr_read` | native | — | L M W | — |
| Barcode Generator | `dev.barcode_generate` | native | — | L M W | — |
| Barcode Reader | `dev.barcode_read` | native | — | L M W | — |
| JSON Formatter & Validator | `dev.json` | native | — | L M W | — |
| YAML Formatter & Validator | `dev.yaml` | native | — | L M W | — |
| TOML Formatter & Validator | `dev.toml` | native | — | L M W | — |
| XML Formatter & Validator | `dev.xml` | native | — | L M W | — |
| SQL Formatter | `dev.sql_format` | native | — | L M W | — |
| Convert Data Format | `dev.convert` | native | — | L M W | — |
| Base64 Encode / Decode | `dev.base64` | native | — | L M W | — |
| URL Encode / Decode | `dev.url_encode` | native | — | L M W | — |
| HTML Entities Encode / Decode | `dev.html_entities` | native | — | L M W | — |
| Hex Encode / Decode | `dev.hex` | native | — | L M W | — |
| Number Base Converter | `dev.number_base` | native | — | L M W | — |
| Text ↔ Binary | `dev.binary_text` | native | — | L M W | — |
| Hash Generator | `dev.hash` | native | — | L M W | — |
| HMAC Generator & Verifier | `dev.hmac` | native | — | L M W | — |
| Password & Random Generator | `dev.random` | native | — | L M W | — |
| UUID Generator & Decoder | `dev.uuid` | native | — | L M W | — |
| JWT Decoder & Verifier | `dev.jwt` | native | — | L M W | — |
| Unix Timestamp Converter | `dev.timestamp` | native | — | L M W | — |
| Date Calculator & Formatter | `dev.datetime` | native | — | L M W | — |
| Time Zone Converter | `dev.timezone` | native | — | L M W | — |
| Cron Expression Explainer | `dev.cron` | native | — | L M W | — |
| Regex Tester | `dev.regex` | native | — | L M W | — |
| JSONPath Query | `dev.jsonpath` | native | — | L M W | — |
| jq Filter | `dev.jq` | native | — | L M W | — |
| JSON Schema Validator | `dev.json_schema_validate` | native | — | L M W | — |
| JSON Diff | `dev.diff_json` | native | — | L M W | — |
| Color Converter | `dev.color` | native | — | L M W | — |
| HTTP Message Parser | `dev.http_request` | native | — | L M W | — |
| User-Agent Parser | `dev.user_agent` | native | — | L M W | — |
| String Inspector | `dev.string_inspect` | native | — | L M W | — |

## Data

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| CSV Viewer | `data.csv_view` | native | — | L M W | — |
| Filter & Sort CSV | `data.csv_filter_sort` | native | — | L M W | — |
| CSV to JSON | `data.csv_to_json` | native | — | L M W | — |
| JSON to CSV | `data.json_to_csv` | native | — | L M W | — |
| Spreadsheet Viewer & Export | `data.spreadsheet` | native | — | L M W | — |
| Change CSV Delimiter | `data.delimiter` | native | — | L M W | — |
| Transform Columns | `data.columns` | native | — | L M W | — |
| Validate CSV / JSON Data | `data.validate` | native | — | L M W | — |
| Find Duplicate Rows | `data.duplicates` | native | — | L M W | — |
| Column Statistics | `data.stats` | native | — | L M W | — |
| Clean CSV Data | `data.clean` | native | — | L M W | — |
| Convert Table Data | `data.convert` | native | — | L M W | — |

## Network & Web

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| URL Parser | `network.url_parse` | native | — | L M W | — |
| URL Normalizer | `network.url_normalize` | native | — | L M W | — |
| Query Parameter Editor | `network.query_params` | native | — | L M W | — |
| IP Address Info | `network.ip_info` | native | — | L M W | — |
| CIDR Calculator | `network.cidr` | native | — | L M W | — |
| Subnet Planner (VLSM) | `network.subnet_plan` | native | — | L M W | — |
| Certificate Decoder | `network.cert_decode` | native | — | L M W | — |
| Key Inspector | `network.key_inspect` | native | — | L M W | — |
| DNS Lookup | `network.dns_lookup` | network | — | L M W | network |
| Reverse DNS | `network.reverse_dns` | network | — | L M W | network |
| TLS / SSL Inspector | `network.tls_inspect` | network | — | L M W | network |
| HTTP Headers & Security Check | `network.http_headers` | network | — | L M W | network |
| Favicon Fetcher | `network.favicon` | network | — | L M W | network |
| Connectivity Check | `network.connectivity` | network | — | L M W | network |
| Port Checker | `network.port_check` | network | — | L M W | network |
| RDAP Lookup | `network.rdap` | network | — | L M W | network |
| WHOIS Lookup | `network.whois` | network | — | L M W | network |
| Domain Inspector | `network.domain_inspect` | network | — | L M W | network |

## Design

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| Color Palette Generator | `design.palette` | native | — | L M W | — |
| Extract Palette from Image | `design.palette_extract` | native | — | L M W | — |
| Contrast Checker | `design.contrast` | native | — | L M W | — |
| Gradient Generator | `design.gradient` | native | — | L M W | — |
| Aspect Ratio Calculator | `design.aspect_ratio` | native | — | L M W | — |
| Resize Canvas / Pad | `design.canvas_resize` | native | — | L M W | — |
| Overlay Images | `design.compose` | native | — | L M W | — |
| Bulk Image Dimensions | `design.image_dimensions` | native | — | L M W | — |
| Styled QR Code | `design.qr_style` | native | — | L M W | — |

## System

| Tool | Id | Engine | Requires | Platforms | Permissions |
|---|---|---|---|---|---|
| System Overview | `system.overview` | native | — | L M W | system_info |
| CPU Info | `system.cpu` | native | — | L M W | system_info |
| Memory Usage | `system.memory` | native | — | L M W | system_info |
| Disks & Storage | `system.disks` | native | — | L M W | system_info |
| Hardware Info | `system.hardware` | native | — | L M W | system_info |
| Operating System | `system.os` | native | — | L M W | system_info |
| Network Interfaces | `system.network_interfaces` | native | — | L M W | system_info |
| Top Processes | `system.processes` | native | — | L M W | process_info |
| Uptime | `system.uptime` | native | — | L M W | system_info |
| Temperature Sensors | `system.temperatures` | native | — | L M W | system_info |
| Environment Diagnostics | `system.diagnostics` | native | — | L M W | system_info |
| Battery Health | `system.battery` | native | — | L M W | system_info |
