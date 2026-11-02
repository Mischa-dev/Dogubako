//! More `text.*` tools: encodings, markdown/html, cleanup and line utilities.

use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        encoding(),
        markdown_preview(),
        markdown_to_html(),
        html_to_text(),
        html_to_markdown(),
        cleanup(),
        slugify(),
        reverse(),
        lorem(),
        wrap(),
        column_extract(),
        number_lines(),
        prefix_suffix(),
        filter_lines(),
        unicode_inspect(),
        word_frequency(),
    ]
}

fn text_param() -> ParamSpec {
    ParamSpec::textarea("text")
        .label("Text")
        .required()
        .primary()
}

// ---------------------------------------------------------------------------
// text.encoding

const ENCODINGS: &[(&str, &str)] = &[
    ("utf-8", "UTF-8"),
    ("utf-8-bom", "UTF-8 with BOM"),
    ("utf-16le", "UTF-16 LE (with BOM)"),
    ("utf-16be", "UTF-16 BE (with BOM)"),
    ("windows-1252", "Windows-1252 (Western)"),
    (
        "iso-8859-1",
        "ISO-8859-1 / Latin-1 (treated as Windows-1252, per WHATWG)",
    ),
    ("iso-8859-2", "ISO-8859-2 (Central European)"),
    ("iso-8859-5", "ISO-8859-5 (Cyrillic)"),
    ("iso-8859-7", "ISO-8859-7 (Greek)"),
    ("iso-8859-15", "ISO-8859-15 (Latin-9)"),
    ("windows-1250", "Windows-1250 (Central European)"),
    ("windows-1251", "Windows-1251 (Cyrillic)"),
    ("windows-1253", "Windows-1253 (Greek)"),
    ("windows-1254", "Windows-1254 (Turkish)"),
    ("windows-1255", "Windows-1255 (Hebrew)"),
    ("windows-1256", "Windows-1256 (Arabic)"),
    ("windows-1257", "Windows-1257 (Baltic)"),
    ("windows-1258", "Windows-1258 (Vietnamese)"),
    ("koi8-r", "KOI8-R"),
    ("koi8-u", "KOI8-U"),
    ("ibm866", "IBM866 (DOS Cyrillic)"),
    ("macintosh", "Mac Roman"),
    ("shift_jis", "Shift_JIS"),
    ("euc-jp", "EUC-JP"),
    ("iso-2022-jp", "ISO-2022-JP"),
    ("gbk", "GBK"),
    ("gb18030", "GB18030"),
    ("big5", "Big5"),
    ("euc-kr", "EUC-KR"),
];

fn encode_to(text: &str, to: &str, unmappable: &str) -> ToolResult<(Vec<u8>, usize)> {
    match to {
        "utf-8" => return Ok((text.as_bytes().to_vec(), 0)),
        "utf-8-bom" => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            return Ok((v, 0));
        }
        "utf-16le" | "utf-16be" => {
            let le = to == "utf-16le";
            let mut v = Vec::with_capacity(text.len() * 2 + 2);
            for u in std::iter::once(0xFEFFu16).chain(text.encode_utf16()) {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            return Ok((v, 0));
        }
        _ => {}
    }
    let enc = encoding_rs::Encoding::for_label(to.as_bytes())
        .ok_or_else(|| ToolError::invalid("to", format!("unknown encoding {to}")))?;
    let mut encoder = enc.new_encoder();
    let cap = encoder
        .max_buffer_length_from_utf8_without_replacement(text.len())
        .unwrap_or(text.len() * 4 + 16)
        + 16;
    let mut out = Vec::with_capacity(cap);
    let mut src = text;
    let mut errors = 0usize;
    loop {
        let mut buf = vec![0u8; 8192];
        let (res, read, written) =
            encoder.encode_from_utf8_without_replacement(src, &mut buf, true);
        out.extend_from_slice(&buf[..written]);
        src = &src[read..];
        match res {
            encoding_rs::EncoderResult::InputEmpty => break,
            encoding_rs::EncoderResult::OutputFull => continue,
            encoding_rs::EncoderResult::Unmappable(c) => {
                errors += 1;
                match unmappable {
                    "error" => {
                        return Err(ToolError::invalid(
                            "text",
                            format!(
                                "character '{c}' (U+{:04X}) cannot be represented in {}",
                                c as u32,
                                enc.name()
                            ),
                        ))
                    }
                    "html" => out.extend_from_slice(format!("&#{};", c as u32).as_bytes()),
                    _ => out.push(b'?'),
                }
            }
        }
    }
    Ok((out, errors))
}

fn encoding() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.encoding", "Convert Text Encoding", Category::Text)
        .describe("Detect and convert the character encoding of a file or text (UTF-8, UTF-16, Windows-1252, Latin-1, Shift_JIS, GBK, Big5 ...)")
        .keywords(&["charset", "encoding", "utf-8", "utf8", "latin1", "ansi", "mojibake", "iconv", "bom"])
        .param(ParamSpec::file("file").label("File").primary())
        .param(ParamSpec::textarea("text").label("Or paste text").describe("Pasted text is already Unicode; it is encoded into the target encoding"))
        .param({
            let mut opts = vec![("auto", "Auto-detect")];
            opts.extend(ENCODINGS.iter().filter(|e| !e.0.starts_with("utf-8-") ).copied());
            // Always visible: a text field never equals `false`, so a show_if on
            // it hid the selector for uploads. Ignored for pasted text.
            ParamSpec::choice("from", &opts).label("Source encoding").default_str("auto").describe("Encoding of the uploaded file (pasted text is already Unicode)")
        })
        .param(ParamSpec::choice("to", ENCODINGS).label("Target encoding").default_str("utf-8"))
        .param(
            ParamSpec::choice("unmappable", &[("question", "Replace with ?"), ("html", "HTML numeric entity (&#233;)"), ("error", "Fail")])
                .label("Characters missing in target")
                .default_str("question")
                .advanced(),
        )
        .output(OutputKind::Files);
    sync_tool(spec, |ctx, p| {
        let (text, detected, from_name, name, dec_errors) = if let Some(f) = p.opt_file("file") {
            let bytes = read_file(f)?;
            let from = p.str_or("from", "auto");
            let (enc, bom_len) = if from == "auto" {
                match encoding_rs::Encoding::for_bom(&bytes) {
                    Some((e, n)) => (e, n),
                    None if std::str::from_utf8(&bytes).is_ok() => (encoding_rs::UTF_8, 0),
                    None => (detect_encoding(&bytes), 0),
                }
            } else {
                let e = encoding_rs::Encoding::for_label(from.as_bytes()).ok_or_else(|| {
                    ToolError::invalid("from", format!("unknown encoding {from}"))
                })?;
                let bom = encoding_rs::Encoding::for_bom(&bytes)
                    .filter(|(b, _)| *b == e)
                    .map(|(_, n)| n)
                    .unwrap_or(0);
                (e, bom)
            };
            let (s, had_errors) = enc.decode_without_bom_handling(&bytes[bom_len..]);
            (
                s.into_owned(),
                Some(enc.name()),
                enc.name().to_string(),
                f.name.clone(),
                had_errors,
            )
        } else if p.has("text") {
            (
                p.text("text").to_string(),
                None,
                "Unicode text".to_string(),
                "text.txt".to_string(),
                false,
            )
        } else {
            return Err(ToolError::invalid("file", "upload a file or paste text"));
        };
        let to = p.str("to")?;
        let (bytes, unmappable) = encode_to(&text, to, p.str_or("unmappable", "question"))?;
        let a = ctx.write_output(&suffixed(&name, to, None), &bytes)?;
        let mut out = ToolOutput::files(vec![a]).with_data(json!({
            "detected": detected,
            "from": from_name,
            "to": to,
            "characters": text.chars().count(),
            "output_bytes": bytes.len(),
            "unmappable_characters": unmappable,
            "decode_errors": dec_errors,
        }));
        if dec_errors {
            out = out.warn(
                "Some bytes were invalid in the source encoding and were replaced with U+FFFD",
            );
        }
        if unmappable > 0 {
            out = out.warn(format!(
                "{unmappable} character(s) could not be represented in {to}"
            ));
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// Markdown

pub(crate) fn markdown_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_SMART_PUNCTUATION;
    let mut out = String::with_capacity(md.len() * 2);
    html::push_html(&mut out, Parser::new_ext(md, opts));
    out
}

pub(crate) fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default()
        .add_tags(&["input"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attributes("code", &["class"])
        .add_generic_attributes(&["id"])
        .clean(html)
        .to_string()
}

fn markdown_preview() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.markdown_preview", "Markdown Preview", Category::Text)
        .describe("Render Markdown (GitHub-flavored: tables, task lists, strikethrough, footnotes) as safe, sanitized HTML")
        .keywords(&["markdown", "md", "preview", "render", "gfm", "readme"])
        .param(ParamSpec::textarea("text").label("Markdown").required().primary().syntax("markdown"))
        .text_output("text/html")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        Ok(ToolOutput::text_as(
            sanitize_html(&markdown_html(p.text("text"))),
            "text/html",
        ))
    })
}

fn markdown_to_html() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.markdown_to_html", "Markdown to HTML", Category::Text)
        .describe("Convert Markdown (GitHub-flavored) to HTML, optionally as a complete standalone HTML document")
        .keywords(&["markdown", "html", "convert", "md to html", "gfm"])
        .param(ParamSpec::textarea("text").label("Markdown").primary().syntax("markdown"))
        .param(ParamSpec::file("file").label("Or upload a .md file").accept(&[".md", ".markdown", ".txt", "text/*"]))
        .param(ParamSpec::boolean("sanitize").label("Sanitize HTML (strip scripts and unsafe markup)").default_bool(true))
        .param(ParamSpec::boolean("full_document").label("Wrap in a complete HTML document"))
        .param(ParamSpec::text("title").label("Document title").default_str("Document").show_if_bool("full_document", true))
        .text_output("text/html");
    sync_tool(spec, |_ctx, p| {
        let md = text_or_file(p, "text", "file")?;
        let mut html = markdown_html(&md);
        if p.bool("sanitize") {
            html = sanitize_html(&html);
        }
        if p.bool("full_document") {
            html = format!(
                "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n</head>\n<body>\n{}</body>\n</html>\n",
                crate::escape::html_escape(p.str_or("title", "Document"), false),
                html
            );
        }
        Ok(ToolOutput::text_as(html, "text/html"))
    })
}

fn html_to_text() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.html_to_text", "HTML to Plain Text", Category::Text)
        .describe("Convert HTML into readable plain text, keeping lists, tables and link targets")
        .keywords(&["html", "strip tags", "plain text", "remove html"])
        .param(
            ParamSpec::textarea("text")
                .label("HTML")
                .primary()
                .syntax("html"),
        )
        .param(
            ParamSpec::file("file")
                .label("Or upload an HTML file")
                .accept(&[".html", ".htm", ".xhtml", "text/html"]),
        )
        .param(
            ParamSpec::integer("width")
                .label("Line width")
                .default_int(80)
                .range(10.0, 10000.0),
        )
        .param(
            ParamSpec::boolean("decorate")
                .label("Mark links, emphasis and headings")
                .default_bool(true)
                .advanced(),
        )
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let html = text_or_file(p, "text", "file")?;
        Ok(ToolOutput::text(html_text(
            &html,
            p.int_or("width", 80) as usize,
            p.bool("decorate"),
        )?))
    })
}

pub(crate) fn html_text(html: &str, width: usize, decorate: bool) -> ToolResult<String> {
    let r = if decorate {
        html2text::from_read(html.as_bytes(), width)
    } else {
        html2text::config::plain_no_decorate().string_from_read(html.as_bytes(), width)
    };
    r.map_err(|e| ToolError::invalid("text", format!("could not convert HTML: {e}")))
}

fn html_to_markdown() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.html_to_markdown", "HTML to Markdown", Category::Text)
        .describe("Convert HTML into clean Markdown")
        .keywords(&["html", "markdown", "convert", "html to md"])
        .param(
            ParamSpec::textarea("text")
                .label("HTML")
                .primary()
                .syntax("html"),
        )
        .param(
            ParamSpec::file("file")
                .label("Or upload an HTML file")
                .accept(&[".html", ".htm", ".xhtml", "text/html"]),
        )
        .text_output("text/markdown")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let html = text_or_file(p, "text", "file")?;
        let conv = htmd::HtmlToMarkdown::builder()
            .skip_tags(vec!["script", "style", "head"])
            .build();
        let md = conv
            .convert(&html)
            .map_err(|e| ToolError::invalid("text", format!("could not convert HTML: {e}")))?;
        Ok(ToolOutput::text_as(md, "text/markdown"))
    })
}

// ---------------------------------------------------------------------------
// text.cleanup

fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{00AD}' | '\u{180E}'
        | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{2061}'..='\u{2064}' | '\u{034F}')
}

/// Undo the classic "UTF-8 decoded as Windows-1252" corruption when the
/// round-trip is lossless and yields valid UTF-8.
pub(crate) fn fix_mojibake(s: &str) -> Option<String> {
    let suspicious = |t: &str| {
        t.chars()
            .filter(|c| {
                matches!(
                    c,
                    'Ã' | 'Â' | 'â' | 'Ð' | 'Ñ' | 'Å' | 'Ä' | '€' | '™' | 'œ' | 'ž' | 'Ÿ'
                )
            })
            .count()
    };
    if suspicious(s) == 0 {
        return None;
    }
    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1252.encode(s);
    if had_errors {
        return None;
    }
    let fixed = String::from_utf8(bytes.into_owned()).ok()?;
    (fixed != s && suspicious(&fixed) < suspicious(s)).then_some(fixed)
}

fn cleanup() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.cleanup", "Clean Up Text", Category::Text)
        .describe("Straighten smart quotes, remove invisible characters, normalize Unicode, strip accents, fix mojibake and tidy whitespace")
        .keywords(&["smart quotes", "zero width", "invisible", "unicode normalize", "nfc", "nfkc", "diacritics", "accents", "mojibake", "sanitize", "nbsp"])
        .param(text_param())
        .param(
            ParamSpec::multi_choice(
                "operations",
                &[
                    ("quotes", "Smart quotes → straight quotes"),
                    ("dashes", "Em/en dashes → hyphen"),
                    ("ellipsis", "… → ..."),
                    ("invisible", "Remove zero-width & invisible characters"),
                    ("nbsp", "Non-breaking/odd spaces → normal space"),
                    ("control", "Remove control characters"),
                    ("diacritics", "Strip accents/diacritics (é → e)"),
                    ("whitespace", "Collapse repeated spaces & trim lines"),
                    ("mojibake", "Fix mojibake (Ã© → é)"),
                ],
            )
            .default_value(json!(["quotes", "invisible", "nbsp", "control"])),
        )
        .param(ParamSpec::choice("normalize", &[("none", "None"), ("nfc", "NFC"), ("nfd", "NFD"), ("nfkc", "NFKC (compatibility)"), ("nfkd", "NFKD")]).default_str("nfc"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let ops: HashSet<String> = p.list("operations").into_iter().collect();
        let mut s = p.text("text").to_string();
        let original_chars = s.chars().count();
        let mut changes = serde_json::Map::new();
        if ops.contains("mojibake") {
            if let Some(f) = fix_mojibake(&s) {
                s = f;
                changes.insert("mojibake_fixed".into(), json!(true));
            }
        }
        let mut count_map =
            |name: &str, s: &mut String, f: &dyn Fn(char) -> Option<&'static str>| {
                let mut n = 0;
                let mut out = String::with_capacity(s.len());
                for c in s.chars() {
                    match f(c) {
                        Some(r) => {
                            n += 1;
                            out.push_str(r)
                        }
                        None => out.push(c),
                    }
                }
                if n > 0 {
                    changes.insert(name.into(), json!(n));
                }
                *s = out;
            };
        if ops.contains("invisible") {
            count_map("invisible_removed", &mut s, &|c| {
                is_invisible(c).then_some("")
            });
        }
        if ops.contains("quotes") {
            count_map("quotes", &mut s, &|c| match c {
                '‘' | '’' | '‚' | '‛' | '′' | '`' | '´' => Some("'"),
                '“' | '”' | '„' | '‟' | '″' | '«' | '»' => Some("\""),
                _ => None,
            });
        }
        if ops.contains("dashes") {
            count_map("dashes", &mut s, &|c| {
                matches!(c, '–' | '—' | '―' | '‒' | '−' | '‐' | '‑').then_some("-")
            });
        }
        if ops.contains("ellipsis") {
            count_map("ellipsis", &mut s, &|c| (c == '…').then_some("..."));
        }
        if ops.contains("nbsp") {
            count_map("spaces", &mut s, &|c| {
                matches!(
                    c,
                    '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
                )
                .then_some(" ")
            });
        }
        if ops.contains("control") {
            count_map("control_removed", &mut s, &|c| {
                (c.is_control() && !matches!(c, '\n' | '\t' | '\r')).then_some("")
            });
        }
        s = match p.str("normalize")? {
            "nfc" => s.nfc().collect(),
            "nfd" => s.nfd().collect(),
            "nfkc" => s.nfkc().collect(),
            "nfkd" => s.nfkd().collect(),
            _ => s,
        };
        if ops.contains("diacritics") {
            let before = s.clone();
            s = strip_diacritics(&s);
            if before != s {
                changes.insert("diacritics_stripped".into(), json!(true));
            }
        }
        if ops.contains("whitespace") {
            s = s
                .lines()
                .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join("\n");
        }
        changes.insert("characters_before".into(), json!(original_chars));
        changes.insert("characters_after".into(), json!(s.chars().count()));
        Ok(ToolOutput::text(s).with_data(Value::Object(changes)))
    })
}

pub(crate) fn strip_diacritics(s: &str) -> String {
    s.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(|c| {
            let r: &str = match c {
                'ß' => "ss",
                'æ' => "ae",
                'Æ' => "AE",
                'ø' => "o",
                'Ø' => "O",
                'œ' => "oe",
                'Œ' => "OE",
                'ł' => "l",
                'Ł' => "L",
                'đ' => "d",
                'Đ' => "D",
                'þ' => "th",
                'Þ' => "Th",
                'ı' => "i",
                _ => return vec![c],
            };
            r.chars().collect()
        })
        .nfc()
        .collect()
}

// ---------------------------------------------------------------------------
// text.slugify

pub(crate) fn slug(s: &str, sep: &str, lower: bool, max: usize) -> String {
    let ascii = strip_diacritics(s);
    let mut out = String::new();
    let mut pending_sep = false;
    for c in ascii.chars() {
        let c = if c == '&' { None } else { Some(c) };
        match c {
            Some(c) if c.is_ascii_alphanumeric() => {
                if pending_sep && !out.is_empty() {
                    out.push_str(sep);
                }
                pending_sep = false;
                out.push(if lower { c.to_ascii_lowercase() } else { c });
            }
            Some('\'') | Some('’') => {}
            _ => pending_sep = true,
        }
    }
    if max > 0 && out.len() > max {
        // Cut at a character boundary (the separator may be non-ASCII).
        let mut cut = max;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        if !sep.is_empty() {
            while out.ends_with(sep) {
                out.truncate(out.len() - sep.len());
            }
            // Drop a separator cut in half, e.g. "a-" from "--".
            if let Some(k) = (1..sep.len())
                .rev()
                .find(|&k| sep.is_char_boundary(k) && out.ends_with(&sep[..k]))
            {
                out.truncate(out.len() - k);
            }
        }
    }
    out
}

fn slugify() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.slugify", "Slugify", Category::Text)
        .describe("Turn text into URL-friendly slugs (each line separately): Crème Brûlée! → creme-brulee")
        .keywords(&["slug", "url", "permalink", "seo", "filename", "clean url"])
        .param(text_param())
        .param(ParamSpec::text("separator").label("Separator").default_str("-"))
        .param(ParamSpec::boolean("lowercase").label("Lowercase").default_bool(true))
        .param(ParamSpec::integer("max_length").label("Max length (0 = unlimited)").default_int(0).min(0.0).advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let sep = p.text("separator");
        let max = p.int_or("max_length", 0) as usize;
        // At most one separator per two input bytes ("a a a").
        let len = p.text("text").len() as u128;
        crate::util::check_generated_size(len + (len / 2 + 1) * sep.len() as u128)?;
        let out: Vec<String> = p
            .text("text")
            .lines()
            .map(|l| slug(l, sep, p.bool("lowercase"), max))
            .collect();
        Ok(ToolOutput::text(out.join("\n")))
    })
}

// ---------------------------------------------------------------------------
// text.reverse

fn reverse() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.reverse", "Reverse Text", Category::Text)
        .describe("Reverse the characters, the word order or the line order of text")
        .keywords(&["reverse", "backwards", "mirror", "flip"])
        .param(text_param())
        .param(
            ParamSpec::choice(
                "mode",
                &[
                    ("chars", "Characters"),
                    ("words", "Word order (per line)"),
                    ("lines", "Line order"),
                    ("each_word", "Letters of each word"),
                ],
            )
            .default_str("chars"),
        )
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text");
        let out = match p.str("mode")? {
            "chars" => t.graphemes(true).rev().collect(),
            "words" => t
                .lines()
                .map(|l| l.split_whitespace().rev().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join("\n"),
            "lines" => {
                let (mut l, tr) = split_lines(t);
                l.reverse();
                join_lines(&l, tr)
            }
            _ => t
                .split_word_bounds()
                .map(|w| {
                    if w.chars().any(|c| c.is_alphanumeric()) {
                        w.graphemes(true).rev().collect()
                    } else {
                        w.to_string()
                    }
                })
                .collect(),
        };
        Ok(ToolOutput::text(out))
    })
}

// ---------------------------------------------------------------------------
// text.lorem

const LOREM: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "in",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "eu",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
    "perspiciatis",
    "unde",
    "omnis",
    "iste",
    "natus",
    "error",
    "voluptatem",
    "accusantium",
    "doloremque",
    "laudantium",
    "totam",
    "rem",
    "aperiam",
    "eaque",
    "ipsa",
    "quae",
    "ab",
    "illo",
    "inventore",
    "veritatis",
    "quasi",
    "architecto",
    "beatae",
    "vitae",
    "dicta",
    "explicabo",
    "nemo",
    "ipsam",
    "quia",
    "voluptas",
    "aspernatur",
    "aut",
    "odit",
    "fugit",
    "consequuntur",
    "magni",
    "dolores",
    "eos",
    "ratione",
    "sequi",
    "nesciunt",
    "neque",
    "porro",
    "quisquam",
    "dolorem",
    "adipisci",
    "numquam",
    "eius",
    "modi",
    "tempora",
    "incidunt",
    "magnam",
    "quaerat",
    "minima",
    "nostrum",
    "exercitationem",
    "ullam",
    "corporis",
    "suscipit",
    "laboriosam",
    "aliquid",
    "commodi",
    "autem",
    "vel",
    "eum",
    "iure",
    "quam",
    "nihil",
    "molestiae",
    "illum",
    "quo",
];

fn lorem() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.lorem", "Lorem Ipsum Generator", Category::Text)
        .describe("Generate lorem ipsum placeholder text as words, sentences or paragraphs")
        .keywords(&[
            "lorem ipsum",
            "placeholder",
            "dummy text",
            "filler",
            "generator",
        ])
        .param(
            ParamSpec::choice(
                "unit",
                &[
                    ("paragraphs", "Paragraphs"),
                    ("sentences", "Sentences"),
                    ("words", "Words"),
                ],
            )
            .default_str("paragraphs"),
        )
        .param(
            ParamSpec::integer("count")
                .label("How many")
                .default_int(3)
                .range(1.0, 1000.0),
        )
        .param(
            ParamSpec::boolean("start_with_lorem")
                .label("Start with \"Lorem ipsum dolor sit amet\"")
                .default_bool(true),
        )
        .param(
            ParamSpec::boolean("html")
                .label("Wrap paragraphs in <p> tags")
                .advanced(),
        )
        .text_output("text/plain");
    sync_tool(spec, |_ctx, p| {
        use rand::prelude::*;
        let mut rng = rand::rng();
        let n = p.int_or("count", 3).max(1) as usize;
        let start = p.bool("start_with_lorem");
        let mut first = start;
        let words = |k: usize, rng: &mut ThreadRng, first: &mut bool| -> Vec<String> {
            let mut v: Vec<String> = Vec::with_capacity(k);
            if *first {
                v.extend(
                    ["lorem", "ipsum", "dolor", "sit", "amet"]
                        .iter()
                        .take(k)
                        .map(|s| s.to_string()),
                );
                *first = false;
            }
            while v.len() < k {
                v.push(LOREM.choose(rng).unwrap().to_string());
            }
            v
        };
        let sentence = |rng: &mut ThreadRng, first: &mut bool| -> String {
            let k = rng.random_range(6..16);
            let mut w = words(k, rng, first);
            if k > 9 && rng.random_bool(0.5) {
                let i = rng.random_range(3..k - 3);
                w[i].push(',');
            }
            let mut s = w.join(" ");
            if let Some(f) = s.get(0..1) {
                s = f.to_uppercase() + &s[1..];
            }
            s + "."
        };
        let out = match p.str("unit")? {
            "words" => {
                let w = words(n, &mut rng, &mut first);
                w.join(" ")
            }
            "sentences" => (0..n)
                .map(|_| sentence(&mut rng, &mut first))
                .collect::<Vec<_>>()
                .join(" "),
            _ => {
                let html = p.bool("html");
                let paras: Vec<String> = (0..n)
                    .map(|_| {
                        let k = rng.random_range(4..8);
                        let para = (0..k)
                            .map(|_| sentence(&mut rng, &mut first))
                            .collect::<Vec<_>>()
                            .join(" ");
                        if html {
                            format!("<p>{para}</p>")
                        } else {
                            para
                        }
                    })
                    .collect();
                paras.join("\n\n")
            }
        };
        Ok(ToolOutput::text(out))
    })
}

// ---------------------------------------------------------------------------
// text.wrap

fn wrap() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.wrap", "Wrap Text", Category::Text)
        .describe("Hard-wrap text at a given line width, or unwrap paragraphs into single lines")
        .keywords(&[
            "wrap",
            "word wrap",
            "line width",
            "columns",
            "reflow",
            "unwrap",
            "fold",
        ])
        .param(text_param())
        .param(
            ParamSpec::choice("mode", &[("wrap", "Wrap"), ("unwrap", "Unwrap paragraphs")])
                .default_str("wrap"),
        )
        .param(
            ParamSpec::integer("width")
                .label("Width (characters)")
                .default_int(80)
                .range(1.0, 100000.0)
                .show_if("mode", &["wrap"]),
        )
        .param(
            ParamSpec::boolean("break_words")
                .label("Break words longer than the width")
                .default_bool(true)
                .show_if("mode", &["wrap"]),
        )
        .param(
            ParamSpec::text("indent")
                .label("Indent wrapped lines with")
                .default_str("")
                .advanced(),
        )
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text").replace("\r\n", "\n");
        if p.str("mode")? == "unwrap" {
            let paras: Vec<String> = t
                .split("\n\n")
                .map(|para| {
                    para.split('\n')
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            return Ok(ToolOutput::text(paras.join("\n\n")));
        }
        let width = p.int_or("width", 80).max(1) as usize;
        let indent = p.text("indent");
        // Every wrapped line repeats the indent. A line ends at a word or
        // (when breaking words) after at least width - indent characters.
        let words = t.split_whitespace().count() as u128;
        let room = width.saturating_sub(indent.chars().count()).max(1) as u128;
        let lines = words + t.len() as u128 / room + 1;
        check_generated_size(t.len() as u128 * 2 + lines * indent.len() as u128)?;
        let opts = textwrap::Options::new(width)
            .break_words(p.bool("break_words"))
            .subsequent_indent(indent);
        let out: Vec<String> = t
            .split('\n')
            .map(|line| {
                if line.trim().is_empty() {
                    String::new()
                } else {
                    textwrap::wrap(line, &opts).join("\n")
                }
            })
            .collect();
        Ok(ToolOutput::text(out.join("\n")))
    })
}

// ---------------------------------------------------------------------------
// text.column_extract

/// Parse "1,3-5,-1" into column selectors (1-based; negatives count from the end).
pub(crate) fn parse_columns(spec: &str) -> ToolResult<Vec<(i64, i64)>> {
    let mut v = vec![];
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let bad = || {
            ToolError::invalid(
                "columns",
                format!("'{part}' is not a column number or range like 2-4"),
            )
        };
        // Only ASCII digits and '-' are valid; checking first also keeps the
        // byte slicing below on character boundaries ("１", "−1").
        if !part
            .bytes()
            .all(|c| c.is_ascii_digit() || c == b'-' || c == b' ')
        {
            return Err(bad());
        }
        let (a, b) = match part[1..].find('-') {
            Some(i) => (part[..i + 1].trim(), part[i + 2..].trim()),
            None => (part, part),
        };
        let a: i64 = if a.is_empty() {
            1
        } else {
            a.parse().map_err(|_| bad())?
        };
        let b: i64 = if b.is_empty() {
            -1
        } else {
            b.parse().map_err(|_| bad())?
        };
        if a == 0 || b == 0 {
            return Err(ToolError::invalid("columns", "columns are numbered from 1"));
        }
        v.push((a, b));
    }
    if v.is_empty() {
        return Err(ToolError::invalid("columns", "no columns given"));
    }
    Ok(v)
}

fn column_extract() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.column_extract", "Extract Columns", Category::Text)
        .describe("Split each line by a delimiter and keep selected columns (like cut / awk '{print $2}')")
        .keywords(&["cut", "awk", "columns", "fields", "split", "delimiter"])
        .param(text_param())
        .param(ParamSpec::text("delimiter").label("Delimiter").default_str("whitespace").describe("Any string, or 'tab' / 'whitespace'"))
        .param(ParamSpec::text("columns").label("Columns").default_str("1").describe("e.g. 1,3-4 or -1 for the last column"))
        .param(ParamSpec::text("output_delimiter").label("Output delimiter").default_str("tab").describe("Any string, or 'tab' / 'space'"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let cols = parse_columns(p.text("columns"))?;
        let delim = match p.text("delimiter") {
            "tab" | "\\t" => "\t".to_string(),
            "" | "whitespace" => String::new(),
            d => d.to_string(),
        };
        let od = match p.text("output_delimiter") {
            "tab" | "\\t" => "\t",
            "space" => " ",
            d => d,
        };
        let mut out = vec![];
        let mut budget = crate::util::OutputBudget::default();
        for line in p.text("text").lines() {
            let fields: Vec<&str> = if delim.is_empty() {
                line.split_whitespace().collect()
            } else {
                line.split(delim.as_str()).collect()
            };
            let n = fields.len() as i64;
            let idx = |i: i64| if i < 0 { n + i } else { i - 1 };
            // Build the line directly: ranges may repeat fields ("1,1,1,..."),
            // so count as we go, and charge at least a byte per selection so
            // empty fields with an empty delimiter can't loop unboundedly.
            let mut sel = String::new();
            let mut first = true;
            for (a, b) in &cols {
                let (s, e) = (idx(*a).max(0), idx(*b).min(n - 1));
                let mut i = s;
                while i <= e {
                    let f = fields[i as usize];
                    budget.add((f.len() + od.len()).max(1))?;
                    if !first {
                        sel.push_str(od);
                    }
                    first = false;
                    sel.push_str(f);
                    i += 1;
                }
            }
            out.push(sel);
        }
        Ok(ToolOutput::text(out.join("\n")))
    })
}

// ---------------------------------------------------------------------------
// text.number_lines / prefix_suffix / filter_lines

fn number_lines() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.number_lines", "Number Lines", Category::Text)
        .describe("Add line numbers to each line of text")
        .keywords(&["line numbers", "nl", "enumerate", "numbering"])
        .param(text_param())
        .param(ParamSpec::integer("start").label("Start at").default_int(1))
        .param(
            ParamSpec::integer("step")
                .label("Step")
                .default_int(1)
                .min(1.0)
                .advanced(),
        )
        .param(
            ParamSpec::text("separator")
                .label("Separator")
                .default_str(". "),
        )
        .param(
            ParamSpec::boolean("pad")
                .label("Pad numbers to equal width")
                .default_bool(true),
        )
        .param(ParamSpec::boolean("skip_empty").label("Don't number empty lines"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (lines, tr) = split_lines(p.text("text"));
        let (start, step) = (p.int_or("start", 1), p.int_or("step", 1).max(1));
        let skip = p.bool("skip_empty");
        let total = if skip {
            lines.iter().filter(|l| !l.trim().is_empty()).count()
        } else {
            lines.len()
        } as i64;
        let width = if p.bool("pad") {
            [
                start.to_string().len(),
                (start + step * (total - 1).max(0)).to_string().len(),
            ]
            .into_iter()
            .max()
            .unwrap()
        } else {
            0
        };
        let sep = p.text("separator");
        crate::util::check_generated_size(
            p.text("text").len() as u128
                + lines.len() as u128 * (sep.len() as u128 + width.max(20) as u128),
        )?;
        let mut n = start;
        let out: Vec<String> = lines
            .iter()
            .map(|l| {
                if skip && l.trim().is_empty() {
                    return l.to_string();
                }
                let s = format!("{:>width$}{sep}{l}", n);
                n += step;
                s
            })
            .collect();
        Ok(ToolOutput::text(join_lines(&out, tr)))
    })
}

fn prefix_suffix() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.prefix_suffix", "Add Prefix / Suffix", Category::Text)
        .describe(
            "Add text to the start and/or end of every line (e.g. quotes, commas, bullet points)",
        )
        .keywords(&[
            "prefix",
            "suffix",
            "prepend",
            "append",
            "wrap lines",
            "quote lines",
            "bullet",
        ])
        .param(text_param())
        .param(ParamSpec::text("prefix").label("Prefix").default_str(""))
        .param(ParamSpec::text("suffix").label("Suffix").default_str(""))
        .param(
            ParamSpec::boolean("skip_empty")
                .label("Skip empty lines")
                .default_bool(true),
        )
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (lines, tr) = split_lines(p.text("text"));
        let (pre, suf, skip) = (p.text("prefix"), p.text("suffix"), p.bool("skip_empty"));
        crate::util::check_generated_size(
            p.text("text").len() as u128 + lines.len() as u128 * (pre.len() + suf.len()) as u128,
        )?;
        let out: Vec<String> = lines
            .iter()
            .map(|l| {
                if skip && l.trim().is_empty() {
                    l.to_string()
                } else {
                    format!("{pre}{l}{suf}")
                }
            })
            .collect();
        Ok(ToolOutput::text(join_lines(&out, tr)))
    })
}

fn filter_lines() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.filter_lines", "Filter Lines", Category::Text)
        .describe("Keep (or remove) lines that contain a word, start/end with text or match a regular expression (like grep)")
        .keywords(&["grep", "filter", "lines containing", "remove lines", "keep lines", "search"])
        .param(text_param())
        .param(ParamSpec::text("pattern").label("Pattern").required())
        .param(
            ParamSpec::choice("match", &[("contains", "Contains"), ("starts", "Starts with"), ("ends", "Ends with"), ("exact", "Equals"), ("regex", "Regular expression")])
                .default_str("contains"),
        )
        .param(ParamSpec::boolean("invert").label("Remove matching lines instead"))
        .param(ParamSpec::boolean("case_insensitive").label("Ignore case"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (lines, tr) = split_lines(p.text("text"));
        let ci = p.bool("case_insensitive");
        let pat = p.text("pattern");
        let mode = p.str("match")?;
        let re = if mode == "regex" {
            Some(
                regex::RegexBuilder::new(pat)
                    .case_insensitive(ci)
                    .size_limit(64 << 20)
                    .build()
                    .map_err(|e| ToolError::invalid("pattern", e.to_string()))?,
            )
        } else {
            None
        };
        let lp = pat.to_lowercase();
        let is_match = |l: &str| -> bool {
            if let Some(re) = &re {
                return re.is_match(l);
            }
            let (l, pt) = if ci {
                (l.to_lowercase(), lp.clone())
            } else {
                (l.to_string(), pat.to_string())
            };
            match mode {
                "starts" => l.starts_with(&pt),
                "ends" => l.ends_with(&pt),
                "exact" => l == pt,
                _ => l.contains(&pt),
            }
        };
        let inv = p.bool("invert");
        let out: Vec<&str> = lines
            .iter()
            .copied()
            .filter(|l| is_match(l) != inv)
            .collect();
        let n = out.len();
        Ok(ToolOutput::text(join_lines(&out, tr && n > 0))
            .with_data(json!({"kept": n, "removed": lines.len() - n})))
    })
}

// ---------------------------------------------------------------------------
// text.unicode_inspect

pub(crate) fn category_code(c: char) -> &'static str {
    use unicode_general_category::GeneralCategory::*;
    match unicode_general_category::get_general_category(c) {
        UppercaseLetter => "Lu – Uppercase letter",
        LowercaseLetter => "Ll – Lowercase letter",
        TitlecaseLetter => "Lt – Titlecase letter",
        ModifierLetter => "Lm – Modifier letter",
        OtherLetter => "Lo – Other letter",
        NonspacingMark => "Mn – Nonspacing mark",
        SpacingMark => "Mc – Spacing mark",
        EnclosingMark => "Me – Enclosing mark",
        DecimalNumber => "Nd – Decimal number",
        LetterNumber => "Nl – Letter number",
        OtherNumber => "No – Other number",
        ConnectorPunctuation => "Pc – Connector punctuation",
        DashPunctuation => "Pd – Dash punctuation",
        OpenPunctuation => "Ps – Open punctuation",
        ClosePunctuation => "Pe – Close punctuation",
        InitialPunctuation => "Pi – Initial quote",
        FinalPunctuation => "Pf – Final quote",
        OtherPunctuation => "Po – Other punctuation",
        MathSymbol => "Sm – Math symbol",
        CurrencySymbol => "Sc – Currency symbol",
        ModifierSymbol => "Sk – Modifier symbol",
        OtherSymbol => "So – Other symbol",
        SpaceSeparator => "Zs – Space",
        LineSeparator => "Zl – Line separator",
        ParagraphSeparator => "Zp – Paragraph separator",
        Control => "Cc – Control",
        Format => "Cf – Format",
        Surrogate => "Cs – Surrogate",
        PrivateUse => "Co – Private use",
        Unassigned => "Cn – Unassigned",
        _ => "?? – Unknown",
    }
}

pub(crate) fn char_name(c: char) -> String {
    if let Some(n) = unicode_names2::name(c) {
        return n.to_string();
    }
    match c as u32 {
        0x00..=0x1F | 0x7F..=0x9F => {
            const C0: [&str; 32] = [
                "NULL",
                "START OF HEADING",
                "START OF TEXT",
                "END OF TEXT",
                "END OF TRANSMISSION",
                "ENQUIRY",
                "ACKNOWLEDGE",
                "BELL",
                "BACKSPACE",
                "CHARACTER TABULATION",
                "LINE FEED",
                "LINE TABULATION",
                "FORM FEED",
                "CARRIAGE RETURN",
                "SHIFT OUT",
                "SHIFT IN",
                "DATA LINK ESCAPE",
                "DEVICE CONTROL ONE",
                "DEVICE CONTROL TWO",
                "DEVICE CONTROL THREE",
                "DEVICE CONTROL FOUR",
                "NEGATIVE ACKNOWLEDGE",
                "SYNCHRONOUS IDLE",
                "END OF TRANSMISSION BLOCK",
                "CANCEL",
                "END OF MEDIUM",
                "SUBSTITUTE",
                "ESCAPE",
                "INFORMATION SEPARATOR FOUR",
                "INFORMATION SEPARATOR THREE",
                "INFORMATION SEPARATOR TWO",
                "INFORMATION SEPARATOR ONE",
            ];
            match c as u32 {
                v @ 0..=0x1F => format!("<control> {}", C0[v as usize]),
                0x7F => "<control> DELETE".into(),
                _ => "<control>".into(),
            }
        }
        _ => String::new(),
    }
}

fn unicode_inspect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.unicode_inspect", "Unicode Inspector", Category::Text)
        .describe("Show every character's code point, official Unicode name, category and UTF-8/UTF-16 encoding — find hidden or look-alike characters")
        .keywords(&["unicode", "code point", "character", "emoji", "utf-8 bytes", "hidden characters", "homoglyph", "char info"])
        .param(text_param())
        .param(ParamSpec::integer("limit").label("Max characters").default_int(2000).range(1.0, 20000.0).advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text");
        let limit = p.int_or("limit", 2000) as usize;
        let total = t.chars().count();
        let mut rows = vec![];
        let mut non_ascii = 0;
        let mut invisible = 0;
        for c in t.chars() {
            if !c.is_ascii() {
                non_ascii += 1;
            }
            let cat = category_code(c);
            if cat.starts_with("Cf") || cat.starts_with("Cc") && !matches!(c, '\n' | '\t' | '\r') {
                invisible += 1;
            }
            if rows.len() >= limit {
                continue;
            }
            let mut b = [0u8; 4];
            let utf8: Vec<String> = c
                .encode_utf8(&mut b)
                .bytes()
                .map(|x| format!("{x:02X}"))
                .collect();
            let mut u = [0u16; 2];
            let utf16: Vec<String> = c
                .encode_utf16(&mut u)
                .iter()
                .map(|x| format!("{x:04X}"))
                .collect();
            let shown = if c.is_control() || cat.starts_with('Z') || cat.starts_with("Cf") {
                format!("U+{:04X}", c as u32)
            } else {
                c.to_string()
            };
            rows.push(vec![
                json!(shown),
                json!(format!("U+{:04X}", c as u32)),
                json!(char_name(c)),
                json!(cat),
                json!(utf8.join(" ")),
                json!(utf16.join(" ")),
                json!(c as u32),
            ]);
        }
        let mut out = ToolOutput::data(json!({
            "characters": total,
            "graphemes": t.graphemes(true).count(),
            "bytes_utf8": t.len(),
            "utf16_units": t.encode_utf16().count(),
            "non_ascii": non_ascii,
            "invisible_or_control": invisible,
            "table": table(&["char", "code_point", "name", "category", "utf8", "utf16", "decimal"], rows),
        }));
        if total > limit {
            out = out.warn(format!("Showing the first {limit} of {total} characters"));
        }
        Ok(out)
    })
}

fn word_frequency() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.word_frequency", "Word Frequency", Category::Text)
        .describe("Count how often each word appears and list the most frequent ones")
        .keywords(&[
            "word frequency",
            "keyword density",
            "top words",
            "concordance",
            "count words",
        ])
        .param(ParamSpec::textarea("text").label("Text").primary())
        .param(ParamSpec::file("file").label("Or upload a text file"))
        .param(
            ParamSpec::integer("top")
                .label("Show top N (0 = all)")
                .default_int(50)
                .min(0.0),
        )
        .param(
            ParamSpec::integer("min_length")
                .label("Minimum word length")
                .default_int(1)
                .range(1.0, 100.0),
        )
        .param(ParamSpec::boolean("ignore_stopwords").label("Ignore common English words"))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = text_or_file(p, "text", "file")?;
        let freq = super::word_frequencies(
            &t,
            p.int_or("min_length", 1) as usize,
            p.bool("ignore_stopwords"),
        );
        let total: usize = freq.iter().map(|f| f.1).sum();
        let distinct = freq.len();
        let top = p.int_or("top", 50) as usize;
        let take = if top == 0 {
            distinct
        } else {
            top.min(distinct)
        };
        let rows: Vec<Vec<Value>> = freq
            .iter()
            .take(take)
            .enumerate()
            .map(|(i, (w, c))| {
                vec![
                    json!(i + 1),
                    json!(w),
                    json!(c),
                    json!(format!("{:.2}%", *c as f64 * 100.0 / total.max(1) as f64)),
                ]
            })
            .collect();
        let csv: String = std::iter::once("word,count".to_string())
            .chain(
                freq.iter()
                    .take(take)
                    .map(|(w, c)| format!("{},{c}", crate::escape::csv_escape(w, ','))),
            )
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ToolOutput::text_as(csv, "text/csv").with_data(json!({"total_words": total, "distinct_words": distinct, "table": table(&["rank", "word", "count", "share"], rows)})))
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn column_selectors_reject_non_ascii() {
        assert_eq!(
            super::parse_columns("1,3-5,-1").unwrap(),
            vec![(1, 1), (3, 5), (-1, -1)]
        );
        for bad in ["\u{ff11}", "\u{2212}1", "1-\u{ff13}", "a"] {
            assert_eq!(
                super::parse_columns(bad).unwrap_err().code(),
                "invalid_input",
                "{bad}"
            );
        }
    }

    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    async fn txt(h: &TestHarness, id: &str, p: serde_json::Value) -> String {
        h.run(id, p).await.unwrap().text.unwrap()
    }

    #[tokio::test]
    async fn encoding_roundtrip() {
        let h = TestHarness::new(crate::tools());
        let (latin, _, _) = encoding_rs::WINDOWS_1252
            .encode("Café crème – naïve, déjà vu, garçon. Ça coûte très cher à Noël.");
        let f = h.write("latin.txt", &latin);
        let out = h
            .run(
                "text.encoding",
                json!({"file": f.to_str().unwrap(), "to": "utf-8"}),
            )
            .await
            .unwrap();
        let s = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
        assert!(s.starts_with("Café crème"));
        assert_eq!(out.data["detected"], "windows-1252");
        let out = h
            .run(
                "text.encoding",
                json!({"text": "héllo 😀", "to": "iso-8859-1"}),
            )
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(&out.artifacts[0].path).unwrap(),
            b"h\xe9llo ?"
        );
        assert_eq!(out.data["unmappable_characters"], 1);
        let err = h
            .run(
                "text.encoding",
                json!({"text": "😀", "to": "windows-1252", "unmappable": "error"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_input");
        let out = h
            .run("text.encoding", json!({"text": "A", "to": "utf-16le"}))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(&out.artifacts[0].path).unwrap(),
            b"\xff\xfeA\0"
        );
        assert_eq!(
            h.run("text.encoding", json!({"to": "utf-8"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn markdown_html() {
        let h = TestHarness::new(crate::tools());
        let html = txt(&h, "text.markdown_preview", json!({"text": "# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n~~old~~ <script>alert(1)</script>"})).await;
        assert!(
            html.contains("<h1>Hi</h1>")
                && html.contains("<table>")
                && html.contains("checkbox")
                && html.contains("<del>old</del>")
        );
        assert!(!html.contains("<script"));
        let doc = txt(
            &h,
            "text.markdown_to_html",
            json!({"text": "*x*", "full_document": true, "title": "T<"}),
        )
        .await;
        assert!(doc.contains("<em>x</em>") && doc.contains("<title>T&lt;</title>"));
        let t = txt(&h, "text.html_to_text", json!({"text": "<h1>Title</h1><p>Hello <b>world</b></p><ul><li>one</li></ul>", "decorate": false})).await;
        assert!(t.contains("Title") && t.contains("Hello world") && t.contains("one"));
        let md = txt(&h, "text.html_to_markdown", json!({"text": "<h2>Sub</h2><p><a href=\"https://x.y\">link</a> <script>x()</script></p>"})).await;
        assert!(md.contains("## Sub") && md.contains("[link](https://x.y)") && !md.contains("x()"));
    }

    #[tokio::test]
    async fn cleanup_slug_reverse() {
        // Truncation never splits a (non-ASCII) separator and never panics.
        assert_eq!(super::slug("a b", "é", true, 2), "a");
        assert_eq!(super::slug("a b", "é", true, 3), "a");
        assert_eq!(super::slug("a b", "é", true, 4), "aéb");
        assert_eq!(super::slug("ab cd", "--", true, 3), "ab");
        assert_eq!(super::slug("ab cd", "--", true, 6), "ab--cd");
        let h = TestHarness::new(crate::tools());
        let out = h
            .run("text.cleanup", json!({"text": "“Hi”\u{200B} it’s\u{00A0}café", "operations": ["quotes", "invisible", "nbsp", "diacritics"]}))
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "\"Hi\" it's cafe");
        assert_eq!(
            txt(
                &h,
                "text.cleanup",
                json!({"text": "CafÃ© dÃ©jÃ\u{a0} vu", "operations": ["mojibake"]})
            )
            .await,
            "Café déjà vu"
        );
        assert_eq!(
            txt(
                &h,
                "text.cleanup",
                json!({"text": "ﬁ①", "operations": [], "normalize": "nfkc"})
            )
            .await,
            "fi1"
        );
        assert_eq!(
            txt(
                &h,
                "text.slugify",
                json!({"text": "Crème Brûlée & Straße!\nHello  World"})
            )
            .await,
            "creme-brulee-strasse\nhello-world"
        );
        assert_eq!(
            txt(
                &h,
                "text.reverse",
                json!({"text": "abc😀", "mode": "chars"})
            )
            .await,
            "😀cba"
        );
        assert_eq!(
            txt(
                &h,
                "text.reverse",
                json!({"text": "one two three", "mode": "words"})
            )
            .await,
            "three two one"
        );
    }

    #[tokio::test]
    async fn generators_and_lines() {
        let h = TestHarness::new(crate::tools());
        let w = txt(&h, "text.lorem", json!({"unit": "words", "count": 12})).await;
        assert_eq!(w.split(' ').count(), 12);
        assert!(w.starts_with("lorem ipsum"));
        let p = txt(&h, "text.lorem", json!({"unit": "paragraphs", "count": 2})).await;
        assert_eq!(p.split("\n\n").count(), 2);
        assert_eq!(
            txt(&h, "text.wrap", json!({"text": "aaa bbb ccc", "width": 7})).await,
            "aaa bbb\nccc"
        );
        assert_eq!(
            txt(
                &h,
                "text.wrap",
                json!({"text": "a\nb\n\nc", "mode": "unwrap"})
            )
            .await,
            "a b\n\nc"
        );
        assert_eq!(
            txt(
                &h,
                "text.column_extract",
                json!({"text": "a b c\nd e f", "columns": "1,-1", "output_delimiter": ","})
            )
            .await,
            "a,c\nd,f"
        );
        assert_eq!(
            txt(
                &h,
                "text.column_extract",
                json!({"text": "a;b;c", "delimiter": ";", "columns": "2-"})
            )
            .await,
            "b\tc"
        );
        assert_eq!(
            h.run("text.column_extract", json!({"text": "a", "columns": "x"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            txt(&h, "text.number_lines", json!({"text": "a\nb\n"})).await,
            "1. a\n2. b\n"
        );
        assert_eq!(
            txt(
                &h,
                "text.prefix_suffix",
                json!({"text": "a\n\nb", "prefix": "'", "suffix": "',"})
            )
            .await,
            "'a',\n\n'b',"
        );
        let out = h.run("text.filter_lines", json!({"text": "apple\nBanana\ncherry", "pattern": "^b", "match": "regex", "case_insensitive": true})).await.unwrap();
        assert_eq!(out.text.unwrap(), "Banana");
        assert_eq!(
            txt(
                &h,
                "text.filter_lines",
                json!({"text": "apple\nbanana", "pattern": "app", "invert": true})
            )
            .await,
            "banana"
        );
    }

    #[tokio::test]
    async fn unicode_and_frequency() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run("text.unicode_inspect", json!({"text": "aé\u{200B}😀"}))
            .await
            .unwrap();
        let rows = out.data["table"]["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1][2], "LATIN SMALL LETTER E WITH ACUTE");
        assert_eq!(rows[1][4], "C3 A9");
        assert_eq!(rows[3][5], "D83D DE00");
        assert_eq!(out.data["invisible_or_control"], 1);
        let out = h
            .run(
                "text.word_frequency",
                json!({"text": "the cat and the hat and the bat"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["table"]["rows"][0][1], "the");
        assert_eq!(out.data["table"]["rows"][0][2], 3);
    }
}
