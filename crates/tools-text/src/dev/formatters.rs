//! Formatters / validators: JSON, YAML, TOML, XML, SQL and format conversion.

use crate::util::*;
use dogubako_core::prelude::*;
use quick_xml::events::{BytesText, Event};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        json_tool(),
        yaml_tool(),
        toml_tool(),
        xml_tool(),
        sql_format(),
        convert(),
    ]
}

fn indent_param() -> ParamSpec {
    ParamSpec::choice(
        "indent",
        &[("2", "2 spaces"), ("4", "4 spaces"), ("tab", "Tab")],
    )
    .default_str("2")
}

fn indent_str(p: &Params) -> String {
    match p.str_or("indent", "2") {
        "4" => "    ".into(),
        "tab" => "\t".into(),
        _ => "  ".into(),
    }
}

// ---------------------------------------------------------------------------
// JSON

pub(crate) fn parse_json(text: &str, param: &str) -> ToolResult<Value> {
    let v = parse_json_raw(text, param)?;
    check_json_numbers(text).map_err(|m| ToolError::invalid(param, m))?;
    Ok(v)
}

/// serde_json (without arbitrary precision) stores numbers as i64, u64 or
/// f64, so a literal such as 18446744073709551617 or
/// 0.10000000000000000001 would come back changed. Reject those instead of
/// silently altering data. `text` must already be valid JSON.
pub(crate) fn check_json_numbers(text: &str) -> Result<(), String> {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while i < b.len() && matches!(b[i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
                {
                    i += 1;
                }
                let tok = &text[start..i];
                let exact = if tok.bytes().all(|c| c.is_ascii_digit() || c == b'-') {
                    tok.parse::<i64>().is_ok() || tok.parse::<u64>().is_ok()
                } else {
                    tok.parse::<f64>().is_ok_and(|f| {
                        f.is_finite()
                            && crate::util::compare_numbers(tok, &f.to_string())
                                == Some(std::cmp::Ordering::Equal)
                    })
                };
                if !exact {
                    let shown: String = tok.chars().take(40).collect();
                    return Err(format!(
                        "the number {shown} cannot be represented exactly (64-bit integers and double-precision floats only); quote it as a string to keep it unchanged"
                    ));
                }
            }
            _ => i += 1,
        }
    }
    Ok(())
}

fn parse_json_raw(text: &str, param: &str) -> ToolResult<Value> {
    serde_json::from_str(text).map_err(|e| {
        let line = e.line();
        let col = e.column();
        let snippet = text.lines().nth(line.saturating_sub(1)).unwrap_or("");
        let snippet: String = snippet.chars().take(200).collect();
        ToolError::invalid(
            param,
            format!(
                "{e}\n  {line} | {snippet}\n  {}^",
                " ".repeat(line.to_string().len() + col.saturating_sub(1) + 2)
            ),
        )
    })
}

pub(crate) fn sort_keys(v: &mut Value) {
    match v {
        Value::Object(m) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(m).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, mut val) in entries {
                sort_keys(&mut val);
                m.insert(k, val);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(sort_keys),
        _ => {}
    }
}

pub(crate) fn to_json_indent(v: &Value, indent: &str) -> String {
    use serde::Serialize;
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    v.serialize(&mut ser).ok();
    String::from_utf8(buf).unwrap_or_default()
}

fn json_stats(v: &Value) -> (usize, usize) {
    // (depth, total values)
    match v {
        Value::Object(m) => {
            let (d, n) = m
                .values()
                .map(json_stats)
                .fold((0, 0), |a, b| (a.0.max(b.0), a.1 + b.1));
            (d + 1, n + 1)
        }
        Value::Array(a) => {
            let (d, n) = a
                .iter()
                .map(json_stats)
                .fold((0, 0), |a, b| (a.0.max(b.0), a.1 + b.1));
            (d + 1, n + 1)
        }
        _ => (0, 1),
    }
}

fn json_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.json", "JSON Formatter & Validator", Category::Dev)
        .describe("Format (pretty-print), minify, validate and sort the keys of JSON, with precise error line and column")
        .keywords(&["json", "pretty print", "beautify", "minify", "validate", "lint", "sort keys", "formatter"])
        .param(ParamSpec::textarea("text").label("JSON").required().primary().syntax("json"))
        .param(ParamSpec::choice("mode", &[("format", "Format"), ("minify", "Minify"), ("validate", "Validate only")]).default_str("format"))
        .param(indent_param().show_if("mode", &["format"]))
        .param(ParamSpec::boolean("sort_keys").label("Sort object keys"))
        .param(ParamSpec::boolean("ascii").label("Escape non-ASCII characters").advanced())
        .text_output("application/json")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let text = p.text("text");
        if text.trim().is_empty() {
            return Err(ToolError::invalid("text", "input is empty"));
        }
        let mut v = parse_json(text, "text")?;
        if p.bool("sort_keys") {
            sort_keys(&mut v);
        }
        let (depth, values) = json_stats(&v);
        let kind = match &v {
            Value::Object(m) => format!("object with {} keys", m.len()),
            Value::Array(a) => format!("array with {} items", a.len()),
            Value::String(_) => "string".into(),
            Value::Number(_) => "number".into(),
            Value::Bool(_) => "boolean".into(),
            Value::Null => "null".into(),
        };
        let data = json!({"valid": true, "root": kind, "depth": depth, "values": values});
        let mut out = match p.str("mode")? {
            "validate" => return Ok(ToolOutput::data(data)),
            "minify" => serde_json::to_string(&v)?,
            _ => to_json_indent(&v, &indent_str(p)),
        };
        if p.bool("ascii") {
            out = out
                .chars()
                .map(|c| {
                    if c.is_ascii() {
                        c.to_string()
                    } else {
                        crate::escape::backslash_escape(&c.to_string(), crate::escape::Lang::Java)
                    }
                })
                .collect();
        }
        Ok(ToolOutput::text_as(out, "application/json").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// YAML

pub(crate) fn parse_yaml_docs(text: &str, param: &str) -> ToolResult<Vec<Value>> {
    parse_yaml_native(text, param)?
        .into_iter()
        .map(|v| yaml_to_json(v).map_err(|m| ToolError::invalid(param, m)))
        .collect()
}

/// YAML documents as YAML values (keys keep their types: `1` and `"1"` stay
/// distinct), for formatting without a lossy trip through JSON.
fn parse_yaml_native(text: &str, param: &str) -> ToolResult<Vec<serde_yaml_ng::Value>> {
    use serde::Deserialize;
    serde_yaml_ng::Deserializer::from_str(text)
        .map(|de| serde_yaml_ng::Value::deserialize(de).map_err(|e| yaml_err(e, param)))
        .collect()
}

/// Sort mapping keys recursively (by their YAML text).
fn sort_yaml_keys(v: &mut serde_yaml_ng::Value) {
    use serde_yaml_ng::Value as Y;
    match v {
        Y::Mapping(m) => {
            let mut entries: Vec<(Y, Y)> = std::mem::take(m).into_iter().collect();
            for (_, val) in entries.iter_mut() {
                sort_yaml_keys(val);
            }
            entries.sort_by_cached_key(|(k, _)| serde_yaml_ng::to_string(k).unwrap_or_default());
            *m = entries.into_iter().collect();
        }
        Y::Sequence(s) => s.iter_mut().for_each(sort_yaml_keys),
        Y::Tagged(t) => sort_yaml_keys(&mut t.value),
        _ => {}
    }
}

fn yaml_err(e: serde_yaml_ng::Error, param: &str) -> ToolError {
    match e.location() {
        Some(l) => ToolError::invalid(
            param,
            format!("{e} (line {}, column {})", l.line(), l.column()),
        ),
        None => ToolError::invalid(param, e.to_string()),
    }
}

fn yaml_to_json(v: serde_yaml_ng::Value) -> Result<Value, String> {
    use serde_yaml_ng::Value as Y;
    Ok(match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                json!(i)
            } else if let Some(u) = n.as_u64() {
                json!(u)
            } else {
                let f = n.as_f64().unwrap_or(f64::NAN);
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or_else(|| json!(f.to_string()))
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(s.into_iter().map(yaml_to_json).collect::<Result<_, _>>()?),
        Y::Mapping(m) => {
            let mut o = serde_json::Map::new();
            for (k, v) in m {
                let key = match k {
                    Y::String(s) => s,
                    Y::Number(n) => n.to_string(),
                    Y::Bool(b) => b.to_string(),
                    Y::Null => "null".into(),
                    other => serde_yaml_ng::to_string(&other)
                        .map_err(|e| e.to_string())?
                        .trim()
                        .to_string(),
                };
                // `1` and `"1"` are distinct YAML keys but the same JSON key.
                if o.contains_key(&key) {
                    return Err(format!(
                        "YAML keys that differ only in type (e.g. 1 and \"1\") both become the JSON key \"{key}\""
                    ));
                }
                o.insert(key, yaml_to_json(v)?);
            }
            Value::Object(o)
        }
        Y::Tagged(t) => yaml_to_json(t.value)?,
    })
}

pub(crate) fn to_yaml(v: &Value) -> ToolResult<String> {
    serde_yaml_ng::to_string(v).ctx("serializing YAML")
}

fn yaml_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.yaml", "YAML Formatter & Validator", Category::Dev)
        .describe("Validate and re-format YAML (multi-document aware), with error line and column")
        .keywords(&[
            "yaml",
            "yml",
            "validate",
            "lint",
            "format",
            "pretty print",
            "kubernetes",
            "docker compose",
        ])
        .param(
            ParamSpec::textarea("text")
                .label("YAML")
                .required()
                .primary()
                .syntax("yaml"),
        )
        .param(
            ParamSpec::choice(
                "mode",
                &[("format", "Format"), ("validate", "Validate only")],
            )
            .default_str("format"),
        )
        .param(ParamSpec::boolean("sort_keys").label("Sort mapping keys"))
        .text_output("application/yaml")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        // Formatted as YAML values, never via JSON (which would merge keys
        // like 1 and "1" and drop tags).
        let mut docs = parse_yaml_native(p.text("text"), "text")?;
        if p.bool("sort_keys") {
            docs.iter_mut().for_each(sort_yaml_keys);
        }
        let data = json!({"valid": true, "documents": docs.len()});
        if p.str("mode")? == "validate" {
            return Ok(ToolOutput::data(data));
        }
        let parts: Vec<String> = docs
            .iter()
            .map(|d| serde_yaml_ng::to_string(d).ctx("serializing YAML"))
            .collect::<ToolResult<_>>()?;
        Ok(ToolOutput::text_as(parts.join("---\n"), "application/yaml").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// TOML

pub(crate) fn parse_toml(text: &str, param: &str) -> ToolResult<Value> {
    toml_to_json(toml::Value::Table(parse_toml_native(text, param)?), "")
        .map_err(|m| ToolError::invalid(param, m))
}

/// TOML -> JSON for conversions: datetimes become strings; non-finite floats
/// (nan, inf), which JSON cannot represent, are rejected with their path.
fn toml_to_json(v: toml::Value, path: &str) -> Result<Value, String> {
    use toml::Value as T;
    Ok(match v {
        T::String(s) => Value::String(s),
        T::Integer(i) => json!(i),
        T::Float(f) => serde_json::Number::from_f64(f)
            .map(Value::Number)
            .ok_or_else(|| {
                format!(
                    "'{}' is {f}, which JSON cannot represent",
                    if path.is_empty() { "value" } else { path }
                )
            })?,
        T::Boolean(b) => Value::Bool(b),
        T::Datetime(d) => Value::String(d.to_string()),
        T::Array(a) => Value::Array(
            a.into_iter()
                .enumerate()
                .map(|(i, x)| toml_to_json(x, &format!("{path}[{i}]")))
                .collect::<Result<_, _>>()?,
        ),
        T::Table(t) => Value::Object(
            t.into_iter()
                .map(|(k, x)| {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    toml_to_json(x, &p).map(|j| (k, j))
                })
                .collect::<Result<_, _>>()?,
        ),
    })
}

fn parse_toml_native(text: &str, param: &str) -> ToolResult<toml::Table> {
    toml::from_str(text).map_err(|e| {
        let loc = e.span().map(|s| line_col(text, s.start));
        match loc {
            Some((l, c)) => {
                ToolError::invalid(param, format!("{} (line {l}, column {c})", e.message()))
            }
            None => ToolError::invalid(param, e.message().to_string()),
        }
    })
}

pub(crate) fn to_toml(v: &Value) -> ToolResult<String> {
    if !v.is_object() {
        return Err(ToolError::invalid(
            "text",
            "TOML requires a table (object) at the top level",
        ));
    }
    fn has_null(v: &Value) -> bool {
        match v {
            Value::Null => true,
            Value::Array(a) => a.iter().any(has_null),
            Value::Object(m) => m.values().any(has_null),
            _ => false,
        }
    }
    if has_null(v) {
        return Err(ToolError::invalid(
            "text",
            "TOML has no null value; remove or replace null values first",
        ));
    }
    toml::to_string_pretty(v)
        .map_err(|e| ToolError::invalid("text", format!("cannot represent as TOML: {e}")))
}

fn toml_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.toml", "TOML Formatter & Validator", Category::Dev)
        .describe("Validate and re-format TOML with error line and column")
        .keywords(&[
            "toml",
            "cargo.toml",
            "pyproject",
            "config",
            "validate",
            "format",
            "lint",
        ])
        .param(
            ParamSpec::textarea("text")
                .label("TOML")
                .required()
                .primary()
                .syntax("toml"),
        )
        .param(
            ParamSpec::choice(
                "mode",
                &[("format", "Format"), ("validate", "Validate only")],
            )
            .default_str("format"),
        )
        .text_output("application/toml")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        // Formatted as TOML values, never via JSON (nan/inf and datetimes
        // have no JSON equivalent).
        let t = parse_toml_native(p.text("text"), "text")?;
        let data = json!({"valid": true, "top_level_keys": t.len()});
        if p.str("mode")? == "validate" {
            return Ok(ToolOutput::data(data));
        }
        let out = toml::to_string_pretty(&t).ctx("serializing TOML")?;
        Ok(ToolOutput::text_as(out, "application/toml").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// XML

/// Re-serialize XML. `indent` None = minify. Never resolves external entities
/// (quick-xml does not process DTDs).
pub(crate) fn reformat_xml(text: &str, indent: Option<(u8, usize)>) -> ToolResult<(String, usize)> {
    let verbatim = mixed_content_elements(text);
    let mut reader = quick_xml::Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut writer = match indent {
        Some((c, n)) => quick_xml::Writer::new_with_indent(Vec::new(), c, n),
        None => quick_xml::Writer::new(Vec::new()),
    };
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut elements = 0usize;
    // Mixed-content / xml:space="preserve" elements are copied unchanged
    // (no whitespace dropped or indentation added) via a plain sub-writer.
    let mut start_index = 0usize;
    let mut sub: Option<(quick_xml::Writer<Vec<u8>>, usize)> = None;
    let err = |reader: &quick_xml::Reader<&[u8]>, e: String| {
        let (l, c) = line_col(text, reader.buffer_position() as usize);
        ToolError::invalid("text", format!("{e} (line {l}, column {c})"))
    };
    loop {
        let ev = reader
            .read_event()
            .map_err(|e| err(&reader, e.to_string()))?;
        match &ev {
            Event::Start(e) | Event::Empty(e) if e.attributes().any(|a| a.is_err()) => {
                // The event reader does not validate attributes itself
                // (duplicates, missing quotes); report the first error.
                let bad = e.attributes().find_map(|a| a.err()).unwrap();
                return Err(err(&reader, format!("invalid attribute: {bad}")));
            }
            Event::Start(_) => {
                if depth == 0 {
                    roots += 1;
                }
                depth += 1;
                elements += 1;
                if sub.is_none() && verbatim.contains(&start_index) {
                    sub = Some((quick_xml::Writer::new(Vec::new()), depth));
                }
                start_index += 1;
            }
            Event::Empty(_) => {
                if depth == 0 {
                    roots += 1;
                }
                elements += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Text(t) => {
                if t.trim().is_empty() && sub.is_none() {
                    continue;
                }
                if depth == 0 {
                    return Err(err(&reader, "text outside of the root element".into()));
                }
            }
            // CDATA and character/entity references are character data too.
            Event::CData(_) | Event::GeneralRef(_) if depth == 0 => {
                return Err(err(&reader, "text outside of the root element".into()));
            }
            Event::Eof => break,
            _ => {}
        }
        if roots > 1 {
            return Err(err(&reader, "more than one root element".into()));
        }
        let ends_here = matches!(ev, Event::End(_));
        let ev = match ev {
            Event::GeneralRef(r) => Event::Text(BytesText::from_escaped(format!("&{};", &*r))),
            e => e,
        };
        if let Some((w, at)) = &mut sub {
            w.write_event(ev).ctx("writing XML")?;
            if ends_here && depth + 1 == *at {
                let (w, _) = sub.take().unwrap();
                // Placed like an element (newline + indent), content untouched.
                if indent.is_some() && !writer.get_ref().is_empty() {
                    writer.write_indent().ctx("writing XML")?;
                }
                use std::io::Write as _;
                writer
                    .get_mut()
                    .write_all(&w.into_inner())
                    .ctx("writing XML")?;
            }
            continue;
        }
        writer.write_event(ev).ctx("writing XML")?;
    }
    if depth > 0 {
        return Err(ToolError::invalid(
            "text",
            format!("unexpected end of document: {depth} element(s) not closed"),
        ));
    }
    if roots == 0 {
        return Err(ToolError::invalid("text", "no root element found"));
    }
    Ok((
        String::from_utf8(writer.into_inner()).ctx("XML output")?,
        elements,
    ))
}

/// Indices (in start-tag order) of elements to copy unchanged: mixed content
/// (child elements plus text, including whitespace between children that is
/// not just line-break indentation, as in `<b>Hello</b> <i>world</i>`) and
/// `xml:space="preserve"`. Text-only elements are handled by the writer.
/// Errors are left to the main pass.
fn mixed_content_elements(text: &str) -> std::collections::HashSet<usize> {
    struct Open {
        id: usize,
        preserve: bool,
        child: bool,
        text: bool,
        /// Any text at all, whitespace included.
        any_text: bool,
    }
    let mut reader = quick_xml::Reader::from_str(text);
    let mut out = std::collections::HashSet::new();
    let mut stack: Vec<Open> = vec![];
    let mut next = 0usize;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if let Some(parent) = stack.last_mut() {
                    parent.child = true;
                }
                let inherited = stack.last().map(|s| s.preserve).unwrap_or(false);
                let preserve = match e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.into_inner() == "xml:space")
                {
                    Some(a) => &*a.value == "preserve",
                    None => inherited,
                };
                stack.push(Open {
                    id: next,
                    preserve,
                    child: false,
                    text: false,
                    any_text: false,
                });
                next += 1;
            }
            Ok(Event::Empty(_)) => {
                if let Some(parent) = stack.last_mut() {
                    parent.child = true;
                }
            }
            Ok(Event::End(_)) => {
                if let Some(o) = stack.pop() {
                    // Leaf text is data even when it is only whitespace
                    // (<value> </value>); only element content is indentation.
                    if o.preserve || (o.child && o.text) || (!o.child && o.any_text) {
                        out.insert(o.id);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(o) = stack.last_mut() {
                    let s: &str = t.as_ref();
                    o.any_text = true;
                    if !s.trim().is_empty() || !s.contains(['\n', '\r']) {
                        o.text = true;
                    }
                }
            }
            Ok(Event::CData(_)) | Ok(Event::GeneralRef(_)) => {
                if let Some(o) = stack.last_mut() {
                    o.text = true;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn xml_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.xml", "XML Formatter & Validator", Category::Dev)
        .describe("Pretty-print, minify or check XML for well-formedness (no external entities are ever loaded)")
        .keywords(&["xml", "pretty print", "beautify", "minify", "validate", "well-formed", "svg", "rss"])
        .param(ParamSpec::textarea("text").label("XML").required().primary().syntax("xml"))
        .param(ParamSpec::choice("mode", &[("format", "Format"), ("minify", "Minify"), ("validate", "Check well-formedness")]).default_str("format"))
        .param(indent_param().show_if("mode", &["format"]))
        .text_output("application/xml")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let mode = p.str("mode")?;
        let indent = match (mode, p.str_or("indent", "2")) {
            ("minify", _) => None,
            (_, "4") => Some((b' ', 4)),
            (_, "tab") => Some((b'\t', 1)),
            _ => Some((b' ', 2)),
        };
        let (out, elements) = reformat_xml(p.text("text"), indent)?;
        let data = json!({"well_formed": true, "elements": elements});
        if mode == "validate" {
            return Ok(ToolOutput::data(data));
        }
        Ok(ToolOutput::text_as(out.trim_start().to_string(), "application/xml").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// SQL

fn sql_format() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.sql_format", "SQL Formatter", Category::Dev)
        .describe(
            "Format SQL queries with consistent indentation and keyword case (dialect-agnostic)",
        )
        .keywords(&[
            "sql",
            "format",
            "beautify",
            "pretty print",
            "query",
            "postgres",
            "mysql",
            "sqlite",
        ])
        .param(
            ParamSpec::textarea("text")
                .label("SQL")
                .required()
                .primary()
                .syntax("sql"),
        )
        .param(
            ParamSpec::choice(
                "keyword_case",
                &[
                    ("upper", "UPPERCASE"),
                    ("lower", "lowercase"),
                    ("preserve", "Preserve"),
                ],
            )
            .default_str("upper"),
        )
        .param(indent_param())
        .param(
            ParamSpec::integer("lines_between_queries")
                .default_int(1)
                .range(0.0, 5.0)
                .advanced(),
        )
        .text_output("application/sql")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        use sqlformat::{FormatOptions, Indent, QueryParams};
        let opts = FormatOptions {
            indent: match p.str_or("indent", "2") {
                "4" => Indent::Spaces(4),
                "tab" => Indent::Tabs,
                _ => Indent::Spaces(2),
            },
            uppercase: match p.str("keyword_case")? {
                "upper" => Some(true),
                "lower" => Some(false),
                _ => None,
            },
            lines_between_queries: p.int_or("lines_between_queries", 1) as u8 + 1,
            ..Default::default()
        };
        Ok(ToolOutput::text_as(
            sqlformat::format(p.text("text"), &QueryParams::None, &opts),
            "application/sql",
        ))
    })
}

// ---------------------------------------------------------------------------
// dev.convert

/// XML → JSON using the common convention: attributes as "@name", text as
/// "#text", repeated children become arrays.
/// Resolves the predefined XML entities and character references. Anything
/// else (such as an entity declared in the document's DTD) is an error: the
/// DTD is never read, so substituting text for it would silently lose data.
fn unescape_or_reject(raw: &str) -> ToolResult<String> {
    quick_xml::escape::unescape(raw)
        .map(|c| c.into_owned())
        .map_err(|e| {
            ToolError::invalid(
                "text",
                format!(
                    "{e}; only the predefined XML entities (&amp; &lt; &gt; &quot; &apos;) \
                     and character references like &#169; can be converted"
                ),
            )
        })
}

pub(crate) fn xml_to_json(text: &str) -> ToolResult<Value> {
    let mut reader = quick_xml::Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    // stack of (name, object, text)
    let mut stack: Vec<(String, serde_json::Map<String, Value>, String)> =
        vec![("#root".into(), serde_json::Map::new(), String::new())];
    // xml:space="preserve" for each open element, inherited by its children
    // unless they reset it with xml:space="default".
    let mut preserve = vec![false];
    fn preserves(obj: &serde_json::Map<String, Value>, inherited: bool) -> bool {
        match obj.get("@xml:space").and_then(Value::as_str) {
            Some("preserve") => true,
            Some("default") => false,
            _ => inherited,
        }
    }
    fn attrs(e: &quick_xml::events::BytesStart) -> ToolResult<serde_json::Map<String, Value>> {
        let mut m = serde_json::Map::new();
        for a in e.attributes() {
            let a = a.map_err(|e| ToolError::invalid("text", format!("bad attribute: {e}")))?;
            let k = a.key.as_ref().to_string();
            let v = unescape_or_reject(&a.value)?;
            m.insert(format!("@{k}"), Value::String(v));
        }
        Ok(m)
    }
    fn finish(obj: serde_json::Map<String, Value>, text: String, keep_ws: bool) -> Value {
        let t = if keep_ws {
            text
        } else {
            text.trim().to_string()
        };
        if obj.is_empty() {
            if t.is_empty() {
                Value::Null
            } else {
                Value::String(t)
            }
        } else {
            let mut obj = obj;
            if !t.is_empty() {
                obj.insert("#text".into(), Value::String(t));
            }
            Value::Object(obj)
        }
    }
    fn add(parent: &mut serde_json::Map<String, Value>, name: String, v: Value) {
        match parent.get_mut(&name) {
            Some(Value::Array(a)) => a.push(v),
            Some(existing) => {
                let old = existing.take();
                *existing = Value::Array(vec![old, v]);
            }
            None => {
                parent.insert(name, v);
            }
        }
    }
    loop {
        let ev = reader.read_event().map_err(|e| {
            let (l, c) = line_col(text, reader.buffer_position() as usize);
            ToolError::invalid("text", format!("{e} (line {l}, column {c})"))
        })?;
        match ev {
            Event::Start(e) => {
                let n = e.name().as_ref().to_string();
                let a = attrs(&e)?;
                preserve.push(preserves(&a, *preserve.last().unwrap()));
                stack.push((n, a, String::new()));
            }
            Event::Empty(e) => {
                let n = e.name().as_ref().to_string();
                let v = finish(attrs(&e)?, String::new(), false);
                add(&mut stack.last_mut().unwrap().1, n, v);
            }
            Event::End(_) => {
                let (n, obj, txt) = stack.pop().unwrap();
                let v = finish(obj, txt, preserve.pop().unwrap_or(false));
                let parent = stack
                    .last_mut()
                    .ok_or_else(|| ToolError::invalid("text", "unbalanced XML"))?;
                add(&mut parent.1, n, v);
            }
            Event::Text(t) => stack.last_mut().unwrap().2.push_str(&t),
            Event::CData(t) => stack.last_mut().unwrap().2.push_str(&t),
            Event::GeneralRef(g) => {
                let s = unescape_or_reject(&format!("&{};", &*g))?;
                stack.last_mut().unwrap().2.push_str(&s);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err(ToolError::invalid(
            "text",
            "unexpected end of XML document (unclosed elements)",
        ));
    }
    let root = stack.pop().unwrap().1;
    if root.is_empty() {
        return Err(ToolError::invalid("text", "no root element found"));
    }
    Ok(Value::Object(root))
}

pub(crate) fn json_to_xml(v: &Value, root_name: &str) -> ToolResult<String> {
    // Parsers turn a literal CR (and CRLF) into LF everywhere, and literal
    // tabs and newlines into spaces inside attribute values, so those are
    // written as character references to survive a round trip.
    fn esc(s: &str) -> ToolResult<String> {
        crate::escape::xml_escape(s)
            .map(|e| e.replace('\r', "&#13;"))
            .map_err(|e| ToolError::invalid("text", format!("{e}: {s:?}")))
    }
    fn esc_attr(s: &str) -> ToolResult<String> {
        Ok(esc(s)?.replace('\n', "&#10;").replace('\t', "&#9;"))
    }
    // XML 1.0 (fifth edition) NameStartChar and NameChar.
    fn name_start(c: char) -> bool {
        matches!(c,
            ':' | 'A'..='Z' | '_' | 'a'..='z'
            | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
            | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}'
            | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
            | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}' | '\u{10000}'..='\u{EFFFF}')
    }
    fn name_char(c: char) -> bool {
        name_start(c)
            || matches!(c,
                '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
    }
    fn valid_name(n: &str) -> String {
        let mut s: String = n
            .chars()
            .map(|c| if name_char(c) { c } else { '_' })
            .collect();
        // A leading colon would read as an empty namespace prefix.
        if !s.chars().next().is_some_and(|c| name_start(c) && c != ':') {
            s = format!("_{s}");
        }
        s
    }
    fn write(out: &mut String, name: &str, v: &Value, depth: usize) -> ToolResult<()> {
        let pad = "  ".repeat(depth);
        match v {
            Value::Array(a) => {
                for item in a {
                    write(out, name, item, depth)?;
                }
            }
            Value::Object(m) => {
                let n = valid_name(name);
                out.push_str(&format!("{pad}<{n}"));
                let mut seen = std::collections::HashMap::new();
                for (k, val) in m.iter().filter(|(k, _)| k.starts_with('@')) {
                    let an = valid_name(&k[1..]);
                    if let Some(prev) = seen.insert(an.clone(), k) {
                        return Err(ToolError::invalid(
                            "text",
                            format!(
                                "attributes {prev:?} and {k:?} on <{n}> would both be \
                                 written as {an:?}; rename one of them"
                            ),
                        ));
                    }
                    out.push_str(&format!(" {an}=\"{}\"", esc_attr(&cell_string(val))?));
                }
                let children: Vec<_> = m
                    .iter()
                    .filter(|(k, _)| !k.starts_with('@') && *k != "#text")
                    .collect();
                let text = m.get("#text").map(cell_string);
                if children.is_empty() && text.is_none() {
                    out.push_str("/>\n");
                    return Ok(());
                }
                out.push('>');
                if let Some(t) = &text {
                    out.push_str(&esc(t)?);
                }
                if !children.is_empty() {
                    let mut seen = std::collections::HashMap::new();
                    for (k, _) in &children {
                        let cn = valid_name(k);
                        if let Some(prev) = seen.insert(cn.clone(), *k) {
                            return Err(ToolError::invalid(
                                "text",
                                format!(
                                    "keys {prev:?} and {k:?} in <{n}> would both be \
                                     written as <{cn}>; rename one of them"
                                ),
                            ));
                        }
                    }
                    out.push('\n');
                    for (k, val) in children {
                        write(out, k, val, depth + 1)?;
                    }
                    out.push_str(&pad);
                }
                out.push_str(&format!("</{n}>\n"));
            }
            Value::Null => out.push_str(&format!("{pad}<{}/>\n", valid_name(name))),
            other => {
                let n = valid_name(name);
                out.push_str(&format!("{pad}<{n}>{}</{n}>\n", esc(&cell_string(other))?));
            }
        }
        Ok(())
    }
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match v {
        Value::Object(m) if m.len() == 1 && !m.values().next().unwrap().is_array() => {
            let (k, val) = m.iter().next().unwrap();
            write(&mut out, k, val, 0)?;
        }
        Value::Array(_) => {
            let root = valid_name(root_name);
            out.push_str(&format!("<{root}>\n"));
            write(&mut out, "item", v, 1)?;
            out.push_str(&format!("</{root}>\n"));
        }
        _ => write(&mut out, root_name, v, 0)?,
    }
    Ok(out)
}

pub(crate) fn query_to_json(q: &str) -> Value {
    let q = q.trim();
    let q = q.split_once('?').map(|(_, r)| r).unwrap_or(q);
    let q = q.split('#').next().unwrap_or("");
    let mut m = serde_json::Map::new();
    for (k, v) in url::form_urlencoded::parse(q.as_bytes()) {
        let (k, v) = (k.into_owned(), Value::String(v.into_owned()));
        match m.get_mut(&k) {
            Some(Value::Array(a)) => a.push(v),
            Some(e) => {
                let old = e.take();
                *e = Value::Array(vec![old, v]);
            }
            None => {
                m.insert(k, v);
            }
        }
    }
    Value::Object(m)
}

pub(crate) fn json_to_query(v: &Value) -> ToolResult<String> {
    let m = v.as_object().ok_or_else(|| {
        ToolError::invalid("text", "a query string needs an object of key/value pairs")
    })?;
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, val) in m {
        match val {
            Value::Array(a) => {
                for x in a {
                    if x.is_object() || x.is_array() {
                        return Err(ToolError::invalid(
                            "text",
                            format!("'{k}' contains nested data, which a query string cannot hold"),
                        ));
                    }
                    ser.append_pair(k, &cell_string(x));
                }
            }
            Value::Object(_) => {
                return Err(ToolError::invalid(
                    "text",
                    format!("'{k}' is a nested object, which a query string cannot hold"),
                ))
            }
            x => {
                ser.append_pair(k, &cell_string(x));
            }
        }
    }
    Ok(ser.finish())
}

pub(crate) fn parse_any(text: &str, from: &str) -> ToolResult<Value> {
    Ok(match from {
        "json" => parse_json(text, "text")?,
        "yaml" => {
            let mut docs = parse_yaml_docs(text, "text")?;
            match docs.len() {
                0 => Value::Null,
                1 => docs.remove(0),
                _ => Value::Array(docs),
            }
        }
        "toml" => parse_toml(text, "text")?,
        "xml" => xml_to_json(text)?,
        "csv" | "tsv" => {
            let delim = if from == "tsv" {
                b'\t'
            } else {
                crate::data::sniff_delimiter(text)
            };
            let t = crate::data::parse_csv(text, delim, true)?;
            crate::data::rows_to_objects(&t, true)
        }
        "ndjson" => Value::Array(
            text.lines()
                .enumerate()
                .filter(|(_, l)| !l.trim().is_empty())
                .map(|(i, l)| {
                    let v = serde_json::from_str(l)
                        .map_err(|e| ToolError::invalid("text", format!("line {}: {e}", i + 1)))?;
                    check_json_numbers(l)
                        .map_err(|m| ToolError::invalid("text", format!("line {}: {m}", i + 1)))?;
                    Ok(v)
                })
                .collect::<ToolResult<_>>()?,
        ),
        "query" => query_to_json(text),
        other => {
            return Err(ToolError::invalid(
                "from",
                format!("unknown format {other}"),
            ))
        }
    })
}

pub(crate) fn detect_format(text: &str) -> &'static str {
    let t = text.trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        if serde_json::from_str::<Value>(text).is_ok() {
            return "json";
        }
        if t.starts_with('{')
            && text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .all(|l| serde_json::from_str::<Value>(l).is_ok())
        {
            return "ndjson";
        }
    }
    if t.starts_with('<') {
        return "xml";
    }
    if toml::from_str::<toml::Table>(text).is_ok()
        && (t.starts_with('[') || text.lines().any(|l| l.contains(" = ")))
    {
        return "toml";
    }
    let first = t.lines().next().unwrap_or("");
    if !first.contains(' ') && first.contains('=') && !first.contains(": ") && !text.contains('\n')
    {
        return "query";
    }
    if first.contains('\t') {
        return "tsv";
    }
    if first.contains(',') && !first.contains(": ") && !first.trim_start().starts_with('-') {
        return "csv";
    }
    "yaml"
}

pub(crate) fn render_any(v: &Value, to: &str, root: &str) -> ToolResult<(String, &'static str)> {
    Ok(match to {
        "json" => (serde_json::to_string_pretty(v)?, "application/json"),
        "json_min" => (serde_json::to_string(v)?, "application/json"),
        "yaml" => (to_yaml(v)?, "application/yaml"),
        "toml" => (to_toml(v)?, "application/toml"),
        "xml" => (json_to_xml(v, root)?, "application/xml"),
        "csv" | "tsv" => {
            let rows = match v {
                Value::Array(a) => a.clone(),
                Value::Object(_) => vec![v.clone()],
                _ => {
                    return Err(ToolError::invalid(
                        "text",
                        "CSV output needs an array of objects",
                    ))
                }
            };
            let (cols, data) = crate::data::flatten_records(&rows, ".");
            (
                crate::data::write_csv(
                    &cols,
                    &data,
                    if to == "tsv" { b'\t' } else { b',' },
                    false,
                )?,
                if to == "tsv" {
                    "text/tab-separated-values"
                } else {
                    "text/csv"
                },
            )
        }
        "ndjson" => {
            let items = match v {
                Value::Array(a) => a.clone(),
                other => vec![other.clone()],
            };
            (
                items
                    .iter()
                    .map(|i| serde_json::to_string(i).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n",
                "application/x-ndjson",
            )
        }
        "query" => (json_to_query(v)?, "text/plain"),
        other => return Err(ToolError::invalid("to", format!("unknown format {other}"))),
    })
}

const FORMATS: &[(&str, &str)] = &[
    ("json", "JSON"),
    ("yaml", "YAML"),
    ("toml", "TOML"),
    ("xml", "XML"),
    ("csv", "CSV"),
    ("tsv", "TSV"),
    ("ndjson", "NDJSON / JSON Lines"),
    ("query", "URL query string"),
];

fn convert() -> Arc<dyn Tool> {
    let mut from_opts = vec![("auto", "Auto-detect")];
    from_opts.extend_from_slice(FORMATS);
    let mut to_opts = FORMATS.to_vec();
    to_opts.insert(1, ("json_min", "JSON (minified)"));
    let spec = ToolSpec::new("dev.convert", "Convert Data Format", Category::Dev)
        .describe("Convert structured data between JSON, YAML, TOML, XML, CSV/TSV, NDJSON and URL query strings")
        .keywords(&["json to yaml", "yaml to json", "toml", "xml to json", "json to csv", "csv to json", "convert", "query string"])
        .param(ParamSpec::textarea("text").label("Input").required().primary())
        .param(ParamSpec::choice("from", &from_opts).default_str("auto"))
        .param(ParamSpec::choice("to", &to_opts).default_str("yaml"))
        .param(ParamSpec::text("root").label("XML root element name").default_str("root").show_if("to", &["xml"]).advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let text = p.text("text");
        if text.trim().is_empty() {
            return Err(ToolError::invalid("text", "input is empty"));
        }
        let from = match p.str("from")? {
            "auto" => detect_format(text),
            f => f,
        };
        let v = parse_any(text, from)?;
        let (out, mime) = render_any(&v, p.str("to")?, p.str_or("root", "root"))?;
        Ok(ToolOutput::text_as(out, mime).with_data(json!({"from": from, "to": p.str("to")?})))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    #[tokio::test]
    async fn json_tool() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "dev.json",
                json!({"text": "{\"b\":1,\"a\":[1,2]}", "sort_keys": true}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": 1\n}"
        );
        let out = h
            .run(
                "dev.json",
                json!({"text": "{ \"a\" : 1 }", "mode": "minify"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "{\"a\":1}");
        let err = h
            .run("dev.json", json!({"text": "{\n  \"a\": 1,\n  \"b\": }"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("line 3"), "{err}");
        assert_eq!(
            h.run("dev.json", json!({"text": "  "}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        // Numbers that would change are rejected; representable ones (and
        // number-like text inside strings) pass through.
        for bad in [
            "{\"id\":18446744073709551617}",
            "[-9223372036854775809]",
            "[0.10000000000000000001]",
        ] {
            let e = h.run("dev.json", json!({"text": bad})).await.unwrap_err();
            assert!(
                e.to_string().contains("cannot be represented exactly"),
                "{bad}: {e}"
            );
        }
        let out = h
            .run(
                "dev.json",
                json!({"text": "{\"a\":18446744073709551615,\"b\":-9223372036854775808,\"c\":[0.1,1.5e3,1E-7,-0,2.0],\"d\":\"99999999999999999999\\\"9\"}", "mode": "minify"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "{\"a\":18446744073709551615,\"b\":-9223372036854775808,\"c\":[0.1,1500.0,1e-7,-0.0,2.0],\"d\":\"99999999999999999999\\\"9\"}"
        );
        assert!(h
            .run(
                "dev.diff_json",
                json!({"original": "[18446744073709551617]", "changed": "[18446744073709551618]"}),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn yaml_toml_xml() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "dev.yaml",
                json!({"text": "b: 1\na: [x, y]\n---\nc: true\n"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["documents"], 2);
        assert!(out.text.unwrap().contains("---"));
        // Keys that differ only in type survive formatting (and sorting)...
        let both = "1: first\n\"1\": second\n";
        for sort in [false, true] {
            let out = h
                .run("dev.yaml", json!({"text": both, "sort_keys": sort}))
                .await
                .unwrap();
            let t = out.text.unwrap();
            assert!(t.contains("first") && t.contains("second"), "{t}");
        }
        // ...and a conversion to JSON refuses rather than dropping one.
        let err = h
            .run(
                "dev.convert",
                json!({"text": both, "from": "yaml", "to": "json"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_input");
        let err = h
            .run("dev.yaml", json!({"text": "a: [1, 2\nb: 3"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("line"), "{err}");
        let out = h
            .run(
                "dev.toml",
                json!({"text": "title='x'\n[owner]\nname = \"Tom\""}),
            )
            .await
            .unwrap();
        assert!(out.text.unwrap().contains("[owner]"));
        let err = h
            .run("dev.toml", json!({"text": "a = \n"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("line 1"), "{err}");
        // TOML-only values survive formatting...
        let native = "x = nan\ny = +inf\nwhen = 1979-05-27T07:32:00Z\n";
        let t = h
            .run("dev.toml", json!({"text": native}))
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(
            t.contains("nan") && t.contains("inf") && t.contains("1979-05-27T07:32:00Z"),
            "{t}"
        );
        // ...datetimes convert to JSON strings, and nan is rejected, not nulled.
        let out = h
            .run(
                "dev.convert",
                json!({"text": "when = 1979-05-27T07:32:00Z", "from": "toml", "to": "json"}),
            )
            .await
            .unwrap();
        assert!(out.text.unwrap().contains("\"1979-05-27T07:32:00Z\""));
        let err = h
            .run(
                "dev.convert",
                json!({"text": "[a]\nx = nan", "from": "toml", "to": "json"}),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("a.x"), "{err}");
        let out = h
            .run(
                "dev.xml",
                json!({"text": "<a><b x=\"1\">t &amp; u</b><c/></a>"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "<a>\n  <b x=\"1\">t &amp; u</b>\n  <c/>\n</a>"
        );
        let out = h
            .run(
                "dev.xml",
                json!({"text": "<a>\n  <b>x</b>\n</a>", "mode": "minify"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "<a><b>x</b></a>");
        // Whitespace-only text in a leaf element is data, kept in both modes.
        for mode in ["format", "minify"] {
            let out = h
                .run(
                    "dev.xml",
                    json!({"text": "<r>\n  <value> </value>\n  <nl>\n</nl>\n  <e></e>\n</r>", "mode": mode}),
                )
                .await
                .unwrap();
            let t = out.text.unwrap();
            assert!(t.contains("<value> </value>"), "{mode}: {t}");
            assert!(t.contains("<nl>\n</nl>"), "{mode}: {t}");
        }
        // Character data outside the root is rejected in every form.
        for bad in [
            "<r/><![CDATA[extra]]>",
            "<![CDATA[x]]><r/>",
            "<r/>&amp;",
            "<r/>x",
        ] {
            let e = h.run("dev.xml", json!({"text": bad})).await.unwrap_err();
            assert!(e.to_string().contains("outside of the root"), "{bad}: {e}");
        }
        // Attribute errors (duplicates, unquoted values) are not well-formed,
        // on empty and start elements alike.
        for bad in [
            "<r a=\"1\" a=\"2\"/>",
            "<r><s a=\"1\" a=\"2\">x</s></r>",
            "<r a=1/>",
        ] {
            let e = h.run("dev.xml", json!({"text": bad})).await.unwrap_err();
            assert!(e.to_string().contains("attribute"), "{bad}: {e}");
        }
        // Mixed content keeps its meaningful whitespace (minify and format).
        let mixed = "<doc>\n  <p><b>Hello</b> <i>world</i></p>\n</doc>";
        let out = h
            .run("dev.xml", json!({"text": mixed, "mode": "minify"}))
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "<doc><p><b>Hello</b> <i>world</i></p></doc>"
        );
        let out = h.run("dev.xml", json!({"text": mixed})).await.unwrap();
        assert!(
            out.text
                .unwrap()
                .contains("<p><b>Hello</b> <i>world</i></p>"),
            "mixed content must not be re-indented"
        );
        let out = h
            .run(
                "dev.xml",
                json!({"text": "<a><pre xml:space=\"preserve\">  x\n  y </pre></a>", "mode": "minify"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "<a><pre xml:space=\"preserve\">  x\n  y </pre></a>"
        );
        assert_eq!(
            h.run("dev.xml", json!({"text": "<a><b></a>"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.xml", json!({"text": "<a>"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.xml", json!({"text": "<a/><b/>"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        // External entities are never expanded.
        let xxe = "<?xml version=\"1.0\"?><!DOCTYPE r [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><r>&x;</r>";
        let out = h
            .run("dev.xml", json!({"text": xxe, "mode": "minify"}))
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(out.contains("&x;") && !out.contains("root:"));
        // Converting to JSON never drops an entity it cannot resolve, in text
        // or in attributes; predefined and character references still work.
        for bad in [
            "<!DOCTYPE r [<!ENTITY company \"ACME\">]><r>Hello &company;!</r>",
            "<!DOCTYPE r [<!ENTITY company \"ACME\">]><r a=\"&company;\"/>",
        ] {
            let e = h
                .run(
                    "dev.convert",
                    json!({"text": bad, "from": "xml", "to": "json"}),
                )
                .await
                .unwrap_err();
            assert_eq!(e.code(), "invalid_input", "{bad}");
            assert!(e.to_string().contains("company"), "{bad}: {e}");
        }
        let out = h
            .run(
                "dev.convert",
                json!({"text": "<r a=\"&lt;&#65;\">x &amp; &#x42;</r>", "from": "xml", "to": "json"}),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["r"]["@a"], "<A");
        assert_eq!(v["r"]["#text"], "x & B");
        // Whitespace in attributes and CRs in text survive JSON -> XML -> JSON.
        let src = json!({"r": {"@a": "line1\nline2\tend\r", "#text": "x\r\ny"}});
        let xml = h
            .run(
                "dev.convert",
                json!({"text": src.to_string(), "from": "json", "to": "xml"}),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(xml.contains("line1&#10;line2&#9;end&#13;"), "{xml}");
        assert!(xml.contains("x&#13;\ny"), "{xml}");
        let back = super::xml_to_json(&xml).unwrap();
        assert_eq!(back["r"]["@a"], "line1\nline2\tend\r");
        assert_eq!(back["r"]["#text"], "x\r\ny");
        // Characters XML 1.0 cannot carry are an error, not unparseable XML.
        for bad in [r#"{"r":"a\u0000b"}"#, r#"{"r":{"@a":"\u001b"}}"#] {
            let e = h
                .run(
                    "dev.convert",
                    json!({"text": bad, "from": "json", "to": "xml"}),
                )
                .await
                .unwrap_err();
            assert!(e.to_string().contains("XML 1.0"), "{bad}: {e}");
        }
        // Attribute keys that sanitize to the same name are rejected rather
        // than written as duplicate attributes.
        let e = h
            .run(
                "dev.convert",
                json!({"text": r#"{"r":{"@a!":"first","@a?":"second"}}"#, "from": "json", "to": "xml"}),
            )
            .await
            .unwrap_err();
        assert!(e.to_string().contains("a_"), "{e}");
        // The root name is cleaned up for every input shape, arrays included.
        for text in ["[1]", "1", "{}"] {
            let xml = h
                .run(
                    "dev.convert",
                    json!({"text": text, "from": "json", "to": "xml", "root": "my items"}),
                )
                .await
                .unwrap()
                .text
                .unwrap();
            assert!(
                xml.contains("<my_items") && !xml.contains("my items"),
                "{text}: {xml}"
            );
            super::xml_to_json(&xml).unwrap();
        }
        // xml:space="preserve" keeps text exactly, and children inherit it
        // until xml:space="default" resets it.
        let v = super::xml_to_json(
            "<r xml:space=\"preserve\"><a>  x  </a><b> </b><c xml:space=\"default\"> y </c></r>",
        )
        .unwrap();
        assert_eq!(v["r"]["a"], "  x  ");
        assert_eq!(v["r"]["b"], " ");
        assert_eq!(v["r"]["c"]["#text"], "y");
        assert_eq!(super::xml_to_json("<r> z </r>").unwrap()["r"], "z");
        // Names follow XML's character ranges, not Unicode alphanumerics,
        // and keys that clean up to the same element name are rejected.
        let xml = h
            .run(
                "dev.convert",
                json!({"text": r#"{"r":{"x²":"v","@a²":"w","1st":"y","é·b":"z"}}"#, "from": "json", "to": "xml"}),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(
            xml.contains("<x_>v</x_>")
                && xml.contains("a_=\"w\"")
                && xml.contains("<_1st>")
                && xml.contains("<é·b>"),
            "{xml}"
        );
        super::xml_to_json(&xml).unwrap();
        let e = h
            .run(
                "dev.convert",
                json!({"text": r#"{"r":{"a!":1,"a?":2}}"#, "from": "json", "to": "xml"}),
            )
            .await
            .unwrap_err();
        assert!(e.to_string().contains("<a_>"), "{e}");
    }

    #[tokio::test]
    async fn sql_and_convert() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "dev.sql_format",
                json!({"text": "select a, b from t where x = 1"}),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(out.starts_with("SELECT") && out.contains("\nFROM") && out.contains("WHERE"));
        let c = |t: &'static str, from: &'static str, to: &'static str| json!({"text": t, "from": from, "to": to});
        let out = h
            .run(
                "dev.convert",
                c("{\"a\": 1, \"b\": [true, null]}", "auto", "yaml"),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "a: 1\nb:\n- true\n- null\n");
        assert_eq!(out.data["from"], "json");
        let out = h
            .run("dev.convert", c("a: 1\nb: x\n", "yaml", "toml"))
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(out, "a = 1\nb = \"x\"\n");
        let out = h
            .run(
                "dev.convert",
                c("name,age\nAnn,30\nBob,", "csv", "json_min"),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(
            out,
            "[{\"name\":\"Ann\",\"age\":30},{\"name\":\"Bob\",\"age\":null}]"
        );
        let out = h
            .run(
                "dev.convert",
                c(
                    "<r><i id=\"1\">a</i><i id=\"2\">b</i></r>",
                    "xml",
                    "json_min",
                ),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(
            out,
            "{\"r\":{\"i\":[{\"@id\":\"1\",\"#text\":\"a\"},{\"@id\":\"2\",\"#text\":\"b\"}]}}"
        );
        let out = h
            .run("dev.convert", c("a=1&b=x%20y&a=2", "query", "json_min"))
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(out, "{\"a\":[\"1\",\"2\"],\"b\":\"x y\"}");
        let out = h
            .run("dev.convert", c("[{\"a\":{\"b\":1}}]", "json", "csv"))
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(out, "a.b\n1\n");
        let out = h
            .run(
                "dev.convert",
                c("{\"r\":{\"@id\":\"1\",\"v\":\"x<\"}}", "json", "xml"),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert!(out.contains("<r id=\"1\">") && out.contains("<v>x&lt;</v>"));
        assert_eq!(
            h.run("dev.convert", c("{\"a\": null}", "json", "toml"))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.convert", c("{bad", "json", "yaml"))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
}
