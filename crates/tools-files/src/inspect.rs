//! File inspection: type detection, binaries, hex dumps, MIME lookup.

use crate::util::*;
use dogubako_core::prelude::*;
use std::io::{Read, Seek, SeekFrom};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![type_detect(), binary_info(), hexdump(), mime_types()]
}

/// Detect a MIME type from content.
pub fn detect(prefix: &[u8]) -> (String, Option<String>, &'static str) {
    if let Some(t) = infer::get(prefix) {
        let kind = match t.matcher_type() {
            infer::MatcherType::App => "application",
            infer::MatcherType::Archive => "archive",
            infer::MatcherType::Audio => "audio",
            infer::MatcherType::Book => "book",
            infer::MatcherType::Doc => "document",
            infer::MatcherType::Font => "font",
            infer::MatcherType::Image => "image",
            infer::MatcherType::Text => "text",
            infer::MatcherType::Video => "video",
            infer::MatcherType::Custom => "other",
        };
        return (
            t.mime_type().to_string(),
            Some(t.extension().to_string()),
            kind,
        );
    }
    if let Some(m) = dogubako_core::util::sniff_bytes(prefix) {
        return (m.to_string(), None, "other");
    }
    if prefix.is_empty() {
        return ("application/x-empty".into(), None, "empty");
    }
    if !looks_binary(prefix) {
        let s = String::from_utf8_lossy(prefix);
        let t = s.trim_start_matches('\u{feff}').trim_start();
        let (m, e) = if t.starts_with("<?xml") {
            if t.contains("<svg") {
                ("image/svg+xml", "svg")
            } else {
                ("application/xml", "xml")
            }
        } else if t.starts_with("<svg") {
            ("image/svg+xml", "svg")
        } else if t.to_ascii_lowercase().starts_with("<!doctype html")
            || t.to_ascii_lowercase().starts_with("<html")
        {
            ("text/html", "html")
        } else if (t.starts_with('{') || t.starts_with('['))
            && serde_json::from_str::<Value>(&s).is_ok()
        {
            ("application/json", "json")
        } else if t.starts_with("#!") {
            ("text/x-script", "sh")
        } else {
            ("text/plain", "txt")
        };
        return (m.into(), Some(e.into()), "text");
    }
    ("application/octet-stream".into(), None, "binary")
}

fn entropy(b: &[u8]) -> f64 {
    if b.is_empty() {
        return 0.0;
    }
    let mut c = [0u64; 256];
    for &x in b {
        c[x as usize] += 1;
    }
    let n = b.len() as f64;
    c.iter()
        .filter(|&&k| k > 0)
        .map(|&k| {
            let p = k as f64 / n;
            -p * p.log2()
        })
        .sum()
}

fn type_detect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.type_detect", "Detect File Type", Category::Files)
        .describe("Identify files by their content (magic bytes), show the MIME type and warn when the extension does not match")
        .keywords(&["file type", "magic", "mime", "identify", "extension", "what is this file", "signature"])
        .param(ParamSpec::files("input").label("Files").required().primary())
        .output(OutputKind::Data)
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let mut rows = vec![];
        let mut details = vec![];
        let mut warnings = vec![];
        for f in files {
            ctx.check_cancelled()?;
            let prefix = read_prefix(&f.path, 1 << 20)?;
            let (mime, det_ext, kind) = detect(&prefix);
            let ext = f.extension();
            let ext_mime = mime_guess::from_ext(&ext)
                .first_raw()
                .map(|s| s.to_string());
            let exts_for_mime: Vec<&str> = mime_guess::get_mime_extensions_str(&mime)
                .map(|v| v.to_vec())
                .unwrap_or_default();
            let generic = matches!(
                mime.as_str(),
                "application/octet-stream"
                    | "text/plain"
                    | "application/x-empty"
                    | "application/zip"
                    | "application/xml"
            );
            let matches = ext.is_empty()
                || det_ext.as_deref() == Some(ext.as_str())
                || exts_for_mime.contains(&ext.as_str())
                || ext_mime.as_deref() == Some(mime.as_str())
                || (generic && (kind == "text" || mime == "application/zip"))
                || (det_ext.as_deref() == Some("jpg") && ext == "jpeg");
            let mismatch = !matches && !generic;
            if mismatch {
                warnings.push(format!(
                    "{}: content is {} but the extension is .{}",
                    f.name, mime, ext
                ));
            }
            let size = f.size();
            let text = kind == "text";
            let mut d = json!({
                "name": f.name,
                "size": size,
                "size_human": human_size(size),
                "mime": mime,
                "kind": kind,
                "detected_extension": det_ext,
                "extension": ext,
                "extension_mime": ext_mime,
                "extension_mismatch": mismatch,
                "entropy_bits_per_byte": (entropy(&prefix) * 100.0).round() / 100.0,
            });
            if text {
                let bom = if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
                    Some("UTF-8 BOM")
                } else if prefix.starts_with(&[0xFF, 0xFE]) {
                    Some("UTF-16LE BOM")
                } else if prefix.starts_with(&[0xFE, 0xFF]) {
                    Some("UTF-16BE BOM")
                } else {
                    None
                };
                let crlf = prefix.windows(2).filter(|w| w == b"\r\n").count();
                let lf = prefix.iter().filter(|&&c| c == b'\n').count();
                d["encoding"] = json!(if std::str::from_utf8(&prefix).is_ok() {
                    "UTF-8 (or ASCII)"
                } else {
                    "8-bit / unknown"
                });
                d["bom"] = json!(bom);
                d["line_endings"] = json!(if lf == 0 {
                    "none"
                } else if crlf == lf {
                    "CRLF (Windows)"
                } else if crlf == 0 {
                    "LF (Unix)"
                } else {
                    "mixed"
                });
                if size <= 256 << 20 {
                    let mut lines = 0u64;
                    let mut r = std::fs::File::open(&f.path)?;
                    let mut buf = vec![0u8; 1 << 20];
                    loop {
                        let n = r.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        lines += buf[..n].iter().filter(|&&c| c == b'\n').count() as u64;
                    }
                    d["lines"] = json!(lines);
                }
            }
            if kind == "image" {
                if let Ok(Some(dim)) = image_dims(&prefix) {
                    d["dimensions"] = json!(dim);
                }
            }
            rows.push(json!([
                f.name,
                mime,
                kind,
                human_size(size),
                if mismatch {
                    format!("⚠ .{ext} expected .{}", det_ext.clone().unwrap_or_default())
                } else {
                    "ok".into()
                }
            ]));
            details.push(d);
        }
        let mut o = ToolOutput::data(
            json!({"table": table(&["File", "MIME type", "Kind", "Size", "Extension"], rows), "files": details}),
        );
        o.warnings = warnings;
        Ok(o)
    })
}

/// Width×height for PNG/GIF/JPEG/BMP/WebP headers without decoding.
fn image_dims(b: &[u8]) -> Result<Option<String>, ()> {
    let be32 = |i: usize| {
        b.get(i..i + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    };
    let le16 = |i: usize| {
        b.get(i..i + 2)
            .map(|s| u16::from_le_bytes([s[0], s[1]]) as u32)
    };
    let be16 = |i: usize| {
        b.get(i..i + 2)
            .map(|s| u16::from_be_bytes([s[0], s[1]]) as u32)
    };
    let le32 = |i: usize| {
        b.get(i..i + 4)
            .map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]).unsigned_abs())
    };
    let d = if b.starts_with(b"\x89PNG") {
        be32(16).zip(be32(20))
    } else if b.starts_with(b"GIF8") {
        le16(6).zip(le16(8))
    } else if b.starts_with(b"BM") {
        le32(18).zip(le32(22))
    } else if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        let mut r = None;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let m = b[i + 1];
            if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
                r = be16(i + 7).zip(be16(i + 5));
                break;
            }
            let len = be16(i + 2).unwrap_or(0) as usize;
            i += 2 + len;
        }
        r
    } else {
        None
    };
    Ok(d.map(|(w, h)| format!("{w}×{h}")))
}

fn hex_dump(data: &[u8], base: u64, width: usize) -> String {
    let mut s = String::new();
    for (i, chunk) in data.chunks(width).enumerate() {
        s.push_str(&format!("{:08x}  ", base + (i * width) as u64));
        for j in 0..width {
            match chunk.get(j) {
                Some(b) => s.push_str(&format!("{b:02x} ")),
                None => s.push_str("   "),
            }
            if j + 1 == width / 2 {
                s.push(' ');
            }
        }
        s.push_str(" |");
        for &b in chunk {
            s.push(if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            });
        }
        s.push_str("|\n");
    }
    s
}

fn hexdump() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.hexdump", "Hex Viewer", Category::Files)
        .describe("Show a hex + ASCII view of any part of a file")
        .keywords(&["hex", "hexdump", "xxd", "binary", "bytes", "viewer", "od"])
        .param(ParamSpec::file("input").label("File").required().primary())
        .param(
            ParamSpec::integer("offset")
                .label("Offset (bytes)")
                .min(0.0)
                .default_int(0),
        )
        .param(
            ParamSpec::integer("length")
                .label("Length (bytes)")
                .range(1.0, 1_048_576.0)
                .default_int(1024),
        )
        .param(
            ParamSpec::choice(
                "width",
                &[
                    ("16", "16 bytes per line"),
                    ("8", "8 bytes per line"),
                    ("32", "32 bytes per line"),
                ],
            )
            .default_str("16")
            .advanced(),
        )
        .text_output("text/plain")
        .timeout(60);
    sync_tool(spec, |_ctx, p| {
        let f = p.file("input")?;
        let size = f.size();
        let offset = p.int_or("offset", 0).max(0) as u64;
        if offset > size {
            return Err(ToolError::invalid(
                "offset",
                format!("beyond the end of the file ({size} bytes)"),
            ));
        }
        let len = (p.int_or("length", 1024).max(1) as u64).min(size - offset);
        let width: usize = p.str_or("width", "16").parse().unwrap_or(16);
        let mut file = std::fs::File::open(&f.path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf)?;
        Ok(ToolOutput::text(hex_dump(&buf, offset, width))
            .with_data(json!({"file_size": size, "offset": offset, "length": len})))
    })
}

fn binary_info() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.binary_info", "Executable Info", Category::Files)
        .describe("Inspect ELF, PE (Windows .exe/.dll) and Mach-O binaries: architecture, entry point, sections, linked libraries, imports and exports")
        .keywords(&["elf", "pe", "exe", "dll", "mach-o", "binary", "executable", "readelf", "objdump", "architecture", "libraries"])
        .param(ParamSpec::file("input").label("Executable or library").required().primary())
        .output(OutputKind::Data)
        .timeout(120);
    sync_tool(spec, |_ctx, p| {
        let f = p.file("input")?;
        if f.size() > 1 << 30 {
            return Err(ToolError::limit(
                "binaries larger than 1 GiB are not inspected",
            ));
        }
        let bytes = std::fs::read(&f.path)?;
        let mut data = binary_details(&bytes).map_err(|e| ToolError::invalid("input", e))?;
        data["file"] = json!(f.name);
        data["size"] = json!(human_size(bytes.len() as u64));
        data["header_hex"] = json!(hex_dump(&bytes[..bytes.len().min(256)], 0, 16));
        Ok(ToolOutput::data(data))
    })
}

fn pe_machine(m: u16) -> &'static str {
    match m {
        0x14c => "x86 (i386)",
        0x8664 => "x86-64",
        0xaa64 => "ARM64",
        0x1c0 | 0x1c4 => "ARM",
        0x200 => "IA-64",
        0x5064 => "RISC-V 64",
        _ => "unknown",
    }
}

pub fn binary_details(bytes: &[u8]) -> Result<Value, String> {
    use goblin::Object;
    let obj = Object::parse(bytes).map_err(|e| format!("not a recognized executable: {e}"))?;
    Ok(match obj {
        Object::Elf(elf) => {
            let sections: Vec<Value> = elf
                .section_headers
                .iter()
                .take(200)
                .map(|s| {
                    json!([
                        elf.shdr_strtab.get_at(s.sh_name).unwrap_or(""),
                        format!("0x{:x}", s.sh_addr),
                        s.sh_size,
                        goblin::elf::section_header::sht_to_str(s.sh_type)
                    ])
                })
                .collect();
            let imports = elf.dynsyms.iter().filter(|s| s.is_import()).count();
            let exports = elf
                .dynsyms
                .iter()
                .filter(|s| {
                    !s.is_import() && s.st_bind() != goblin::elf::sym::STB_LOCAL && s.st_name != 0
                })
                .count();
            json!({
                "format": "ELF",
                "type": goblin::elf::header::et_to_str(elf.header.e_type),
                "architecture": goblin::elf::header::machine_to_str(elf.header.e_machine),
                "bits": if elf.is_64 { 64 } else { 32 },
                "endianness": if elf.little_endian { "little" } else { "big" },
                "entry_point": format!("0x{:x}", elf.entry),
                "interpreter": elf.interpreter,
                "soname": elf.soname,
                "libraries": elf.libraries,
                "imports": imports,
                "exports": exports,
                "symbols": elf.syms.len(),
                "stripped": elf.syms.is_empty(),
                "table": table(&["Section", "Address", "Size", "Type"], sections),
            })
        }
        Object::PE(pe) => {
            let sections: Vec<Value> = pe
                .sections
                .iter()
                .map(|s| {
                    json!([
                        s.name().unwrap_or("?"),
                        format!("0x{:x}", s.virtual_address),
                        s.size_of_raw_data,
                        format!("0x{:x}", s.characteristics)
                    ])
                })
                .collect();
            let subsystem = pe
                .header
                .optional_header
                .map(|oh| match oh.windows_fields.subsystem {
                    2 => "Windows GUI",
                    3 => "Windows console",
                    1 => "Native",
                    10 => "EFI application",
                    _ => "other",
                });
            json!({
                "format": "PE",
                "type": if pe.is_lib { "DLL" } else { "Executable" },
                "architecture": pe_machine(pe.header.coff_header.machine),
                "bits": if pe.is_64 { 64 } else { 32 },
                "entry_point": format!("0x{:x}", pe.entry),
                "image_base": format!("0x{:x}", pe.image_base),
                "subsystem": subsystem,
                "libraries": pe.libraries,
                "imports": pe.imports.len(),
                "exports": pe.exports.len(),
                "table": table(&["Section", "Virtual address", "Raw size", "Characteristics"], sections),
            })
        }
        Object::Mach(goblin::mach::Mach::Binary(m)) => macho_json(&m),
        Object::Mach(goblin::mach::Mach::Fat(fat)) => {
            let arches: Vec<Value> = fat
                .arches()
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            json!(goblin::mach::cputype::get_arch_name_from_types(
                                x.cputype,
                                x.cpusubtype
                            )
                            .unwrap_or("unknown"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!({"format": "Mach-O universal (fat)", "architectures": arches})
        }
        Object::Archive(a) => {
            json!({"format": "Unix ar archive (static library)", "members": a.len(), "table": table(&["Member"], a.members().iter().take(500).map(|m| json!([m])).collect())})
        }
        _ => return Err("not an ELF, PE or Mach-O binary".into()),
    })
}

fn macho_json(m: &goblin::mach::MachO) -> Value {
    let mut sections = vec![];
    for seg in m.segments.iter() {
        if let Ok(secs) = seg.sections() {
            for (s, _) in secs.iter().take(200) {
                sections.push(json!([
                    format!("{},{}", s.segname().unwrap_or("?"), s.name().unwrap_or("?")),
                    format!("0x{:x}", s.addr),
                    s.size
                ]));
            }
        }
    }
    json!({
        "format": "Mach-O",
        "architecture": goblin::mach::cputype::get_arch_name_from_types(m.header.cputype(), m.header.cpusubtype()).unwrap_or("unknown"),
        "bits": if m.is_64 { 64 } else { 32 },
        "type": match m.header.filetype { 2 => "Executable", 6 => "Dynamic library", 8 => "Bundle", 1 => "Object", _ => "other" },
        "entry_point": format!("0x{:x}", m.entry),
        "libraries": m.libs.iter().filter(|l| **l != "self").collect::<Vec<_>>(),
        "imports": m.imports().map(|v| v.len()).unwrap_or(0),
        "exports": m.exports().map(|v| v.len()).unwrap_or(0),
        "table": table(&["Section", "Address", "Size"], sections),
    })
}

fn mime_types() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.mime_types", "MIME Type Lookup", Category::Files)
        .describe("Look up the MIME type for file extensions, or the extensions for a MIME type")
        .keywords(&["mime", "content-type", "media type", "extension", "lookup"])
        .param(
            ParamSpec::text("query")
                .label("Extensions or MIME types")
                .placeholder("pdf, .webp, image/png")
                .required()
                .primary(),
        )
        .output(OutputKind::Data)
        .ui("live")
        .timeout(10);
    sync_tool(spec, |_ctx, p| {
        let q = p.str("query")?;
        let mut rows = vec![];
        for item in q
            .split([',', ' ', '\n', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .take(200)
        {
            if item.contains('/') {
                let exts = mime_guess::get_mime_extensions_str(&item.to_ascii_lowercase())
                    .map(|v| v.join(", "))
                    .unwrap_or_default();
                rows.push(json!([
                    item,
                    if exts.is_empty() {
                        "(unknown MIME type)".to_string()
                    } else {
                        exts
                    }
                ]));
            } else {
                let ext = item
                    .trim_start_matches('*')
                    .trim_start_matches('.')
                    .to_ascii_lowercase();
                let mimes: Vec<String> = mime_guess::from_ext(&ext)
                    .iter()
                    .map(|m| m.to_string())
                    .collect();
                rows.push(json!([
                    format!(".{ext}"),
                    if mimes.is_empty() {
                        "(unknown extension)".to_string()
                    } else {
                        mimes.join(", ")
                    }
                ]));
            }
        }
        if rows.is_empty() {
            return Err(ToolError::invalid(
                "query",
                "enter an extension or MIME type",
            ));
        }
        Ok(ToolOutput::data(
            json!({"table": table(&["Query", "Result"], rows)}),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects() {
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n0000").0, "image/png");
        assert_eq!(detect(b"{\"a\": 1}").0, "application/json");
        assert_eq!(detect(b"hello").0, "text/plain");
        assert_eq!(detect(b"").0, "application/x-empty");
        assert_eq!(detect(&[0, 1, 2, 3, 0xff]).0, "application/octet-stream");
        let d = hex_dump(b"Hello", 16, 16);
        assert!(d.starts_with("00000010  48 65 6c 6c 6f"));
        assert!(d.trim_end().ends_with("|Hello|"));
    }
}
