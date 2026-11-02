//! Encoders: Base64, URL, HTML entities, hex, number bases, binary text.

use crate::escape;
use crate::util::*;
use base64::engine::general_purpose::{GeneralPurpose, NO_PAD_INDIFFERENT};
use base64::{alphabet, Engine};
use dogubako_core::prelude::*;
use num_bigint::BigInt;
use num_traits::{Signed, Zero};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![base64_tool(), url_encode(), html_entities(), hex_tool(), number_base(), binary_text()]
}

// ---------------------------------------------------------------------------
// Base64

const STD_ANY: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, NO_PAD_INDIFFERENT);
const URL_ANY: GeneralPurpose = GeneralPurpose::new(&alphabet::URL_SAFE, NO_PAD_INDIFFERENT);

pub(crate) fn b64_decode_lenient(s: &str) -> Result<Vec<u8>, String> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let engine = if clean.contains(['-', '_']) { &URL_ANY } else { &STD_ANY };
    engine.decode(clean.as_bytes()).map_err(|e| format!("invalid Base64: {e}"))
}

fn ext_for_mime(m: &str) -> &'static str {
    match m {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "image/heic" => "heic",
        "application/pdf" => "pdf",
        "application/zip" => "zip",
        "application/gzip" => "gz",
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        "audio/flac" => "flac",
        "audio/ogg" => "ogg",
        "audio/mp4" => "m4a",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "application/json" => "json",
        "text/plain" => "txt",
        "text/html" => "html",
        "text/css" => "css",
        _ => "bin",
    }
}

fn base64_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.base64", "Base64 Encode / Decode", Category::Dev)
        .describe("Encode text or files to Base64 (standard or URL-safe, with or without padding, data URI) and decode Base64 back to text or a file")
        .keywords(&["base64", "encode", "decode", "data uri", "base64url", "atob", "btoa"])
        .param(ParamSpec::choice("mode", &[("encode", "Encode"), ("decode", "Decode")]).default_str("encode"))
        .param(ParamSpec::textarea("text").label("Text / Base64").primary())
        .param(ParamSpec::file("file").label("Or a file to encode").show_if("mode", &["encode"]))
        .param(ParamSpec::boolean("url_safe").label("URL-safe alphabet (- and _)").show_if("mode", &["encode"]))
        .param(ParamSpec::boolean("no_padding").label("Omit = padding").show_if("mode", &["encode"]))
        .param(ParamSpec::boolean("data_uri").label("Output as data: URI").show_if("mode", &["encode"]).advanced())
        .param(ParamSpec::integer("wrap").label("Wrap lines at (0 = no wrapping, 76 = MIME)").default_int(0).range(0.0, 1000.0).show_if("mode", &["encode"]).advanced())
        .param(
            ParamSpec::choice("output", &[("auto", "Text if possible, else file"), ("text", "Text"), ("file", "File")])
                .label("Decoded output")
                .default_str("auto")
                .show_if("mode", &["decode"]),
        )
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |ctx, p| {
        if p.str("mode")? == "encode" {
            let (bytes, mime) = match p.opt_file("file") {
                Some(f) => (read_file(f)?, dogubako_core::util::mime_for_path(std::path::Path::new(&f.name))),
                None => (p.text("text").as_bytes().to_vec(), "text/plain;charset=utf-8".to_string()),
            };
            let engine = match (p.bool("url_safe"), p.bool("no_padding")) {
                (false, false) => base64::engine::general_purpose::STANDARD,
                (false, true) => base64::engine::general_purpose::STANDARD_NO_PAD,
                (true, false) => base64::engine::general_purpose::URL_SAFE,
                (true, true) => base64::engine::general_purpose::URL_SAFE_NO_PAD,
            };
            let mut enc = engine.encode(&bytes);
            let wrap = p.int_or("wrap", 0) as usize;
            if wrap > 0 {
                enc = enc.as_bytes().chunks(wrap).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("\n");
            }
            if p.bool("data_uri") {
                enc = format!("data:{mime};base64,{enc}");
            }
            return Ok(ToolOutput::text(enc).with_data(json!({"input_bytes": bytes.len()})));
        }
        let mut s = p.text("text").trim();
        let mut mime: Option<String> = None;
        if let Some(rest) = s.strip_prefix("data:") {
            let (meta, payload) = rest.split_once(',').ok_or_else(|| ToolError::invalid("text", "malformed data: URI (missing comma)"))?;
            if !meta.ends_with(";base64") {
                return Err(ToolError::invalid("text", "data: URI is not Base64-encoded"));
            }
            mime = Some(meta.trim_end_matches(";base64").split(';').next().unwrap_or("").to_string());
            s = payload;
        }
        let bytes = b64_decode_lenient(s).map_err(|e| ToolError::invalid("text", e))?;
        let out_mode = p.str_or("output", "auto");
        let as_text = match out_mode {
            "text" => true,
            "file" => false,
            _ => std::str::from_utf8(&bytes).is_ok() && !bytes.iter().any(|b| *b == 0) && mime.as_deref().map_or(true, |m| m.starts_with("text/") || m.is_empty()),
        };
        if as_text {
            return Ok(ToolOutput::text(String::from_utf8_lossy(&bytes).into_owned()).with_data(json!({"bytes": bytes.len()})));
        }
        let sniffed = dogubako_core::util::sniff_bytes(&bytes).map(|s| s.to_string());
        let m = sniffed.or(mime).unwrap_or_default();
        let ext = ext_for_mime(&m);
        let a = ctx.write_output(&format!("decoded.{ext}"), &bytes)?;
        Ok(ToolOutput::files(vec![a]).with_data(json!({"bytes": bytes.len(), "detected_type": if m.is_empty() { "unknown".into() } else { m }})))
    })
}

// ---------------------------------------------------------------------------
// URL encoding

fn url_encode() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.url_encode", "URL Encode / Decode", Category::Dev)
        .describe("Percent-encode or decode text for URLs — a single component (encodeURIComponent) or a full URL (encodeURI)")
        .keywords(&["url encode", "url decode", "percent encoding", "urlencode", "encodeURIComponent", "escape url"])
        .param(ParamSpec::textarea("text").label("Text").required().primary())
        .param(ParamSpec::choice("mode", &[("encode", "Encode"), ("decode", "Decode")]).default_str("encode"))
        .param(ParamSpec::choice("scope", &[("component", "Component (encode everything reserved)"), ("full", "Full URL (keep :/?#&=)")]).default_str("component").show_if("mode", &["encode"]))
        .param(ParamSpec::boolean("plus_spaces").label("Use + for spaces (form encoding)"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text");
        let plus = p.bool("plus_spaces");
        let out = if p.str("mode")? == "encode" {
            escape::url_encode(t, p.str_or("scope", "component") == "full", plus)
        } else {
            escape::url_decode(t, plus).map_err(|e| ToolError::invalid("text", e))?
        };
        Ok(ToolOutput::text(out))
    })
}

fn html_entities() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.html_entities", "HTML Entities Encode / Decode", Category::Dev)
        .describe("Escape special characters as HTML entities (optionally all non-ASCII as numeric references) or decode all named and numeric entities")
        .keywords(&["html entities", "escape html", "unescape html", "&amp;", "nbsp", "numeric character reference"])
        .param(ParamSpec::textarea("text").label("Text").required().primary())
        .param(ParamSpec::choice("mode", &[("encode", "Encode"), ("decode", "Decode")]).default_str("encode"))
        .param(ParamSpec::boolean("non_ascii").label("Also encode non-ASCII characters (&#xE9;)").show_if("mode", &["encode"]))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text");
        Ok(ToolOutput::text(if p.str("mode")? == "encode" { escape::html_escape(t, p.bool("non_ascii")) } else { escape::html_unescape(t) }))
    })
}

// ---------------------------------------------------------------------------
// Hex

fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let mut digits = String::with_capacity(s.len());
    for tok in s.split(|c: char| c.is_whitespace() || c == ',' || c == ':' || c == '-') {
        let tok = tok.trim_start_matches("0x").trim_start_matches("0X").trim_start_matches("\\x");
        let tok = tok.replace("\\x", "");
        if tok.len() % 2 == 1 && s.split_whitespace().count() > 1 {
            digits.push('0');
        }
        digits.push_str(&tok);
    }
    if let Some(bad) = digits.chars().find(|c| !c.is_ascii_hexdigit()) {
        return Err(format!("'{bad}' is not a hex digit"));
    }
    if digits.len() % 2 == 1 {
        return Err("odd number of hex digits".into());
    }
    hex::decode(&digits).map_err(|e| e.to_string())
}

pub(crate) fn hexdump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hexs: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let (a, b) = hexs.split_at(hexs.len().min(8));
        let ascii: String = chunk.iter().map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' }).collect();
        out.push_str(&format!("{:08x}  {:<23}  {:<23}  |{}|\n", i * 16, a.join(" "), b.join(" "), ascii));
    }
    out
}

fn hex_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.hex", "Hex Encode / Decode", Category::Dev)
        .describe("Convert text or files to hexadecimal (with separators), decode hex back to text or bytes, or show a classic hex dump")
        .keywords(&["hex", "hexadecimal", "hexdump", "xxd", "bytes", "encode", "decode"])
        .param(ParamSpec::choice("mode", &[("encode", "Text → hex"), ("decode", "Hex → text"), ("dump", "Hex dump")]).default_str("encode"))
        .param(ParamSpec::textarea("text").label("Text / hex").primary())
        .param(ParamSpec::file("file").label("Or a file").show_if("mode", &["encode", "dump"]))
        .param(ParamSpec::choice("separator", &[("none", "None"), ("space", "Space"), ("colon", "Colon"), ("comma", "Comma + 0x"), ("escape", "\\x escapes")]).default_str("space").show_if("mode", &["encode"]))
        .param(ParamSpec::boolean("uppercase").label("Uppercase").show_if("mode", &["encode"]))
        .param(ParamSpec::choice("output", &[("text", "Text"), ("file", "Binary file")]).label("Decoded output").default_str("text").show_if("mode", &["decode"]))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let mode = p.str("mode")?;
        if mode == "decode" {
            let bytes = parse_hex(p.text("text")).map_err(|e| ToolError::invalid("text", e))?;
            if p.str_or("output", "text") == "file" {
                return Ok(ToolOutput::files(vec![ctx.write_output("decoded.bin", &bytes)?]));
            }
            let mut out = ToolOutput::text(String::from_utf8_lossy(&bytes).into_owned());
            if std::str::from_utf8(&bytes).is_err() {
                out = out.warn("Decoded bytes are not valid UTF-8; invalid sequences were replaced. Choose 'Binary file' output to keep the raw bytes.");
            }
            return Ok(out);
        }
        let bytes = match p.opt_file("file") {
            Some(f) => read_file(f)?,
            None => p.text("text").as_bytes().to_vec(),
        };
        if mode == "dump" {
            const MAX: usize = 1 << 20;
            let mut out = ToolOutput::text(hexdump(&bytes[..bytes.len().min(MAX)]));
            if bytes.len() > MAX {
                out = out.warn("Showing the first 1 MiB");
            }
            return Ok(out);
        }
        let up = p.bool("uppercase");
        let f = |b: &u8| if up { format!("{b:02X}") } else { format!("{b:02x}") };
        let parts: Vec<String> = bytes.iter().map(f).collect();
        let out = match p.str_or("separator", "space") {
            "none" => parts.concat(),
            "colon" => parts.join(":"),
            "comma" => parts.iter().map(|x| format!("0x{x}")).collect::<Vec<_>>().join(", "),
            "escape" => parts.iter().map(|x| format!("\\x{x}")).collect(),
            _ => parts.join(" "),
        };
        Ok(ToolOutput::text(out).with_data(json!({"bytes": bytes.len()})))
    })
}

// ---------------------------------------------------------------------------
// Number base

fn parse_big(s: &str, base: u32) -> Result<(BigInt, u32), String> {
    let t: String = s.trim().chars().filter(|c| *c != '_' && *c != ' ' && *c != ',').collect();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, t.trim_start_matches('+').to_string()),
    };
    let lower = t.to_ascii_lowercase();
    let (radix, digits) = if base == 0 {
        if let Some(r) = lower.strip_prefix("0x") {
            (16, r.to_string())
        } else if let Some(r) = lower.strip_prefix("0b") {
            (2, r.to_string())
        } else if let Some(r) = lower.strip_prefix("0o") {
            (8, r.to_string())
        } else if lower.chars().all(|c| c.is_ascii_digit()) {
            (10, lower)
        } else if lower.chars().all(|c| c.is_ascii_hexdigit()) {
            (16, lower)
        } else {
            return Err(format!("can't detect the base of '{s}'; choose one explicitly"));
        }
    } else {
        let stripped = match base {
            16 => lower.trim_start_matches("0x").to_string(),
            2 => lower.trim_start_matches("0b").to_string(),
            8 => lower.trim_start_matches("0o").to_string(),
            _ => lower,
        };
        (base, stripped)
    };
    if digits.is_empty() {
        return Err("no digits".into());
    }
    let v = BigInt::parse_bytes(digits.as_bytes(), radix).ok_or_else(|| format!("'{s}' is not a valid base-{radix} number"))?;
    Ok((if neg { -v } else { v }, radix))
}

fn twos_complement(v: &BigInt, bits: u32) -> Option<String> {
    let modulus = BigInt::from(1u8) << bits;
    let half = &modulus >> 1u32;
    if v < &(-half) || v >= &modulus {
        return None;
    }
    let u = if v.is_negative() { &modulus + v } else { v.clone() };
    let s = u.to_str_radix(2);
    Some(format!("{}{}", "0".repeat((bits as usize).saturating_sub(s.len())), s))
}

fn group(s: &str, n: usize, sep: char) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % n == 0 {
            out.push(sep);
        }
        out.push(*c);
    }
    out
}

fn number_base() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.number_base", "Number Base Converter", Category::Dev)
        .describe("Convert integers of any size between binary, octal, decimal, hexadecimal, base36 or any base 2–36, with two's complement views")
        .keywords(&["binary", "hex", "octal", "decimal", "base converter", "radix", "base36", "two's complement", "bin to dec", "hex to dec"])
        .param(ParamSpec::text("value").label("Number").required().primary().placeholder("0xff, 0b1010, 255"))
        .param(ParamSpec::integer("from").label("Input base (0 = auto from prefix)").default_int(0).range(0.0, 36.0))
        .param(ParamSpec::integer("to").label("Extra output base").default_int(36).range(2.0, 36.0).advanced())
        .param(ParamSpec::choice("bits", &[("8", "8-bit"), ("16", "16-bit"), ("32", "32-bit"), ("64", "64-bit"), ("128", "128-bit")]).label("Two's complement width").default_str("32").advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let from = p.int_or("from", 0) as u32;
        if from == 1 {
            return Err(ToolError::invalid("from", "base must be 0 (auto) or 2–36"));
        }
        let (v, radix) = parse_big(p.text("value"), from).map_err(|e| ToolError::invalid("value", e))?;
        let bits: u32 = p.str_or("bits", "32").parse().unwrap_or(32);
        let to = p.int_or("to", 36) as u32;
        let neg = v.is_negative();
        let mag = v.abs();
        let sign = if neg { "-" } else { "" };
        let bin = mag.to_str_radix(2);
        let mut data = json!({
            "input_base": radix,
            "decimal": v.to_str_radix(10),
            "hexadecimal": format!("{sign}0x{}", mag.to_str_radix(16)),
            "octal": format!("{sign}0o{}", mag.to_str_radix(8)),
            "binary": format!("{sign}0b{bin}"),
            "binary_grouped": format!("{sign}{}", group(&bin, 4, ' ')),
            "decimal_grouped": format!("{sign}{}", group(&mag.to_str_radix(10), 3, ',')),
            "base36": format!("{sign}{}", mag.to_str_radix(36)),
            format!("base{to}"): format!("{sign}{}", mag.to_str_radix(to)),
            "bit_length": if v.is_zero() { 0 } else { mag.bits() },
        });
        match twos_complement(&v, bits) {
            Some(tc) => {
                let hexw = (bits / 4) as usize;
                let u = BigInt::parse_bytes(tc.as_bytes(), 2).unwrap_or_default();
                let modulus = BigInt::from(1u8) << bits;
                let signed = if tc.starts_with('1') { &u - &modulus } else { u.clone() };
                data[format!("twos_complement_{bits}bit")] = json!(group(&tc, 4, ' '));
                data[format!("twos_complement_{bits}bit_hex")] = json!(format!("0x{:0>hexw$}", u.to_str_radix(16)));
                data[format!("as_signed_{bits}bit")] = json!(signed.to_str_radix(10));
                data[format!("as_unsigned_{bits}bit")] = json!(u.to_str_radix(10));
            }
            None => {
                data[format!("twos_complement_{bits}bit")] = json!(format!("does not fit in {bits} bits"));
            }
        }
        Ok(ToolOutput::data(data))
    })
}

// ---------------------------------------------------------------------------
// Binary text

fn binary_text() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.binary_text", "Text ↔ Binary", Category::Dev)
        .describe("Convert text to its UTF-8 bytes written in binary, octal, decimal or hex — and back")
        .keywords(&["binary", "text to binary", "binary to text", "ascii", "bytes", "01010", "octal", "decimal bytes"])
        .param(ParamSpec::textarea("text").label("Text / numbers").required().primary())
        .param(ParamSpec::choice("mode", &[("encode", "Text → numbers"), ("decode", "Numbers → text")]).default_str("encode"))
        .param(ParamSpec::choice("format", &[("bin", "Binary"), ("oct", "Octal"), ("dec", "Decimal"), ("hex", "Hex")]).default_str("bin"))
        .param(ParamSpec::text("separator").label("Separator").default_str(" ").show_if("mode", &["encode"]))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let fmt = p.str("format")?;
        let t = p.text("text");
        if p.str("mode")? == "encode" {
            let parts: Vec<String> = t
                .bytes()
                .map(|b| match fmt {
                    "oct" => format!("{b:03o}"),
                    "dec" => b.to_string(),
                    "hex" => format!("{b:02x}"),
                    _ => format!("{b:08b}"),
                })
                .collect();
            return Ok(ToolOutput::text(parts.join(p.text("separator"))));
        }
        let radix = match fmt {
            "oct" => 8,
            "dec" => 10,
            "hex" => 16,
            _ => 2,
        };
        let tokens: Vec<String> = if fmt == "bin" && !t.trim().contains(char::is_whitespace) && !t.contains(',') {
            let digits: Vec<char> = t.trim().chars().collect();
            if digits.len() % 8 != 0 {
                return Err(ToolError::invalid("text", "binary without separators must be a multiple of 8 digits"));
            }
            digits.chunks(8).map(|c| c.iter().collect()).collect()
        } else {
            t.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
        };
        let bytes: Vec<u8> = tokens
            .iter()
            .map(|tok| {
                let tok = tok.trim_start_matches("0x").trim_start_matches("0b");
                u8::from_str_radix(tok, radix).map_err(|_| ToolError::invalid("text", format!("'{tok}' is not a byte value in base {radix}")))
            })
            .collect::<ToolResult<_>>()?;
        let mut out = ToolOutput::text(String::from_utf8_lossy(&bytes).into_owned());
        if std::str::from_utf8(&bytes).is_err() {
            out = out.warn("The bytes are not valid UTF-8; invalid sequences were replaced with �");
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    async fn txt(h: &TestHarness, id: &str, p: serde_json::Value) -> String {
        h.run(id, p).await.unwrap().text.unwrap()
    }

    #[tokio::test]
    async fn base64() {
        let h = TestHarness::new(crate::tools());
        assert_eq!(txt(&h, "dev.base64", json!({"text": "hello?>"})).await, "aGVsbG8/Pg==");
        assert_eq!(txt(&h, "dev.base64", json!({"text": "hello?>", "url_safe": true, "no_padding": true})).await, "aGVsbG8_Pg");
        assert_eq!(txt(&h, "dev.base64", json!({"text": "aGVsbG8_Pg", "mode": "decode"})).await, "hello?>");
        assert_eq!(txt(&h, "dev.base64", json!({"text": "aGVs\nbG8/Pg==", "mode": "decode"})).await, "hello?>");
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        let f = h.write("x.png", png);
        let enc = txt(&h, "dev.base64", json!({"file": f.to_str().unwrap(), "data_uri": true})).await;
        assert!(enc.starts_with("data:image/png;base64,"));
        let out = h.run("dev.base64", json!({"text": enc, "mode": "decode"})).await.unwrap();
        assert!(out.artifacts[0].name.ends_with(".png"));
        assert_eq!(std::fs::read(&out.artifacts[0].path).unwrap(), png);
        assert_eq!(h.run("dev.base64", json!({"text": "@@@", "mode": "decode"})).await.unwrap_err().code(), "invalid_input");
    }

    #[tokio::test]
    async fn url_html_hex() {
        let h = TestHarness::new(crate::tools());
        assert_eq!(txt(&h, "dev.url_encode", json!({"text": "a b&c/d"})).await, "a%20b%26c%2Fd");
        assert_eq!(txt(&h, "dev.url_encode", json!({"text": "https://x.y/a b?q=1&r=ü", "scope": "full"})).await, "https://x.y/a%20b?q=1&r=%C3%BC");
        assert_eq!(txt(&h, "dev.url_encode", json!({"text": "a+b%20c", "mode": "decode", "plus_spaces": true})).await, "a b c");
        assert_eq!(h.run("dev.url_encode", json!({"text": "%E0%A4%A", "mode": "decode"})).await.unwrap_err().code(), "invalid_input");
        assert_eq!(txt(&h, "dev.html_entities", json!({"text": "<é>", "non_ascii": true})).await, "&lt;&#xE9;&gt;");
        assert_eq!(txt(&h, "dev.html_entities", json!({"text": "&copy;&nbsp;&#169;", "mode": "decode"})).await, "©\u{a0}©");
        assert_eq!(txt(&h, "dev.hex", json!({"text": "Hi!", "separator": "colon", "uppercase": true})).await, "48:69:21");
        assert_eq!(txt(&h, "dev.hex", json!({"text": "0x48 0x69", "mode": "decode"})).await, "Hi");
        assert_eq!(txt(&h, "dev.hex", json!({"text": "48:69", "mode": "decode"})).await, "Hi");
        assert!(txt(&h, "dev.hex", json!({"text": "ABC", "mode": "dump"})).await.starts_with("00000000  41 42 43"));
        assert_eq!(h.run("dev.hex", json!({"text": "zz", "mode": "decode"})).await.unwrap_err().code(), "invalid_input");
    }

    #[tokio::test]
    async fn bases() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("dev.number_base", json!({"value": "0xff"})).await.unwrap();
        assert_eq!(out.data["decimal"], "255");
        assert_eq!(out.data["binary"], "0b11111111");
        let out = h.run("dev.number_base", json!({"value": "-1", "bits": "8"})).await.unwrap();
        assert_eq!(out.data["twos_complement_8bit"], "1111 1111");
        let out = h.run("dev.number_base", json!({"value": "123456789012345678901234567890", "from": 10})).await.unwrap();
        assert_eq!(out.data["hexadecimal"], "0x18ee90ff6c373e0ee4e3f0ad2");
        let out = h.run("dev.number_base", json!({"value": "zz", "from": 36})).await.unwrap();
        assert_eq!(out.data["decimal"], "1295");
        assert_eq!(h.run("dev.number_base", json!({"value": "12x", "from": 10})).await.unwrap_err().code(), "invalid_input");
        assert_eq!(txt(&h, "dev.binary_text", json!({"text": "Hi"})).await, "01001000 01101001");
        assert_eq!(txt(&h, "dev.binary_text", json!({"text": "0100100001101001", "mode": "decode"})).await, "Hi");
        assert_eq!(txt(&h, "dev.binary_text", json!({"text": "72 105", "mode": "decode", "format": "dec"})).await, "Hi");
        assert_eq!(h.run("dev.binary_text", json!({"text": "300", "mode": "decode", "format": "dec"})).await.unwrap_err().code(), "invalid_input");
    }
}
