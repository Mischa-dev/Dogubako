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
