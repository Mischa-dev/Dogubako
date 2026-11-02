//! `data.*` tools: CSV, spreadsheets and structured data.

use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        csv_view(),
        csv_filter_sort(),
        csv_to_json(),
        json_to_csv(),
        spreadsheet(),
        delimiter(),
        columns(),
        validate(),
        duplicates(),
        stats(),
        clean(),
        convert(),
    ]
}

/// A parsed table of strings.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn col(&self, name: &str) -> ToolResult<usize> {
        let n = name.trim();
        if let Some(i) = self.headers.iter().position(|h| h == n) {
            return Ok(i);
        }
        if let Some(i) = self.headers.iter().position(|h| h.eq_ignore_ascii_case(n)) {
            return Ok(i);
        }
        // allow #3 / 3 (1-based)
        if let Ok(i) = n.trim_start_matches('#').parse::<usize>() {
            if i >= 1 && i <= self.headers.len() {
                return Ok(i - 1);
            }
        }
        Err(ToolError::invalid(
            "column",
            format!(
                "no column named '{n}' (columns: {})",
                self.headers.join(", ")
            ),
        ))
    }
    pub fn cols(&self, list: &str) -> ToolResult<Vec<usize>> {
        split_list(list).iter().map(|c| self.col(c)).collect()
    }
    fn normalize(&mut self) {
        let w = self
            .rows
            .iter()
            .map(|r| r.len())
            .max()
            .unwrap_or(0)
            .max(self.headers.len());
        for i in self.headers.len()..w {
            self.headers.push(format!("column{}", i + 1));
        }
        for r in &mut self.rows {
            r.resize(w, String::new());
        }
    }
    pub fn preview(&self, offset: usize, limit: usize) -> Value {
        let rows: Vec<Vec<Value>> = self
            .rows
            .iter()
            .skip(offset)
            .take(limit)
            .map(|r| r.iter().map(|c| json!(c)).collect())
            .collect();
        table_owned(&self.headers, rows)
    }
}

pub fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// Guess the delimiter from the first lines.
pub fn sniff_delimiter(text: &str) -> u8 {
    let sample: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let mut best = (b',', 0.0f64);
    for d in *b",\t;|" {
        let counts: Vec<usize> = sample
            .iter()
            .map(|l| count_outside_quotes(l, d as char))
            .collect();
        if counts.is_empty() || counts[0] == 0 {
            continue;
        }
        let consistent =
            counts.iter().filter(|c| **c == counts[0]).count() as f64 / counts.len() as f64;
        let score = consistent * 10.0 + counts[0] as f64 * 0.01;
        if score > best.1 {
            best = (d, score);
        }
    }
    best.0
}

fn count_outside_quotes(line: &str, d: char) -> usize {
    let mut q = false;
    line.chars()
        .filter(|c| {
            if *c == '"' {
                q = !q;
            }
            !q && *c == d
        })
        .count()
}

pub fn parse_csv(text: &str, delim: u8, has_header: bool) -> ToolResult<Table> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.matches('"').count() % 2 == 1 {
        return Err(ToolError::invalid(
            "text",
            "malformed CSV: a quoted field is never closed (unbalanced \")",
        ));
    }
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows: Vec<Vec<String>> = vec![];
    for (i, rec) in rdr.records().enumerate() {
        let rec = rec.map_err(|e| {
            ToolError::invalid("text", format!("CSV error near record {}: {e}", i + 1))
        })?;
        rows.push(rec.iter().map(|s| s.to_string()).collect());
    }
    let mut t = if has_header && !rows.is_empty() {
        let headers = rows.remove(0);
        Table {
            headers: dedupe_headers(headers),
            rows,
        }
    } else {
        Table {
            headers: vec![],
            rows,
        }
    };
    t.normalize();
    Ok(t)
}

fn dedupe_headers(h: Vec<String>) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    h.into_iter()
        .enumerate()
        .map(|(i, s)| {
            let base = if s.trim().is_empty() {
                format!("column{}", i + 1)
            } else {
                s.trim().to_string()
            };
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                format!("{base}_{n}")
            } else {
                base
            }
        })
        .collect()
}

/// Heuristic: the first row is a header if it has no numeric cells while later rows do,
/// or if all its cells are distinct non-empty strings.
fn looks_like_header(text: &str, delim: u8) -> bool {
    let Ok(t) = parse_csv(text, delim, false) else {
        return true;
    };
    let Some(first) = t.rows.first() else {
        return true;
    };
    let numeric = |s: &str| parse_number(s).is_some();
    if first.iter().any(|c| numeric(c)) {
        return false;
    }
    if first.iter().any(|c| c.trim().is_empty()) {
        return false;
    }
    let distinct: HashSet<&String> = first.iter().collect();
    distinct.len() == first.len()
}

pub fn write_csv(
    headers: &[String],
    rows: &[Vec<String>],
    delim: u8,
    always_quote: bool,
) -> ToolResult<String> {
    write_csv_style(
        headers,
        rows,
        delim,
        if always_quote {
            csv::QuoteStyle::Always
        } else {
            csv::QuoteStyle::Necessary
        },
    )
}

pub fn write_csv_style(
    headers: &[String],
    rows: &[Vec<String>],
    delim: u8,
    style: csv::QuoteStyle,
) -> ToolResult<String> {
    let mut w = csv::WriterBuilder::new()
        .delimiter(delim)
        .quote_style(style)
        .from_writer(vec![]);
    if !headers.is_empty() {
        w.write_record(headers).ctx("writing CSV")?;
    }
    for r in rows {
        w.write_record(r).ctx("writing CSV")?;
    }
    String::from_utf8(w.into_inner().ctx("writing CSV")?).ctx("CSV output")
}

pub fn rows_to_objects(t: &Table, infer: bool) -> Value {
    Value::Array(
        t.rows
            .iter()
            .map(|r| {
                let mut m = serde_json::Map::new();
                for (h, c) in t.headers.iter().zip(r) {
                    m.insert(h.clone(), if infer { infer_value(c) } else { json!(c) });
                }
                Value::Object(m)
            })
            .collect(),
    )
}

/// Flatten JSON records into columns using dotted keys.
pub fn flatten_records(items: &[Value], sep: &str) -> (Vec<String>, Vec<Vec<String>>) {
    flatten_records_with(items, sep, "index")
}

pub fn flatten_records_with(
    items: &[Value],
    sep: &str,
    arrays: &str,
) -> (Vec<String>, Vec<Vec<String>>) {
    fn walk(prefix: &str, v: &Value, sep: &str, arrays: &str, out: &mut Vec<(String, String)>) {
        match v {
            Value::Object(m) if !m.is_empty() => {
                for (k, x) in m {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}{sep}{k}")
                    };
                    walk(&key, x, sep, arrays, out);
                }
            }
            Value::Array(a) if !a.is_empty() => match arrays {
                "join" if a.iter().all(|x| !x.is_object() && !x.is_array()) => out.push((
                    prefix.to_string(),
                    a.iter().map(cell_string).collect::<Vec<_>>().join("; "),
                )),
                "json" => out.push((
                    prefix.to_string(),
                    serde_json::to_string(v).unwrap_or_default(),
                )),
                _ => {
                    for (i, x) in a.iter().enumerate() {
                        let key = if prefix.is_empty() {
                            i.to_string()
                        } else {
                            format!("{prefix}{sep}{i}")
                        };
                        walk(&key, x, sep, arrays, out);
                    }
                }
            },
            Value::Object(_) => out.push((prefix.to_string(), "{}".into())),
            Value::Array(_) => out.push((prefix.to_string(), "[]".into())),
            other => out.push((
                if prefix.is_empty() {
                    "value".into()
                } else {
                    prefix.to_string()
                },
                cell_string(other),
            )),
        }
    }
    let mut cols: Vec<String> = vec![];
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut flat_rows = vec![];
    for it in items {
        let mut pairs = vec![];
        walk("", it, sep, arrays, &mut pairs);
        for (k, _) in &pairs {
            if !idx.contains_key(k) {
                idx.insert(k.clone(), cols.len());
                cols.push(k.clone());
            }
        }
        flat_rows.push(pairs);
    }
    let rows = flat_rows
        .into_iter()
        .map(|pairs| {
            let mut r = vec![String::new(); cols.len()];
            for (k, v) in pairs {
                r[idx[&k]] = v;
            }
            r
        })
        .collect();
    (cols, rows)
}

fn delim_param(name: &str, auto: bool) -> ParamSpec {
    let mut opts = vec![];
    if auto {
        opts.push(("auto", "Auto-detect"));
    }
    opts.extend([
        ("comma", "Comma ,"),
        ("tab", "Tab"),
        ("semicolon", "Semicolon ;"),
        ("pipe", "Pipe |"),
    ]);
    ParamSpec::choice(name, &opts).default_str(if auto { "auto" } else { "comma" })
}

fn delim_byte(s: &str) -> Option<u8> {
    Some(match s {
        "comma" | "," => b',',
        "tab" | "\\t" | "\t" => b'\t',
        "semicolon" | ";" => b';',
        "pipe" | "|" => b'|',
        _ => return None,
    })
}

fn input_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::file("file").label("File").primary().accept(&[
            ".csv", ".tsv", ".txt", ".json", ".ndjson", ".jsonl", ".xlsx", ".xls", ".xlsb", ".ods",
            "text/*",
        ]),
        ParamSpec::textarea("text")
            .label("Or paste CSV")
            .syntax("csv"),
        delim_param("delimiter", true).advanced(),
        ParamSpec::choice(
            "header",
            &[
                ("auto", "Auto-detect"),
                ("yes", "First row is a header"),
                ("no", "No header"),
            ],
        )
        .default_str("auto")
        .advanced(),
    ]
}

/// Load a table from the `file`/`text` params (CSV/TSV, JSON/NDJSON, spreadsheets).
pub fn load_table(p: &Params) -> ToolResult<(Table, String)> {
    let header_mode = p.str_or("header", "auto");
    if let Some(f) = p.opt_file("file") {
        let ext = f.extension();
        if matches!(ext.as_str(), "xlsx" | "xlsm" | "xls" | "xlsb" | "ods") {
            use calamine::Reader;
            let mut book = crate::sheet::open(f)?;
            let names = book.sheet_names();
            let first = names
                .first()
                .ok_or_else(|| ToolError::invalid("file", "workbook has no sheets"))?
                .clone();
            let rows = crate::sheet::sheet_rows(&mut book, &first)?;
            return Ok((
                table_from_cells(
                    rows.iter()
                        .map(|r| r.iter().map(crate::sheet::cell_text).collect())
                        .collect(),
                    header_mode != "no",
                ),
                f.name.clone(),
            ));
        }
        let text = decode_bytes(&read_file(f)?).0;
        if matches!(ext.as_str(), "json" | "ndjson" | "jsonl") {
            return Ok((json_table(&text)?, f.name.clone()));
        }
        return Ok((
            parse_text_table(&text, p, header_mode, ext == "tsv")?,
            f.name.clone(),
        ));
    }
    let text = p.text("text");
    if text.trim().is_empty() {
        return Err(ToolError::invalid(
            "text",
            "paste CSV text or upload a file",
        ));
    }
    let t = text.trim_start();
    if (t.starts_with('[') || t.starts_with('{')) && p.str_or("delimiter", "auto") == "auto" {
        if let Ok(tab) = json_table(text) {
            return Ok((tab, "data.csv".into()));
        }
    }
    Ok((
        parse_text_table(text, p, header_mode, false)?,
        "data.csv".into(),
    ))
}

fn parse_text_table(text: &str, p: &Params, header_mode: &str, tsv: bool) -> ToolResult<Table> {
    let delim = match p.str_or("delimiter", "auto") {
        "auto" if tsv => b'\t',
        "auto" => sniff_delimiter(text),
        d => delim_byte(d).unwrap_or(b','),
    };
    let header = match header_mode {
        "yes" => true,
        "no" => false,
        _ => looks_like_header(text, delim),
    };
    parse_csv(text, delim, header)
}

fn table_from_cells(mut rows: Vec<Vec<String>>, header: bool) -> Table {
    let mut t = if header && !rows.is_empty() {
        let h = rows.remove(0);
        Table {
            headers: dedupe_headers(h),
            rows,
        }
    } else {
        Table {
            headers: vec![],
            rows,
        }
    };
    t.normalize();
    t
}

pub fn json_table(text: &str) -> ToolResult<Table> {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            // NDJSON?
            let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
            if lines.len() > 1 {
                Value::Array(
                    lines
                        .iter()
                        .map(|l| serde_json::from_str(l))
                        .collect::<Result<_, _>>()
                        .map_err(|_| ToolError::invalid("text", format!("invalid JSON: {e}")))?,
                )
            } else {
                return Err(ToolError::invalid("text", format!("invalid JSON: {e}")));
            }
        }
    };
    let items = match v {
        Value::Array(a) => a,
        Value::Object(ref m) => match m.values().find(|x| x.is_array()) {
            Some(Value::Array(a)) if m.len() == 1 => a.clone(),
            _ => vec![v],
        },
        other => vec![other],
    };
    if items.iter().all(|x| x.is_array()) && !items.is_empty() {
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|r| r.as_array().unwrap().iter().map(cell_string).collect())
            .collect();
        return Ok(table_from_cells(rows, true));
    }
    let (headers, rows) = flatten_records(&items, ".");
    Ok(Table { headers, rows })
}

fn out_delim(p: &Params) -> u8 {
    delim_byte(p.str_or("output_delimiter", "comma")).unwrap_or(b',')
}

/// Emit a table as CSV: an artifact when the input was a file, else text; plus a preview.
fn emit_table(
    ctx: &ToolContext,
    p: &Params,
    t: &Table,
    name: &str,
    suffix: &str,
    delim: u8,
    extra: Value,
) -> ToolResult<ToolOutput> {
    let csv = write_csv(&t.headers, &t.rows, delim, false)?;
    let mut data = json!({"rows": t.rows.len(), "columns": t.headers.len(), "table": t.preview(0, PREVIEW_ROWS)});
    if let (Value::Object(d), Value::Object(e)) = (&mut data, extra) {
        d.extend(e);
    }
    let ext = if delim == b'\t' { "tsv" } else { "csv" };
    let mut out = if p.opt_file("file").is_some() {
        ToolOutput::files(vec![
            ctx.write_output(&suffixed(name, suffix, Some(ext)), csv.as_bytes())?
        ])
        .with_data(data)
    } else {
        ToolOutput::text_as(
            csv,
            if ext == "tsv" {
                "text/tab-separated-values"
            } else {
                "text/csv"
            },
        )
        .with_data(data)
    };
    if t.rows.len() > PREVIEW_ROWS {
        out = out.warn(format!(
            "Preview shows the first {PREVIEW_ROWS} of {} rows; the output contains all rows",
            t.rows.len()
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// column type inference

fn infer_type(values: &[&String]) -> &'static str {
    let non_empty: Vec<&&String> = values.iter().filter(|v| !v.trim().is_empty()).collect();
    if non_empty.is_empty() {
        return "empty";
    }
    if non_empty.iter().all(|v| v.trim().parse::<i64>().is_ok()) {
        return "integer";
    }
    if non_empty.iter().all(|v| parse_number(v).is_some()) {
        return "number";
    }
    if non_empty.iter().all(|v| {
        matches!(
            v.trim().to_lowercase().as_str(),
            "true" | "false" | "yes" | "no"
        )
    }) {
        return "boolean";
    }
    if non_empty
        .iter()
        .all(|v| crate::dev::time::parse_loose(v.trim(), false).is_some())
    {
        return "date";
    }
    "text"
}

// ---------------------------------------------------------------------------
// data.csv_view

fn csv_view() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.csv_view", "CSV Viewer", Category::Data)
        .describe("Preview a CSV/TSV file or pasted text as a table with automatic delimiter, header and column type detection")
        .keywords(&["csv", "tsv", "viewer", "table", "open csv", "preview", "spreadsheet"])
        .params(input_params())
        .param(ParamSpec::integer("offset").label("Start at row").default_int(0).min(0.0))
        .param(ParamSpec::integer("limit").label("Rows to show").default_int(100).range(1.0, PREVIEW_ROWS as f64))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (t, _) = load_table(p)?;
        let offset = p.int_or("offset", 0).max(0) as usize;
        let limit = (p.int_or("limit", 100).max(1) as usize).min(PREVIEW_ROWS);
        let types: Vec<Value> = (0..t.headers.len())
            .map(|i| {
                let vals: Vec<&String> = t.rows.iter().take(1000).map(|r| &r[i]).collect();
                json!({"column": t.headers[i], "type": infer_type(&vals)})
            })
            .collect();
        let shown = t.rows.len().saturating_sub(offset).min(limit);
        let mut out = ToolOutput::data(json!({
            "total_rows": t.rows.len(),
            "columns": t.headers.len(),
            "showing": format!("rows {}–{} of {}", if shown == 0 { 0 } else { offset + 1 }, offset + shown, t.rows.len()),
            "column_types": types,
            "table": t.preview(offset, limit),
        }));
        if offset + shown < t.rows.len() {
            out = out.warn(format!(
                "Showing {shown} of {} rows; adjust offset/limit to page",
                t.rows.len()
            ));
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// data.csv_filter_sort

#[derive(serde::Deserialize)]
struct Filter {
    column: String,
    #[serde(default = "default_op")]
    op: String,
    #[serde(default)]
    value: Value,
}

fn default_op() -> String {
    "equals".into()
}

const FILTER_OPS: &[(&str, &str)] = &[
    ("equals", "equals"),
    ("not_equals", "does not equal"),
    ("contains", "contains"),
    ("not_contains", "does not contain"),
    ("starts_with", "starts with"),
    ("ends_with", "ends with"),
    ("regex", "matches regex"),
    ("gt", ">"),
    ("gte", "≥"),
    ("lt", "<"),
    ("lte", "≤"),
    ("empty", "is empty"),
    ("not_empty", "is not empty"),
];

fn row_matches(
    cell: &str,
    op: &str,
    value: &str,
    re: Option<&regex::Regex>,
    ci: bool,
) -> ToolResult<bool> {
    let (c, v) = if ci {
        (cell.to_lowercase(), value.to_lowercase())
    } else {
        (cell.to_string(), value.to_string())
    };
    let num = || -> Option<std::cmp::Ordering> {
        match (parse_number(cell), parse_number(value)) {
            (Some(a), Some(b)) => a.partial_cmp(&b),
            _ => None,
        }
    };
    let ord = || num().unwrap_or_else(|| natural_cmp(&c, &v));
    Ok(match op {
        "equals" => match (parse_number(cell), parse_number(value)) {
            (Some(a), Some(b)) => a == b,
            _ => c == v,
        },
        "not_equals" => !row_matches(cell, "equals", value, re, ci)?,
        "contains" => c.contains(&v),
        "not_contains" => !c.contains(&v),
        "starts_with" => c.starts_with(&v),
        "ends_with" => c.ends_with(&v),
        "regex" => re.map(|r| r.is_match(cell)).unwrap_or(false),
        "gt" => ord().is_gt(),
        "gte" => ord().is_ge(),
        "lt" => ord().is_lt(),
        "lte" => ord().is_le(),
        "empty" => cell.trim().is_empty(),
        "not_empty" => !cell.trim().is_empty(),
        other => {
            return Err(ToolError::invalid(
                "filters",
                format!("unknown operator '{other}'"),
            ))
        }
    })
}

fn csv_filter_sort() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.csv_filter_sort", "Filter & Sort CSV", Category::Data)
        .describe("Filter rows by column conditions (equals, contains, regex, >, <), sort by one or more columns and choose/reorder columns")
        .keywords(&["csv", "filter", "sort", "where", "query", "select columns", "order by"])
        .params(input_params())
        .param(ParamSpec::text("filter_column").label("Filter column"))
        .param(ParamSpec::choice("filter_op", FILTER_OPS).label("Condition").default_str("contains"))
        .param(ParamSpec::text("filter_value").label("Value").default_str(""))
        .param(
            ParamSpec::json("filters")
                .label("More filters (all must match)")
                .describe("JSON list like [{\"column\": \"age\", \"op\": \"gte\", \"value\": 18}]")
                .advanced(),
        )
        .param(ParamSpec::boolean("case_insensitive").label("Ignore case when comparing").default_bool(true))
        .param(ParamSpec::text("sort").label("Sort by").placeholder("price desc, name").describe("Comma-separated columns, each optionally followed by asc/desc"))
        .param(ParamSpec::text("columns").label("Output columns (blank = all)").placeholder("name, price"))
        .param(delim_param("output_delimiter", false).advanced())
        .output(OutputKind::Data);
    sync_tool(spec, |ctx, p| {
        let (mut t, name) = load_table(p)?;
        let mut filters: Vec<Filter> = vec![];
        if let Some(c) = p.opt_str("filter_column") {
            filters.push(Filter {
                column: c.into(),
                op: p.str_or("filter_op", "contains").into(),
                value: json!(p.text("filter_value")),
            });
        }
        if let Some(f) = p.json("filters") {
            let more: Vec<Filter> = serde_json::from_value(f.clone())
                .map_err(|e| ToolError::invalid("filters", e.to_string()))?;
            filters.extend(more);
        }
        let ci = p.bool("case_insensitive");
        let compiled: Vec<(usize, String, String, Option<regex::Regex>)> = filters
            .iter()
            .map(|f| {
                let v = cell_string(&f.value);
                let re = if f.op == "regex" {
                    Some(
                        regex::RegexBuilder::new(&v)
                            .case_insensitive(ci)
                            .build()
                            .map_err(|e| ToolError::invalid("filters", e.to_string()))?,
                    )
                } else {
                    None
                };
                Ok((t.col(&f.column)?, f.op.clone(), v, re))
            })
            .collect::<ToolResult<_>>()?;
        let before = t.rows.len();
        let mut err = None;
        t.rows.retain(|r| {
            compiled.iter().all(|(i, op, v, re)| {
                match row_matches(&r[*i], op, v, re.as_ref(), ci) {
                    Ok(b) => b,
                    Err(e) => {
                        err = Some(e);
                        false
                    }
                }
            })
        });
        if let Some(e) = err {
            return Err(e);
        }
        if let Some(s) = p.opt_str("sort") {
            let keys: Vec<(usize, bool)> = s
                .split(',')
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(|k| {
                    let (c, desc) = match k.rsplit_once(' ') {
                        Some((c, d)) if d.eq_ignore_ascii_case("desc") => (c.trim(), true),
                        Some((c, d)) if d.eq_ignore_ascii_case("asc") => (c.trim(), false),
                        _ => (k, false),
                    };
                    Ok((t.col(c)?, desc))
                })
                .collect::<ToolResult<_>>()?;
            t.rows.sort_by(|a, b| {
                for (i, desc) in &keys {
                    let (x, y) = (&a[*i], &b[*i]);
                    let o = match (parse_number(x), parse_number(y)) {
                        (Some(m), Some(n)) => {
                            m.partial_cmp(&n).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (Some(_), None) if y.trim().is_empty() => std::cmp::Ordering::Less,
                        (None, Some(_)) if x.trim().is_empty() => std::cmp::Ordering::Greater,
                        _ => natural_cmp(&x.to_lowercase(), &y.to_lowercase()),
                    };
                    let o = if *desc { o.reverse() } else { o };
                    if o != std::cmp::Ordering::Equal {
                        return o;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        if let Some(c) = p.opt_str("columns") {
            let idx = t.cols(c)?;
            t = Table {
                headers: idx.iter().map(|i| t.headers[*i].clone()).collect(),
                rows: t
                    .rows
                    .iter()
                    .map(|r| idx.iter().map(|i| r[*i].clone()).collect())
                    .collect(),
            };
        }
        let removed = before - t.rows.len();
        emit_table(
            ctx,
            p,
            &t,
            &name,
            "filtered",
            out_delim(p),
            json!({"matched": t.rows.len(), "removed": removed}),
        )
    })
}

// ---------------------------------------------------------------------------
// data.csv_to_json / data.json_to_csv

fn csv_to_json() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.csv_to_json", "CSV to JSON", Category::Data)
        .describe("Convert CSV/TSV into a JSON array of objects, an array of arrays or NDJSON, with optional type inference")
        .keywords(&["csv to json", "tsv to json", "convert", "ndjson", "json lines"])
        .params(input_params())
        .param(ParamSpec::choice("shape", &[("objects", "Array of objects"), ("arrays", "Array of arrays"), ("ndjson", "NDJSON (one object per line)")]).default_str("objects"))
        .param(ParamSpec::boolean("infer_types").label("Detect numbers, booleans and nulls").default_bool(true))
        .param(ParamSpec::boolean("pretty").label("Pretty print").default_bool(true))
        .text_output("application/json")
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let (t, name) = load_table(p)?;
        let infer = p.bool("infer_types");
        let cell = |c: &String| if infer { infer_value(c) } else { json!(c) };
        let shape = p.str("shape")?;
        let (out, mime, ext) = match shape {
            "arrays" => {
                let mut v: Vec<Value> = vec![json!(t.headers)];
                v.extend(
                    t.rows
                        .iter()
                        .map(|r| Value::Array(r.iter().map(cell).collect())),
                );
                let v = Value::Array(v);
                (
                    if p.bool("pretty") {
                        serde_json::to_string_pretty(&v)?
                    } else {
                        serde_json::to_string(&v)?
                    },
                    "application/json",
                    "json",
                )
            }
            "ndjson" => {
                let objs = rows_to_objects(&t, infer);
                (
                    objs.as_array()
                        .unwrap()
                        .iter()
                        .map(|o| serde_json::to_string(o).unwrap_or_default())
                        .collect::<Vec<_>>()
                        .join("\n")
                        + "\n",
                    "application/x-ndjson",
                    "ndjson",
                )
            }
            _ => {
                let v = rows_to_objects(&t, infer);
                (
                    if p.bool("pretty") {
                        serde_json::to_string_pretty(&v)?
                    } else {
                        serde_json::to_string(&v)?
                    },
                    "application/json",
                    "json",
                )
            }
        };
        let data = json!({"records": t.rows.len(), "columns": t.headers});
        if p.opt_file("file").is_some() {
            return Ok(ToolOutput::files(vec![
                ctx.write_output(&with_extension(&name, ext), out.as_bytes())?
            ])
            .with_data(data));
        }
        Ok(ToolOutput::text_as(out, mime).with_data(data))
    })
}

fn json_to_csv() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.json_to_csv", "JSON to CSV", Category::Data)
        .describe("Convert a JSON array (or NDJSON) into CSV, flattening nested objects into dotted column names")
        .keywords(&["json to csv", "flatten", "convert", "export", "ndjson", "excel"])
        .param(ParamSpec::file("file").label("JSON file").primary().accept(&[".json", ".ndjson", ".jsonl", "application/json"]))
        .param(ParamSpec::textarea("text").label("Or paste JSON").syntax("json"))
        .param(ParamSpec::text("separator").label("Nested key separator").default_str("."))
        .param(
            ParamSpec::choice("arrays", &[("index", "Separate columns (tags.0, tags.1)"), ("join", "Join values with ;"), ("json", "Keep as JSON text")])
                .label("Arrays")
                .default_str("index"),
        )
        .param(delim_param("output_delimiter", false))
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let text = text_or_file(p, "text", "file")?;
        let items: Vec<Value> = match serde_json::from_str::<Value>(&text) {
            Ok(Value::Array(a)) => a,
            Ok(Value::Object(m)) => match m.values().find(|x| x.is_array()) {
                Some(Value::Array(a)) if m.len() == 1 => a.clone(),
                _ => vec![Value::Object(m)],
            },
            Ok(other) => vec![other],
            Err(e) => {
                let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                if lines.len() > 1 {
                    lines
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            serde_json::from_str(l).map_err(|e| {
                                ToolError::invalid("text", format!("line {}: {e}", i + 1))
                            })
                        })
                        .collect::<ToolResult<_>>()?
                } else {
                    return Err(crate::dev::formatters::parse_json(&text, "text")
                        .err()
                        .unwrap_or_else(|| ToolError::invalid("text", e.to_string())));
                }
            }
        };
        let (headers, rows) = flatten_records_with(
            &items,
            p.str_or("separator", "."),
            p.str_or("arrays", "index"),
        );
        let t = Table { headers, rows };
        let name = input_name(p, "file", "data.json");
        emit_table(ctx, p, &t, &name, "converted", out_delim(p), json!({}))
    })
}

// ---------------------------------------------------------------------------
// data.spreadsheet

fn spreadsheet() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.spreadsheet", "Spreadsheet Viewer & Export", Category::Data)
        .describe("Open Excel (xlsx, xls, xlsb) and OpenDocument (ods) spreadsheets: list sheets, preview a sheet and export it as CSV or JSON")
        .keywords(&["excel", "xlsx", "xls", "ods", "spreadsheet", "sheet", "xlsx to csv", "excel to json"])
        .param(ParamSpec::file("file").label("Spreadsheet").required().primary().accept(&[".xlsx", ".xlsm", ".xls", ".xlsb", ".ods"]))
        .param(ParamSpec::text("sheet").label("Sheet name (blank = first)"))
        .param(ParamSpec::choice("action", &[("preview", "Preview"), ("csv", "Export as CSV"), ("json", "Export as JSON"), ("all_csv", "Export every sheet as CSV")]).default_str("preview"))
        .param(ParamSpec::boolean("header").label("First row is a header").default_bool(true))
        .param(ParamSpec::integer("offset").label("Start at row").default_int(0).min(0.0).show_if("action", &["preview"]))
        .param(ParamSpec::integer("limit").label("Rows to show").default_int(100).range(1.0, PREVIEW_ROWS as f64).show_if("action", &["preview"]))
        .output(OutputKind::Data)
        .timeout(300);
    sync_tool(spec, |ctx, p| {
        use calamine::Reader;
        let f = p.file("file")?;
        let mut book = crate::sheet::open(f)?;
        let names = book.sheet_names();
        if names.is_empty() {
            return Err(ToolError::invalid("file", "workbook has no sheets"));
        }
        let sheet = match p.opt_str("sheet") {
            Some(s) => names
                .iter()
                .find(|n| n.as_str() == s || n.eq_ignore_ascii_case(s))
                .cloned()
                .ok_or_else(|| {
                    ToolError::invalid(
                        "sheet",
                        format!("no sheet named '{s}' (sheets: {})", names.join(", ")),
                    )
                })?,
            None => names[0].clone(),
        };
        let header = p.bool("header");
        let action = p.str("action")?;
        if action == "all_csv" {
            let mut arts = vec![];
            for (i, n) in names.iter().enumerate() {
                ctx.check_cancelled()?;
                ctx.progress(i as f32 / names.len() as f32, format!("sheet {n}"));
                let rows = crate::sheet::sheet_rows(&mut book, n)?;
                let cells: Vec<Vec<String>> = rows
                    .iter()
                    .map(|r| r.iter().map(crate::sheet::cell_text).collect())
                    .collect();
                let csv = write_csv(&[], &cells, b',', false)?;
                arts.push(ctx.write_output(
                    &format!(
                        "{}-{}.csv",
                        f.stem(),
                        crate::text::more::slug(n, "-", false, 60)
                    ),
                    csv.as_bytes(),
                )?);
            }
            return Ok(ToolOutput::files(arts).with_data(json!({"sheets": names})));
        }
        let rows = crate::sheet::sheet_rows(&mut book, &sheet)?;
        match action {
            "json" => {
                let mut rows = rows;
                let headers: Vec<String> = if header && !rows.is_empty() {
                    dedupe_headers(rows.remove(0).iter().map(crate::sheet::cell_text).collect())
                } else {
                    vec![]
                };
                let v: Vec<Value> = rows
                    .iter()
                    .map(|r| {
                        if header {
                            let mut m = serde_json::Map::new();
                            for (i, c) in r.iter().enumerate() {
                                let k = headers
                                    .get(i)
                                    .cloned()
                                    .unwrap_or_else(|| format!("column{}", i + 1));
                                m.insert(k, crate::sheet::cell_json(c));
                            }
                            Value::Object(m)
                        } else {
                            Value::Array(r.iter().map(crate::sheet::cell_json).collect())
                        }
                    })
                    .collect();
                let s = serde_json::to_string_pretty(&v)?;
                let a = ctx.write_output(
                    &format!(
                        "{}-{}.json",
                        f.stem(),
                        crate::text::more::slug(&sheet, "-", false, 60)
                    ),
                    s.as_bytes(),
                )?;
                Ok(ToolOutput::files(vec![a])
                    .with_data(json!({"sheet": sheet, "records": v.len()})))
            }
            "csv" => {
                let cells: Vec<Vec<String>> = rows
                    .iter()
                    .map(|r| r.iter().map(crate::sheet::cell_text).collect())
                    .collect();
                let csv = write_csv(&[], &cells, b',', false)?;
                let a = ctx.write_output(
                    &format!(
                        "{}-{}.csv",
                        f.stem(),
                        crate::text::more::slug(&sheet, "-", false, 60)
                    ),
                    csv.as_bytes(),
                )?;
                Ok(ToolOutput::files(vec![a])
                    .with_data(json!({"sheet": sheet, "rows": cells.len()})))
            }
            _ => {
                let t = table_from_cells(
                    rows.iter()
                        .map(|r| r.iter().map(crate::sheet::cell_text).collect())
                        .collect(),
                    header,
                );
                let offset = p.int_or("offset", 0).max(0) as usize;
                let limit = (p.int_or("limit", 100).max(1) as usize).min(PREVIEW_ROWS);
                let mut out = ToolOutput::data(
                    json!({"sheets": names, "sheet": sheet, "rows": t.rows.len(), "columns": t.headers.len(), "table": t.preview(offset, limit)}),
                );
                if t.rows.len() > offset + limit {
                    out = out.warn(format!("Showing up to {limit} of {} rows", t.rows.len()));
                }
                Ok(out)
            }
        }
    })
}

// ---------------------------------------------------------------------------
// data.delimiter

fn delimiter() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.delimiter", "Change CSV Delimiter", Category::Data)
        .describe("Convert between comma, tab, semicolon and pipe separated values and change the quoting style")
        .keywords(&["csv", "tsv", "delimiter", "separator", "semicolon", "tab", "quote", "csv to tsv", "tsv to csv"])
        .params(input_params())
        .param(delim_param("output_delimiter", false).default_str("tab"))
        .param(
            ParamSpec::choice("quote", &[("necessary", "Only when needed"), ("always", "Always"), ("non_numeric", "Non-numeric fields"), ("never", "Never")])
                .label("Quoting")
                .default_str("necessary"),
        )
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let (t, name) = load_table(p)?;
        let style = match p.str("quote")? {
            "always" => csv::QuoteStyle::Always,
            "non_numeric" => csv::QuoteStyle::NonNumeric,
            "never" => csv::QuoteStyle::Never,
            _ => csv::QuoteStyle::Necessary,
        };
        let d = out_delim(p);
        let csv = write_csv_style(&t.headers, &t.rows, d, style)?;
        let ext = if d == b'\t' { "tsv" } else { "csv" };
        let data = json!({"rows": t.rows.len(), "columns": t.headers.len()});
        if p.opt_file("file").is_some() {
            return Ok(ToolOutput::files(vec![
                ctx.write_output(&suffixed(&name, "converted", Some(ext)), csv.as_bytes())?
            ])
            .with_data(data));
        }
        Ok(ToolOutput::text_as(csv, "text/csv").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// data.columns

fn columns() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.columns", "Transform Columns", Category::Data)
        .describe("Rename, drop, reorder, split, merge, change case, trim or find-and-replace in CSV columns, or add a row index")
        .keywords(&["csv", "columns", "rename", "drop", "reorder", "split column", "merge columns", "transform"])
        .params(input_params())
        .param(
            ParamSpec::choice(
                "operation",
                &[
                    ("rename", "Rename"),
                    ("drop", "Drop columns"),
                    ("select", "Keep / reorder columns"),
                    ("split", "Split a column"),
                    ("merge", "Merge columns"),
                    ("upper", "UPPERCASE values"),
                    ("lower", "lowercase values"),
                    ("title", "Title Case values"),
                    ("trim", "Trim values"),
                    ("replace", "Find & replace in values"),
                    ("index", "Add row number column"),
                ],
            )
            .default_str("rename"),
        )
        .param(ParamSpec::text("columns").label("Column(s)").placeholder("name, email").describe("Comma-separated column names or 1-based numbers"))
        .param(ParamSpec::text("mapping").label("Renames").placeholder("old=new, other=Other").show_if("operation", &["rename"]))
        .param(ParamSpec::text("separator").label("Separator").default_str(" ").show_if("operation", &["split", "merge"]))
        .param(ParamSpec::text("new_names").label("New column name(s)").placeholder("first, last").show_if("operation", &["split", "merge", "index"]))
        .param(ParamSpec::text("find").label("Find").show_if("operation", &["replace"]))
        .param(ParamSpec::text("replace").label("Replace with").default_str("").show_if("operation", &["replace"]))
        .param(ParamSpec::boolean("regex").label("Find is a regular expression").show_if("operation", &["replace"]))
        .param(ParamSpec::boolean("keep_original").label("Keep the original column(s)").show_if("operation", &["split", "merge"]))
        .param(delim_param("output_delimiter", false).advanced())
        .output(OutputKind::Data);
    sync_tool(spec, |ctx, p| {
        let (mut t, name) = load_table(p)?;
        let op = p.str("operation")?;
        let need_cols = || -> ToolResult<Vec<usize>> {
            let c = p
                .opt_str("columns")
                .ok_or_else(|| ToolError::invalid("columns", "select at least one column"))?;
            t.cols(c)
        };
        match op {
            "rename" => {
                let m = p
                    .opt_str("mapping")
                    .ok_or_else(|| ToolError::invalid("mapping", "give renames like old=new"))?;
                for pair in split_list(m) {
                    let (a, b) = pair.split_once('=').ok_or_else(|| {
                        ToolError::invalid("mapping", format!("'{pair}' should look like old=new"))
                    })?;
                    let i = t.col(a)?;
                    t.headers[i] = b.trim().to_string();
                }
            }
            "drop" => {
                let idx: HashSet<usize> = need_cols()?.into_iter().collect();
                let keep: Vec<usize> = (0..t.headers.len()).filter(|i| !idx.contains(i)).collect();
                t = project(&t, &keep);
            }
            "select" => {
                let idx = need_cols()?;
                t = project(&t, &idx);
            }
            "split" => {
                let idx = need_cols()?;
                let i = idx[0];
                let sep = p.text("separator");
                let sep = if sep.is_empty() { " " } else { sep };
                let parts: Vec<Vec<String>> = t
                    .rows
                    .iter()
                    .map(|r| r[i].split(sep).map(|s| s.trim().to_string()).collect())
                    .collect();
                let n = parts.iter().map(|v| v.len()).max().unwrap_or(1);
                let names: Vec<String> = split_list(p.text("new_names"));
                let new_headers: Vec<String> = (0..n)
                    .map(|k| {
                        names
                            .get(k)
                            .cloned()
                            .unwrap_or_else(|| format!("{}_{}", t.headers[i], k + 1))
                    })
                    .collect();
                let keep = p.bool("keep_original");
                let mut headers = t.headers.clone();
                let at = if keep { i + 1 } else { i };
                if !keep {
                    headers.remove(i);
                }
                for (k, h) in new_headers.iter().enumerate() {
                    headers.insert(at + k, h.clone());
                }
                for (r, pv) in t.rows.iter_mut().zip(parts) {
                    if !keep {
                        r.remove(i);
                    }
                    for k in 0..n {
                        r.insert(at + k, pv.get(k).cloned().unwrap_or_default());
                    }
                }
                t.headers = headers;
            }
            "merge" => {
                let idx = need_cols()?;
                let sep = p.text("separator");
                let new_name = p
                    .opt_str("new_names")
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| {
                        idx.iter()
                            .map(|i| t.headers[*i].clone())
                            .collect::<Vec<_>>()
                            .join("_")
                    });
                let merged: Vec<String> = t
                    .rows
                    .iter()
                    .map(|r| {
                        idx.iter()
                            .map(|i| r[*i].as_str())
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(sep)
                    })
                    .collect();
                let at = idx[0];
                let keep = p.bool("keep_original");
                if keep {
                    t.headers.push(new_name);
                    for (r, m) in t.rows.iter_mut().zip(merged) {
                        r.push(m);
                    }
                } else {
                    let drop: HashSet<usize> = idx.iter().copied().collect();
                    let mut headers = vec![];
                    for (k, h) in t.headers.iter().enumerate() {
                        if k == at {
                            headers.push(new_name.clone());
                        } else if !drop.contains(&k) {
                            headers.push(h.clone());
                        }
                    }
                    for (r, m) in t.rows.iter_mut().zip(merged) {
                        let mut nr = vec![];
                        for (k, c) in r.iter().enumerate() {
                            if k == at {
                                nr.push(m.clone());
                            } else if !drop.contains(&k) {
                                nr.push(c.clone());
                            }
                        }
                        *r = nr;
                    }
                    t.headers = headers;
                }
            }
            "upper" | "lower" | "title" | "trim" | "replace" => {
                let idx = need_cols()?;
                let re = if op == "replace" {
                    let find = p
                        .opt_str("find")
                        .ok_or_else(|| ToolError::invalid("find", "enter text to find"))?;
                    let pat = if p.bool("regex") {
                        find.to_string()
                    } else {
                        regex::escape(find)
                    };
                    Some(
                        regex::Regex::new(&pat)
                            .map_err(|e| ToolError::invalid("find", e.to_string()))?,
                    )
                } else {
                    None
                };
                let rep = p.text("replace");
                for r in &mut t.rows {
                    for i in &idx {
                        let c = &r[*i];
                        r[*i] = match op {
                            "upper" => c.to_uppercase(),
                            "lower" => c.to_lowercase(),
                            "title" => crate::text::convert_case(c, "title")?,
                            "trim" => c.split_whitespace().collect::<Vec<_>>().join(" "),
                            _ => re.as_ref().unwrap().replace_all(c, rep).into_owned(),
                        };
                    }
                }
            }
            "index" => {
                let n = p.opt_str("new_names").unwrap_or("index").to_string();
                t.headers.insert(0, n);
                for (k, r) in t.rows.iter_mut().enumerate() {
                    r.insert(0, (k + 1).to_string());
                }
            }
            other => {
                return Err(ToolError::invalid(
                    "operation",
                    format!("unknown operation {other}"),
                ))
            }
        }
        emit_table(ctx, p, &t, &name, "columns", out_delim(p), json!({}))
    })
}

fn project(t: &Table, idx: &[usize]) -> Table {
    Table {
        headers: idx.iter().map(|i| t.headers[*i].clone()).collect(),
        rows: t
            .rows
            .iter()
            .map(|r| idx.iter().map(|i| r[*i].clone()).collect())
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// data.validate

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Rule {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    unique: bool,
    #[serde(default)]
    regex: Option<String>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    values: Option<Vec<String>>,
    #[serde(default)]
    max_length: Option<usize>,
}

fn check_type(kind: &str, v: &str) -> bool {
    let t = v.trim();
    match kind {
        "integer" | "int" => t.parse::<i64>().is_ok(),
        "number" | "float" | "decimal" => parse_number(t).is_some(),
        "boolean" | "bool" => matches!(
            t.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "1" | "0"
        ),
        "date" | "datetime" => crate::dev::time::parse_loose(t, false).is_some(),
        "email" => {
            let re = regex::Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap();
            re.is_match(t)
        }
        "url" => url::Url::parse(t)
            .map(|u| matches!(u.scheme(), "http" | "https" | "ftp"))
            .unwrap_or(false),
        "uuid" => uuid::Uuid::parse_str(t).is_ok(),
        _ => true,
    }
}

fn validate() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.validate", "Validate CSV / JSON Data", Category::Data)
        .describe("Check CSV rows against rules (required columns, types, unique, regex, ranges, allowed values) or JSON against a JSON Schema; lists every error")
        .keywords(&["validate", "csv", "data quality", "rules", "schema", "check", "lint", "json schema"])
        .params(input_params())
        .param(ParamSpec::text("required_columns").label("Required columns").placeholder("id, email"))
        .param(
            ParamSpec::json("rules")
                .label("Column rules")
                .describe("JSON like {\"id\": {\"type\": \"integer\", \"required\": true, \"unique\": true}, \"email\": {\"type\": \"email\"}, \"age\": {\"min\": 0, \"max\": 130}, \"status\": {\"values\": [\"open\", \"closed\"]}}. Types: integer, number, boolean, date, email, url, uuid, string"),
        )
        .param(ParamSpec::json("schema").label("JSON Schema (for JSON input)").advanced())
        .output(OutputKind::Data);
    sync_tool(spec, |_ctx, p| {
        if let Some(schema) = p.json("schema") {
            let text = text_or_file(p, "text", "file")?;
            let inst = crate::dev::formatters::parse_json(&text, "text")?;
            return crate::dev::query::schema_validate(schema, &inst);
        }
        let (t, _) = load_table(p)?;
        let mut errors: Vec<Vec<Value>> = vec![];
        for c in split_list(p.text("required_columns")) {
            if t.col(&c).is_err() {
                errors.push(vec![
                    json!(null),
                    json!(c),
                    json!(""),
                    json!("required column is missing"),
                ]);
            }
        }
        let rules: serde_json::Map<String, Value> = match p.json("rules") {
            Some(Value::Object(m)) => m.clone(),
            Some(Value::String(s)) if !s.trim().is_empty() => {
                serde_json::from_str(s).map_err(|e| ToolError::invalid("rules", e.to_string()))?
            }
            Some(Value::Null) | None => Default::default(),
            Some(Value::String(_)) => Default::default(),
            Some(_) => {
                return Err(ToolError::invalid(
                    "rules",
                    "rules must be an object keyed by column name",
                ))
            }
        };
        let mut total_errors = errors.len();
        for (col, r) in &rules {
            let rule: Rule = serde_json::from_value(r.clone())
                .map_err(|e| ToolError::invalid("rules", format!("{col}: {e}")))?;
            let i = match t.col(col) {
                Ok(i) => i,
                Err(_) => {
                    errors.push(vec![
                        json!(null),
                        json!(col),
                        json!(""),
                        json!("column from rules not found"),
                    ]);
                    total_errors += 1;
                    continue;
                }
            };
            let re = match &rule.regex {
                Some(r) => Some(
                    regex::Regex::new(&format!("^(?:{r})$"))
                        .map_err(|e| ToolError::invalid("rules", format!("{col}: {e}")))?,
                ),
                None => None,
            };
            let mut seen: HashMap<&str, usize> = HashMap::new();
            for (n, row) in t.rows.iter().enumerate() {
                let v = row[i].as_str();
                let line = n + 2;
                let mut push = |msg: String| {
                    total_errors += 1;
                    if errors.len() < 1000 {
                        errors.push(vec![json!(line), json!(col), json!(v), json!(msg)]);
                    }
                };
                if v.trim().is_empty() {
                    if rule.required {
                        push("value is required".into());
                    }
                    continue;
                }
                if let Some(k) = &rule.kind {
                    if !check_type(k, v) {
                        push(format!("not a valid {k}"));
                    }
                }
                if let Some(re) = &re {
                    if !re.is_match(v) {
                        push("does not match the pattern".into());
                    }
                }
                if rule.min.is_some() || rule.max.is_some() {
                    match parse_number(v) {
                        Some(x) => {
                            if rule.min.is_some_and(|m| x < m) {
                                push(format!("less than minimum {}", rule.min.unwrap()));
                            }
                            if rule.max.is_some_and(|m| x > m) {
                                push(format!("greater than maximum {}", rule.max.unwrap()));
                            }
                        }
                        None => push("not a number (needed for min/max)".into()),
                    }
                }
                if let Some(vals) = &rule.values {
                    if !vals.iter().any(|a| a == v) {
                        push(format!("not one of: {}", vals.join(", ")));
                    }
                }
                if let Some(ml) = rule.max_length {
                    if v.chars().count() > ml {
                        push(format!("longer than {ml} characters"));
                    }
                }
                if rule.unique {
                    if let Some(first) = seen.get(v) {
                        push(format!("duplicate of row {}", first));
                    } else {
                        seen.insert(v, line);
                    }
                }
            }
        }
        let mut out = ToolOutput::data(json!({
            "valid": total_errors == 0,
            "rows_checked": t.rows.len(),
            "errors": total_errors,
            "table": table(&["row", "column", "value", "problem"], errors),
        }));
        if total_errors > 1000 {
            out = out.warn(format!("Showing the first 1000 of {total_errors} errors"));
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// data.duplicates

fn duplicates() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.duplicates", "Find Duplicate Rows", Category::Data)
        .describe("Find or remove duplicate rows in CSV data, comparing all columns or only selected ones")
        .keywords(&["duplicates", "dedupe", "unique rows", "csv", "distinct", "remove duplicates"])
        .params(input_params())
        .param(ParamSpec::text("columns").label("Compare columns (blank = all)").placeholder("email"))
        .param(ParamSpec::boolean("ignore_case").label("Ignore case").default_bool(true))
        .param(ParamSpec::boolean("trim").label("Ignore surrounding whitespace").default_bool(true))
        .param(ParamSpec::choice("keep", &[("first", "Keep first occurrence"), ("last", "Keep last occurrence")]).default_str("first"))
        .param(delim_param("output_delimiter", false).advanced())
        .output(OutputKind::Data);
    sync_tool(spec, |ctx, p| {
        let (t, name) = load_table(p)?;
        let idx: Vec<usize> = match p.opt_str("columns") {
            Some(c) => t.cols(c)?,
            None => (0..t.headers.len()).collect(),
        };
        let (ic, tr) = (p.bool("ignore_case"), p.bool("trim"));
        let key = |r: &Vec<String>| -> Vec<String> {
            idx.iter()
                .map(|i| {
                    let mut s = if tr {
                        r[*i].trim().to_string()
                    } else {
                        r[*i].clone()
                    };
                    if ic {
                        s = s.to_lowercase();
                    }
                    s
                })
                .collect()
        };
        let mut groups: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        let mut order: Vec<Vec<String>> = vec![];
        for (n, r) in t.rows.iter().enumerate() {
            let k = key(r);
            let e = groups.entry(k.clone()).or_default();
            if e.is_empty() {
                order.push(k);
            }
            e.push(n);
        }
        let last = p.str("keep")? == "last";
        let keep: HashSet<usize> = groups
            .values()
            .map(|v| if last { *v.last().unwrap() } else { v[0] })
            .collect();
        let deduped = Table {
            headers: t.headers.clone(),
            rows: t
                .rows
                .iter()
                .enumerate()
                .filter(|(n, _)| keep.contains(n))
                .map(|(_, r)| r.clone())
                .collect(),
        };
        let dup_groups: Vec<Vec<Value>> = order
            .iter()
            .filter_map(|k| {
                let rows = &groups[k];
                (rows.len() > 1).then(|| {
                    vec![
                        json!(k.join(" | ")),
                        json!(rows.len()),
                        json!(rows
                            .iter()
                            .map(|r| (r + 2).to_string())
                            .collect::<Vec<_>>()
                            .join(", ")),
                    ]
                })
            })
            .take(PREVIEW_ROWS)
            .collect();
        let removed = t.rows.len() - deduped.rows.len();
        let n_groups = groups.values().filter(|v| v.len() > 1).count();
        let csv = write_csv(&deduped.headers, &deduped.rows, out_delim(p), false)?;
        let data = json!({
            "rows": t.rows.len(),
            "duplicate_rows_removed": removed,
            "duplicate_groups": n_groups,
            "unique_rows": deduped.rows.len(),
            "table": table(&["key", "count", "rows (line numbers)"], dup_groups),
        });
        if p.opt_file("file").is_some() {
            return Ok(ToolOutput::files(vec![
                ctx.write_output(&suffixed(&name, "deduped", None), csv.as_bytes())?
            ])
            .with_data(data));
        }
        Ok(ToolOutput::text_as(csv, "text/csv").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// data.stats

fn stats() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.stats", "Column Statistics", Category::Data)
        .describe("Summarize each column: count, empty values, distinct values, min/max/mean/median/standard deviation for numbers and top values for text")
        .keywords(&["statistics", "summary", "describe", "profile", "csv", "mean", "median", "column stats"])
        .params(input_params())
        .param(ParamSpec::integer("top").label("Top values per column").default_int(3).range(0.0, 20.0).advanced())
        .output(OutputKind::Data);
    sync_tool(spec, |_ctx, p| {
        let (t, _) = load_table(p)?;
        let top_n = p.int_or("top", 3) as usize;
        let mut rows = vec![];
        let fmt = |x: f64| -> Value {
            if x.fract() == 0.0 && x.abs() < 1e15 {
                json!(x as i64)
            } else {
                json!((x * 10000.0).round() / 10000.0)
            }
        };
        for (i, h) in t.headers.iter().enumerate() {
            let vals: Vec<&String> = t.rows.iter().map(|r| &r[i]).collect();
            let non_empty: Vec<&&String> = vals.iter().filter(|v| !v.trim().is_empty()).collect();
            let distinct: HashSet<&str> = non_empty.iter().map(|v| v.as_str()).collect();
            let kind = infer_type(&vals);
            let nums: Vec<f64> = non_empty.iter().filter_map(|v| parse_number(v)).collect();
            let (mut min, mut max, mut mean, mut median, mut sd, mut sum) = (
                json!(null),
                json!(null),
                json!(null),
                json!(null),
                json!(null),
                json!(null),
            );
            if (kind == "integer" || kind == "number") && !nums.is_empty() {
                let mut s = nums.clone();
                s.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let n = s.len() as f64;
                let total: f64 = s.iter().sum();
                let m = total / n;
                let var = if s.len() > 1 {
                    s.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)
                } else {
                    0.0
                };
                let med = if s.len() % 2 == 1 {
                    s[s.len() / 2]
                } else {
                    (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0
                };
                min = fmt(s[0]);
                max = fmt(*s.last().unwrap());
                mean = fmt(m);
                median = fmt(med);
                sd = fmt(var.sqrt());
                sum = fmt(total);
            } else if !non_empty.is_empty() {
                let mut s: Vec<&str> = non_empty.iter().map(|v| v.as_str()).collect();
                s.sort_by(|a, b| natural_cmp(a, b));
                min = json!(s[0]);
                max = json!(s[s.len() - 1]);
            }
            let mut freq: HashMap<&str, usize> = HashMap::new();
            for v in &non_empty {
                *freq.entry(v.as_str()).or_default() += 1;
            }
            let mut fv: Vec<_> = freq.into_iter().collect();
            fv.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let top: Vec<String> = fv
                .iter()
                .take(top_n)
                .map(|(v, c)| format!("{v} ({c})"))
                .collect();
            rows.push(vec![
                json!(h),
                json!(kind),
                json!(non_empty.len()),
                json!(vals.len() - non_empty.len()),
                json!(distinct.len()),
                min,
                max,
                mean,
                median,
                sd,
                sum,
                json!(top.join(", ")),
            ]);
        }
        Ok(ToolOutput::data(json!({
            "rows": t.rows.len(),
            "columns": t.headers.len(),
            "table": table(&["column", "type", "count", "empty", "distinct", "min", "max", "mean", "median", "stddev", "sum", "top values"], rows),
        })))
    })
}

// ---------------------------------------------------------------------------
// data.clean

fn clean() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.clean", "Clean CSV Data", Category::Data)
        .describe("Tidy CSV data: trim cells, normalize whitespace, blank out placeholder values, standardize dates to ISO 8601, remove empty rows and columns")
        .keywords(&["clean", "tidy", "csv", "normalize", "trim", "dates", "iso 8601", "empty rows", "data cleaning"])
        .params(input_params())
        .param(
            ParamSpec::multi_choice(
                "operations",
                &[
                    ("trim", "Trim cells"),
                    ("whitespace", "Collapse inner whitespace & line breaks"),
                    ("nulls", "Blank out NULL / N/A / - / none"),
                    ("dates", "Standardize dates to YYYY-MM-DD"),
                    ("empty_rows", "Remove empty rows"),
                    ("empty_columns", "Remove empty columns"),
                    ("headers", "Clean header names (snake_case)"),
                ],
            )
            .default_value(json!(["trim", "nulls", "empty_rows", "empty_columns"])),
        )
        .param(ParamSpec::boolean("day_first").label("Ambiguous dates are day-first (31/12/2024)").advanced())
        .param(delim_param("output_delimiter", false).advanced())
        .output(OutputKind::Data);
    sync_tool(spec, |ctx, p| {
        let (mut t, name) = load_table(p)?;
        let ops: HashSet<String> = p.list("operations").into_iter().collect();
        let day_first = p.bool("day_first");
        let mut changed = 0usize;
        let nulls = [
            "null",
            "n/a",
            "na",
            "none",
            "nil",
            "-",
            "--",
            "?",
            "#n/a",
            "nan",
            "undefined",
        ];
        for r in &mut t.rows {
            for c in r.iter_mut() {
                let orig = c.clone();
                if ops.contains("trim") {
                    *c = c.trim().to_string();
                }
                if ops.contains("whitespace") {
                    *c = c.split_whitespace().collect::<Vec<_>>().join(" ");
                }
                if ops.contains("nulls") && nulls.contains(&c.trim().to_lowercase().as_str()) {
                    c.clear();
                }
                if ops.contains("dates") && !c.is_empty() && parse_number(c).is_none() {
                    if let Some(dt) = crate::dev::time::parse_loose(c.trim(), day_first) {
                        *c = if dt.time() == chrono::NaiveTime::MIN {
                            dt.date().to_string()
                        } else {
                            dt.format("%Y-%m-%dT%H:%M:%S").to_string()
                        };
                    }
                }
                if *c != orig {
                    changed += 1;
                }
            }
        }
        let rows_before = t.rows.len();
        if ops.contains("empty_rows") {
            t.rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
        }
        let cols_before = t.headers.len();
        if ops.contains("empty_columns") {
            let keep: Vec<usize> = (0..t.headers.len())
                .filter(|i| t.rows.iter().any(|r| !r[*i].trim().is_empty()))
                .collect();
            t = project(&t, &keep);
        }
        if ops.contains("headers") {
            t.headers = dedupe_headers(
                t.headers
                    .iter()
                    .map(|h| crate::text::convert_case(h, "snake").unwrap_or_else(|_| h.clone()))
                    .collect(),
            );
        }
        let extra = json!({"cells_changed": changed, "rows_removed": rows_before - t.rows.len(), "columns_removed": cols_before - t.headers.len()});
        emit_table(ctx, p, &t, &name, "clean", out_delim(p), extra)
    })
}

// ---------------------------------------------------------------------------
// data.convert

fn sql_ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn sql_value(s: &str, infer: bool) -> String {
    if s.is_empty() {
        return "NULL".into();
    }
    if infer {
        if let Value::Number(n) = infer_value(s) {
            return n.to_string();
        }
    }
    format!("'{}'", s.replace('\'', "''"))
}

pub fn render_table(t: &Table, to: &str, opts: &Params) -> ToolResult<(Vec<u8>, &'static str)> {
    let infer = opts.bool("infer_types");
    Ok(match to {
        "csv" => (
            write_csv(&t.headers, &t.rows, b',', false)?.into_bytes(),
            "csv",
        ),
        "tsv" => (
            write_csv(&t.headers, &t.rows, b'\t', false)?.into_bytes(),
            "tsv",
        ),
        "json" => (
            serde_json::to_string_pretty(&rows_to_objects(t, infer))?.into_bytes(),
            "json",
        ),
        "ndjson" => (
            (rows_to_objects(t, infer)
                .as_array()
                .unwrap()
                .iter()
                .map(|o| serde_json::to_string(o).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n")
                .into_bytes(),
            "ndjson",
        ),
        "yaml" => (
            crate::dev::formatters::to_yaml(&rows_to_objects(t, infer))?.into_bytes(),
            "yaml",
        ),
        "markdown" => {
            let esc = |s: &str| s.replace('|', "\\|").replace('\n', "<br>");
            let mut out = format!(
                "| {} |\n|{}|\n",
                t.headers
                    .iter()
                    .map(|h| esc(h))
                    .collect::<Vec<_>>()
                    .join(" | "),
                t.headers
                    .iter()
                    .map(|_| " --- ")
                    .collect::<Vec<_>>()
                    .join("|")
            );
            for r in &t.rows {
                out.push_str(&format!(
                    "| {} |\n",
                    r.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")
                ));
            }
            (out.into_bytes(), "md")
        }
        "html" => {
            let e = |s: &str| crate::escape::html_escape(s, false);
            let mut out = String::from("<table>\n  <thead>\n    <tr>");
            for h in &t.headers {
                out.push_str(&format!("<th>{}</th>", e(h)));
            }
            out.push_str("</tr>\n  </thead>\n  <tbody>\n");
            for r in &t.rows {
                out.push_str("    <tr>");
                for c in r {
                    out.push_str(&format!("<td>{}</td>", e(c)));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("  </tbody>\n</table>\n");
            (out.into_bytes(), "html")
        }
        "sql" => {
            let table_name = opts.str_or("table_name", "data");
            let cols = t
                .headers
                .iter()
                .map(|h| sql_ident(h))
                .collect::<Vec<_>>()
                .join(", ");
            let mut out = String::new();
            for r in &t.rows {
                out.push_str(&format!(
                    "INSERT INTO {} ({cols}) VALUES ({});\n",
                    sql_ident(table_name),
                    r.iter()
                        .map(|c| sql_value(c, infer))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            (out.into_bytes(), "sql")
        }
        "xlsx" => {
            let mut wb = rust_xlsxwriter::Workbook::new();
            let ws = wb.add_worksheet();
            let bold = rust_xlsxwriter::Format::new().set_bold();
            for (c, h) in t.headers.iter().enumerate() {
                ws.write_string_with_format(0, c as u16, h, &bold)
                    .ctx("writing XLSX")?;
            }
            if t.rows.len() >= 1_048_575 {
                return Err(ToolError::limit("XLSX supports at most 1,048,576 rows"));
            }
            for (ri, r) in t.rows.iter().enumerate() {
                for (c, v) in r.iter().enumerate() {
                    let row = (ri + 1) as u32;
                    match if infer { infer_value(v) } else { json!(v) } {
                        Value::Number(n) if n.as_f64().is_some() => ws
                            .write_number(row, c as u16, n.as_f64().unwrap())
                            .map(|_| ()),
                        Value::Bool(b) => ws.write_boolean(row, c as u16, b).map(|_| ()),
                        Value::Null => Ok(()),
                        _ => ws.write_string(row, c as u16, v).map(|_| ()),
                    }
                    .ctx("writing XLSX")?;
                }
            }
            ws.set_freeze_panes(1, 0).ctx("writing XLSX")?;
            (wb.save_to_buffer().ctx("writing XLSX")?, "xlsx")
        }
        other => return Err(ToolError::invalid("to", format!("unknown format {other}"))),
    })
}

fn convert() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("data.convert", "Convert Table Data", Category::Data)
        .describe("Convert tabular data between CSV, TSV, JSON, NDJSON, YAML, Excel XLSX, Markdown tables, HTML tables and SQL INSERT statements")
        .keywords(&["convert", "csv to excel", "csv to xlsx", "xlsx to csv", "markdown table", "html table", "sql insert", "csv to json"])
        .params(input_params())
        .param(
            ParamSpec::choice(
                "to",
                &[
                    ("xlsx", "Excel (.xlsx)"),
                    ("csv", "CSV"),
                    ("tsv", "TSV"),
                    ("json", "JSON"),
                    ("ndjson", "NDJSON"),
                    ("yaml", "YAML"),
                    ("markdown", "Markdown table"),
                    ("html", "HTML table"),
                    ("sql", "SQL INSERT statements"),
                ],
            )
            .default_str("xlsx"),
        )
        .param(ParamSpec::boolean("infer_types").label("Detect numbers and booleans").default_bool(true))
        .param(ParamSpec::text("table_name").label("SQL table name").default_str("data").show_if("to", &["sql"]))
        .output(OutputKind::Files);
    sync_tool(spec, |ctx, p| {
        let (t, name) = load_table(p)?;
        let to = p.str("to")?;
        let (bytes, ext) = render_table(&t, to, p)?;
        let data = json!({"rows": t.rows.len(), "columns": t.headers.len(), "format": to});
        let a = ctx.write_output(&with_extension(&name, ext), &bytes)?;
        let mut out = ToolOutput::files(vec![a]).with_data(data);
        if to != "xlsx" && bytes.len() < 1_000_000 {
            let mime = match ext {
                "md" => "text/markdown",
                "html" => "text/html",
                "json" => "application/json",
                "csv" => "text/csv",
                _ => "text/plain",
            };
            out = out.with_text(String::from_utf8_lossy(&bytes).into_owned(), mime);
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    const CSV: &str = "name,age,city\nAnn,30,Paris\nBob,25,Berlin\nCid,35,paris\nBob,25,Berlin\n";

    #[tokio::test]
    async fn view_and_detect() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run("data.csv_view", json!({"text": "a;b\n1;x\n2;y"}))
            .await
            .unwrap();
        assert_eq!(out.data["table"]["columns"], json!(["a", "b"]));
        assert_eq!(out.data["total_rows"], 2);
        assert_eq!(out.data["column_types"][0]["type"], "integer");
        let out = h
            .run("data.csv_view", json!({"text": "1,2\n3,4"}))
            .await
            .unwrap();
        assert_eq!(out.data["table"]["columns"], json!(["column1", "column2"]));
        let f = h.write("t.tsv", b"x\ty\n1\t2\n");
        let out = h
            .run("data.csv_view", json!({"file": f.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(out.data["table"]["rows"][0], json!(["1", "2"]));
        assert_eq!(
            h.run("data.csv_view", json!({"text": "a,\"b\n1,2"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("data.csv_view", json!({})).await.unwrap_err().code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn filter_sort() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("data.csv_filter_sort", json!({"text": CSV, "filter_column": "city", "filter_op": "equals", "filter_value": "paris", "sort": "age desc", "columns": "name"})).await.unwrap();
        assert_eq!(out.text.unwrap(), "name\nCid\nAnn\n");
        let out = h
            .run(
                "data.csv_filter_sort",
                json!({"text": CSV, "filters": [{"column": "age", "op": "lt", "value": 30}]}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["matched"], 2);
        assert_eq!(
            h.run(
                "data.csv_filter_sort",
                json!({"text": CSV, "filter_column": "nope"})
            )
            .await
            .unwrap_err()
            .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn json_csv_roundtrip() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "data.csv_to_json",
                json!({"text": "a,b\n1,true\n007,", "pretty": false}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "[{\"a\":1,\"b\":true},{\"a\":\"007\",\"b\":null}]"
        );
        let out = h
            .run(
                "data.csv_to_json",
                json!({"text": "a,b\n1,2", "shape": "ndjson"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "{\"a\":1,\"b\":2}\n");
        let out = h.run("data.json_to_csv", json!({"text": "[{\"id\":1,\"u\":{\"n\":\"x\"},\"tags\":[\"a\",\"b\"]},{\"id\":2,\"extra\":\"y,z\"}]", "arrays": "join"})).await.unwrap();
        assert_eq!(
            out.text.unwrap(),
            "id,u.n,tags,extra\n1,x,a; b,\n2,,,\"y,z\"\n"
        );
        assert_eq!(
            h.run("data.json_to_csv", json!({"text": "[{bad"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn spreadsheet_and_convert() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run("data.convert", json!({"text": CSV, "to": "xlsx"}))
            .await
            .unwrap();
        let xlsx = &out.artifacts[0].path;
        assert!(out.artifacts[0].name.ends_with(".xlsx"));
        let out = h
            .run("data.spreadsheet", json!({"file": xlsx.to_str().unwrap()}))
            .await
            .unwrap();
        assert_eq!(out.data["table"]["columns"], json!(["name", "age", "city"]));
        assert_eq!(out.data["table"]["rows"][0], json!(["Ann", "30", "Paris"]));
        let out = h
            .run(
                "data.spreadsheet",
                json!({"file": xlsx.to_str().unwrap(), "action": "json"}),
            )
            .await
            .unwrap();
        let j: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&out.artifacts[0].path).unwrap()).unwrap();
        assert_eq!(j[0]["age"], 30);
        assert_eq!(
            h.run(
                "data.spreadsheet",
                json!({"file": xlsx.to_str().unwrap(), "sheet": "Nope"})
            )
            .await
            .unwrap_err()
            .code(),
            "invalid_input"
        );
        let out = h
            .run(
                "data.convert",
                json!({"text": "a,b\n1,x'y", "to": "sql", "table_name": "t"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "INSERT INTO \"t\" (\"a\", \"b\") VALUES (1, 'x''y');\n"
        );
        let out = h
            .run(
                "data.convert",
                json!({"text": "a,b\n1,x|y", "to": "markdown"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "| a | b |\n| --- | --- |\n| 1 | x\\|y |\n"
        );
        let out = h
            .run("data.convert", json!({"text": "a\n<b>", "to": "html"}))
            .await
            .unwrap();
        assert!(out.text.unwrap().contains("<td>&lt;b&gt;</td>"));
        let f = h.write("bad.xlsx", b"not a spreadsheet");
        assert_eq!(
            h.run("data.spreadsheet", json!({"file": f.to_str().unwrap()}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn delimiter_columns() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "data.delimiter",
                json!({"text": "a,b\n1,\"x;y\"", "output_delimiter": "semicolon"}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "a;b\n1;\"x;y\"\n");
        let c = |op: &str, extra: serde_json::Value| {
            let mut v = json!({"text": "full,age\nAnn Lee,30\n", "operation": op});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            v
        };
        assert_eq!(
            h.run("data.columns", c("rename", json!({"mapping": "age=years"})))
                .await
                .unwrap()
                .text
                .unwrap(),
            "full,years\nAnn Lee,30\n"
        );
        assert_eq!(
            h.run("data.columns", c("drop", json!({"columns": "age"})))
                .await
                .unwrap()
                .text
                .unwrap(),
            "full\nAnn Lee\n"
        );
        assert_eq!(
            h.run("data.columns", c("select", json!({"columns": "age,full"})))
                .await
                .unwrap()
                .text
                .unwrap(),
            "age,full\n30,Ann Lee\n"
        );
        assert_eq!(
            h.run(
                "data.columns",
                c(
                    "split",
                    json!({"columns": "full", "new_names": "first,last"})
                )
            )
            .await
            .unwrap()
            .text
            .unwrap(),
            "first,last,age\nAnn,Lee,30\n"
        );
        assert_eq!(
            h.run(
                "data.columns",
                c(
                    "merge",
                    json!({"columns": "full,age", "separator": "-", "new_names": "x"})
                )
            )
            .await
            .unwrap()
            .text
            .unwrap(),
            "x\nAnn Lee-30\n"
        );
        assert_eq!(
            h.run("data.columns", c("upper", json!({"columns": "1"})))
                .await
                .unwrap()
                .text
                .unwrap(),
            "full,age\nANN LEE,30\n"
        );
        assert_eq!(
            h.run("data.columns", c("index", json!({})))
                .await
                .unwrap()
                .text
                .unwrap(),
            "index,full,age\n1,Ann Lee,30\n"
        );
        assert_eq!(
            h.run("data.columns", c("drop", json!({"columns": "zzz"})))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn validate_dupes_stats_clean() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run("data.validate", json!({"text": "id,email,age\n1,a@b.co,30\n1,bad,200\nx,,5", "required_columns": "id,phone", "rules": {"id": {"type": "integer", "unique": true}, "email": {"type": "email", "required": true}, "age": {"max": 130}}}))
            .await
            .unwrap();
        assert_eq!(out.data["valid"], false);
        assert_eq!(out.data["errors"], 6);
        let out = h.run("data.validate", json!({"text": "[1, \"x\"]", "schema": {"type": "array", "items": {"type": "integer"}}})).await.unwrap();
        assert_eq!(out.data["valid"], false);
        let out = h
            .run("data.duplicates", json!({"text": CSV}))
            .await
            .unwrap();
        assert_eq!(out.data["duplicate_rows_removed"], 1);
        let out = h
            .run("data.duplicates", json!({"text": CSV, "columns": "city"}))
            .await
            .unwrap();
        assert_eq!(out.data["duplicate_rows_removed"], 2);
        let out = h.run("data.stats", json!({"text": CSV})).await.unwrap();
        let rows = out.data["table"]["rows"].as_array().unwrap();
        assert_eq!(rows[1][1], "integer");
        assert_eq!(rows[1][7], 28.75);
        assert_eq!(rows[1][8], 27.5);
        let out = h
            .run("data.clean", json!({"text": "a,b,c\n  x ,N/A,\n,,\n03/15/2024,y,", "operations": ["trim", "nulls", "dates", "empty_rows", "empty_columns"]}))
            .await
            .unwrap();
        assert_eq!(out.text.unwrap(), "a,b\nx,\n2024-03-15,y\n");
    }
}
