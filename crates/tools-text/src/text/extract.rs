//! `text.extract`: pull plain text out of documents.

use crate::util::*;
use dogubako_core::prelude::*;
use quick_xml::events::Event;
use std::io::{Read, Seek};

/// Per-entry decompression cap for office/epub parts.
const MAX_PART: u64 = 256 * 1024 * 1024;

pub fn tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.extract", "Extract Text from Document", Category::Text)
        .describe("Extract plain text from TXT, Markdown, HTML, JSON, CSV, DOCX, ODT, PPTX, ODP, XLSX/XLS/ODS, EPUB, RTF and PDF (PDF needs Poppler)")
        .keywords(&["extract text", "docx to text", "word to text", "epub", "rtf", "pptx", "xlsx", "odt", "pdf to text", "plain text"])
        .param(
            ParamSpec::file("file")
                .label("Document")
                .required()
                .primary()
                .accept(&[
                    "text/*", ".txt", ".md", ".html", ".htm", ".xhtml", ".json", ".csv", ".tsv", ".xml", ".docx", ".odt", ".pptx", ".odp", ".xlsx", ".xlsm",
                    ".xls", ".xlsb", ".ods", ".epub", ".rtf", ".pdf",
                ]),
        )
        .param(ParamSpec::integer("width").label("Line width for HTML-based formats").default_int(100).range(20.0, 10000.0).advanced())
        .text_output("text/plain")
        .timeout(300);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("file")?.clone();
        let width = p.int_or("width", 100) as usize;
        let ext = f.extension();
        let sniff = dogubako_core::util::sniff_mime(&f.path);
        if ext == "pdf" || sniff.as_deref() == Some("application/pdf") {
            let path = f.path.to_string_lossy().to_string();
            let arg = if path.starts_with('-') {
                format!("./{path}")
            } else {
                path
            };
            let out = ctx
                .command_sibling("poppler", "pdftotext")?
                .args(["-enc", "UTF-8", "-layout"])
                .arg(arg)
                .arg("-")
                .run()
                .await?;
            let text = out.stdout_text();
            let mut o = ToolOutput::text(text.clone())
                .with_data(json!({"format": "pdf", "characters": text.chars().count()}));
            if text.trim().is_empty() {
                o = o.warn(
                    "No text layer found — the PDF may be scanned; use OCR (pdf.ocr) instead",
                );
            }
            return Ok(o);
        }
        let limits = ctx.limits().clone();
        ctx.blocking(move || {
            let (format, text) = extract_any(&f, &ext, width, limits.max_extract_bytes)?;
            Ok(ToolOutput::text(text.clone()).with_data(json!({"format": format, "characters": text.chars().count(), "words": text.split_whitespace().count()})))
        })
        .await
    })
}

fn extract_any(
    f: &InputFile,
    ext: &str,
    width: usize,
    max_total: u64,
) -> ToolResult<(&'static str, String)> {
    let bytes = read_file(f)?;
    let is_zip = bytes.starts_with(b"PK\x03\x04");
    let zip_open = || -> ToolResult<zip::ZipArchive<std::io::Cursor<Vec<u8>>>> {
        zip::ZipArchive::new(std::io::Cursor::new(bytes.clone()))
            .map_err(|e| ToolError::invalid("file", format!("not a valid {ext} file: {e}")))
    };
    Ok(match ext {
        "docx" | "docm" | "dotx" if is_zip => ("docx", docx(&mut zip_open()?, max_total)?),
        "pptx" | "ppsx" if is_zip => ("pptx", pptx(&mut zip_open()?, max_total)?),
        "odt" | "odp" | "ott" if is_zip => ("opendocument", odf(&mut zip_open()?, max_total)?),
        "epub" if is_zip => ("epub", epub(&mut zip_open()?, width, max_total)?),
        "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => ("spreadsheet", spreadsheet(f)?),
        "rtf" => ("rtf", rtf_to_text(&decode_bytes(&bytes).0)),
        "html" | "htm" | "xhtml" => (
            "html",
            crate::text::more::html_text(&decode_bytes(&bytes).0, width, false)?,
        ),
        _ if is_zip => {
            // Unknown extension: guess from the archive's content.
            let mut z = zip_open()?;
            let names: Vec<String> = z
                .file_names()
                .filter_map(|s| s.ok().map(|c| c.to_string()))
                .collect();
            if names.iter().any(|n| n == "word/document.xml") {
                ("docx", docx(&mut z, max_total)?)
            } else if names.iter().any(|n| n.starts_with("ppt/slides/")) {
                ("pptx", pptx(&mut z, max_total)?)
            } else if names.iter().any(|n| n == "content.xml") {
                ("opendocument", odf(&mut z, max_total)?)
            } else if names.iter().any(|n| n == "META-INF/container.xml") {
                ("epub", epub(&mut z, width, max_total)?)
            } else if names.iter().any(|n| n.starts_with("xl/")) {
                ("spreadsheet", spreadsheet(f)?)
            } else {
                return Err(ToolError::unsupported(
                    "this ZIP archive is not a supported document format",
                ));
            }
        }
        _ => {
            if bytes.starts_with(b"{\\rtf") {
                ("rtf", rtf_to_text(&String::from_utf8_lossy(&bytes)))
            } else if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
                ("spreadsheet", spreadsheet(f).map_err(|_| ToolError::unsupported("legacy binary Office documents (.doc/.ppt) are not supported; convert them with LibreOffice first"))?)
            } else {
                let nul = bytes.iter().take(8192).filter(|b| **b == 0).count();
                let (s, enc) = decode_bytes(&bytes);
                if nul > 0 && !enc.starts_with("UTF-16") {
                    return Err(ToolError::unsupported(format!(
                        "'{}' looks like a binary file, not text",
                        f.name
                    )));
                }
                ("text", s)
            }
        }
    })
}

fn read_part<R: Read + Seek>(
    z: &mut zip::ZipArchive<R>,
    name: &str,
    budget: &mut u64,
) -> ToolResult<Option<String>> {
    let entry = match z.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => {
            return Err(ToolError::invalid(
                "file",
                format!("cannot read {name}: {e}"),
            ))
        }
    };
    let cap = MAX_PART.min(*budget);
    let mut buf = Vec::new();
    entry
        .take(cap + 1)
        .read_to_end(&mut buf)
        .map_err(|e| ToolError::invalid("file", format!("cannot decompress {name}: {e}")))?;
    if buf.len() as u64 > cap {
        return Err(ToolError::limit(format!(
            "document part {name} is too large when decompressed"
        )));
    }
    *budget -= buf.len() as u64;
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

struct XmlRules<'a> {
    para: &'a [&'a str],
    brk: &'a [&'a str],
    tab: &'a [&'a str],
    skip: &'a [&'a str],
    /// When set, only text inside these elements is kept.
    text_in: Option<&'a [&'a str]>,
}

fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or("").to_string()
}

fn xml_text(xml: &str, r: &XmlRules) -> ToolResult<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = String::new();
    let mut skip_depth = 0usize;
    let mut text_depth = 0usize;
    loop {
        let ev = reader.read_event().map_err(|e| {
            ToolError::invalid(
                "file",
                format!("malformed XML at byte {}: {e}", reader.buffer_position()),
            )
        })?;
        match ev {
            Event::Start(e) => {
                let n = local(e.name().as_ref());
                if skip_depth > 0 || r.skip.contains(&n.as_str()) {
                    skip_depth += 1;
                    continue;
                }
                if r.text_in.is_some_and(|t| t.contains(&n.as_str())) {
                    text_depth += 1;
                }
            }
            Event::End(e) => {
                let n = local(e.name().as_ref());
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if r.text_in.is_some_and(|t| t.contains(&n.as_str())) {
                    text_depth = text_depth.saturating_sub(1);
                }
                if r.para.contains(&n.as_str()) {
                    out.push('\n');
                }
            }
            Event::Empty(e) => {
                if skip_depth > 0 {
                    continue;
                }
                let n = local(e.name().as_ref());
                if r.brk.contains(&n.as_str()) {
                    out.push('\n');
                } else if r.tab.contains(&n.as_str()) {
                    out.push('\t');
                } else if n == "s" {
                    // ODF repeated spaces: <text:s text:c="3"/>
                    let c = e
                        .attributes()
                        .flatten()
                        .find(|a| local(a.key.as_ref()) == "c")
                        .and_then(|a| a.value.parse::<usize>().ok())
                        .unwrap_or(1)
                        .min(1000);
                    out.push_str(&" ".repeat(c));
                } else if r.para.contains(&n.as_str()) {
                    out.push('\n');
                }
            }
            Event::Text(t) if skip_depth == 0 && (r.text_in.is_none() || text_depth > 0) => {
                out.push_str(&t)
            }
            Event::CData(t) if skip_depth == 0 && (r.text_in.is_none() || text_depth > 0) => {
                out.push_str(&t)
            }
            Event::GeneralRef(g) if skip_depth == 0 && (r.text_in.is_none() || text_depth > 0) => {
                let ent = format!("&{};", &*g);
                if let Ok(s) = quick_xml::escape::unescape(&ent) {
                    out.push_str(&s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn tidy(s: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for l in s.lines() {
        let l = l.trim_end();
        if l.is_empty() && out.last().is_some_and(|p| p.is_empty()) {
            continue;
        }
        out.push(l);
    }
    out.join("\n").trim().to_string() + "\n"
}

fn docx<R: Read + Seek>(z: &mut zip::ZipArchive<R>, mut budget: u64) -> ToolResult<String> {
    let rules = XmlRules {
        para: &["p"],
        brk: &["br", "cr"],
        tab: &["tab"],
        skip: &["instrText", "delText"],
        text_in: Some(&["t"]),
    };
    let doc = read_part(z, "word/document.xml", &mut budget)?
        .ok_or_else(|| ToolError::invalid("file", "word/document.xml missing"))?;
    let mut out = xml_text(&doc, &rules)?;
    for extra in ["word/footnotes.xml", "word/endnotes.xml"] {
        if let Some(x) = read_part(z, extra, &mut budget)? {
            let t = xml_text(&x, &rules)?;
            if !t.trim().is_empty() {
                out.push_str("\n\n");
                out.push_str(&t);
            }
        }
    }
    Ok(tidy(&out))
}

fn pptx<R: Read + Seek>(z: &mut zip::ZipArchive<R>, mut budget: u64) -> ToolResult<String> {
    // Presentation order comes from ppt/presentation.xml (sldIdLst) resolved
    // through its relationships; slideN.xml names are part ids, not positions.
    let mut slides: Vec<(u32, String)> = pptx_slide_order(z, &mut budget)?
        .into_iter()
        .enumerate()
        .map(|(i, p)| (i as u32 + 1, p))
        .collect();
    if slides.is_empty() {
        slides = z
            .file_names()
            .filter_map(|n| n.ok())
            .filter_map(|n| {
                let num = n
                    .strip_prefix("ppt/slides/slide")?
                    .strip_suffix(".xml")?
                    .parse::<u32>()
                    .ok()?;
                Some((num, n.to_string()))
            })
            .collect();
        slides.sort();
    }
    let rules = XmlRules {
        para: &["p"],
        brk: &["br"],
        tab: &[],
        skip: &[],
        text_in: Some(&["t"]),
    };
    let mut out = String::new();
    for (num, name) in slides {
        if let Some(x) = read_part(z, &name, &mut budget)? {
            out.push_str(&format!("--- Slide {num} ---\n"));
            out.push_str(&tidy(&xml_text(&x, &rules)?));
            out.push('\n');
        }
    }
    Ok(out)
}

/// Slide part paths in presentation order; empty if the parts are missing.
fn pptx_slide_order<R: Read + Seek>(
    z: &mut zip::ZipArchive<R>,
    budget: &mut u64,
) -> ToolResult<Vec<String>> {
    let (Some(pres), Some(rels)) = (
        read_part(z, "ppt/presentation.xml", budget)?,
        read_part(z, "ppt/_rels/presentation.xml.rels", budget)?,
    ) else {
        return Ok(vec![]);
    };
    let attrs_of =
        |e: &quick_xml::events::BytesStart| -> std::collections::HashMap<String, String> {
            e.attributes()
                .flatten()
                .map(|a| (local(a.key.as_ref()), a.value.to_string()))
                .collect()
        };
    let mut targets = std::collections::HashMap::new();
    let mut reader = quick_xml::Reader::from_str(&rels);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e))
                if local(e.name().as_ref()) == "Relationship" =>
            {
                let a = attrs_of(&e);
                if let (Some(id), Some(t)) = (a.get("Id"), a.get("Target")) {
                    targets.insert(id.clone(), t.clone());
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let mut order = vec![];
    let mut reader = quick_xml::Reader::from_str(&pres);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if local(e.name().as_ref()) == "sldId" => {
                // `r:id` (namespaced); local() strips the prefix.
                if let Some(t) = attrs_of(&e).get("id").and_then(|id| targets.get(id)) {
                    let path = if let Some(abs) = t.strip_prefix('/') {
                        abs.to_string()
                    } else {
                        normalize_zip_path(&format!("ppt/{t}"))
                    };
                    order.push(path);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    Ok(order)
}

fn odf<R: Read + Seek>(z: &mut zip::ZipArchive<R>, mut budget: u64) -> ToolResult<String> {
    let x = read_part(z, "content.xml", &mut budget)?
        .ok_or_else(|| ToolError::invalid("file", "content.xml missing"))?;
    let rules = XmlRules {
        para: &["p", "h", "list-item"],
        brk: &["line-break"],
        tab: &["tab"],
        skip: &[
            "automatic-styles",
            "font-face-decls",
            "tracked-changes",
            "annotation",
            "scripts",
            "forms",
        ],
        text_in: Some(&["p", "h"]),
    };
    Ok(tidy(&xml_text(&x, &rules)?))
}

fn epub<R: Read + Seek>(
    z: &mut zip::ZipArchive<R>,
    width: usize,
    mut budget: u64,
) -> ToolResult<String> {
    let container = read_part(z, "META-INF/container.xml", &mut budget)?
        .ok_or_else(|| ToolError::invalid("file", "META-INF/container.xml missing"))?;
    let opf_path = attr_of(&container, "rootfile", "full-path")
        .ok_or_else(|| ToolError::invalid("file", "no rootfile in container.xml"))?;
    let opf = read_part(z, &opf_path, &mut budget)?
        .ok_or_else(|| ToolError::invalid("file", format!("{opf_path} missing")))?;
    let base = opf_path
        .rsplit_once('/')
        .map(|(d, _)| format!("{d}/"))
        .unwrap_or_default();
    // manifest id -> href, spine order
    let mut items = std::collections::HashMap::new();
    let mut spine = vec![];
    let mut reader = quick_xml::Reader::from_str(&opf);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let n = local(e.name().as_ref());
                let attrs: std::collections::HashMap<String, String> = e
                    .attributes()
                    .flatten()
                    .map(|a| (local(a.key.as_ref()), a.value.to_string()))
                    .collect();
                if n == "item" {
                    if let (Some(id), Some(href)) = (attrs.get("id"), attrs.get("href")) {
                        items.insert(id.clone(), href.clone());
                    }
                } else if n == "itemref" {
                    if let Some(id) = attrs.get("idref") {
                        spine.push(id.clone());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ToolError::invalid("file", format!("malformed OPF: {e}"))),
            _ => {}
        }
    }
    let mut out = String::new();
    for id in spine {
        let Some(href) = items.get(&id) else { continue };
        let href = percent_encoding::percent_decode_str(href.split('#').next().unwrap_or(""))
            .decode_utf8_lossy()
            .to_string();
        let path = normalize_zip_path(&format!("{base}{href}"));
        if let Some(html) = read_part(z, &path, &mut budget)? {
            out.push_str(&crate::text::more::html_text(&html, width, false)?);
            out.push_str("\n\n");
        }
    }
    Ok(tidy(&out))
}

fn normalize_zip_path(p: &str) -> String {
    let mut parts: Vec<&str> = vec![];
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn attr_of(xml: &str, elem: &str, attr: &str) -> Option<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if local(e.name().as_ref()) == elem => {
                return e
                    .attributes()
                    .flatten()
                    .find(|a| local(a.key.as_ref()) == attr)
                    .map(|a| a.value.to_string());
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

fn spreadsheet(f: &InputFile) -> ToolResult<String> {
    use calamine::Reader;
    let mut book = crate::sheet::open(f)?;
    let mut out = String::new();
    for name in book.sheet_names() {
        let rows = crate::sheet::sheet_rows(&mut book, &name)?;
        out.push_str(&format!("--- {name} ---\n"));
        for r in rows {
            out.push_str(
                &r.iter()
                    .map(crate::sheet::cell_text)
                    .collect::<Vec<_>>()
                    .join("\t"),
            );
            out.push('\n');
        }
        out.push('\n');
    }
    Ok(out)
}

/// Minimal RTF → text: handles groups, destinations, \par, \tab, \'hh (cp1252 by
/// default or \ansicpgN), \uN with \ucN skip counts.
pub(crate) fn rtf_to_text(rtf: &str) -> String {
    const SKIP_DEST: &[&str] = &[
        "fonttbl",
        "colortbl",
        "stylesheet",
        "info",
        "pict",
        "header",
        "footer",
        "headerl",
        "headerr",
        "footerl",
        "footerr",
        "object",
        "themedata",
        "colorschememapping",
        "datastore",
        "latentstyles",
        "listtable",
        "listoverridetable",
        "rsidtbl",
        "generator",
        "xmlnstbl",
        "filetbl",
        "revtbl",
        "fldinst",
        "bkmkstart",
        "bkmkend",
    ];
    let chars: Vec<char> = rtf.chars().collect();
    let mut out = String::new();
    let mut bytes: Vec<u8> = vec![];
    let mut codepage: &'static encoding_rs::Encoding = encoding_rs::WINDOWS_1252;
    // stack of (skip, uc)
    let mut stack: Vec<(bool, usize)> = vec![];
    let mut skip = false;
    let mut uc = 1usize;
    let mut pending_skip = 0usize;
    let mut i = 0;
    let flush = |bytes: &mut Vec<u8>, out: &mut String, cp: &'static encoding_rs::Encoding| {
        if !bytes.is_empty() {
            out.push_str(&cp.decode_without_bom_handling(bytes).0);
            bytes.clear();
        }
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '{' => {
                stack.push((skip, uc));
                i += 1;
            }
            '}' => {
                flush(&mut bytes, &mut out, codepage);
                if let Some((s, u)) = stack.pop() {
                    skip = s;
                    uc = u;
                }
                i += 1;
            }
            '\\' => {
                i += 1;
                let Some(&n) = chars.get(i) else { break };
                if n == '\'' {
                    let h: String = chars.iter().skip(i + 1).take(2).collect();
                    i += 3;
                    if pending_skip > 0 {
                        pending_skip -= 1;
                        continue;
                    }
                    if let Ok(b) = u8::from_str_radix(&h, 16) {
                        if !skip {
                            bytes.push(b);
                        }
                    }
                    continue;
                }
                if !n.is_ascii_alphabetic() {
                    // control symbol
                    i += 1;
                    if skip {
                        continue;
                    }
                    match n {
                        '*' => skip = true,
                        '~' => out.push('\u{a0}'),
                        '-' => {}
                        '_' => out.push('-'),
                        '\n' | '\r' => {
                            flush(&mut bytes, &mut out, codepage);
                            out.push('\n')
                        }
                        other => {
                            flush(&mut bytes, &mut out, codepage);
                            out.push(other)
                        }
                    }
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let nstart = i;
                if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                    i += 1;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let num: Option<i64> = chars[nstart..i].iter().collect::<String>().parse().ok();
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                if SKIP_DEST.contains(&word.as_str()) {
                    skip = true;
                    continue;
                }
                if skip {
                    continue;
                }
                flush(&mut bytes, &mut out, codepage);
                match word.as_str() {
                    "par" | "line" | "sect" | "page" | "row" => out.push('\n'),
                    "tab" | "cell" => out.push('\t'),
                    "emdash" => out.push('—'),
                    "endash" => out.push('–'),
                    "bullet" => out.push('•'),
                    "lquote" => out.push('‘'),
                    "rquote" => out.push('’'),
                    "ldblquote" => out.push('“'),
                    "rdblquote" => out.push('”'),
                    "uc" => uc = num.unwrap_or(1).max(0) as usize,
                    "ansicpg" => {
                        if let Some(enc) = num.and_then(|n| {
                            encoding_rs::Encoding::for_label(format!("windows-{n}").as_bytes())
                                .or_else(|| {
                                    encoding_rs::Encoding::for_label(format!("cp{n}").as_bytes())
                                })
                        }) {
                            codepage = enc;
                        }
                    }
                    "u" => {
                        if let Some(mut v) = num {
                            if v < 0 {
                                v += 65536;
                            }
                            out.push(char::from_u32(v as u32).unwrap_or('\u{fffd}'));
                            pending_skip = uc;
                        }
                    }
                    _ => {}
                }
            }
            '\r' | '\n' => i += 1,
            _ => {
                i += 1;
                if pending_skip > 0 {
                    pending_skip -= 1;
                    continue;
                }
                if !skip {
                    flush(&mut bytes, &mut out, codepage);
                    out.push(c);
                }
            }
        }
    }
    flush(&mut bytes, &mut out, codepage);
    tidy(&out)
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (n, c) in entries {
                z.start_file(*n, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(c.as_bytes()).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[tokio::test]
    async fn extract_formats() {
        let h = TestHarness::new(crate::tools());
        let docx = make_zip(&[(
            "word/document.xml",
            r#"<?xml version="1.0"?><w:document xmlns:w="x"><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve"> world &amp; co</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:body></w:document>"#,
        )]);
        let f = h.write("a.docx", &docx);
        let out = h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "Hello\t world & co\nSecond\n");
        let odt = make_zip(&[(
            "content.xml",
            r#"<office:document-content xmlns:office="o" xmlns:text="t"><office:body><text:h>Title</text:h><text:p>a<text:s text:c="2"/>b</text:p></office:body></office:document-content>"#,
        )]);
        let f = h.write("b.odt", &odt);
        assert_eq!(
            h.run("text.extract", json!({"file": f.to_str().unwrap()}))
                .await
                .unwrap()
                .text
                .unwrap(),
            "Title\na  b\n"
        );
        let pptx = make_zip(&[
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld xmlns:a="a" xmlns:p="p"><a:p><a:r><a:t>Two</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:a="a" xmlns:p="p"><a:p><a:r><a:t>One</a:t></a:r></a:p></p:sld>"#,
            ),
        ]);
        let f = h.write("c.pptx", &pptx);
        let t = h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(t.find("One").unwrap() < t.find("Two").unwrap());
        // Reordered deck: presentation.xml decides the order and numbering.
        let reordered = make_zip(&[
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId9"/><p:sldId id="257" r:id="rId8"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="rId8" Target="slides/slide1.xml"/><Relationship Id="rId9" Target="slides/slide2.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld xmlns:a="a" xmlns:p="p"><a:p><a:r><a:t>One</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld xmlns:a="a" xmlns:p="p"><a:p><a:r><a:t>Two</a:t></a:r></a:p></p:sld>"#,
            ),
        ]);
        let f = h.write("d.pptx", &reordered);
        let t = h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(t.find("Two").unwrap() < t.find("One").unwrap(), "{t}");
        assert!(t.starts_with("--- Slide 1 ---\nTwo"), "{t}");
        let epub = make_zip(&[
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
            ),
            (
                "OEBPS/content.opf",
                r#"<package><manifest><item id="c1" href="ch1.xhtml"/></manifest><spine><itemref idref="c1"/></spine></package>"#,
            ),
            (
                "OEBPS/ch1.xhtml",
                "<html><body><h1>Chapter</h1><p>Once upon a time</p></body></html>",
            ),
        ]);
        let f = h.write("d.epub", &epub);
        let t = h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(t.contains("Chapter") && t.contains("Once upon a time"));
        let f = h.write("e.rtf", br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0 Arial;}}\f0 Caf\'e9 \b bold\b0\par Line \u8364?2}");
        assert_eq!(
            h.run("text.extract", json!({"file": f.to_str().unwrap()}))
                .await
                .unwrap()
                .text
                .unwrap(),
            "Café bold\nLine €2\n"
        );
        let f = h.write("f.html", b"<p>Hi <b>there</b></p>");
        assert!(h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
            .unwrap()
            .text
            .unwrap()
            .contains("Hi there"));
        // malformed
        let f = h.write("bad.docx", b"PK\x03\x04garbage");
        assert!(h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
            .is_err());
        let f = h.write("bin.txt", &[0u8, 1, 2, 3, 0, 0, 255]);
        assert_eq!(
            h.run("text.extract", json!({"file": f.to_str().unwrap()}))
                .await
                .unwrap_err()
                .code(),
            "unsupported"
        );
    }

    #[tokio::test]
    async fn extract_pdf() {
        let h = TestHarness::new(crate::tools());
        // Build a tiny PDF with ghostscript-free hand-written content.
        let pdf = tiny_pdf("Hello PDF");
        let f = h.write("x.pdf", &pdf);
        match h
            .run("text.extract", json!({"file": f.to_str().unwrap()}))
            .await
        {
            Ok(out) => assert!(out.text.unwrap().contains("Hello PDF")),
            Err(e) => assert_eq!(e.code(), "missing_dependency"),
        }
    }

    fn tiny_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 24 Tf 72 720 Td ({text}) Tj ET");
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = vec![];
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes(),
        );
        for o in offs {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objs.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}
