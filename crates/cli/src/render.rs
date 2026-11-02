//! Human and JSON output for the CLI.

use dogubako_core::engine::{Engine, ToolInfo};
use dogubako_core::error::ToolError;
use dogubako_core::spec::{ParamKind, ToolSpec};
use dogubako_core::tool::ToolOutput;
use dogubako_core::util::human_size;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::path::PathBuf;

pub struct Out {
    pub json: bool,
    pub quiet: bool,
}

impl Out {
    pub fn info(&self, msg: &str) {
        if !self.quiet && !self.json {
            eprintln!("{msg}");
        }
    }

    pub fn error(&self, msg: &str, code: i32) -> i32 {
        if self.json {
            println!("{}", json!({"ok": false, "error": {"code": "error", "message": msg}}));
        } else {
            eprintln!("error: {msg}");
        }
        code
    }

    pub fn tool_error(&self, e: &ToolError) -> i32 {
        if self.json {
            println!("{}", json!({"ok": false, "error": e}));
        } else {
            eprintln!("error: {e}");
            if let ToolError::MissingDependency { .. } = e {
                eprintln!("hint: run `dogubako deps` to see what is installed");
            }
        }
        e.exit_code()
    }

    pub fn progress_bar(&self, label: &str) -> Option<ProgressBar> {
        if self.quiet || self.json || !std::io::stderr().is_terminal() {
            return None;
        }
        let b = ProgressBar::new(1000);
        b.set_style(ProgressStyle::with_template("{spinner} {prefix} [{bar:30}] {percent:>3}% {msg}").unwrap().progress_chars("━━ "));
        b.set_prefix(label.to_string());
        b.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(b)
    }
}

pub fn print_output(spec: &ToolSpec, o: &ToolOutput, saved: &[PathBuf], duration_ms: u64, out: &Out) {
    if out.json {
        let artifacts: Vec<Value> = o
            .artifacts
            .iter()
            .zip(saved.iter())
            .map(|(a, p)| json!({"name": a.name, "path": p, "mime": a.mime, "size": a.size, "label": a.label}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true, "tool": spec.id, "text": o.text, "text_mime": o.text_mime,
                "data": o.data, "artifacts": artifacts, "warnings": o.warnings, "duration_ms": duration_ms
            }))
            .unwrap()
        );
        return;
    }
    if let Some(t) = &o.text {
        print!("{t}");
        if !t.ends_with('\n') {
            println!();
        }
    } else if !o.data.is_null() && o.data != json!({}) {
        print_data(&o.data);
    }
    for w in &o.warnings {
        out.info(&format!("warning: {w}"));
    }
    for (a, p) in o.artifacts.iter().zip(saved.iter()) {
        if p.as_os_str() != "-" {
            out.info(&format!("saved {} ({})", p.display(), human_size(a.size)));
        }
    }
}

fn print_data(d: &Value) {
    if let Some(t) = d.get("table") {
        print_table(t);
        let rest: serde_json::Map<String, Value> = d.as_object().map(|o| o.iter().filter(|(k, _)| *k != "table").map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
        if !rest.is_empty() {
            println!();
            print_kv(&Value::Object(rest), 0);
        }
        return;
    }
    print_kv(d, 0);
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "—".into(),
        other => other.to_string(),
    }
}

fn print_kv(v: &Value, indent: usize) {
    let pad = " ".repeat(indent);
    match v {
        Value::Object(o) => {
            let w = o.keys().map(|k| k.len()).max().unwrap_or(0);
            for (k, val) in o {
                match val {
                    Value::Object(_) => {
                        println!("{pad}{k}:");
                        print_kv(val, indent + 2);
                    }
                    Value::Array(a) if a.iter().any(|x| x.is_object() || x.is_array()) => {
                        println!("{pad}{k}:");
                        for item in a {
                            match item {
                                Value::Object(_) => {
                                    println!("{pad}  -");
                                    print_kv(item, indent + 4);
                                }
                                _ => println!("{pad}  - {}", scalar(item)),
                            }
                        }
                    }
                    Value::Array(a) => println!("{pad}{k:w$}  {}", a.iter().map(scalar).collect::<Vec<_>>().join(", ")),
                    _ => println!("{pad}{k:w$}  {}", scalar(val)),
                }
            }
        }
        Value::Array(a) => {
            for item in a {
                print_kv(item, indent);
            }
        }
        other => println!("{pad}{}", scalar(other)),
    }
}

fn print_table(t: &Value) {
    let cols: Vec<String> = t.get("columns").and_then(|c| c.as_array()).map(|a| a.iter().map(scalar).collect()).unwrap_or_default();
    let rows: Vec<Vec<String>> = t
        .get("rows")
        .and_then(|r| r.as_array())
        .map(|rs| rs.iter().map(|r| r.as_array().map(|c| c.iter().map(scalar).collect()).unwrap_or_default()).collect())
        .unwrap_or_default();
    let n = cols.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
    let mut widths = vec![0usize; n];
    for (i, c) in cols.iter().enumerate() {
        widths[i] = widths[i].max(c.chars().count());
    }
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count()).min(60);
        }
    }
    let fmt_row = |r: &[String]| -> String {
        r.iter()
            .enumerate()
            .map(|(i, c)| {
                let s: String = c.chars().take(60).collect();
                format!("{s:<w$}", w = widths[i])
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    if !cols.is_empty() {
        println!("{}", fmt_row(&cols));
        println!("{}", widths.iter().map(|w| "─".repeat(*w)).collect::<Vec<_>>().join("  "));
    }
    for r in &rows {
        println!("{}", fmt_row(r));
    }
}

pub fn tool_list(infos: &[ToolInfo], out: &Out) {
    if out.json {
        let v: Vec<Value> = infos
            .iter()
            .map(|i| json!({"id": i.spec.id, "name": i.spec.name, "category": i.spec.category.id(), "description": i.spec.description, "available": i.availability.available, "reason": i.availability.reason}))
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return;
    }
    let w = infos.iter().map(|i| i.spec.id.len()).max().unwrap_or(10);
    let mut last_cat = None;
    for i in infos {
        if last_cat != Some(i.spec.category) {
            println!("\n{}", i.spec.category.label());
            last_cat = Some(i.spec.category);
        }
        let mark = if i.availability.available { " " } else { "·" };
        println!("  {mark} {:w$}  {}", i.spec.id, i.spec.description);
    }
    let unavailable = infos.iter().filter(|i| !i.availability.available).count();
    out.info(&format!("\n{} tools{}", infos.len(), if unavailable > 0 { format!(" ({unavailable} marked · need extra dependencies — see `dogubako deps`)") } else { String::new() }));
}

pub fn tool_info(i: &ToolInfo, out: &Out) {
    let s = &i.spec;
    if out.json {
        let mut v = serde_json::to_value(i).unwrap();
        v["input_schema"] = dogubako_core::schema::input_schema(s, "path");
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return;
    }
    let cli = format!("dogubako {} {}", s.category.id(), s.id.split_once('.').map(|x| x.1).unwrap_or("").replace('_', "-"));
    println!("{} ({})\n{}\n", s.name, s.id, s.description);
    println!("Usage:     {cli} --help");
    println!("Available: {}", if i.availability.available { "yes".to_string() } else { format!("no — {}", i.availability.reason.clone().unwrap_or_default()) });
    if !s.requires.is_empty() {
        println!("Requires:  {}", s.requires.join(", "));
    }
    if !s.permissions.is_empty() {
        println!("Permissions: {}", s.permissions.iter().map(|p| p.id()).collect::<Vec<_>>().join(", "));
    }
    println!("\nParameters:");
    for p in &s.params {
        let kind = match &p.kind {
            ParamKind::Text { .. } => "text".to_string(),
            ParamKind::Secret => "secret".into(),
            ParamKind::Integer { min, max } => format!("integer{}", range(min.map(|x| x as f64), max.map(|x| x as f64))),
            ParamKind::Number { min, max, .. } => format!("number{}", range(*min, *max)),
            ParamKind::Boolean => "boolean".into(),
            ParamKind::Enum { options, multi } => format!("{}one of: {}", if *multi { "any " } else { "" }, options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>().join("|")),
            ParamKind::Color => "color".into(),
            ParamKind::File { multiple, .. } => if *multiple { "files".into() } else { "file".into() },
            ParamKind::Path { .. } => "path".into(),
            ParamKind::Json { .. } => "json".into(),
        };
        let req = if p.required { " (required)" } else { "" };
        let def = p.default.as_ref().filter(|d| !d.is_null()).map(|d| format!(" [default: {}]", scalar(d))).unwrap_or_default();
        println!("  {:<18} {kind}{req}{def}", p.name);
        if !p.description.is_empty() {
            println!("  {:<18} {}", "", p.description);
        }
    }
}

fn range(min: Option<f64>, max: Option<f64>) -> String {
    match (min, max) {
        (Some(a), Some(b)) => format!(" {a}..{b}"),
        (Some(a), None) => format!(" ≥{a}"),
        (None, Some(b)) => format!(" ≤{b}"),
        _ => String::new(),
    }
}

pub fn deps(engine: &Engine, out: &Out) -> i32 {
    let deps = engine.deps.all();
    if out.json {
        println!("{}", serde_json::to_string_pretty(&deps).unwrap());
        return 0;
    }
    for d in &deps {
        let users = engine.registry.specs().filter(|s| s.requires.contains(&d.id)).count();
        if d.found {
            println!("  ✓ {:<20} {:<14} {}", d.name, d.version.clone().unwrap_or_default(), d.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default());
        } else {
            println!("  ✗ {:<20} {:<14} {} tool(s) need it — {}", d.name, "missing", users, d.install_hint);
        }
    }
    0
}

/// Markdown capability matrix generated from the registry.
pub fn matrix(engine: &Engine) {
    use dogubako_core::spec::{Category, Platform};
    let specs: Vec<&ToolSpec> = engine.registry.specs().collect();
    println!("# Capability matrix\n");
    println!("Generated from the tool registry by `dogubako tools matrix` (Dogubako {}). {} tools.\n", env!("CARGO_PKG_VERSION"), specs.len());
    println!("Legend — Engine: **native** = built into Dogubako (no external programs), **external** = invokes the listed program(s), **model** = needs a downloadable local model, **network** = makes outbound requests. Platforms: L = Linux, M = macOS, W = Windows.\n");
    println!("| Category | Tools | Native | External | Model | Network |\n|---|---:|---:|---:|---:|---:|");
    for c in Category::ALL {
        let in_cat: Vec<_> = specs.iter().filter(|s| s.category == c).collect();
        if in_cat.is_empty() { continue; }
        let count = |e: dogubako_core::spec::Execution| in_cat.iter().filter(|s| s.execution == e).count();
        use dogubako_core::spec::Execution::*;
        println!("| {} | {} | {} | {} | {} | {} |", c.label(), in_cat.len(), count(Native), count(External), count(Model), count(Network));
    }
    for c in Category::ALL {
        let in_cat: Vec<_> = specs.iter().filter(|s| s.category == c).collect();
        if in_cat.is_empty() { continue; }
        println!("\n## {}\n\n| Tool | Id | Engine | Requires | Platforms | Permissions |\n|---|---|---|---|---|---|", c.label());
        for s in in_cat {
            let plats: String = [(Platform::Linux, "L"), (Platform::Macos, "M"), (Platform::Windows, "W")].iter().filter(|(p, _)| s.platforms.contains(p)).map(|(_, l)| *l).collect::<Vec<_>>().join(" ");
            let mut req = s.requires.join(", ");
            if let Some(m) = &s.requires_model { if !req.is_empty() { req.push_str(", "); } req.push_str(&format!("model: {m}")); }
            let eng = format!("{:?}", s.execution).to_lowercase();
            let perms = s.permissions.iter().map(|p| p.id()).collect::<Vec<_>>().join(", ");
            println!("| {} | `{}` | {} | {} | {} | {} |", s.name.replace('|', "/"), s.id, eng, if req.is_empty() { "—".into() } else { req }, plats, if perms.is_empty() { "—".into() } else { perms });
        }
    }
}
