//! `dev.regex`: regular expression tester.

use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![regex_tester()]
}

/// Converts byte offsets to UTF-16 code unit offsets (what JavaScript uses).
struct Utf16Index {
    /// (byte offset, utf16 offset) for every char boundary.
    map: Vec<(usize, usize)>,
}

impl Utf16Index {
    fn new(s: &str) -> Self {
        let mut map = Vec::with_capacity(s.len() + 1);
        let mut u = 0;
        for (b, c) in s.char_indices() {
            map.push((b, u));
            u += c.len_utf16();
        }
        map.push((s.len(), u));
        Utf16Index { map }
    }
    fn at(&self, byte: usize) -> usize {
        match self.map.binary_search_by_key(&byte, |e| e.0) {
            Ok(i) => self.map[i].1,
            Err(i) => self.map[i.saturating_sub(1)].1,
        }
    }
}

struct M {
    start: usize,
    end: usize,
    groups: Vec<Option<(usize, usize)>>,
}

fn regex_tester() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.regex", "Regex Tester", Category::Dev)
        .describe("Test a regular expression against text: highlights every match with numbered and named capture groups, previews a replacement and explains syntax errors")
        .keywords(&["regex", "regexp", "regular expression", "tester", "match", "capture groups", "lookahead", "pcre"])
        .param(ParamSpec::text("pattern").label("Pattern").required().primary().placeholder("(?P<year>\\d{4})-(\\d{2})"))
        .param(ParamSpec::textarea("text").label("Test text").required())
        .param(crate::text::flag_param())
        .param(
            ParamSpec::choice("engine", &[("standard", "Standard (fast, linear time)"), ("fancy", "Extended (lookaround, backreferences)")])
                .default_str("standard")
                .describe("The extended engine supports (?=…), (?<=…), \\1 backreferences and atomic groups, with a backtracking limit"),
        )
        .param(ParamSpec::text("replacement").label("Replacement preview (optional)").placeholder("$year/${2}"))
        .param(ParamSpec::integer("max_matches").label("Max matches").default_int(1000).range(1.0, 100000.0).advanced())
        .output(OutputKind::Data)
        .ui("regex");
    sync_tool(spec, |_ctx, p| {
        let pattern = p.text("pattern");
        let text = p.text("text");
        let flags = p.list("flags");
        let max = p.int_or("max_matches", 1000).max(1) as usize;
        let fancy = p.str_or("engine", "standard") == "fancy";
        let mut matches: Vec<M> = vec![];
        let names: Vec<Option<String>>;
        let replaced: Option<String>;
        let mut truncated = false;
        if fancy {
            let mut prefix = String::new();
            for (f, c) in [("i", 'i'), ("m", 'm'), ("s", 's'), ("x", 'x')] {
                if flags.iter().any(|x| x == f) {
                    prefix.push(c);
                }
            }
            let full = if prefix.is_empty() { pattern.to_string() } else { format!("(?{prefix}){pattern}") };
            let re = fancy_regex::RegexBuilder::new(&full).backtrack_limit(1_000_000).build().map_err(|e| ToolError::invalid("pattern", e.to_string()))?;
            names = re.capture_names().map(|n| n.map(|s| s.to_string())).collect();
            for c in re.captures_iter(text) {
                let c = c.map_err(|e| ToolError::invalid("pattern", format!("matching failed: {e}")))?;
                if matches.len() >= max {
                    truncated = true;
                    break;
                }
                let m0 = c.get(0).unwrap();
                matches.push(M { start: m0.start(), end: m0.end(), groups: (1..c.len()).map(|i| c.get(i).map(|g| (g.start(), g.end()))).collect() });
            }
            replaced = match p.opt_str("replacement") {
                Some(r) => Some(re.try_replacen(text, 0, r).map_err(|e| ToolError::invalid("pattern", e.to_string()))?.into_owned()),
                None => None,
            };
        } else {
            let re = crate::text::build_regex(pattern, &flags).map_err(|e| match e {
                ToolError::InvalidInput { message, .. } if message.contains("look-around") || message.contains("backreferences") => {
                    ToolError::invalid("pattern", format!("{message}\nTip: switch the engine to 'Extended' for lookaround and backreferences."))
                }
                e => e,
            })?;
            names = re.capture_names().map(|n| n.map(|s| s.to_string())).collect();
            for c in re.captures_iter(text) {
                if matches.len() >= max {
                    truncated = true;
                    break;
                }
                let m0 = c.get(0).unwrap();
                matches.push(M { start: m0.start(), end: m0.end(), groups: (1..c.len()).map(|i| c.get(i).map(|g| (g.start(), g.end()))).collect() });
            }
            replaced = p.opt_str("replacement").map(|r| re.replace_all(text, r).into_owned());
        }
        let idx = Utf16Index::new(text);
        let group_names: Vec<Value> = names.iter().skip(1).map(|n| json!(n)).collect();
        let list: Vec<Value> = matches
            .iter()
            .map(|m| {
                let mut named = serde_json::Map::new();
                let groups: Vec<Value> = m
                    .groups
                    .iter()
                    .enumerate()
                    .map(|(i, g)| {
                        let v = g.map(|(s, e)| json!(&text[s..e])).unwrap_or(Value::Null);
                        if let Some(Some(n)) = names.get(i + 1) {
                            named.insert(n.clone(), v.clone());
                        }
                        v
                    })
                    .collect();
                let spans: Vec<Value> = m.groups.iter().map(|g| g.map(|(s, e)| json!([idx.at(s), idx.at(e)])).unwrap_or(Value::Null)).collect();
                json!({
                    "start": idx.at(m.start),
                    "end": idx.at(m.end),
                    "byte_start": m.start,
                    "byte_end": m.end,
                    "text": &text[m.start..m.end],
                    "groups": groups,
                    "group_spans": spans,
                    "named": named,
                })
            })
            .collect();
        let mut data = json!({
            "count": list.len(),
            "group_names": group_names,
            "engine": if fancy { "extended" } else { "standard" },
            "offsets": "start/end are UTF-16 code unit offsets (JavaScript string indices); byte_start/byte_end are UTF-8 byte offsets",
            "matches": list,
        });
        if let Some(r) = &replaced {
            data["replaced"] = json!(r);
        }
        let mut out = ToolOutput::data(data);
        if truncated {
            out = out.warn(format!("Stopped after {max} matches"));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    #[tokio::test]
    async fn regex_tester() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("dev.regex", json!({"pattern": "(?P<y>\\d{4})-(\\d{2})", "text": "é 2024-05 and 1999-12", "replacement": "$2/${y}"})).await.unwrap();
        assert_eq!(out.data["count"], 2);
        let m = &out.data["matches"][0];
        assert_eq!(m["text"], "2024-05");
        assert_eq!(m["start"], 2);
        assert_eq!(m["byte_start"], 3);
        assert_eq!(m["groups"], json!(["2024", "05"]));
        assert_eq!(m["named"]["y"], "2024");
        assert_eq!(out.data["replaced"], "é 05/2024 and 12/1999");
        let out = h.run("dev.regex", json!({"pattern": "^b", "text": "a\nb", "flags": ["m"]})).await.unwrap();
        assert_eq!(out.data["count"], 1);
        let err = h.run("dev.regex", json!({"pattern": "a(?=b)", "text": "ab"})).await.unwrap_err();
        assert!(err.to_string().contains("Extended"), "{err}");
        let out = h.run("dev.regex", json!({"pattern": "(\\w)\\1", "text": "aa bc dd", "engine": "fancy"})).await.unwrap();
        assert_eq!(out.data["count"], 2);
        let out = h.run("dev.regex", json!({"pattern": "A", "text": "a", "flags": ["i"], "engine": "fancy"})).await.unwrap();
        assert_eq!(out.data["count"], 1);
        assert_eq!(h.run("dev.regex", json!({"pattern": "[", "text": "x"})).await.unwrap_err().code(), "invalid_input");
    }
}
