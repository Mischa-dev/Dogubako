//! Shared helpers for the PDF / document tools.

use dogubako_core::prelude::*;
use dogubako_core::process::{Exec, ExecOutput};
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use std::ffi::OsString;
use std::path::Path;

pub const PDF_ACCEPT: &[&str] = &["application/pdf", ".pdf"];

/// Render a path as a program argument that can never be mistaken for an
/// option (relative paths starting with `-` get a `./` prefix).
pub fn arg_path(p: &Path) -> OsString {
    let s = p.as_os_str();
    if !p.is_absolute() && s.to_string_lossy().starts_with('-') {
        let mut o = OsString::from("./");
        o.push(s);
        o
    } else {
        s.to_os_string()
    }
}

/// `qpdf` with warnings not treated as failures.
pub fn qpdf(ctx: &ToolContext) -> ToolResult<Exec> {
    Ok(ctx.command("qpdf")?.arg("--warning-exit-0"))
}

/// Warnings from qpdf's stderr, for surfacing to the user.
pub fn qpdf_warnings(out: &ExecOutput) -> Vec<String> {
    out.stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("WARNING") || l.starts_with("qpdf:"))
        .filter(|l| !l.is_empty() && !l.contains("operation succeeded with warnings"))
        .take(20)
        .map(|l| l.to_string())
        .collect()
}

/// Number of pages of a PDF, via qpdf.
pub async fn page_count(ctx: &ToolContext, file: &Path) -> ToolResult<u32> {
    let out = ctx
        .command("qpdf")?
        .arg("--show-npages")
        .arg(arg_path(file))
        .ok_codes(&[3])
        .run()
        .await
        .map_err(|e| match e {
            ToolError::External { stderr, .. } => {
                if stderr.contains("password") {
                    ToolError::input(
                        "the PDF is password protected; remove the password with pdf.unlock first",
                    )
                } else {
                    ToolError::input(format!("not a readable PDF: {stderr}"))
                }
            }
            other => other,
        })?;
    out.stdout_text()
        .trim()
        .parse::<u32>()
        .map_err(|_| ToolError::failed("could not determine the page count"))
}

/// Report per-file progress for batch operations.
pub fn progress_files(ctx: &ToolContext, i: usize, total: usize, name: &str) {
    ctx.progress(
        i as f32 / total.max(1) as f32,
        format!("{} ({}/{})", name, i + 1, total),
    );
}

// ---------------------------------------------------------------------------
// lopdf helpers

/// Load a PDF with lopdf, rejecting encrypted documents (so protection is
/// never silently stripped) and bounding stream decompression.
pub fn load_pdf(path: &Path, param: &str) -> ToolResult<Document> {
    let opts = lopdf::LoadOptions {
        max_decompressed_size: Some(512 * 1024 * 1024),
        ..Default::default()
    };
    let doc = Document::load_with_options(path, opts)
        .map_err(|e| ToolError::invalid(param, format!("cannot parse PDF: {e}")))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(ToolError::invalid(
            param,
            "the PDF is encrypted; remove the protection with pdf.unlock first",
        ));
    }
    Ok(doc)
}

pub fn save_pdf(doc: &mut Document, path: &Path) -> ToolResult<()> {
    doc.compress();
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    doc.save_to(&mut f).ctx("writing PDF")?;
    use std::io::Write;
    f.flush()?;
    Ok(())
}

/// Follow a reference (once is enough for well-formed files; loops capped).
pub fn deref<'a>(doc: &'a Document, mut obj: &'a Object) -> &'a Object {
    for _ in 0..16 {
        match obj {
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(o) => obj = o,
                Err(_) => return &Object::Null,
            },
            _ => return obj,
        }
    }
    &Object::Null
}

pub fn deref_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match deref(doc, obj) {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

/// Look up a (possibly inherited) page attribute.
pub fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut cur = doc.get_dictionary(page).ok()?;
    for _ in 0..64 {
        if let Ok(v) = cur.get(key) {
            return Some(deref(doc, v));
        }
        let parent = cur.get(b"Parent").ok()?;
        cur = deref_dict(doc, parent)?;
    }
    None
}

pub fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

/// Visible geometry of a page.
#[derive(Debug, Clone, Copy)]
pub struct PageGeom {
    pub x0: f32,
    pub y0: f32,
    /// Unrotated box size.
    pub w: f32,
    pub h: f32,
    /// /Rotate normalized to 0, 90, 180 or 270.
    pub rotate: i32,
}

impl PageGeom {
    pub fn of(doc: &Document, page: ObjectId) -> PageGeom {
        let rect = |key: &[u8]| -> Option<[f32; 4]> {
            let arr = inherited(doc, page, key)?.as_array().ok()?;
            if arr.len() != 4 {
                return None;
            }
            let v: Vec<f32> = arr.iter().filter_map(|o| num(deref(doc, o))).collect();
            if v.len() != 4 {
                return None;
            }
            Some([
                v[0].min(v[2]),
                v[1].min(v[3]),
                v[0].max(v[2]),
                v[1].max(v[3]),
            ])
        };
        let r = rect(b"CropBox")
            .or_else(|| rect(b"MediaBox"))
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let rot = inherited(doc, page, b"Rotate")
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        let rotate = (((rot % 360) + 360) % 360 / 90 * 90) as i32;
        PageGeom {
            x0: r[0],
            y0: r[1],
            w: (r[2] - r[0]).max(1.0),
            h: (r[3] - r[1]).max(1.0),
            rotate,
        }
    }

    /// Size as displayed (width, height) after applying /Rotate.
    pub fn visible(&self) -> (f32, f32) {
        if self.rotate == 90 || self.rotate == 270 {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        }
    }

    /// Map a point in visible coordinates (origin bottom-left of the page as
    /// displayed) to PDF user space.
    pub fn to_user(self, vx: f32, vy: f32) -> (f32, f32) {
        let (dx, dy) = match self.rotate {
            90 => (self.w - vy, vx),
            180 => (self.w - vx, self.h - vy),
            270 => (vy, self.h - vx),
            _ => (vx, vy),
        };
        (self.x0 + dx, self.y0 + dy)
    }
}

/// Encode a PDF text string: PDFDocEncoding-compatible ASCII as a literal,
/// anything else as UTF-16BE with BOM.
pub fn text_string(s: &str) -> Object {
    if s.chars().all(|c| (' '..='~').contains(&c)) {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut b = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            b.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(b, StringFormat::Hexadecimal)
    }
}

/// Decode a PDF text string (UTF-16BE/LE with BOM, UTF-8 BOM, else Latin-1-ish PDFDocEncoding).
pub fn decode_text(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF {
        let u: Vec<u16> = b[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    } else if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    } else if b.len() >= 3 && b[..3] == [0xEF, 0xBB, 0xBF] {
        String::from_utf8_lossy(&b[3..]).to_string()
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

pub fn obj_to_string(doc: &Document, o: &Object) -> Option<String> {
    match deref(doc, o) {
        Object::String(b, _) => Some(decode_text(b)),
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        Object::Integer(i) => Some(i.to_string()),
        Object::Real(r) => Some(r.to_string()),
        Object::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Current UTC time as a PDF date string `D:YYYYMMDDHHmmSSZ`.
pub fn pdf_date_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "D:{:04}{:02}{:02}{:02}{:02}{:02}Z",
        y,
        m,
        d,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

// ---------------------------------------------------------------------------
// Standard Helvetica (WinAnsiEncoding) text support.

/// Advance widths of Helvetica for WinAnsi codes 32..=126 (from the AFM).
const HELV_ASCII: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
    278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0-9
    278, 278, 584, 584, 584, 556, 1015, // : ; < = > ? @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667,
    611, 722, 667, 944, 667, 667, 611, // A-Z
    278, 278, 278, 469, 556, 333, // [ \ ] ^ _ `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500,
    278, 556, 500, 722, 500, 500, 500, // a-z
    334, 260, 334, 584, // { | } ~
];

/// Encode text to WinAnsi bytes; unsupported characters become `?`.
pub fn winansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '\u{A0}'..='\u{FF}' => c as u32 as u8,
            '€' => 0x80,
            '‚' => 0x82,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '™' => 0x99,
            _ => b'?',
        })
        .collect()
}

/// Width of WinAnsi-encoded text in Helvetica at `size` points.
pub fn helvetica_width(bytes: &[u8], size: f32) -> f32 {
    let units: u32 = bytes
        .iter()
        .map(|&b| match b {
            32..=126 => HELV_ASCII[(b - 32) as usize] as u32,
            0x85 => 1000,
            0x95 => 350,
            0x96 => 556,
            0x97 | 0x99 => 1000,
            0x91 | 0x92 | 0x82 => 222,
            0x93 | 0x94 | 0x84 => 333,
            _ => 556,
        })
        .sum();
    units as f32 * size / 1000.0
}

/// Escape bytes for a PDF literal string.
pub fn pdf_literal(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 2);
    out.push(b'(');
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\n' => out.extend_from_slice(b"\\n"),
            _ => out.push(b),
        }
    }
    out.push(b')');
    out
}

pub fn helvetica_font_dict() -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Font".to_vec()));
    d.set("Subtype", Object::Name(b"Type1".to_vec()));
    d.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    d.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    d
}

/// Parse `#rrggbb` into 0..1 floats.
pub fn rgb(hex: &str) -> (f32, f32, f32) {
    let h = hex.trim_start_matches('#');
    let c = |i: usize| {
        u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32 / 255.0
    };
    (c(0), c(2), c(4))
}

/// Format a float compactly for content streams.
pub fn pf(v: f32) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Collect the resource sub-dictionaries of a page into an owned dictionary
/// that can be modified without affecting other pages sharing resources.
pub fn owned_resources(doc: &Document, page: ObjectId) -> Dictionary {
    let mut res = inherited(doc, page, b"Resources")
        .and_then(|o| deref_dict(doc, o))
        .cloned()
        .unwrap_or_default();
    for key in [b"Font".as_slice(), b"ExtGState", b"XObject"] {
        if let Ok(v) = res.get(key) {
            if let Some(d) = deref_dict(doc, v).cloned() {
                res.set(key.to_vec(), Object::Dictionary(d));
            }
        }
    }
    res
}

/// Pick a resource name not yet used in `sub`.
pub fn unique_name(res: &Dictionary, sub: &[u8], base: &str) -> Vec<u8> {
    let existing = res.get(sub).ok().and_then(|o| o.as_dict().ok());
    for i in 1.. {
        let cand = format!("{base}{i}").into_bytes();
        if existing.map(|d| !d.has(&cand)).unwrap_or(true) {
            return cand;
        }
    }
    unreachable!()
}

pub fn add_resource(res: &mut Dictionary, sub: &[u8], name: Vec<u8>, value: Object) {
    match res.get_mut(sub) {
        Ok(Object::Dictionary(d)) => d.set(name, value),
        _ => {
            let mut d = Dictionary::new();
            d.set(name, value);
            res.set(sub.to_vec(), Object::Dictionary(d));
        }
    }
}

/// Draw `ops` on top of a page's existing content. The original content is
/// wrapped in `q … Q` so its graphics state cannot affect the overlay.
pub fn overlay_page(
    doc: &mut Document,
    page: ObjectId,
    resources: Dictionary,
    ops: Vec<u8>,
) -> ToolResult<()> {
    let existing: Vec<Object> = {
        let d = doc.get_dictionary(page).ctx("page")?;
        match d.get(b"Contents") {
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(a)) => a.clone(),
                Ok(_) => vec![Object::Reference(*id)],
                Err(_) => vec![],
            },
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![],
        }
    };
    let pre = doc.add_object(lopdf::Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut post_content = b"\nQ\n".to_vec();
    post_content.extend_from_slice(&ops);
    let mut post_stream = lopdf::Stream::new(Dictionary::new(), post_content);
    let _ = post_stream.compress();
    let post = doc.add_object(post_stream);
    let mut contents = vec![Object::Reference(pre)];
    contents.extend(existing);
    contents.push(Object::Reference(post));
    let d = doc.get_dictionary_mut(page).ctx("page")?;
    d.set("Contents", Object::Array(contents));
    d.set("Resources", Object::Dictionary(resources));
    Ok(())
}

/// Parse a free-form list of page numbers into a filter set over `n` pages.
pub fn page_filter(spec: Option<&str>, n: u32, param: &str) -> ToolResult<Vec<bool>> {
    let mut sel = vec![spec.is_none(); n as usize];
    if let Some(s) = spec {
        for p in crate::ranges::parse(s, n).bad_input(param)? {
            sel[(p - 1) as usize] = true;
        }
    }
    Ok(sel)
}
