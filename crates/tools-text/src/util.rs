//! Shared helpers for the text / dev / data tools.

use dogubako_core::prelude::*;

/// Maximum number of rows returned in table previews.
pub const PREVIEW_ROWS: usize = 500;
/// Maximum size of an uploaded file read fully into memory as text.
pub const MAX_TEXT_FILE: u64 = 64 * 1024 * 1024;
/// Largest text a tool may build from user-controlled repetition (a
/// separator or prefix repeated per byte/line/row). Inputs are capped, but
/// a long separator times many pieces can otherwise need gigabytes.
pub const MAX_GENERATED_TEXT: u128 = 256 * 1024 * 1024;

/// Reject an output whose estimated size (computed by the caller in u128,
/// so it cannot overflow) is over [`MAX_GENERATED_TEXT`], before building it.
pub fn check_generated_size(bytes: u128) -> ToolResult<()> {
    if bytes > MAX_GENERATED_TEXT {
        return Err(ToolError::limit(format!(
            "the output would be about {}, over the {} limit; use a shorter separator, prefix or input",
            human_size(u64::try_from(bytes).unwrap_or(u64::MAX)),
            human_size(MAX_GENERATED_TEXT as u64)
        )));
    }
    Ok(())
}

/// Running total for output assembled piece by piece.
#[derive(Default)]
pub struct OutputBudget(u128);

impl OutputBudget {
    pub fn add(&mut self, bytes: usize) -> ToolResult<()> {
        self.0 += bytes as u128;
        check_generated_size(self.0)
    }
}

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
/// Compare two cells numerically: exactly as integers when both are
/// integral (f64 cannot tell 2^53 from 2^53+1), otherwise as f64.
/// Compare two numeric cells exactly, whatever their notation (integers,
/// decimals, exponents; `_`/`,` separators): no rounding through f64, so
/// "9007199254740992.0" < "9007199254740993" and the order is total.
/// Falls back to f64 only for values this parser does not understand.
pub fn compare_numbers(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    match (exact_decimal(a), exact_decimal(b)) {
        (Some(x), Some(y)) => Some(cmp_decimal(&x, &y)),
        _ => parse_number(a)?.partial_cmp(&parse_number(b)?),
    }
}

/// A decimal as (negative, significant digits without leading/trailing
/// zeros, exponent): value = digits × 10^exp. Zero has no digits.
type Decimal = (bool, Vec<u8>, i64);

fn exact_decimal(s: &str) -> Option<Decimal> {
    let t: String = s
        .trim()
        .chars()
        .filter(|c| *c != '_' && *c != ',')
        .collect();
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(&t)),
    };
    let (mant, exp) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], body[i + 1..].parse::<i64>().ok()?),
        None => (body, 0),
    };
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    if (int.is_empty() && frac.is_empty())
        || !int.bytes().all(|c| c.is_ascii_digit())
        || !frac.bytes().all(|c| c.is_ascii_digit())
        || exp.unsigned_abs() > 1_000_000
    {
        return None;
    }
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes()).map(|c| c - b'0').collect();
    let mut exp = exp - frac.len() as i64;
    while digits.last() == Some(&0) {
        digits.pop();
        exp += 1;
    }
    let lead = digits.iter().take_while(|d| **d == 0).count();
    digits.drain(..lead);
    if digits.is_empty() {
        return Some((false, vec![], 0));
    }
    Some((neg, digits, exp))
}

fn cmp_decimal(x: &Decimal, y: &Decimal) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    let sign = |d: &Decimal| {
        if d.1.is_empty() {
            0
        } else if d.0 {
            -1
        } else {
            1
        }
    };
    let (sx, sy) = (sign(x), sign(y));
    if sx != sy || sx == 0 {
        return sx.cmp(&sy);
    }
    // Same sign, both non-zero: compare magnitudes by position of the
    // leading digit, then digit by digit.
    let mag = (x.1.len() as i64 + x.2)
        .cmp(&(y.1.len() as i64 + y.2))
        .then_with(|| {
            for i in 0..x.1.len().max(y.1.len()) {
                let (a, b) = (
                    x.1.get(i).copied().unwrap_or(0),
                    y.1.get(i).copied().unwrap_or(0),
                );
                if a != b {
                    return a.cmp(&b);
                }
            }
            Equal
        });
    if sx < 0 {
        mag.reverse()
    } else {
        mag
    }
}

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
        if let Ok(u) = t.parse::<u64>() {
            return Value::from(u);
        }
        // Integers beyond u64/i64 would be rounded by f64: keep them as text.
        let digits = t.strip_prefix(['-', '+']).unwrap_or(t);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return Value::String(s.to_string());
        }
        if t.chars().any(|c| c.is_ascii_digit()) && !t.contains(['x', 'X']) {
            if let Ok(f) = t.parse::<f64>() {
                // Only when the double holds the cell's exact value: cells
                // such as 9007199254740993.0 or 0.10000000000000000001 stay text.
                if f.is_finite()
                    && !t.eq_ignore_ascii_case("inf")
                    && !t.eq_ignore_ascii_case("nan")
                    && compare_numbers(t.strip_prefix('+').unwrap_or(t), &f.to_string())
                        == Some(std::cmp::Ordering::Equal)
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
    fn inferred_numbers_are_exact() {
        for keep in ["9007199254740993.0", "0.10000000000000000001", "1e400"] {
            assert_eq!(infer_value(keep), Value::String(keep.into()), "{keep}");
        }
        assert_eq!(infer_value("9007199254740992.0"), json!(9007199254740992.0));
        assert_eq!(infer_value("1.50"), json!(1.5));
        assert_eq!(infer_value("+2.5"), json!(2.5));
        assert_eq!(infer_value("1.5e3"), json!(1500.0));
        assert_eq!(infer_value("-0.25"), json!(-0.25));
    }

    #[test]
    fn exact_number_comparison() {
        use std::cmp::Ordering::*;
        let c = |a, b| compare_numbers(a, b).unwrap();
        assert_eq!(c("9007199254740992.0", "9007199254740993"), Less);
        assert_eq!(c("9007199254740992.0", "9007199254740992"), Equal);
        assert_eq!(c("1e3", "1000"), Equal);
        assert_eq!(c("1,000.50", "1000.5"), Equal);
        assert_eq!(c("-0", "0.000"), Equal);
        assert_eq!(c("-2.5", "-10"), Greater);
        assert_eq!(c("0.1", "0.09"), Greater);
        assert_eq!(c("-1e-3", "0"), Less);
        // Consistent (total) ordering across notations.
        let mut v = vec![
            "9007199254740993",
            "9007199254740992.0",
            "9007199254740992",
            "-1",
            "1e2",
        ];
        v.sort_by(|a, b| c(a, b));
        assert_eq!(
            v,
            [
                "-1",
                "1e2",
                "9007199254740992.0",
                "9007199254740992",
                "9007199254740993"
            ]
        );
    }

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
        // Above i64::MAX but within u64: exact number.
        assert_eq!(
            infer_value("9223372036854775809"),
            json!(9223372036854775809u64)
        );
        // Beyond u64 / below i64::MIN: kept as text, never rounded.
        assert_eq!(
            infer_value("18446744073709551616"),
            json!("18446744073709551616")
        );
        assert_eq!(
            infer_value("-9223372036854775809"),
            json!("-9223372036854775809")
        );
        assert_eq!(infer_value("1e3"), json!(1000.0));
        assert_eq!(infer_value(""), Value::Null);
    }
}
