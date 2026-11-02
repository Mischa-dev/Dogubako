//! Robustness sweep: every text tool, fed awkward text and boundary numbers,
//! must return a result or an error, never panic or hang.

use dogubako_core::spec::ParamKind;
use dogubako_core::testing::TestHarness;
use serde_json::{json, Map, Value};
use std::sync::Arc;

const INPUTS: &[&str] = &[
    "",
    " ",
    "\n",
    "\u{0}",
    "é",
    "猫猫猫",
    "👨‍👩‍👧",
    "\u{301}",
    "a\r\nb",
    "\"",
    "'",
    "\\",
    "{",
    "[",
    "<",
    "&",
    "%",
    "0",
    "-1",
    "1e309",
    "NaN",
    "9999999999999999999999",
    "-9223372036854775808",
    "%zz",
    "&#xFFFFFFFF;",
    "{\"a\":",
    "[1,2",
    "a,b\n1",
    "<a><b></a>",
    "--",
    "\u{FEFF}x",
    "x\u{200B}y",
    "𝔘𝔫𝔦",
    "ﬃ",
    "İ",
    "ß",
    "12:34",
    "2024-02-30",
    "P1Y",
    "1.5.6",
    "0x",
    "==",
    "AAAA",
    "ÿ",
    "\t\t",
    "aaa",
];

#[tokio::test(flavor = "multi_thread")]
async fn fuzz_text_params_no_panics() {
    let tools = dogubako_tools_text::tools();
    let h = Arc::new(TestHarness::new(tools.clone()));
    let mut panics = vec![];
    for t in &tools {
        let spec = t.spec().clone();
        let text_params: Vec<_> = spec
            .params
            .iter()
            .filter(|p| matches!(p.kind, ParamKind::Text { .. }))
            .map(|p| p.name.clone())
            .collect();
        for tp in &text_params {
            for inp in INPUTS {
                let mut m = Map::new();
                for p in &spec.params {
                    if p.required && !matches!(p.kind, ParamKind::Text { .. }) {
                        if let Some(d) = &p.default {
                            m.insert(p.name.clone(), d.clone());
                        }
                    }
                    if matches!(p.kind, ParamKind::Text { .. }) {
                        m.insert(p.name.clone(), p.default.clone().unwrap_or(json!("a")));
                    }
                }
                m.insert(tp.clone(), Value::String(inp.to_string()));
                let h2 = h.clone();
                let id = spec.id.clone();
                let mv = Value::Object(m);
                let r = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    tokio::spawn(async move { h2.run(&id, mv).await.map(|_| ()) }),
                )
                .await;
                match r {
                    Err(_) => panics.push(format!("TIMEOUT {} {}={:?}", spec.id, tp, inp)),
                    Ok(Err(e)) if e.is_panic() => {
                        panics.push(format!("PANIC {} {}={:?}", spec.id, tp, inp))
                    }
                    _ => {}
                }
            }
        }
    }
    for p in &panics {
        eprintln!("{p}");
    }
    assert!(panics.is_empty(), "{} panics", panics.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn fuzz_numeric_params_no_panics() {
    let tools = dogubako_tools_text::tools();
    let h = Arc::new(TestHarness::new(tools.clone()));
    let mut bad = vec![];
    let samples = [
        "hello world 123",
        "{\"a\":[1,2,{\"b\":null}]}",
        "a,b\n1,2\n3,4",
        "2024-01-02T03:04:05Z",
        "<a x=\"1\">t</a>",
    ];
    for t in &tools {
        let spec = t.spec().clone();
        for np in spec.params.iter() {
            let vals: Vec<Value> = match &np.kind {
                ParamKind::Integer { min, max } => {
                    let mut v = vec![json!(0), json!(1), json!(-1)];
                    if let Some(m) = min {
                        v.push(json!(m));
                    }
                    if let Some(m) = max {
                        v.push(json!(m));
                    }
                    v
                }
                ParamKind::Number { min, max, .. } => {
                    let mut v = vec![json!(0.0), json!(-1.0), json!(1e-300)];
                    if let Some(m) = min {
                        v.push(json!(m));
                    }
                    if let Some(m) = max {
                        v.push(json!(m));
                    }
                    v
                }
                _ => continue,
            };
            for val in vals {
                for s in samples {
                    let mut m = Map::new();
                    for p in &spec.params {
                        if matches!(p.kind, ParamKind::Text { .. }) {
                            m.insert(p.name.clone(), json!(s));
                        }
                    }
                    m.insert(np.name.clone(), val.clone());
                    let h2 = h.clone();
                    let id = spec.id.clone();
                    let mv = Value::Object(m);
                    let r = tokio::time::timeout(
                        std::time::Duration::from_secs(10),
                        tokio::spawn(async move { h2.run(&id, mv).await.map(|_| ()) }),
                    )
                    .await;
                    match r {
                        Err(_) => bad.push(format!(
                            "TIMEOUT {} {}={} text={:?}",
                            spec.id, np.name, val, s
                        )),
                        Ok(Err(e)) if e.is_panic() => bad.push(format!(
                            "PANIC {} {}={} text={:?}",
                            spec.id, np.name, val, s
                        )),
                        _ => {}
                    }
                }
            }
        }
    }
    for p in &bad {
        eprintln!("{p}");
    }
    assert!(bad.is_empty(), "{} failures", bad.len());
}
