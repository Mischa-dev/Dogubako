//! A short separator, prefix or column list repeated per byte, line or row can
//! turn a small request into gigabytes; these must fail with `limit_exceeded`
//! before allocating, while ordinary inputs keep working.

use dogubako_core::testing::TestHarness;
use serde_json::{json, Value};

fn harness() -> TestHarness {
    TestHarness::new(dogubako_tools_text::tools())
}

async fn limited(h: &TestHarness, id: &str, params: Value) {
    let err = h.run(id, params).await.unwrap_err();
    assert_eq!(err.code(), "limit_exceeded", "{id}: {err}");
}

#[tokio::test]
async fn repeated_separators_are_bounded() {
    let h = harness();
    let big = "x".repeat(100_000);
    let many = "a".repeat(100_000);
    let lines = "\n".repeat(100_000);

    limited(
        &h,
        "dev.binary_text",
        json!({"mode": "encode", "text": many, "separator": big}),
    )
    .await;
    limited(
        &h,
        "dev.random",
        json!({"kind": "passphrase", "count": 1000, "words": 64, "separator": "x".repeat(10_000)}),
    )
    .await;
    limited(
        &h,
        "text.slugify",
        json!({"text": "a ".repeat(50_000), "separator": big}),
    )
    .await;
    limited(
        &h,
        "text.number_lines",
        json!({"text": lines, "separator": big, "skip_empty": false}),
    )
    .await;
    limited(
        &h,
        "text.prefix_suffix",
        json!({"text": lines, "prefix": big, "skip_empty": false}),
    )
    .await;
    // Repeating a column in the list repeats it in every line.
    limited(
        &h,
        "text.column_extract",
        json!({"text": format!("{big}\n").repeat(100), "columns": "1,".repeat(50_000)}),
    )
    .await;
    limited(
        &h,
        "data.columns",
        json!({"text": "a,b\n".to_string() + &"1,2\n".repeat(50_000), "operation": "merge", "columns": "a,b", "separator": big}),
    )
    .await;
    let deep: Vec<Value> = (0..5000)
        .map(|i| json!({"a": {format!("k{i}"): 1}}))
        .collect();
    limited(
        &h,
        "data.json_to_csv",
        json!({"text": serde_json::to_string(&deep).unwrap(), "separator": "x".repeat(100_000)}),
    )
    .await;

    // Ordinary use is unaffected.
    let out = h
        .run(
            "dev.binary_text",
            json!({"mode": "encode", "text": "Hi", "format": "hex", "separator": " "}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.as_deref(), Some("48 69"));
    let out = h
        .run(
            "text.prefix_suffix",
            json!({"text": "a\nb", "prefix": "- "}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.as_deref(), Some("- a\n- b"));
}

#[tokio::test]
async fn ragged_tables_are_bounded() {
    let h = harness();
    // A 100000-field header over 100000 one-field records would pad to 10^10 cells.
    let header = vec!["h"; 100_000].join(",");
    let csv = format!("{header}\n{}", "x\n".repeat(100_000));
    limited(
        &h,
        "data.csv_view",
        json!({"text": csv, "delimiter": "comma", "header": "yes"}),
    )
    .await;
    // Ordinary ragged rows are still padded.
    let out = h
        .run(
            "data.csv_view",
            json!({"text": "a,b,c\n1\n2,3", "delimiter": "comma", "header": "yes"}),
        )
        .await
        .unwrap();
    assert_eq!(
        out.data["table"]["rows"],
        json!([["1", "", ""], ["2", "3", ""]]),
        "{}",
        out.data
    );
}

#[tokio::test]
async fn json_results_are_bounded() {
    let h = harness();
    // A 4 MiB string under 100 single-child objects: `$..*` matches every
    // ancestor, so the result would hold ~100 copies of it.
    let mut doc = json!("y".repeat(4 << 20));
    for _ in 0..100 {
        doc = json!({ "a": doc });
    }
    let text = serde_json::to_string(&doc).unwrap();
    limited(&h, "dev.jsonpath", json!({"query": "$..*", "text": text})).await;
    // A long key over many array items: every location repeats the key.
    let text = format!(
        "{{\"{}\":[{}0]}}",
        "k".repeat(100_000),
        "0,".repeat(100_000)
    );
    limited(&h, "dev.jsonpath", json!({"query": "$.*[*]", "text": text})).await;
    // Small results are unchanged.
    let out = h
        .run(
            "dev.jsonpath",
            json!({"query": "$..b", "text": r#"{"a":{"b":1},"b":2}"#}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["count"], 2);

    // Deep nesting of tiny values: pretty-printing indents every line by depth.
    let deep = format!(
        "{}{}{}",
        "[".repeat(120),
        "1,".repeat(2_000_000) + "1",
        "]".repeat(120)
    );
    limited(
        &h,
        "dev.json",
        json!({"text": deep, "mode": "format", "indent": "4"}),
    )
    .await;
}

#[tokio::test]
async fn wrap_indent_and_regex_groups_are_bounded() {
    let h = harness();
    limited(
        &h,
        "text.wrap",
        json!({"text": "a ".repeat(100_000), "width": 1, "indent": " ".repeat(100_000)}),
    )
    .await;
    let out = h
        .run(
            "text.wrap",
            json!({"text": "aa bb cc", "width": 5, "indent": "> "}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.as_deref(), Some("aa bb\n> cc"));

    // 100 nested groups each capture the whole 4 MiB match.
    let pattern = format!("{}a+{}", "(".repeat(100), ")".repeat(100));
    limited(
        &h,
        "dev.regex",
        json!({"pattern": pattern, "text": "a".repeat(4 << 20)}),
    )
    .await;
}

#[tokio::test]
async fn replacements_and_column_splits_are_bounded() {
    let h = harness();
    let a = "a".repeat(100_000);
    let b = "b".repeat(100_000);
    let csv = format!("a\n{a}\n");
    for regex in [false, true] {
        limited(
            &h,
            "data.columns",
            json!({"text": csv, "operation": "replace", "columns": "a", "find": "a", "replace": b, "regex": regex}),
        )
        .await;
    }
    limited(
        &h,
        "text.replace",
        json!({"text": a, "find": "a", "replace": b}),
    )
    .await;
    limited(
        &h,
        "text.regex_replace",
        json!({"text": a, "pattern": "a", "replacement": b}),
    )
    .await;
    limited(
        &h,
        "text.regex_replace",
        json!({"text": a, "pattern": "a+", "replacement": "$0".repeat(100_000)}),
    )
    .await;
    // One cell with 99,999 separators over 100,000 rows.
    let wide = format!("a\n{}\n{}", "|".repeat(99_999), "x\n".repeat(99_999));
    limited(
        &h,
        "data.columns",
        json!({"text": wide, "operation": "split", "columns": "a", "separator": "|"}),
    )
    .await;

    // Ordinary replacements are unchanged, including $ expansion.
    let out = h
        .run(
            "text.regex_replace",
            json!({"text": "ab ab", "pattern": "(a)(b)", "replacement": "$2$1", "limit": 1}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.as_deref(), Some("ba ab"));
    let out = h
        .run(
            "text.regex_replace",
            json!({"text": "ab", "pattern": "(a)", "replacement": "$1$", "literal": true}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.as_deref(), Some("$1$b"));
}

#[tokio::test]
async fn table_exports_and_sparse_json_are_bounded() {
    let h = harness();
    let csv = format!("a\n{}", "1\n".repeat(100_000));
    limited(
        &h,
        "data.convert",
        json!({"text": csv, "header": "yes", "to": "sql", "table_name": "t".repeat(100_000)}),
    )
    .await;
    // A long header repeated as a JSON key on every row.
    let csv = format!("{}\n{}", "k".repeat(100_000), "1\n".repeat(100_000));
    limited(
        &h,
        "data.convert",
        json!({"text": csv, "header": "yes", "to": "json"}),
    )
    .await;
    // 100,000 objects, each with its own key: a 100,000 x 100,000 table.
    let sparse: Vec<Value> = (0..100_000)
        .map(|i| json!({ format!("k{i}"): 1 }))
        .collect();
    limited(
        &h,
        "data.json_to_csv",
        json!({"text": serde_json::to_string(&sparse).unwrap()}),
    )
    .await;

    // Normal exports still work.
    let out = h
        .run(
            "data.convert",
            json!({"text": "a,b\n1,x\n", "header": "yes", "to": "sql", "table_name": "t"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts.len() == 1 || out.text.is_some());
}

#[tokio::test]
async fn deep_xml_indentation_is_bounded() {
    let h = harness();
    let xml = format!("{}{}", "<a>".repeat(50_000), "</a>".repeat(50_000));
    limited(&h, "dev.xml", json!({"text": xml})).await;
    let out = h
        .run("dev.xml", json!({"text": "<a><b>x</b></a>"}))
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("  <b>x</b>"));
}

#[tokio::test]
async fn yaml_and_toml_reformatting_is_bounded() {
    let h = harness();
    // Flow-style nesting re-emitted in block style indents every item.
    let deep = format!(
        "{}{}{}",
        "[".repeat(120),
        "1,".repeat(2_000_000) + "1",
        "]".repeat(120)
    );
    limited(&h, "dev.yaml", json!({"text": deep})).await;
    // Inline tables re-emitted as [a.b.c] headers repeat the whole path.
    let key = "k".repeat(200_000);
    let mut toml = "1".to_string();
    for _ in 0..60 {
        toml = format!("{{{key}={toml}}}");
    }
    limited(&h, "dev.toml", json!({"text": format!("{key}={toml}")})).await;

    let out = h
        .run("dev.yaml", json!({"text": "a: [1, 2]"}))
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("- 1"));
    let out = h
        .run("dev.toml", json!({"text": "a = {b = 1}"}))
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("b = 1"));
}
