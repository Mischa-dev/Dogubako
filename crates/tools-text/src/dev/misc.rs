//! Color converter, HTTP message parser, user-agent parser and string inspector.

use crate::util::*;
use dogubako_core::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![color(), http_request(), user_agent(), string_inspect()]
}

// ---------------------------------------------------------------------------
// dev.color

fn r1(x: f32) -> f64 {
    ((x as f64) * 10.0).round() / 10.0
}
fn r3(x: f32) -> f64 {
    ((x as f64) * 1000.0).round() / 1000.0
}

fn rel_luminance(c: &csscolorparser::Color) -> f64 {
    let lin = |v: f32| {
        let v = v as f64;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn contrast(a: f64, b: f64) -> f64 {
    let (l, d) = if a > b { (a, b) } else { (b, a) };
    ((l + 0.05) / (d + 0.05) * 100.0).round() / 100.0
}

fn hex_of(c: &csscolorparser::Color) -> String {
    let [r, g, b, _] = c.to_rgba8();
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn nearest_name(c: &csscolorparser::Color) -> (String, f64) {
    let [l, a, b, _] = c.to_oklaba();
    let mut best = (String::new(), f64::MAX);
    for (name, rgb) in csscolorparser::NAMED_COLORS.entries() {
        let n = csscolorparser::Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255);
        let [l2, a2, b2, _] = n.to_oklaba();
        let d = (((l - l2).powi(2) + (a - a2).powi(2) + (b - b2).powi(2)) as f64).sqrt();
        if d < best.1 {
            best = (name.as_str().to_string(), d);
        }
    }
    best
}

fn color() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.color", "Color Converter", Category::Dev)
        .describe("Convert any CSS color (hex, rgb(), hsl(), hwb(), lab(), lch(), oklch(), named) to HEX, RGB, HSL, HSV, HWB, CMYK, Lab, LCH, OKLab and OKLCH, with WCAG contrast and nearest named color")
        .keywords(&["color", "colour", "hex to rgb", "rgb to hex", "hsl", "cmyk", "oklch", "lab", "css color", "contrast", "color picker", "design", "palette"])
        .param(ParamSpec::text("color").label("Color").required().primary().default_str("#3366ff").placeholder("#3366ff, rgb(51 102 255), tomato, oklch(0.6 0.2 260)"))
        .output(OutputKind::Data)
        .ui("color");
    sync_tool(spec, |_ctx, p| {
        let input = p.str("color")?;
        let c = csscolorparser::parse(input.trim()).map_err(|e| ToolError::invalid("color", format!("'{input}' is not a CSS color: {e}")))?;
        let [r, g, b, a8] = c.to_rgba8();
        let alpha = c.a;
        let hex = if a8 == 255 { hex_of(&c) } else { format!("{}{a8:02x}", hex_of(&c)) };
        let [h, s, l, _] = c.to_hsla();
        let [hv, sv, vv, _] = c.to_hsva();
        let [hw, ww, bw, _] = c.to_hwba();
        let k = 1.0 - c.r.max(c.g).max(c.b);
        let cmyk = if k >= 1.0 { [0.0, 0.0, 0.0, 100.0] } else { [(1.0 - c.r - k) / (1.0 - k) * 100.0, (1.0 - c.g - k) / (1.0 - k) * 100.0, (1.0 - c.b - k) / (1.0 - k) * 100.0, k * 100.0] };
        let [ll, la, lb, _] = c.to_laba();
        let [cl, cc, ch, _] = c.to_lcha();
        let [ol, oa, ob, _] = c.to_oklaba();
        let [ocl, occ, och, _] = c.to_oklcha();
        let nan0 = |x: f32| if x.is_nan() { 0.0 } else { x };
        let lum = rel_luminance(&c);
        let (near, dist) = nearest_name(&c);
        let a_suffix = if a8 == 255 { String::new() } else { format!(" / {}", r3(alpha)) };
        let comp = csscolorparser::Color::from_hsla((h + 180.0) % 360.0, s, l, 1.0);
        let tri1 = csscolorparser::Color::from_hsla((h + 120.0) % 360.0, s, l, 1.0);
        let tri2 = csscolorparser::Color::from_hsla((h + 240.0) % 360.0, s, l, 1.0);
        let lighter = csscolorparser::Color::from_hsla(h, s, (l + 0.2).min(1.0), 1.0);
        let darker = csscolorparser::Color::from_hsla(h, s, (l - 0.2).max(0.0), 1.0);
        Ok(ToolOutput::data(json!({
            "hex": hex,
            "rgb": if a8 == 255 { format!("rgb({r}, {g}, {b})") } else { format!("rgba({r}, {g}, {b}, {})", r3(alpha)) },
            "hsl": format!("hsl({} {}% {}%{a_suffix})", r1(nan0(h)), r1(s * 100.0), r1(l * 100.0)),
            "hsv": format!("hsv({} {}% {}%)", r1(nan0(hv)), r1(sv * 100.0), r1(vv * 100.0)),
            "hwb": format!("hwb({} {}% {}%{a_suffix})", r1(nan0(hw)), r1(ww * 100.0), r1(bw * 100.0)),
            "cmyk": format!("cmyk({}%, {}%, {}%, {}%)", r1(cmyk[0]), r1(cmyk[1]), r1(cmyk[2]), r1(cmyk[3])),
            "lab": format!("lab({} {} {}{a_suffix})", r1(ll), r1(la), r1(lb)),
            "lch": format!("lch({} {} {}{a_suffix})", r1(cl), r1(cc), r1(nan0(ch))),
            "oklab": format!("oklab({} {} {}{a_suffix})", r3(ol), r3(oa), r3(ob)),
            "oklch": format!("oklch({} {} {}{a_suffix})", r3(ocl), r3(occ), r1(nan0(och))),
            "rgb_float": [r3(c.r), r3(c.g), r3(c.b), r3(alpha)],
            "name": c.name(),
            "nearest_named_color": if dist < 1e-4 { near.clone() } else { format!("{near} (approx.)") },
            "relative_luminance": (lum * 10000.0).round() / 10000.0,
            "contrast_on_white": contrast(lum, 1.0),
            "contrast_on_black": contrast(lum, 0.0),
            "best_text_color": if contrast(lum, 1.0) >= contrast(lum, 0.0) { "#ffffff" } else { "#000000" },
            "colors": [
                {"hex": hex_of(&c), "name": "input"},
                {"hex": hex_of(&lighter), "name": "lighter"},
                {"hex": hex_of(&darker), "name": "darker"},
                {"hex": hex_of(&comp), "name": "complementary"},
                {"hex": hex_of(&tri1), "name": "triadic 1"},
                {"hex": hex_of(&tri2), "name": "triadic 2"},
            ],
        })))
    })
}

// ---------------------------------------------------------------------------
// dev.http_request

fn pretty_body(body: &str, ctype: &str) -> String {
    let ct = ctype.to_ascii_lowercase();
    if ct.contains("json") || body.trim_start().starts_with('{') || body.trim_start().starts_with('[') {
        if let Ok(v) = serde_json::from_str::<Value>(body) {
            return serde_json::to_string_pretty(&v).unwrap_or_default();
        }
    }
    if ct.contains("x-www-form-urlencoded") {
        return url::form_urlencoded::parse(body.as_bytes()).map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join("\n");
    }
    if ct.contains("xml") {
        if let Ok((x, _)) = super::formatters::reformat_xml(body, Some((b' ', 2))) {
            return x;
        }
    }
    body.to_string()
}

fn http_request() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.http_request", "HTTP Message Parser", Category::Dev)
        .describe("Parse a raw HTTP request or response: method, URL, query parameters, headers, cookies and a pretty-printed body; generate an equivalent curl command")
        .keywords(&["http", "request", "response", "headers", "curl", "cookies", "raw http", "parse request"])
        .param(ParamSpec::textarea("text").label("Raw HTTP message").required().primary().placeholder("GET /path?q=1 HTTP/1.1\nHost: example.com\n\n"))
        .param(ParamSpec::choice("scheme", &[("https", "https"), ("http", "http")]).label("Scheme for the curl command").default_str("https").advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let raw = p.text("text").trim_start();
        // Normalize line endings so pasted text with LF works too.
        let norm = raw.replace("\r\n", "\n").replace('\n', "\r\n");
        let (head, body) = match norm.find("\r\n\r\n") {
            Some(i) => (format!("{}\r\n\r\n", &norm[..i]), norm[i + 4..].replace("\r\n", "\n")),
            None => (format!("{}\r\n\r\n", norm.trim_end()), String::new()),
        };
        let mut headers_buf = [httparse::EMPTY_HEADER; 256];
        let collect = |hs: &[httparse::Header]| -> Vec<(String, String)> { hs.iter().map(|h| (h.name.to_string(), String::from_utf8_lossy(h.value).trim().to_string())).collect() };
        let header_rows = |hs: &[(String, String)]| -> Vec<Vec<Value>> { hs.iter().map(|(k, v)| vec![json!(k), json!(v)]).collect() };
        let get = |hs: &[(String, String)], name: &str| hs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone());
        if head.starts_with("HTTP/") {
            let mut resp = httparse::Response::new(&mut headers_buf);
            resp.parse(head.as_bytes()).map_err(|e| ToolError::invalid("text", format!("invalid HTTP response: {e}")))?;
            let hs = collect(resp.headers);
            let ctype = get(&hs, "content-type").unwrap_or_default();
            let cookies: Vec<Vec<Value>> = hs
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
                .map(|(_, v)| {
                    let mut parts = v.split(';');
                    let (n, val) = parts.next().unwrap_or("").split_once('=').unwrap_or(("", ""));
                    vec![json!(n.trim()), json!(val.trim()), json!(parts.map(str::trim).collect::<Vec<_>>().join("; "))]
                })
                .collect();
            return Ok(ToolOutput::data(json!({
                "type": "response",
                "version": format!("HTTP/1.{}", resp.version.unwrap_or(1)),
                "status": resp.code,
                "reason": resp.reason,
                "content_type": ctype,
                "body_bytes": body.len(),
                "body": pretty_body(&body, &ctype),
                "cookies": table(&["name", "value", "attributes"], cookies),
                "table": table(&["header", "value"], header_rows(&hs)),
            })));
        }
        let mut req = httparse::Request::new(&mut headers_buf);
        req.parse(head.as_bytes()).map_err(|e| ToolError::invalid("text", format!("invalid HTTP request: {e}")))?;
        let method = req.method.unwrap_or("GET").to_string();
        let target = req.path.unwrap_or("/").to_string();
        let hs = collect(req.headers);
        let host = get(&hs, "host");
        let url = if target.starts_with("http://") || target.starts_with("https://") {
            target.clone()
        } else {
            format!("{}://{}{}", p.str_or("scheme", "https"), host.clone().unwrap_or_else(|| "localhost".into()), target)
        };
        let parsed = url::Url::parse(&url).ok();
        let query: Vec<Vec<Value>> = parsed.as_ref().map(|u| u.query_pairs().map(|(k, v)| vec![json!(k), json!(v)]).collect()).unwrap_or_default();
        let cookies: Vec<Vec<Value>> = get(&hs, "cookie")
            .map(|c| c.split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| vec![json!(k.trim()), json!(v.trim())]).collect())
            .unwrap_or_default();
        let ctype = get(&hs, "content-type").unwrap_or_default();
        let q = crate::escape::shell_quote;
        let mut curl = format!("curl -X {} {}", method, q(&url));
        for (k, v) in &hs {
            if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("content-length") {
                continue;
            }
            curl.push_str(&format!(" \\\n  -H {}", q(&format!("{k}: {v}"))));
        }
        if !body.is_empty() {
            curl.push_str(&format!(" \\\n  --data-raw {}", q(&body)));
        }
        Ok(ToolOutput::data(json!({
            "type": "request",
            "method": method,
            "path": parsed.as_ref().map(|u| u.path().to_string()).unwrap_or(target.clone()),
            "url": url,
            "version": format!("HTTP/1.{}", req.version.unwrap_or(1)),
            "query": table(&["parameter", "value"], query),
            "cookies": table(&["name", "value"], cookies),
            "content_type": ctype,
            "body_bytes": body.len(),
            "body": pretty_body(&body, &ctype),
            "curl": curl,
            "table": table(&["header", "value"], header_rows(&hs)),
        })))
    })
}

// ---------------------------------------------------------------------------
// dev.user_agent

fn user_agent() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.user_agent", "User-Agent Parser", Category::Dev)
        .describe("Identify the browser, version, operating system, device type and bots/crawlers from User-Agent strings (one per line)")
        .keywords(&["user agent", "ua", "browser detection", "bot", "crawler", "useragent parser", "device"])
        .param(ParamSpec::textarea("text").label("User-Agent string(s)").required().primary())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let parser = woothee::parser::Parser::new();
        let mut rows = vec![];
        for line in p.text("text").lines().map(str::trim).filter(|l| !l.is_empty()).take(5000) {
            let r = parser.parse(line);
            let mut row = match &r {
                Some(r) => {
                    let device = match r.category {
                        "pc" => "desktop",
                        "smartphone" => "mobile",
                        "mobilephone" => "feature phone",
                        "crawler" => "bot",
                        "appliance" => "appliance / console",
                        "misc" => "other",
                        _ => "unknown",
                    };
                    let ver = |v: &str| if v == "UNKNOWN" { String::new() } else { v.to_string() };
                    vec![json!(r.name), json!(ver(r.version)), json!(r.os), json!(ver(&r.os_version)), json!(device), json!(r.vendor), json!(r.category == "crawler")]
                }
                None => vec![json!("UNKNOWN"), json!(""), json!("UNKNOWN"), json!(""), json!("unknown"), json!(""), json!(false)],
            };
            // Simple heuristics woothee may miss.
            let lower = line.to_ascii_lowercase();
            if row[6] == json!(false) && ["bot", "crawler", "spider", "curl/", "wget/", "python-requests", "headless", "httpclient", "go-http-client"].iter().any(|k| lower.contains(k)) {
                row[6] = json!(true);
            }
            if row[4] == json!("desktop") && (lower.contains("ipad") || lower.contains("tablet")) {
                row[4] = json!("tablet");
            }
            row.push(json!(line));
            rows.push(row);
        }
        if rows.is_empty() {
            return Err(ToolError::invalid("text", "enter at least one User-Agent string"));
        }
        let first = rows[0].clone();
        Ok(ToolOutput::data(json!({
            "browser": first[0],
            "version": first[1],
            "os": first[2],
            "os_version": first[3],
            "device": first[4],
            "is_bot": first[6],
            "table": table(&["browser", "version", "os", "os version", "device", "vendor", "bot", "user agent"], rows),
        })))
    })
}

// ---------------------------------------------------------------------------
// dev.string_inspect

fn string_inspect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.string_inspect", "String Inspector", Category::Dev)
        .describe("Inspect a string: length in characters, graphemes, UTF-16 units and bytes, character classes, hidden/invisible characters, line endings and whitespace issues")
        .keywords(&["string length", "byte length", "utf-16 length", "inspect", "invisible characters", "whitespace", "debug string"])
        .param(ParamSpec::textarea("text").label("Text").required().primary())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text");
        let count = |f: &dyn Fn(char) -> bool| t.chars().filter(|c| f(*c)).count();
        let invisible: Vec<String> = t
            .chars()
            .filter(|c| {
                let cat = crate::text::more::category_code(*c);
                cat.starts_with("Cf") || (cat.starts_with("Cc") && !matches!(c, '\n' | '\r' | '\t')) || (cat.starts_with('Z') && *c != ' ')
            })
            .map(|c| format!("U+{:04X} {}", c as u32, crate::text::more::char_name(c)))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let scripts: std::collections::BTreeSet<&str> = t
            .chars()
            .filter(|c| c.is_alphabetic())
            .map(|c| match c as u32 {
                0..=0x24F => "Latin",
                0x370..=0x3FF => "Greek",
                0x400..=0x52F => "Cyrillic",
                0x590..=0x5FF => "Hebrew",
                0x600..=0x6FF => "Arabic",
                0x900..=0x97F => "Devanagari",
                0x3040..=0x30FF => "Japanese kana",
                0x4E00..=0x9FFF => "CJK",
                0xAC00..=0xD7AF => "Hangul",
                _ => "Other",
            })
            .collect();
        let lines = split_lines(t).0;
        Ok(ToolOutput::data(json!({
            "characters": t.chars().count(),
            "graphemes": t.graphemes(true).count(),
            "utf16_units": t.encode_utf16().count(),
            "bytes_utf8": t.len(),
            "lines": lines.len(),
            "letters": count(&|c| c.is_alphabetic()),
            "uppercase": count(&|c| c.is_uppercase()),
            "lowercase": count(&|c| c.is_lowercase()),
            "digits": count(&|c| c.is_numeric()),
            "whitespace": count(&|c| c.is_whitespace()),
            "punctuation_symbols": count(&|c| !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control()),
            "non_ascii": count(&|c| !c.is_ascii()),
            "scripts": scripts,
            "mixed_scripts": scripts.len() > 1,
            "leading_whitespace": t.starts_with(char::is_whitespace),
            "trailing_whitespace": t.ends_with(char::is_whitespace),
            "lines_with_trailing_whitespace": lines.iter().filter(|l| l.ends_with([' ', '\t'])).count(),
            "has_bom": t.starts_with('\u{feff}'),
            "line_endings": {"crlf": t.matches("\r\n").count(), "lf": t.matches('\n').count() - t.matches("\r\n").count(), "cr": t.matches('\r').count() - t.matches("\r\n").count()},
            "invisible_characters": invisible,
            "is_ascii": t.is_ascii(),
        })))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    #[tokio::test]
    async fn colors() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("dev.color", json!({"color": "tomato"})).await.unwrap();
        assert_eq!(out.data["hex"], "#ff6347");
        assert_eq!(out.data["rgb"], "rgb(255, 99, 71)");
        assert_eq!(out.data["name"], "tomato");
        assert_eq!(out.data["colors"][0]["hex"], "#ff6347");
        let out = h.run("dev.color", json!({"color": "hsl(120 100% 25%)"})).await.unwrap();
        assert_eq!(out.data["hex"], "#008000");
        assert_eq!(out.data["cmyk"], "cmyk(100%, 0%, 100%, 49.8%)");
        let out = h.run("dev.color", json!({"color": "#000"})).await.unwrap();
        assert_eq!(out.data["contrast_on_white"], 21.0);
        let out = h.run("dev.color", json!({"color": "#3366ff80"})).await.unwrap();
        assert_eq!(out.data["hex"], "#3366ff80");
        assert_eq!(h.run("dev.color", json!({"color": "notacolor"})).await.unwrap_err().code(), "invalid_input");
    }

    #[tokio::test]
    async fn http_and_ua() {
        let h = TestHarness::new(crate::tools());
        let req = "POST /api/items?id=5&x=a%20b HTTP/1.1\nHost: example.com\nContent-Type: application/json\nCookie: a=1; b=2\n\n{\"name\":\"it's\"}";
        let out = h.run("dev.http_request", json!({"text": req})).await.unwrap();
        assert_eq!(out.data["method"], "POST");
        assert_eq!(out.data["url"], "https://example.com/api/items?id=5&x=a%20b");
        assert_eq!(out.data["query"]["rows"][1], json!(["x", "a b"]));
        assert_eq!(out.data["cookies"]["rows"][1], json!(["b", "2"]));
        assert!(out.data["body"].as_str().unwrap().contains("\"name\": \"it's\""));
        let curl = out.data["curl"].as_str().unwrap();
        assert!(curl.starts_with("curl -X POST 'https://example.com/api/items?id=5&x=a%20b'") && curl.contains("--data-raw '{\"name\":\"it'\\''s\"}'"), "{curl}");
        let out = h.run("dev.http_request", json!({"text": "HTTP/1.1 404 Not Found\nSet-Cookie: s=abc; Path=/; HttpOnly\n\n"})).await.unwrap();
        assert_eq!(out.data["status"], 404);
        assert_eq!(out.data["cookies"]["rows"][0][0], "s");
        assert_eq!(h.run("dev.http_request", json!({"text": "NOT HTTP AT ALL\x01"})).await.unwrap_err().code(), "invalid_input");
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36\nGooglebot/2.1 (+http://www.google.com/bot.html)";
        let out = h.run("dev.user_agent", json!({"text": ua})).await.unwrap();
        assert_eq!(out.data["browser"], "Chrome");
        assert_eq!(out.data["os"], "Windows 10");
        assert_eq!(out.data["device"], "desktop");
        assert_eq!(out.data["table"]["rows"][1][6], true);
        let out = h.run("dev.string_inspect", json!({"text": "Héllo\u{200B} wörld \r\n"})).await.unwrap();
        assert_eq!(out.data["characters"], 15);
        assert_eq!(out.data["bytes_utf8"], 19);
        assert_eq!(out.data["invisible_characters"][0], "U+200B ZERO WIDTH SPACE");
        assert_eq!(out.data["trailing_whitespace"], true);
    }
}
