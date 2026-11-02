//! String escaping / unescaping for many target languages.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// RFC 3986 unreserved characters are left alone.
pub const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');
/// Like `encodeURI`: keeps reserved URL delimiters.
pub const FULL_URL: &AsciiSet = &COMPONENT
    .remove(b':')
    .remove(b'/')
    .remove(b'?')
    .remove(b'#')
    .remove(b'[')
    .remove(b']')
    .remove(b'@')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=');
// '%' stays encoded (as in encodeURI): "100% organic" -> "100%25%20organic",
// so the output always decodes back to the input.

/// With spaces written as '+', a literal '+' must be escaped or it would
/// decode as a space.
const FULL_URL_PLUS_SPACES: &AsciiSet = &FULL_URL.add(b'+');

pub fn url_encode(s: &str, full: bool, space_plus: bool) -> String {
    let set = match (full, space_plus) {
        (true, true) => FULL_URL_PLUS_SPACES,
        (true, false) => FULL_URL,
        (false, _) => COMPONENT,
    };
    let enc = utf8_percent_encode(s, set).to_string();
    if space_plus {
        enc.replace("%20", "+")
    } else {
        enc
    }
}

pub fn url_decode(s: &str, plus_space: bool) -> Result<String, String> {
    let s = if plus_space {
        s.replace('+', " ")
    } else {
        s.to_string()
    };
    // Validate escapes so malformed input is reported instead of passed through silently.
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err(format!("incomplete percent escape at position {}", i + 1));
            }
            if !(b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit()) {
                return Err(format!("invalid percent escape at position {}", i + 1));
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    percent_encoding::percent_decode_str(&s)
        .decode_utf8()
        .map(|c| c.into_owned())
        .map_err(|e| format!("decoded bytes are not valid UTF-8: {e}"))
}

pub fn html_escape(s: &str, non_ascii: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if non_ascii && !c.is_ascii() => out.push_str(&format!("&#x{:X};", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn html_unescape(s: &str) -> String {
    html_escape::decode_html_entities(s).into_owned()
}

/// Escapes text for XML 1.0. Characters XML 1.0 cannot carry at all (C0
/// controls other than tab, LF and CR, and U+FFFE/U+FFFF) are an error:
/// even as character references they make the document unparseable.
pub fn xml_escape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if ((c as u32) < 0x20 && !matches!(c, '\n' | '\r' | '\t'))
                || matches!(c, '\u{FFFE}' | '\u{FFFF}') =>
            {
                return Err(format!(
                    "U+{:04X} cannot be represented in XML 1.0",
                    c as u32
                ))
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

pub fn xml_unescape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find(';').filter(|e| *e <= 12).ok_or_else(|| {
            // Preview by characters, not bytes (the input may be multibyte).
            let preview: String = rest[i..].chars().take(12).collect();
            format!("unterminated entity near '{preview}'")
        })?;
        let name = &after[..end];
        let ch = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            n if n.starts_with("#x") || n.starts_with("#X") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("invalid character reference &{n};"))?,
            n if n.starts_with('#') => n[1..]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("invalid character reference &{n};"))?,
            n => return Err(format!("unknown XML entity &{n};")),
        };
        out.push(ch);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Escaping flavours for backslash-style string literals.
#[derive(Clone, Copy, PartialEq)]
pub enum Lang {
    Json,
    Js,
    C,
    Java,
    Python,
}

pub fn backslash_escape(s: &str, lang: Lang) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\'' if matches!(lang, Lang::Js | Lang::C | Lang::Python) => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\u{b}' if lang != Lang::Json && lang != Lang::Java => out.push_str("\\v"),
            '\u{7}' if matches!(lang, Lang::C | Lang::Python) => out.push_str("\\a"),
            '\u{2028}' | '\u{2029}' if lang == Lang::Js => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c if (c as u32) < 0x20 || c == '\u{7f}' => match lang {
                Lang::Json | Lang::Java => out.push_str(&format!("\\u{:04x}", c as u32)),
                Lang::Js | Lang::Python => out.push_str(&format!("\\x{:02x}", c as u32)),
                Lang::C => out.push_str(&format!("\\{:03o}", c as u32)),
            },
            c if !c.is_ascii() && lang == Lang::Java => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", u));
                }
            }
            c if !c.is_ascii() && lang == Lang::C => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("\\{:03o}", b));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Unescape backslash sequences. `hex_is_byte`: `\xHH` produces a raw byte
/// (C) rather than a code point (Python/JS).
pub fn backslash_unescape(s: &str, hex_is_byte: bool) -> Result<String, String> {
    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    let push_char = |bytes: &mut Vec<u8>, c: char| {
        let mut b = [0u8; 4];
        bytes.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
    };
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let hex = |chars: &[char], start: usize, n: usize| -> Result<u32, String> {
        if start + n > chars.len() {
            return Err(format!("truncated escape at position {}", start));
        }
        let h: String = chars[start..start + n].iter().collect();
        u32::from_str_radix(&h, 16)
            .map_err(|_| format!("invalid hex digits '{h}' at position {start}"))
    };
    while i < chars.len() {
        let c = chars[i];
        if c != '\\' {
            push_char(&mut bytes, c);
            i += 1;
            continue;
        }
        let Some(&e) = chars.get(i + 1) else {
            return Err("trailing backslash".into());
        };
        i += 2;
        match e {
            'n' => bytes.push(b'\n'),
            'r' => bytes.push(b'\r'),
            't' => bytes.push(b'\t'),
            'b' => bytes.push(8),
            'f' => bytes.push(12),
            'v' => bytes.push(11),
            'a' => bytes.push(7),
            'e' => bytes.push(27),
            '\\' | '"' | '\'' | '/' | '`' | '?' | '$' => push_char(&mut bytes, e),
            '\n' => {}
            'x' => {
                // 1-2 hex digits
                let mut n = 0;
                while n < 2 && chars.get(i + n).is_some_and(|c| c.is_ascii_hexdigit()) {
                    n += 1;
                }
                if n == 0 {
                    return Err(format!("\\x without hex digits at position {}", i - 1));
                }
                let v = hex(&chars, i, n)?;
                i += n;
                if hex_is_byte {
                    bytes.push(v as u8);
                } else {
                    push_char(&mut bytes, char::from_u32(v).unwrap_or('\u{fffd}'));
                }
            }
            'u' if chars.get(i) == Some(&'{') => {
                let end = chars[i..]
                    .iter()
                    .position(|c| *c == '}')
                    .ok_or("unterminated \\u{...}")?;
                let v = hex(&chars, i + 1, end - 1)?;
                i += end + 1;
                push_char(
                    &mut bytes,
                    char::from_u32(v).ok_or_else(|| format!("invalid code point U+{v:X}"))?,
                );
            }
            'u' => {
                let v = hex(&chars, i, 4)?;
                i += 4;
                if (0xD800..0xDC00).contains(&v) {
                    // surrogate pair
                    if chars.get(i) == Some(&'\\') && chars.get(i + 1) == Some(&'u') {
                        let lo = hex(&chars, i + 2, 4)?;
                        if (0xDC00..0xE000).contains(&lo) {
                            i += 6;
                            let cp = 0x10000 + ((v - 0xD800) << 10) + (lo - 0xDC00);
                            push_char(&mut bytes, char::from_u32(cp).unwrap_or('\u{fffd}'));
                            continue;
                        }
                    }
                    push_char(&mut bytes, '\u{fffd}');
                } else {
                    push_char(&mut bytes, char::from_u32(v).unwrap_or('\u{fffd}'));
                }
            }
            'U' => {
                let v = hex(&chars, i, 8)?;
                i += 8;
                push_char(
                    &mut bytes,
                    char::from_u32(v).ok_or_else(|| format!("invalid code point U+{v:X}"))?,
                );
            }
            '0'..='7' => {
                let mut v = e.to_digit(8).unwrap();
                let mut n = 0;
                while n < 2 && chars.get(i).is_some_and(|c| ('0'..='7').contains(c)) {
                    v = v * 8 + chars[i].to_digit(8).unwrap();
                    i += 1;
                    n += 1;
                }
                if hex_is_byte {
                    bytes.push((v & 0xff) as u8);
                } else {
                    push_char(&mut bytes, char::from_u32(v).unwrap_or('\u{fffd}'));
                }
            }
            other => {
                return Err(format!(
                    "unknown escape sequence \\{other} at position {}",
                    i - 2
                ))
            }
        }
    }
    String::from_utf8(bytes).map_err(|_| "unescaped bytes are not valid UTF-8".to_string())
}

pub fn csv_escape(s: &str, delim: char) -> String {
    if s.contains([delim, '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn csv_unescape(s: &str) -> Result<String, String> {
    let t = s.trim();
    if let Some(inner) = t.strip_prefix('"') {
        let inner = inner
            .strip_suffix('"')
            .ok_or("quoted field is missing its closing quote")?;
        let mut out = String::new();
        let mut it = inner.chars().peekable();
        while let Some(c) = it.next() {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    it.next();
                    out.push('"');
                } else {
                    return Err("unescaped quote inside quoted field".into());
                }
            } else {
                out.push(c);
            }
        }
        Ok(out)
    } else {
        Ok(s.to_string())
    }
}

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parse POSIX shell quoting (single, double, backslash) of one or more words,
/// returning the words joined by single spaces.
pub fn shell_unquote(s: &str) -> Result<String, String> {
    let mut words: Vec<String> = vec![];
    let mut cur = String::new();
    let mut in_word = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match it.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err("unterminated single quote".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match it.next() {
                        Some('"') => break,
                        Some('\\') => match it.next() {
                            Some(ch @ ('"' | '\\' | '$' | '`')) => cur.push(ch),
                            Some('\n') => {}
                            Some(ch) => {
                                cur.push('\\');
                                cur.push(ch);
                            }
                            None => return Err("unterminated double quote".into()),
                        },
                        Some(ch) => cur.push(ch),
                        None => return Err("unterminated double quote".into()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                match it.next() {
                    Some('\n') => {}
                    Some(ch) => cur.push(ch),
                    None => return Err("trailing backslash".into()),
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words.join(" "))
}

pub fn regex_unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some(n) if n.is_ascii_punctuation() || n == ' ' => out.push(n),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips() {
        let s = "a\"b'c\\d\n\té€😀\u{1}";
        for lang in [Lang::Json, Lang::Js, Lang::C, Lang::Java, Lang::Python] {
            let e = backslash_escape(s, lang);
            assert_eq!(backslash_unescape(&e, lang == Lang::C).unwrap(), s, "{e}");
        }
        assert_eq!(
            shell_unquote(&shell_quote("it's a test")).unwrap(),
            "it's a test"
        );
        assert_eq!(
            csv_unescape(&csv_escape("a,\"b\"", ',')).unwrap(),
            "a,\"b\""
        );
        assert_eq!(
            xml_unescape(&xml_escape("<a & 'b'>\t\n").unwrap()).unwrap(),
            "<a & 'b'>\t\n"
        );
        for bad in ["a\u{0}b", "\u{1b}", "\u{FFFE}"] {
            assert!(xml_escape(bad).unwrap_err().contains("XML 1.0"), "{bad:?}");
        }
        // Malformed multibyte input is an error, not a panic.
        assert!(xml_unescape("&\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}\u{e9}").is_err());
        // Form-style spaces escape literal '+' so the output decodes back.
        let src = "https://example.com/search?q=C++ guide";
        let enc = url_encode(src, true, true);
        assert_eq!(enc, "https://example.com/search?q=C%2B%2B+guide");
        assert_eq!(url_decode(&enc, true).unwrap(), src);
        assert_eq!(url_encode("a+b c", true, false), "a+b%20c");
        // Full-URL mode keeps URL syntax but encodes literal '%'.
        let full = url_encode("https://example.com/100% organic?q=a&b=c", true, false);
        assert_eq!(full, "https://example.com/100%25%20organic?q=a&b=c");
        assert_eq!(
            url_decode(&full, false).unwrap(),
            "https://example.com/100% organic?q=a&b=c"
        );
        assert_eq!(
            url_decode(&url_encode("a b/ü", false, false), false).unwrap(),
            "a b/ü"
        );
        assert!(url_decode("%zz", false).is_err());
        assert!(url_decode("abc%2", false).is_err());
    }
}
