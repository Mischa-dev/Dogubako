//! JSON querying and validation: JSONPath, jq, JSON Schema and structural diff.

use super::formatters::{parse_any, parse_json};
use crate::util::*;
use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![jsonpath(), jq(), json_schema_validate(), diff_json()]
}

fn input_doc(p: &Params) -> ToolResult<Value> {
    let text = text_or_file(p, "text", "file")?;
    if text.trim().is_empty() {
        return Err(ToolError::invalid("text", "input is empty"));
    }
    let fmt = p.str_or("format", "json");
    if fmt == "json" {
        parse_json(&text, "text")
    } else {
        parse_any(&text, fmt)
    }
}

fn doc_params(label: &str) -> Vec<ParamSpec> {
    vec![
        ParamSpec::textarea("text").label(label).syntax("json"),
        ParamSpec::file("file").label("Or upload a file").accept(&[
            ".json",
            ".yaml",
            ".yml",
            ".toml",
            "application/json",
        ]),
        ParamSpec::choice(
            "format",
            &[("json", "JSON"), ("yaml", "YAML"), ("toml", "TOML")],
        )
        .label("Input format")
        .default_str("json")
        .advanced(),
    ]
}

// ---------------------------------------------------------------------------
// dev.jsonpath

fn jsonpath() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.jsonpath", "JSONPath Query", Category::Dev)
        .describe("Query JSON (or YAML/TOML) with standard RFC 9535 JSONPath expressions, e.g. $.store.book[?@.price < 10].title")
        .keywords(&["jsonpath", "json query", "rfc 9535", "filter json", "extract", "select"])
        .param(ParamSpec::text("query").label("JSONPath").required().primary().default_str("$").placeholder("$..author"))
        .params(doc_params("JSON"))
        .text_output("application/json")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let doc = input_doc(p)?;
        let q = p.str("query")?;
        let path = serde_json_path::JsonPath::parse(q)
            .map_err(|e| ToolError::invalid("query", e.to_string()))?;
        let located = path.query_located(&doc);
        let nodes: Vec<(String, Value)> = located
            .into_iter()
            .map(|n| (n.location().to_string(), n.node().clone()))
            .collect();
        let values: Vec<Value> = nodes.iter().map(|(_, v)| v.clone()).collect();
        let paths: Vec<Value> = nodes.iter().map(|(l, _)| json!(l)).collect();
        Ok(
            ToolOutput::text_as(serde_json::to_string_pretty(&values)?, "application/json")
                .with_data(json!({"count": values.len(), "paths": paths})),
        )
    })
}

// ---------------------------------------------------------------------------
// dev.jq

pub(crate) fn run_jq(program: &str, input: &Value, max_out: usize) -> ToolResult<Vec<Value>> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{data, unwrap_valr, Compiler, Ctx, Vars};
    let input_s = serde_json::to_string(input)?;
    let input = jaq_json::read::parse_single(input_s.as_bytes())
        .map_err(|e| ToolError::failed(format!("reading input: {e:?}")))?;
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    // `env` would expose the host process environment; never offer it.
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs())
        .filter(|f| !matches!(f.0, "env" | "input" | "inputs"));
    let loader = Loader::new(defs);
    let arena = Arena::default();
    let modules = loader
        .load(
            &arena,
            File {
                code: program,
                path: (),
            },
        )
        .map_err(|errs| {
            ToolError::invalid(
                "filter",
                errs.iter()
                    .map(|(_, e)| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs| {
            ToolError::invalid(
                "filter",
                errs.iter()
                    .map(|(_, e)| format!("{e:?}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
    let ctx = Ctx::<data::JustLut<jaq_json::Val>>::new(&filter.lut, Vars::new([]));
    let mut out = vec![];
    for r in filter.id.run((ctx, input)).map(unwrap_valr) {
        if out.len() >= max_out {
            break;
        }
        let v = r.map_err(|e| ToolError::invalid("filter", format!("runtime error: {e}")))?;
        let s = v.to_string();
        out.push(serde_json::from_str(&s).unwrap_or(Value::String(s)));
    }
    Ok(out)
}

fn jq() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.jq", "jq Filter", Category::Dev)
        .describe("Transform JSON with jq filters (powered by jaq, a jq clone in Rust): select, map, group_by, reduce and more")
        .keywords(&["jq", "jaq", "json transform", "filter", "query json", "map", "select"])
        .param(ParamSpec::text("filter").label("Filter").required().primary().default_str(".").placeholder(".items[] | select(.price > 10) | .name"))
        .params(doc_params("JSON input"))
        .param(ParamSpec::boolean("compact").label("Compact output (one result per line)"))
        .param(ParamSpec::boolean("raw").label("Raw strings (-r)"))
        .param(ParamSpec::integer("max_results").label("Max results").default_int(10000).range(1.0, 1_000_000.0).advanced())
        .text_output("application/json")
        .timeout(30)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let doc = input_doc(p)?;
        let max = p.int_or("max_results", 10000).max(1) as usize;
        let results = run_jq(p.str("filter")?, &doc, max)?;
        let (compact, raw) = (p.bool("compact"), p.bool("raw"));
        let lines: Vec<String> = results
            .iter()
            .map(|v| match v {
                Value::String(s) if raw => s.clone(),
                _ if compact => serde_json::to_string(v).unwrap_or_default(),
                _ => serde_json::to_string_pretty(v).unwrap_or_default(),
            })
            .collect();
        let mime = if raw {
            "text/plain"
        } else {
            "application/json"
        };
        let mut out = ToolOutput::text_as(lines.join("\n"), mime)
            .with_data(json!({"results": results.len()}));
        if results.len() >= max {
            out = out.warn(format!("Output limited to {max} results"));
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// dev.json_schema_validate

pub(crate) fn schema_validate(schema: &Value, instance: &Value) -> ToolResult<ToolOutput> {
    let schema = match schema {
        Value::String(s) => parse_json(s, "schema")?,
        v => v.clone(),
    };
    let validator = jsonschema::validator_for(&schema)
        .map_err(|e| ToolError::invalid("schema", format!("invalid schema: {e}")))?;
    let mut rows = vec![];
    let mut total = 0usize;
    for e in validator.iter_errors(instance) {
        total += 1;
        if rows.len() < 1000 {
            let path = e.instance_path().to_string();
            rows.push(vec![
                json!(if path.is_empty() {
                    "/".to_string()
                } else {
                    path
                }),
                json!(e.to_string()),
                json!(e.schema_path().to_string()),
            ]);
        }
    }
    let mut out = ToolOutput::data(
        json!({"valid": total == 0, "errors": total, "table": table(&["instance path", "error", "schema path"], rows)}),
    );
    if total > 1000 {
        out = out.warn(format!("Showing the first 1000 of {total} errors"));
    }
    Ok(out)
}

fn json_schema_validate() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.json_schema_validate", "JSON Schema Validator", Category::Dev)
        .describe("Validate JSON (or YAML/TOML) against a JSON Schema (drafts 4, 6, 7, 2019-09, 2020-12) and list every error with its path. Remote $refs are not fetched")
        .keywords(&["json schema", "validate", "schema", "openapi", "draft 2020-12", "contract"])
        .param(ParamSpec::textarea("schema").label("JSON Schema").required().syntax("json"))
        .params(doc_params("JSON to validate"))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let schema = parse_json(p.text("schema"), "schema")?;
        let doc = input_doc(p)?;
        schema_validate(&schema, &doc)
    })
}

// ---------------------------------------------------------------------------
// dev.diff_json

fn ptr_escape(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

fn diff_values(
    path: &str,
    a: &Value,
    b: &Value,
    array_by_index: bool,
    out: &mut Vec<(String, &'static str, Value, Value)>,
) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                let p = format!("{path}/{}", ptr_escape(k));
                match y.get(k) {
                    Some(vb) => diff_values(&p, va, vb, array_by_index, out),
                    None => out.push((p, "removed", va.clone(), Value::Null)),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    out.push((
                        format!("{path}/{}", ptr_escape(k)),
                        "added",
                        Value::Null,
                        vb.clone(),
                    ));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if array_by_index => {
            for i in 0..x.len().max(y.len()) {
                let p = format!("{path}/{i}");
                match (x.get(i), y.get(i)) {
                    (Some(va), Some(vb)) => diff_values(&p, va, vb, array_by_index, out),
                    (Some(va), None) => out.push((p, "removed", va.clone(), Value::Null)),
                    (None, Some(vb)) => out.push((p, "added", Value::Null, vb.clone())),
                    _ => {}
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            // Order-insensitive multiset comparison.
            let mut remaining: Vec<&Value> = y.iter().collect();
            for va in x {
                if let Some(pos) = remaining.iter().position(|vb| *vb == va) {
                    remaining.remove(pos);
                } else {
                    out.push((format!("{path}/-"), "removed", va.clone(), Value::Null));
                }
            }
            for vb in remaining {
                out.push((format!("{path}/-"), "added", Value::Null, vb.clone()));
            }
        }
        _ if a == b => {}
        _ => out.push((
            if path.is_empty() {
                "/".into()
            } else {
                path.to_string()
            },
            "changed",
            a.clone(),
            b.clone(),
        )),
    }
}

fn diff_json() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.diff_json", "JSON Diff", Category::Dev)
        .describe("Compare two JSON (or YAML/TOML) documents structurally and list added, removed and changed values by JSON Pointer path — key order and formatting are ignored")
        .keywords(&["json diff", "compare json", "structural diff", "yaml diff", "json pointer", "changes"])
        .param(ParamSpec::textarea("original").label("Original").required().primary().syntax("json"))
        .param(ParamSpec::textarea("changed").label("Changed").required().syntax("json"))
        .param(ParamSpec::choice("format", &[("auto", "Auto-detect"), ("json", "JSON"), ("yaml", "YAML"), ("toml", "TOML")]).label("Input format").default_str("auto").advanced())
        .param(ParamSpec::boolean("ignore_array_order").label("Ignore array order"))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let parse = |name: &str| -> ToolResult<Value> {
            let t = p.text(name);
            let f = match p.str_or("format", "auto") {
                "auto" => match super::formatters::detect_format(t) {
                    f @ ("json" | "yaml" | "toml") => f,
                    _ => "yaml",
                },
                f => f,
            };
            parse_any(t, f).map_err(|e| match e {
                ToolError::InvalidInput { message, .. } => ToolError::invalid(name, message),
                e => e,
            })
        };
        let a = parse("original")?;
        let b = parse("changed")?;
        let mut diffs = vec![];
        diff_values("", &a, &b, !p.bool("ignore_array_order"), &mut diffs);
        let count = |k: &str| diffs.iter().filter(|d| d.1 == k).count();
        let (added, removed, changed) = (count("added"), count("removed"), count("changed"));
        let show = |v: &Value| {
            if v.is_null() {
                Value::Null
            } else {
                json!(serde_json::to_string(v).unwrap_or_default())
            }
        };
        let rows: Vec<Vec<Value>> = diffs
            .iter()
            .take(5000)
            .map(|(path, kind, x, y)| vec![json!(path), json!(kind), show(x), show(y)])
            .collect();
        Ok(ToolOutput::data(json!({
            "identical": diffs.is_empty(),
            "added": added,
            "removed": removed,
            "changed": changed,
            "table": table(&["path", "change", "original", "changed"], rows),
        })))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    const STORE: &str = r#"{"store":{"book":[{"title":"A","price":8},{"title":"B","price":12}]}}"#;

    #[tokio::test]
    async fn jsonpath_and_jq() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "dev.jsonpath",
                json!({"query": "$.store.book[?@.price < 10].title", "text": STORE}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "[\n  \"A\"\n]");
        assert_eq!(out.data["paths"][0], "$['store']['book'][0]['title']");
        let out = h
            .run(
                "dev.jsonpath",
                json!({"query": "$.a", "text": "a: 1\n", "format": "yaml"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["count"], 1);
        assert_eq!(
            h.run("dev.jsonpath", json!({"query": "$[", "text": "{}"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        let out = h.run("dev.jq", json!({"filter": "[.store.book[] | select(.price > 10) | .title]", "text": STORE, "compact": true})).await.unwrap();
        assert_eq!(out.text.unwrap(), "[\"B\"]");
        let out = h
            .run(
                "dev.jq",
                json!({"filter": ".store.book | map(.price) | add", "text": STORE}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "20");
        let out = h
            .run(
                "dev.jq",
                json!({"filter": ".store.book[].title", "text": STORE, "raw": true}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "A\nB");
        assert_eq!(
            h.run("dev.jq", json!({"filter": "env", "text": "{}"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.jq", json!({"filter": ".[", "text": "{}"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.jq", json!({"filter": "error(\"boom\")", "text": "{}"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn schema_and_diff() {
        let h = TestHarness::new(crate::tools());
        let schema = r#"{"type":"object","required":["name"],"properties":{"age":{"type":"integer","minimum":0}}}"#;
        let out = h
            .run(
                "dev.json_schema_validate",
                json!({"schema": schema, "text": "{\"age\": -1}"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["valid"], false);
        assert_eq!(out.data["errors"], 2);
        let out = h
            .run(
                "dev.json_schema_validate",
                json!({"schema": schema, "text": "{\"name\": \"x\", \"age\": 3}"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["valid"], true);
        assert_eq!(
            h.run(
                "dev.json_schema_validate",
                json!({"schema": "{\"type\": 5}", "text": "{}"})
            )
            .await
            .unwrap_err()
            .code(),
            "invalid_input"
        );
        let out = h.run("dev.diff_json", json!({"original": "{\"a\":1,\"b\":[1,2],\"c\":{\"d\":true}}", "changed": "{\"a\":2,\"b\":[1,2,3],\"c\":{}}"})).await.unwrap();
        assert_eq!(out.data["changed"], 1);
        assert_eq!(out.data["added"], 1);
        assert_eq!(out.data["removed"], 1);
        assert_eq!(out.data["table"]["rows"][0][0], "/a");
        let out = h
            .run(
                "dev.diff_json",
                json!({"original": "[1,2]", "changed": "[2,1]", "ignore_array_order": true}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["identical"], true);
        let out = h
            .run(
                "dev.diff_json",
                json!({"original": "a: 1", "changed": "{\"a\": 1}"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["identical"], true);
    }
}
