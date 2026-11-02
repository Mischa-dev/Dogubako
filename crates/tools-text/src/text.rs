//! `text.*` tools: transform, compare and clean up text.

use crate::escape::{self, Lang};
use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::{HashMap, HashSet};
use unicode_segmentation::UnicodeSegmentation;

mod extract;
pub(crate) mod more;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = vec![
        change_case(),
        count(),
        sort_lines(),
        dedupe_lines(),
        trim(),
        line_endings(),
        replace(),
        regex_replace(),
        diff(),
        escape_tool(),
    ];
    v.extend(more::tools());
    v.push(extract::tool());
    v
}

fn text_param() -> ParamSpec {
    ParamSpec::textarea("text")
        .label("Text")
        .required()
        .primary()
}

// ---------------------------------------------------------------------------
// text.case

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at_word_start = true;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '’' {
            if at_word_start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            at_word_start = false;
        } else {
            out.push(c);
            at_word_start = c.is_whitespace() || matches!(c, '-' | '/' | '(' | '[' | '"' | '“');
        }
    }
    out
}

fn sentence_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalize = true;
    let mut prev_end = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if capitalize {
                out.extend(c.to_uppercase());
                capitalize = false;
            } else {
                out.extend(c.to_lowercase());
            }
            prev_end = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?' | '…') {
                prev_end = true;
            } else if c.is_whitespace() {
                if prev_end || c == '\n' {
                    capitalize = true;
                }
            } else if c.is_numeric() {
                capitalize = false;
                prev_end = false;
            }
        }
    }
    // Standalone pronoun "i".
    let words: Vec<&str> = out.split_word_bounds().collect();
    words
        .iter()
        .map(|w| if *w == "i" { "I" } else { w })
        .collect()
}

pub(crate) fn convert_case(text: &str, case: &str) -> ToolResult<String> {
    use heck::*;
    let per_line = |f: &dyn Fn(&str) -> String| -> String {
        text.split('\n')
            .map(|l| {
                let (body, cr) = match l.strip_suffix('\r') {
                    Some(b) => (b, "\r"),
                    None => (l, ""),
                };
                let lead = &body[..body.len() - body.trim_start().len()];
                let trail = &body[body.trim_end().len()..];
                let core = body.trim();
                if core.is_empty() {
                    l.to_string()
                } else {
                    format!("{lead}{}{trail}{cr}", f(core))
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(match case {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "title" => title_case(text),
        "sentence" => sentence_case(text),
        "invert" => text
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<String>()
                } else {
                    c.to_uppercase().collect()
                }
            })
            .collect(),
        "alternating" => {
            let mut upper = false;
            text.chars()
                .map(|c| {
                    if c.is_alphabetic() {
                        upper = !upper;
                        if upper {
                            c.to_lowercase().collect::<String>()
                        } else {
                            c.to_uppercase().collect()
                        }
                    } else {
                        c.to_string()
                    }
                })
                .collect()
        }
        "camel" => per_line(&|s| s.to_lower_camel_case()),
        "pascal" => per_line(&|s| s.to_upper_camel_case()),
        "snake" => per_line(&|s| s.to_snake_case()),
        "kebab" => per_line(&|s| s.to_kebab_case()),
        "constant" => per_line(&|s| s.to_shouty_snake_case()),
        "train" => per_line(&|s| s.to_train_case()),
        "cobol" => per_line(&|s| s.to_shouty_kebab_case()),
        "dot" => per_line(&|s| s.to_snake_case().replace('_', ".")),
        "path" => per_line(&|s| s.to_snake_case().replace('_', "/")),
        "words" => per_line(&|s| s.to_title_case().to_lowercase()),
        other => {
            return Err(ToolError::invalid(
                "case",
                format!("unknown case '{other}'"),
            ))
        }
    })
}

fn change_case() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.case", "Change Case", Category::Text)
        .describe("Convert text to UPPER, lower, Title, Sentence, camelCase, snake_case, kebab-case and more")
        .keywords(&["uppercase", "lowercase", "title case", "camel", "snake", "kebab", "pascal", "constant", "capitalize"])
        .param(text_param())
        .param(
            ParamSpec::choice(
                "case",
                &[
                    ("upper", "UPPER CASE"),
                    ("lower", "lower case"),
                    ("title", "Title Case"),
                    ("sentence", "Sentence case"),
                    ("camel", "camelCase"),
                    ("pascal", "PascalCase"),
                    ("snake", "snake_case"),
                    ("kebab", "kebab-case"),
                    ("constant", "CONSTANT_CASE"),
                    ("train", "Train-Case"),
                    ("cobol", "COBOL-CASE"),
                    ("dot", "dot.case"),
                    ("path", "path/case"),
                    ("words", "space separated words"),
                    ("invert", "iNVERT cASE"),
                    ("alternating", "aLtErNaTiNg"),
                ],
            )
            .default_str("upper"),
        )
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        Ok(ToolOutput::text(convert_case(
            p.text("text"),
            p.str("case")?,
        )?))
    })
}

// ---------------------------------------------------------------------------
// text.count

pub(crate) fn word_frequencies(text: &str, min_len: usize, stop: bool) -> Vec<(String, usize)> {
    let stopwords: HashSet<&str> = if stop {
        STOPWORDS.iter().copied().collect()
    } else {
        HashSet::new()
    };
    let mut freq: HashMap<String, usize> = HashMap::new();
    for w in text.unicode_words() {
        let w = w.to_lowercase();
        if w.chars().count() < min_len
            || stopwords.contains(w.as_str())
            || w.chars().all(|c| c.is_numeric())
        {
            continue;
        }
        *freq.entry(w).or_default() += 1;
    }
    let mut v: Vec<_> = freq.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v
}

const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "above",
    "after",
    "again",
    "against",
    "all",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "below",
    "between",
    "both",
    "but",
    "by",
    "can",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "down",
    "during",
    "each",
    "few",
    "for",
    "from",
    "further",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "here",
    "hers",
    "herself",
    "him",
    "himself",
    "his",
    "how",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "me",
    "more",
    "most",
    "my",
    "myself",
    "no",
    "nor",
    "not",
    "now",
    "of",
    "off",
    "on",
    "once",
    "only",
    "or",
    "other",
    "our",
    "ours",
    "ourselves",
    "out",
    "over",
    "own",
    "same",
    "she",
    "should",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "theirs",
    "them",
    "themselves",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "to",
    "too",
    "under",
    "until",
    "up",
    "very",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "while",
    "who",
    "whom",
    "why",
    "will",
    "with",
    "would",
    "you",
    "your",
    "yours",
    "yourself",
    "yourselves",
    "s",
    "t",
    "don",
    "doesn",
    "didn",
    "isn",
    "aren",
    "wasn",
    "weren",
    "won",
    "ll",
    "re",
    "ve",
    "d",
    "m",
];

fn fmt_duration(minutes: f64) -> String {
    let secs = (minutes * 60.0).round() as u64;
    if secs < 60 {
        format!("{secs} sec")
    } else if secs < 3600 {
        format!("{} min {} sec", secs / 60, secs % 60)
    } else {
        format!("{} h {} min", secs / 3600, (secs % 3600) / 60)
    }
}

fn count() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.count", "Word & Character Count", Category::Text)
        .describe("Count characters, words, lines, sentences, paragraphs and bytes; estimate reading and speaking time; list the most frequent words")
        .keywords(&["word count", "character count", "letters", "reading time", "statistics", "length"])
        .param(text_param())
        .param(ParamSpec::integer("reading_wpm").label("Reading speed (words/min)").default_int(238).range(50.0, 1000.0).advanced())
        .param(ParamSpec::integer("speaking_wpm").label("Speaking speed (words/min)").default_int(150).range(50.0, 500.0).advanced())
        .param(ParamSpec::integer("top").label("Top words").default_int(10).range(0.0, 100.0).advanced())
        .param(ParamSpec::boolean("ignore_stopwords").label("Ignore common English words in top words").default_bool(true).advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let t = p.text("text");
        let words = t.unicode_words().count();
        let chars = t.chars().count();
        let graphemes = t.graphemes(true).count();
        let no_ws = t.chars().filter(|c| !c.is_whitespace()).count();
        let lines = if t.is_empty() {
            0
        } else {
            split_lines(t).0.len()
        };
        let sentences = t
            .unicode_sentences()
            .filter(|s| s.chars().any(|c| c.is_alphanumeric()))
            .count();
        let paragraphs = t
            .replace("\r\n", "\n")
            .split("\n\n")
            .filter(|p| !p.trim().is_empty())
            .count();
        let rw = p.int_or("reading_wpm", 238).max(1) as f64;
        let sw = p.int_or("speaking_wpm", 150).max(1) as f64;
        let top = word_frequencies(t, 1, p.bool("ignore_stopwords"));
        let top: Vec<Vec<Value>> = top
            .into_iter()
            .take(p.int_or("top", 10) as usize)
            .map(|(w, c)| vec![json!(w), json!(c)])
            .collect();
        let avg_word = if words > 0 {
            t.unicode_words().map(|w| w.chars().count()).sum::<usize>() as f64 / words as f64
        } else {
            0.0
        };
        Ok(ToolOutput::data(json!({
            "characters": chars,
            "characters_no_spaces": no_ws,
            "graphemes": graphemes,
            "words": words,
            "lines": lines,
            "sentences": sentences,
            "paragraphs": paragraphs,
            "bytes_utf8": t.len(),
            "average_word_length": (avg_word * 10.0).round() / 10.0,
            "reading_time": fmt_duration(words as f64 / rw),
            "speaking_time": fmt_duration(words as f64 / sw),
            "table": table(&["word", "count"], top),
        })))
    })
}

// ---------------------------------------------------------------------------
// text.sort_lines

fn leading_number(s: &str) -> Option<f64> {
    let t = s.trim_start();
    // Numeric prefix: digits, '.', 'e'/'E', and a sign at the start or right
    // after the exponent marker (`1e-3`, `2E+3`). All ASCII, so byte indices.
    let b = t.as_bytes();
    let mut end = 0;
    while end < b.len() {
        let c = b[end];
        let sign_ok = (c == b'-' || c == b'+') && (end == 0 || matches!(b[end - 1], b'e' | b'E'));
        if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || sign_ok {
            end += 1;
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    // shrink until it parses (handles "1.2.3", "3e" ...)
    let mut e = end;
    while e > 0 {
        if let Ok(v) = t[..e].parse::<f64>() {
            return Some(v);
        }
        e -= 1;
    }
    None
}

fn sort_lines() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.sort_lines", "Sort Lines", Category::Text)
        .describe("Sort lines alphabetically, naturally, numerically, by length or shuffle them; optionally reverse and remove duplicates")
        .keywords(&["sort", "order", "alphabetize", "shuffle", "randomize", "natural sort", "lines"])
        .param(text_param())
        .param(
            ParamSpec::choice(
                "mode",
                &[("alpha", "Alphabetical"), ("natural", "Natural (file2 < file10)"), ("numeric", "Numeric"), ("length", "By length"), ("random", "Shuffle (random)")],
            )
            .default_str("natural"),
        )
        .param(ParamSpec::boolean("reverse").label("Reverse order"))
        .param(ParamSpec::boolean("case_insensitive").label("Ignore case").default_bool(true))
        .param(ParamSpec::boolean("unique").label("Remove duplicates"))
        .param(ParamSpec::boolean("remove_empty").label("Remove empty lines"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (lines, trailing) = split_lines(p.text("text"));
        let ci = p.bool("case_insensitive");
        let key = |s: &str| if ci { s.to_lowercase() } else { s.to_string() };
        let mut lines: Vec<&str> = lines;
        if p.bool("remove_empty") {
            lines.retain(|l| !l.trim().is_empty());
        }
        if p.bool("unique") {
            let mut seen = HashSet::new();
            lines.retain(|l| seen.insert(key(l)));
        }
        match p.str("mode")? {
            "alpha" => lines.sort_by_cached_key(|l| key(l)),
            "natural" => lines.sort_by(|a, b| natural_cmp(&key(a), &key(b)).then_with(|| a.cmp(b))),
            "numeric" => lines.sort_by(|a, b| match (leading_number(a), leading_number(b)) {
                (Some(x), Some(y)) => x.total_cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => natural_cmp(&key(a), &key(b)),
            }),
            "length" => lines.sort_by(|a, b| {
                a.chars()
                    .count()
                    .cmp(&b.chars().count())
                    .then_with(|| natural_cmp(&key(a), &key(b)))
            }),
            "random" => {
                use rand::seq::SliceRandom;
                lines.shuffle(&mut rand::rng());
            }
            other => return Err(ToolError::invalid("mode", format!("unknown mode {other}"))),
        }
        if p.bool("reverse") {
            lines.reverse();
        }
        Ok(ToolOutput::text(join_lines(&lines, trailing)))
    })
}

// ---------------------------------------------------------------------------
// text.dedupe_lines

fn dedupe_lines() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.dedupe_lines", "Remove Duplicate Lines", Category::Text)
        .describe("Remove duplicate lines while keeping the original order, keeping the first or last occurrence")
        .keywords(&["duplicates", "unique", "dedupe", "distinct", "uniq"])
        .param(text_param())
        .param(ParamSpec::choice("keep", &[("first", "Keep first occurrence"), ("last", "Keep last occurrence")]).default_str("first"))
        .param(ParamSpec::boolean("ignore_case").label("Ignore case"))
        .param(ParamSpec::boolean("ignore_whitespace").label("Ignore leading/trailing and repeated whitespace"))
        .param(ParamSpec::boolean("keep_empty").label("Keep all empty lines").default_bool(true).advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (lines, trailing) = split_lines(p.text("text"));
        let (ic, iw, ke) = (
            p.bool("ignore_case"),
            p.bool("ignore_whitespace"),
            p.bool("keep_empty"),
        );
        let key = |s: &str| {
            let mut k = if iw {
                s.split_whitespace().collect::<Vec<_>>().join(" ")
            } else {
                s.to_string()
            };
            if ic {
                k = k.to_lowercase();
            }
            k
        };
        let last = p.str("keep")? == "last";
        let mut seen = HashSet::new();
        let mut keep = vec![false; lines.len()];
        let order: Box<dyn Iterator<Item = usize>> = if last {
            Box::new((0..lines.len()).rev())
        } else {
            Box::new(0..lines.len())
        };
        for i in order {
            if ke && lines[i].trim().is_empty() {
                keep[i] = true;
                continue;
            }
            keep[i] = seen.insert(key(lines[i]));
        }
        let out: Vec<&str> = lines
            .iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(l, _)| *l)
            .collect();
        let removed = lines.len() - out.len();
        Ok(ToolOutput::text(join_lines(&out, trailing))
            .with_data(json!({"removed": removed, "kept": out.len(), "total": lines.len()})))
    })
}

// ---------------------------------------------------------------------------
// text.trim

fn trim() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.trim", "Trim & Remove Whitespace", Category::Text)
        .describe("Trim leading/trailing spaces on every line, collapse repeated spaces and remove empty lines")
        .keywords(&["trim", "whitespace", "spaces", "strip", "empty lines", "blank lines", "tabs"])
        .param(text_param())
        .param(
            ParamSpec::choice("lines", &[("both", "Trim both ends"), ("leading", "Trim leading"), ("trailing", "Trim trailing"), ("none", "Don't trim lines")])
                .label("Trim each line")
                .default_str("both"),
        )
        .param(ParamSpec::boolean("collapse_spaces").label("Collapse repeated spaces/tabs into one space").default_bool(true))
        .param(
            ParamSpec::choice("empty_lines", &[("keep", "Keep"), ("collapse", "Collapse runs into one"), ("remove", "Remove all")])
                .label("Empty lines")
                .default_str("collapse"),
        )
        .param(ParamSpec::boolean("tabs_to_spaces").label("Convert tabs to spaces").advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let text = p.text("text");
        let (lines, _) = split_lines(text);
        let mode = p.str("lines")?;
        let collapse = p.bool("collapse_spaces");
        let tabs = p.bool("tabs_to_spaces");
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        for l in lines {
            let mut s = if tabs {
                l.replace('\t', "    ")
            } else {
                l.to_string()
            };
            s = match mode {
                "both" => s.trim().to_string(),
                "leading" => s.trim_start().to_string(),
                "trailing" => s.trim_end().to_string(),
                _ => s,
            };
            if collapse {
                let lead_len = s.len() - s.trim_start_matches([' ', '\t']).len();
                let lead = if mode == "none" || mode == "trailing" {
                    s[..lead_len].to_string()
                } else {
                    String::new()
                };
                let mut c = String::with_capacity(s.len());
                let mut prev_space = false;
                for ch in s[lead_len..].chars() {
                    if ch == ' ' || ch == '\t' {
                        if !prev_space {
                            c.push(' ');
                        }
                        prev_space = true;
                    } else {
                        c.push(ch);
                        prev_space = false;
                    }
                }
                s = lead + &c;
            }
            out.push(s);
        }
        match p.str("empty_lines")? {
            "remove" => out.retain(|l| !l.trim().is_empty()),
            "collapse" => {
                out.dedup_by(|a, b| a.trim().is_empty() && b.trim().is_empty());
                while out.first().is_some_and(|l| l.trim().is_empty()) {
                    out.remove(0);
                }
                while out.last().is_some_and(|l| l.trim().is_empty()) {
                    out.pop();
                }
            }
            _ => {}
        }
        Ok(ToolOutput::text(out.join("\n")))
    })
}

// ---------------------------------------------------------------------------
// text.line_endings

fn line_endings() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.line_endings", "Line Endings", Category::Text)
        .describe("Detect and convert line endings between LF (Unix), CRLF (Windows) and CR (classic Mac)")
        .keywords(&["crlf", "lf", "newline", "eol", "dos2unix", "unix2dos", "carriage return"])
        .param(ParamSpec::textarea("text").label("Text").primary())
        .param(ParamSpec::file("file").label("Or upload a file").describe("When a file is given, a converted file is produced"))
        .param(ParamSpec::choice("to", &[("detect", "Only detect"), ("lf", "LF (Unix, macOS)"), ("crlf", "CRLF (Windows)"), ("cr", "CR (classic Mac)")]).default_str("lf"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let file_bytes = match p.opt_file("file") {
            Some(f) => Some(read_file(f)?),
            None => None,
        };
        let bytes: &[u8] = match &file_bytes {
            Some(b) => b,
            None => p.text("text").as_bytes(),
        };
        let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                    crlf += 1;
                    i += 1;
                }
                b'\r' => cr += 1,
                b'\n' => lf += 1,
                _ => {}
            }
            i += 1;
        }
        let kinds = [(crlf > 0, "CRLF"), (lf > 0, "LF"), (cr > 0, "CR")];
        let present: Vec<&str> = kinds.iter().filter(|k| k.0).map(|k| k.1).collect();
        let detected = match present.len() {
            0 => "none",
            1 => present[0],
            _ => "mixed",
        };
        let data = json!({"detected": detected, "crlf": crlf, "lf": lf, "cr": cr});
        let to = p.str("to")?;
        if to == "detect" {
            return Ok(ToolOutput::data(data));
        }
        let nl: &[u8] = match to {
            "crlf" => b"\r\n",
            "cr" => b"\r",
            _ => b"\n",
        };
        let mut out = Vec::with_capacity(bytes.len() + crlf + lf);
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' => {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    out.extend_from_slice(nl);
                }
                b'\n' => out.extend_from_slice(nl),
                b => out.push(b),
            }
            i += 1;
        }
        if let Some(f) = p.opt_file("file") {
            let a = ctx.write_output(&suffixed(&f.name, to, None), &out)?;
            return Ok(ToolOutput::files(vec![a]).with_data(data));
        }
        Ok(ToolOutput::text(String::from_utf8_lossy(&out).into_owned()).with_data(data))
    })
}

// ---------------------------------------------------------------------------
// text.replace

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn replace() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.replace", "Find & Replace", Category::Text)
        .describe("Find and replace plain text, optionally case-insensitive or whole words only")
        .keywords(&["find", "replace", "substitute", "search"])
        .param(text_param())
        .param(ParamSpec::text("find").label("Find").required())
        .param(
            ParamSpec::text("replace")
                .label("Replace with")
                .default_str(""),
        )
        .param(
            ParamSpec::boolean("case_sensitive")
                .label("Case sensitive")
                .default_bool(true),
        )
        .param(ParamSpec::boolean("whole_word").label("Whole words only"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let text = p.text("text");
        let find = p.text("find");
        if find.is_empty() {
            return Err(ToolError::invalid("find", "search text must not be empty"));
        }
        let re = regex::RegexBuilder::new(&regex::escape(find))
            .case_insensitive(!p.bool("case_sensitive"))
            .size_limit(64 << 20)
            .build()
            .ctx("building matcher")?;
        let whole = p.bool("whole_word");
        let rep = p.text("replace");
        let mut out = String::with_capacity(text.len());
        let mut last = 0;
        let mut n = 0;
        for m in re.find_iter(text) {
            if whole {
                let before = text[..m.start()].chars().next_back();
                let after = text[m.end()..].chars().next();
                let first = m.as_str().chars().next().unwrap();
                let lastc = m.as_str().chars().next_back().unwrap();
                if (is_word_char(first) && before.is_some_and(is_word_char))
                    || (is_word_char(lastc) && after.is_some_and(is_word_char))
                {
                    continue;
                }
            }
            out.push_str(&text[last..m.start()]);
            out.push_str(rep);
            last = m.end();
            n += 1;
        }
        out.push_str(&text[last..]);
        Ok(ToolOutput::text(out).with_data(json!({"replacements": n})))
    })
}

// ---------------------------------------------------------------------------
// text.regex_replace

pub(crate) fn build_regex(pattern: &str, flags: &[String]) -> ToolResult<regex::Regex> {
    let has = |f: &str| flags.iter().any(|x| x == f);
    regex::RegexBuilder::new(pattern)
        .case_insensitive(has("i"))
        .multi_line(has("m"))
        .dot_matches_new_line(has("s"))
        .ignore_whitespace(has("x"))
        .unicode(!has("ascii"))
        .size_limit(64 << 20)
        .build()
        .map_err(|e| ToolError::invalid("pattern", e.to_string()))
}

pub(crate) fn flag_param() -> ParamSpec {
    ParamSpec::multi_choice(
        "flags",
        &[
            ("i", "i – ignore case"),
            ("m", "m – ^/$ match at line breaks"),
            ("s", "s – . matches newline"),
            ("x", "x – verbose (ignore whitespace, # comments)"),
        ],
    )
    .label("Flags")
    .default_value(json!([]))
}

fn regex_replace() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.regex_replace", "Regex Replace", Category::Text)
        .describe("Search and replace with a regular expression; use $1 or ${name} in the replacement to insert capture groups")
        .keywords(&["regex", "regular expression", "replace", "substitute", "sed", "capture groups"])
        .param(text_param())
        .param(ParamSpec::text("pattern").label("Pattern").required().placeholder("(\\w+)@(\\w+)"))
        .param(ParamSpec::text("replacement").label("Replacement").default_str("").placeholder("$2 at $1"))
        .param(flag_param())
        .param(ParamSpec::integer("limit").label("Max replacements (0 = all)").default_int(0).min(0.0).advanced())
        .param(ParamSpec::boolean("literal").label("Treat replacement literally (no $ expansion)").advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let text = p.text("text");
        let re = build_regex(p.text("pattern"), &p.list("flags"))?;
        let limit = p.int_or("limit", 0).max(0) as usize;
        let total = re.find_iter(text).count();
        let rep = p.text("replacement");
        let out = if p.bool("literal") {
            re.replacen(text, limit, regex::NoExpand(rep))
        } else {
            re.replacen(text, limit, rep)
        };
        let n = if limit == 0 { total } else { total.min(limit) };
        Ok(ToolOutput::text(out.into_owned())
            .with_data(json!({"matches": total, "replacements": n})))
    })
}

// ---------------------------------------------------------------------------
// text.diff

fn diff() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.diff", "Compare Text (Diff)", Category::Text)
        .describe("Compare two texts and highlight insertions and deletions by line, word or character; also produces a unified diff")
        .keywords(&["diff", "compare", "difference", "changes", "patch", "unified diff"])
        .param(ParamSpec::textarea("original").label("Original").required().primary())
        .param(ParamSpec::textarea("changed").label("Changed").required())
        .param(ParamSpec::choice("granularity", &[("line", "Lines"), ("word", "Words"), ("char", "Characters")]).default_str("word"))
        .param(
            ParamSpec::choice("whitespace", &[("exact", "Compare exactly"), ("changes", "Ignore whitespace changes"), ("all", "Ignore all whitespace")])
                .default_str("exact")
                .advanced(),
        )
        .param(ParamSpec::integer("context").label("Unified diff context lines").default_int(3).range(0.0, 1000.0).advanced())
        .text_output("text/x-diff")
        .ui("diff");
    sync_tool(spec, |ctx, p| {
        use similar::{ChangeTag, TextDiffConfig, WhitespaceMode};
        let a = p.text("original");
        let b = p.text("changed");
        let mut cfg = TextDiffConfig::default();
        cfg.timeout(std::time::Duration::from_secs(20));
        cfg.whitespace_mode(match p.str("whitespace")? {
            "changes" => WhitespaceMode::IgnoreChanges,
            "all" => WhitespaceMode::IgnoreAll,
            _ => WhitespaceMode::Exact,
        });
        ctx.check_cancelled()?;
        let d = match p.str("granularity")? {
            "line" => cfg.diff_lines(a, b),
            "char" => cfg.diff_chars(a, b),
            _ => cfg.diff_words(a, b),
        };
        // Chunks are owned strings appended in place and converted to JSON
        // once (re-cloning a chunk per token is quadratic).
        let mut chunks: Vec<(&'static str, String)> = vec![];
        let (mut ins, mut del) = (0usize, 0usize);
        for (n, c) in d.iter_all_changes().enumerate() {
            if n % 65_536 == 0 {
                ctx.check_cancelled()?;
            }
            let tag = match c.tag() {
                ChangeTag::Equal => "equal",
                ChangeTag::Insert => {
                    ins += 1;
                    "insert"
                }
                ChangeTag::Delete => {
                    del += 1;
                    "delete"
                }
            };
            match chunks.last_mut() {
                Some((t, text)) if *t == tag => text.push_str(c.value()),
                _ => chunks.push((tag, c.value().to_string())),
            }
        }
        let chunks: Vec<Value> = chunks
            .into_iter()
            .map(|(tag, text)| json!({"tag": tag, "text": text}))
            .collect();
        let line_diff = cfg.diff_lines(a, b);
        let unified = line_diff
            .unified_diff()
            .context_radius(p.int_or("context", 3) as usize)
            .header("original", "changed")
            .to_string();
        let identical = a == b;
        Ok(ToolOutput::text_as(
            if identical { String::new() } else { unified },
            "text/x-diff",
        )
        .with_data(json!({
            "identical": identical,
            "similarity": (d.ratio() * 1000.0).round() / 1000.0,
            "insertions": ins,
            "deletions": del,
            "diff": chunks,
        })))
    })
}

// ---------------------------------------------------------------------------
// text.escape

pub(crate) fn do_escape(format: &str, unescape: bool, s: &str) -> ToolResult<String> {
    let bad = |e: String| ToolError::invalid("text", e);
    Ok(match (format, unescape) {
        ("json", false) => escape::backslash_escape(s, Lang::Json),
        ("json", true) => {
            let t = s.trim();
            let quoted = if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
                t.to_string()
            } else {
                format!("\"{s}\"")
            };
            serde_json::from_str::<String>(&quoted)
                .map_err(|e| bad(format!("invalid JSON string: {e}")))?
        }
        ("javascript", false) => escape::backslash_escape(s, Lang::Js),
        ("c", false) => escape::backslash_escape(s, Lang::C),
        ("java", false) => escape::backslash_escape(s, Lang::Java),
        ("python", false) => escape::backslash_escape(s, Lang::Python),
        ("javascript" | "java" | "python", true) => {
            escape::backslash_unescape(s, false).map_err(bad)?
        }
        ("c", true) => escape::backslash_unescape(s, true).map_err(bad)?,
        ("html", false) => escape::html_escape(s, false),
        ("html", true) => escape::html_unescape(s),
        ("xml", false) => escape::xml_escape(s).map_err(bad)?,
        ("xml", true) => escape::xml_unescape(s).map_err(bad)?,
        ("url", false) => escape::url_encode(s, false, false),
        ("url", true) => escape::url_decode(s, true).map_err(bad)?,
        ("csv", false) => escape::csv_escape(s, ','),
        ("csv", true) => escape::csv_unescape(s).map_err(bad)?,
        ("shell", false) => escape::shell_quote(s),
        ("shell", true) => escape::shell_unquote(s).map_err(bad)?,
        ("regex", false) => regex::escape(s),
        ("regex", true) => escape::regex_unescape(s),
        ("sql", false) => s.replace('\'', "''"),
        ("sql", true) => {
            let t = s.trim();
            let inner = if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
                &t[1..t.len() - 1]
            } else {
                s
            };
            inner.replace("''", "'")
        }
        (f, _) => return Err(ToolError::invalid("format", format!("unknown format {f}"))),
    })
}

fn escape_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.escape", "Escape / Unescape", Category::Text)
        .describe("Escape or unescape text for JSON, JavaScript, HTML, XML, URLs, CSV, shell, regex, C, Java, Python and SQL string literals")
        .keywords(&["escape", "unescape", "quote", "string literal", "backslash", "html entities", "sql injection", "shell quote"])
        .param(text_param())
        .param(
            ParamSpec::choice(
                "format",
                &[
                    ("json", "JSON string"),
                    ("javascript", "JavaScript string"),
                    ("html", "HTML"),
                    ("xml", "XML"),
                    ("url", "URL (percent-encoding)"),
                    ("csv", "CSV field"),
                    ("shell", "Shell (POSIX single quotes)"),
                    ("regex", "Regular expression"),
                    ("c", "C/C++ string"),
                    ("java", "Java string"),
                    ("python", "Python string"),
                    ("sql", "SQL string literal"),
                ],
            )
            .default_str("json"),
        )
        .param(ParamSpec::choice("mode", &[("escape", "Escape"), ("unescape", "Unescape")]).default_str("escape"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        Ok(ToolOutput::text(do_escape(
            p.str("format")?,
            p.str("mode")? == "unescape",
            p.text("text"),
        )?))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    fn h() -> TestHarness {
        TestHarness::new(crate::tools())
    }

    async fn txt(h: &TestHarness, id: &str, p: serde_json::Value) -> String {
        h.run(id, p).await.unwrap().text.unwrap()
    }

    #[tokio::test]
    async fn cases() {
        let h = h();
        let t = |c: &'static str, s: &'static str| (c, s);
        for (case, want) in [
            t("upper", "HELLO WORLD-FOO"),
            t("lower", "hello world-foo"),
            t("title", "Hello World-Foo"),
            t("camel", "helloWorldFoo"),
            t("pascal", "HelloWorldFoo"),
            t("snake", "hello_world_foo"),
            t("kebab", "hello-world-foo"),
            t("constant", "HELLO_WORLD_FOO"),
            t("dot", "hello.world.foo"),
            t("invert", "hELLO WORLD-FOO"),
        ] {
            assert_eq!(
                txt(
                    &h,
                    "text.case",
                    json!({"text": "Hello world-foo", "case": case})
                )
                .await,
                want,
                "{case}"
            );
        }
        assert_eq!(
            txt(
                &h,
                "text.case",
                json!({"text": "HELLO. how are you? i am fine", "case": "sentence"})
            )
            .await,
            "Hello. How are you? I am fine"
        );
        let err = h
            .run("text.case", json!({"text": "x", "case": "nope"}))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_input");
        let err = h.run("text.case", json!({"bogus": 1})).await.unwrap_err();
        assert_eq!(err.code(), "invalid_input");
    }

    #[tokio::test]
    async fn count() {
        let out = h()
            .run(
                "text.count",
                json!({"text": "Hello world. Hello again!\n\nNew para."}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["words"], 6);
        assert_eq!(out.data["sentences"], 3);
        assert_eq!(out.data["paragraphs"], 2);
        assert_eq!(out.data["lines"], 3);
        assert_eq!(out.data["table"]["rows"][0][0], "hello");
    }

    #[tokio::test]
    async fn sort_and_dedupe() {
        let h = h();
        // Signed exponents are part of the number.
        assert_eq!(
            txt(
                &h,
                "text.sort_lines",
                json!({"text": "1e-3\n0.5\n2e+3\n10\n-4 apples", "mode": "numeric"})
            )
            .await,
            "-4 apples\n1e-3\n0.5\n10\n2e+3"
        );
        assert_eq!(
            txt(
                &h,
                "text.sort_lines",
                json!({"text": "f10\nf2\nF1\n", "mode": "natural"})
            )
            .await,
            "F1\nf2\nf10\n"
        );
        assert_eq!(
            txt(
                &h,
                "text.sort_lines",
                json!({"text": "10\n9\nx\n-1", "mode": "numeric"})
            )
            .await,
            "-1\n9\n10\nx"
        );
        assert_eq!(
            txt(
                &h,
                "text.sort_lines",
                json!({"text": "b\na\nb", "mode": "alpha", "unique": true, "reverse": true})
            )
            .await,
            "b\na"
        );
        let shuffled = txt(
            &h,
            "text.sort_lines",
            json!({"text": "1\n2\n3", "mode": "random"}),
        )
        .await;
        let mut v: Vec<&str> = shuffled.lines().collect();
        v.sort();
        assert_eq!(v, ["1", "2", "3"]);
        let out = h
            .run(
                "text.dedupe_lines",
                json!({"text": "a\nB\nb \nA\nc", "ignore_case": true, "ignore_whitespace": true}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "a\nB\nc");
        assert_eq!(out.data["removed"], 2);
        let out = h
            .run(
                "text.dedupe_lines",
                json!({"text": "a\nb\na", "keep": "last"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "b\na");
    }

    #[tokio::test]
    async fn trim_and_endings() {
        let h = h();
        assert_eq!(
            txt(
                &h,
                "text.trim",
                json!({"text": "  a   b \n\n\n c\t\td  \n"})
            )
            .await,
            "a b\n\nc d"
        );
        assert_eq!(
            txt(
                &h,
                "text.trim",
                json!({"text": "a\n\nb", "empty_lines": "remove"})
            )
            .await,
            "a\nb"
        );
        let out = h
            .run(
                "text.line_endings",
                json!({"text": "a\r\nb\nc", "to": "crlf"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "a\r\nb\r\nc");
        assert_eq!(out.data["detected"], "mixed");
        let f = h.write("w.txt", b"x\r\ny\r\n");
        let out = h
            .run(
                "text.line_endings",
                json!({"file": f.to_str().unwrap(), "to": "lf"}),
            )
            .await
            .unwrap();
        assert_eq!(std::fs::read(&out.artifacts[0].path).unwrap(), b"x\ny\n");
        assert_eq!(out.data["detected"], "CRLF");
    }

    #[tokio::test]
    async fn replace_tools() {
        let h = h();
        let out = h.run("text.replace", json!({"text": "cat Cat concat cat.", "find": "cat", "replace": "dog", "case_sensitive": false, "whole_word": true})).await.unwrap();
        assert_eq!(out.text.unwrap(), "dog dog concat dog.");
        assert_eq!(out.data["replacements"], 3);
        assert_eq!(
            h.run("text.replace", json!({"text": "x", "find": ""}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        let out = h.run("text.regex_replace", json!({"text": "john@example bob@test", "pattern": "(?P<u>\\w+)@(\\w+)", "replacement": "$2:${u}"})).await.unwrap();
        assert_eq!(out.text.unwrap(), "example:john test:bob");
        assert_eq!(out.data["matches"], 2);
        assert_eq!(
            txt(
                &h,
                "text.regex_replace",
                json!({"text": "A a", "pattern": "a", "replacement": "b", "flags": ["i"]})
            )
            .await,
            "b b"
        );
        assert_eq!(
            h.run(
                "text.regex_replace",
                json!({"text": "x", "pattern": "(unclosed"})
            )
            .await
            .unwrap_err()
            .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn diff() {
        let h = h();
        let out = h
            .run(
                "text.diff",
                json!({"original": "the quick fox\nline2\n", "changed": "the slow fox\nline2\n"}),
            )
            .await
            .unwrap();
        let d = out.data["diff"].as_array().unwrap();
        assert!(d
            .iter()
            .any(|c| c["tag"] == "delete" && c["text"] == "quick"));
        assert!(d
            .iter()
            .any(|c| c["tag"] == "insert" && c["text"] == "slow"));
        assert!(out.text.unwrap().contains("-the quick fox"));
        let out = h
            .run(
                "text.diff",
                json!({"original": "same", "changed": "same", "granularity": "char"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["identical"], true);
    }

    #[tokio::test]
    async fn escapes() {
        let h = h();
        let e = |f: &'static str, m: &'static str, t: &'static str| json!({"format": f, "mode": m, "text": t});
        assert_eq!(
            txt(&h, "text.escape", e("json", "escape", "a\"b\n")).await,
            "a\\\"b\\n"
        );
        assert_eq!(
            txt(&h, "text.escape", e("json", "unescape", "a\\\"b\\n\\u00e9")).await,
            "a\"b\né"
        );
        assert_eq!(
            txt(&h, "text.escape", e("html", "escape", "<a href='x'>")).await,
            "&lt;a href=&#39;x&#39;&gt;"
        );
        assert_eq!(
            txt(
                &h,
                "text.escape",
                e("html", "unescape", "&lt;&eacute;&#x41;")
            )
            .await,
            "<éA"
        );
        assert_eq!(
            txt(&h, "text.escape", e("shell", "escape", "it's")).await,
            "'it'\\''s'"
        );
        assert_eq!(
            txt(&h, "text.escape", e("sql", "escape", "O'Brien")).await,
            "O''Brien"
        );
        assert_eq!(
            txt(&h, "text.escape", e("url", "escape", "a b&c")).await,
            "a%20b%26c"
        );
        assert_eq!(
            txt(&h, "text.escape", e("regex", "escape", "a.b*")).await,
            "a\\.b\\*"
        );
        assert_eq!(
            txt(&h, "text.escape", e("csv", "escape", "a,b")).await,
            "\"a,b\""
        );
        assert_eq!(
            h.run("text.escape", e("json", "unescape", "bad \\q"))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("text.escape", e("xml", "unescape", "&bogus;"))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
}
