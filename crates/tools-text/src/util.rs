//! Shared helpers for the text / dev / data tools.

use dogubako_core::prelude::*;

/// Maximum number of rows returned in table previews.
pub const PREVIEW_ROWS: usize = 500;
/// Maximum size of an uploaded file read fully into memory as text.
pub const MAX_TEXT_FILE: u64 = 64 * 1024 * 1024;

/// Decode bytes into a string: strips BOMs, uses UTF-8 when valid and
/// otherwise detects the legacy encoding with `chardetng`.
pub fn decode_bytes(bytes: &[u8]) -> (String, &'static str) {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        let (s, _) = enc.decode_without_bom_handling(&bytes[bom_len..]);
        return (s.into_owned(), enc.name());
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), "UTF-8");
    }
    let enc = detect_encoding(bytes);
    let (s, _, _) = enc.decode(bytes);
    (s.into_owned(), enc.name())
}

pub fn detect_encoding(bytes: &[u8]) -> &'static encoding_rs::Encoding {
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    det.feed(bytes, true);
    det.guess(None, chardetng::Utf8Detection::Allow)
}

/// Read a whole input file, enforcing a size cap.
pub fn read_file(f: &InputFile) -> ToolResult<Vec<u8>> {
    let size = f.size();
    if size > MAX_TEXT_FILE {
        return Err(ToolError::limit(format!(
            "'{}' is too large ({}; max {})",
            f.name,
            human_size(size),
            human_size(MAX_TEXT_FILE)
        )));
    }
    std::fs::read(&f.path).ctx(&format!("reading {}", f.name))
}

/// Text from an uploaded file (if given) or else the pasted text parameter.
pub fn text_or_file(p: &Params, text: &str, file: &str) -> ToolResult<String> {
    if let Some(f) = p.opt_file(file) {
        let bytes = read_file(f)?;
        return Ok(decode_bytes(&bytes).0);
    }
    if p.has(text) {
        return Ok(p.text(text).to_string());
    }
    Err(ToolError::invalid(
        text,
        format!("provide text or upload a file ('{file}')"),
    ))
}

/// Name of the uploaded file, if any, else a default.
pub fn input_name(p: &Params, file: &str, default: &str) -> String {
    p.opt_file(file)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| default.to_string())
}

/// Split into lines, remembering whether the text ended with a newline.
pub fn split_lines(text: &str) -> (Vec<&str>, bool) {
    if text.is_empty() {
        return (vec![], false);
    }
    let trailing = text.ends_with('\n');
    let body = text.strip_suffix('\n').unwrap_or(text);
    let lines = body
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    (lines, trailing)
}

/// Join lines with `\n`, re-adding a trailing newline if requested.
pub fn join_lines<S: AsRef<str>>(lines: &[S], trailing: bool) -> String {
    let mut s = lines
        .iter()
        .map(|l| l.as_ref())
        .collect::<Vec<_>>()
        .join("\n");
    if trailing && !lines.is_empty() {
        s.push('\n');
    }
    s
}

/// Build a `data.table` value.
pub fn table(columns: &[&str], rows: Vec<Vec<Value>>) -> Value {
    json!({"columns": columns, "rows": rows})
}

pub fn table_owned(columns: &[String], rows: Vec<Vec<Value>>) -> Value {
    json!({"columns": columns, "rows": rows})
}

/// Line and column (1-based) of a byte offset.
pub fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..floor_char(text, offset)];
    let line = before.matches('\n').count() + 1;
    let col = before
        .rsplit('\n')
        .next()
        .map(|l| l.chars().count())
        .unwrap_or(0)
        + 1;
    (line, col)
}

pub fn floor_char(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Natural ("human") ordering: digit runs compare numerically.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nb.push(c);
                    bi.next();
                }
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let o = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| na.len().cmp(&nb.len()));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.cmp(&y);
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Parse a number leniently (allows thousands separators `_`/`,` when unambiguous and surrounding spaces).
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.parse::<f64>() {
        return v.is_finite().then_some(v);
    }
    let cleaned: String = t.chars().filter(|c| *c != '_' && *c != ',').collect();
    if cleaned.len() != t.len() && cleaned.chars().any(|c| c.is_ascii_digit()) {
        return cleaned.parse::<f64>().ok().filter(|v| v.is_finite());
    }
    None
}

/// Convert a JSON value into a cell string.
pub fn cell_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Infer a JSON value from a CSV cell.
pub fn infer_value(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() {
        return Value::Null;
    }
    match t {
        "true" | "TRUE" | "True" => return Value::Bool(true),
        "false" | "FALSE" | "False" => return Value::Bool(false),
        "null" | "NULL" => return Value::Null,
        _ => {}
    }
    // Keep things like zip codes / ids with leading zeros as strings.
    let leading_zero = t.len() > 1 && t.starts_with('0') && !t.starts_with("0.");
    if !leading_zero {
        if let Ok(i) = t.parse::<i64>() {
            return Value::from(i);
        }
        if t.chars().any(|c| c.is_ascii_digit()) && !t.contains(['x', 'X']) {
            if let Ok(f) = t.parse::<f64>() {
                if f.is_finite() && !t.eq_ignore_ascii_case("inf") && !t.eq_ignore_ascii_case("nan")
                {
                    return json!(f);
                }
            }
        }
    }
    Value::String(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural() {
        let mut v = vec!["a10", "a2", "a1", "b"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["a1", "a2", "a10", "b"]);
    }

    #[test]
    fn lines() {
        let (l, t) = split_lines("a\r\nb\n");
        assert_eq!(l, ["a", "b"]);
        assert!(t);
        assert_eq!(line_col("ab\ncd", 4), (2, 2));
    }

    #[test]
    fn infer() {
        assert_eq!(infer_value("42"), json!(42));
        assert_eq!(infer_value("007"), json!("007"));
        assert_eq!(infer_value("1.5"), json!(1.5));
        assert_eq!(infer_value(""), Value::Null);
    }
}
