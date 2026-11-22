//! `[http]` manifests: one declarative GET request instead of a command.
//!
//! The scheme and host are literal in the manifest; placeholders may only
//! fill the path, query and header values, and are percent-encoded in the
//! URL. Requests go through [`ToolContext::http_client`], so the instance's
//! network policy (private addresses, redirects, denied hosts) applies.

use crate::error::{ToolError, ToolResult};
use crate::spec::{OutputKind, ParamKind, ParamSpec};
use crate::tool::{ToolContext, ToolOutput};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use serde_json_path::JsonPath;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Everything but RFC 3986 unreserved characters is escaped.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const EXCERPT_CHARS: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HttpSection {
    url: String,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default = "default_http_timeout")]
    timeout_secs: u64,
}

fn default_http_timeout() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TableSection {
    /// JSON path selecting the rows; the document root when absent.
    #[serde(default)]
    rows: Option<String>,
    /// Each column path selects an array; row `i` takes item `i` of each.
    #[serde(default)]
    columnar: bool,
    columns: Vec<ColumnDecl>,
    /// Text template repeated for each row, after `[output] text`.
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ColumnDecl {
    name: String,
    path: String,
    #[serde(default)]
    labels: Option<String>,
}

/// An `[output] json` field: a JSON path, or a path with a label set.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum PickDecl {
    Path(String),
    Labeled {
        path: String,
        labels: Option<String>,
    },
}

/// What the `[output]` section asks of an `[http]` response.
#[derive(Debug, Default)]
pub(crate) struct HttpOutputDecl {
    pub body: Option<String>,
    pub json: BTreeMap<String, PickDecl>,
    pub table: Option<TableSection>,
    pub text: Option<String>,
    pub labels: BTreeMap<String, BTreeMap<String, String>>,
}

type Labels = Arc<BTreeMap<String, String>>;

#[derive(Debug, Clone)]
struct Pick {
    name: String,
    src: String,
    path: JsonPath,
    labels: Option<Labels>,
}

#[derive(Debug, Clone)]
struct Table {
    rows: Option<(String, JsonPath)>,
    columnar: bool,
    columns: Vec<Pick>,
    text: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct HttpRequest {
    url: String,
    host: String,
    headers: Vec<(HeaderName, String)>,
    timeout: Duration,
    body_mime: Option<String>,
    picks: Vec<Pick>,
    table: Option<Table>,
    text: Option<String>,
}

/// A canned response served instead of the network, so manifest tests run
/// offline. Network permission and URL policy are still checked, and every
/// request URL is recorded for assertions.
#[derive(Debug, Clone)]
pub struct RecordedResponse {
    status: u16,
    content_type: Option<String>,
    body: Vec<u8>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl RecordedResponse {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            content_type: None,
            body: body.into(),
            requests: Arc::default(),
        }
    }

    /// A 200 response with a JSON body.
    pub fn json(value: &Value) -> Self {
        Self::new(200, value.to_string()).content_type("application/json")
    }

    pub fn content_type(mut self, mime: &str) -> Self {
        self.content_type = Some(mime.to_string());
        self
    }

    /// The URLs requested so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }
}

impl Pick {
    /// Swap values (or the items of an array) found in the label set for
    /// their labels; other values are kept.
    fn apply_labels(&self, v: Value) -> Value {
        let Some(labels) = &self.labels else { return v };
        let label = |v: Value| {
            let key = match &v {
                Value::String(s) => s.clone(),
                Value::Number(_) | Value::Bool(_) => v.to_string(),
                _ => return v,
            };
            labels
                .get(&key)
                .map(|l| Value::String(l.clone()))
                .unwrap_or(v)
        };
        match v {
            Value::Array(a) => Value::Array(a.into_iter().map(label).collect()),
            other => label(other),
        }
    }
}

impl Table {
    /// Rows from parallel arrays: row `i` takes item `i` of each column.
    fn columnar_rows(&self, doc: &Value) -> Vec<Vec<Value>> {
        let cols: Vec<Vec<Value>> = self
            .columns
            .iter()
            .map(
                |c| match c.apply_labels(collect(&c.path.query(doc).all())) {
                    Value::Array(a) => a,
                    Value::Null => vec![],
                    one => vec![one],
                },
            )
            .collect();
        let n = cols.iter().map(Vec::len).max().unwrap_or(0);
        (0..n)
            .map(|i| {
                cols.iter()
                    .map(|c| c.get(i).cloned().unwrap_or(Value::Null))
                    .collect()
            })
            .collect()
    }
}

/// One match as itself, several (or none) as an array.
fn collect(found: &[&Value]) -> Value {
    match found {
        [] => Value::Null,
        [one] => (*one).clone(),
        many => Value::Array(many.iter().map(|v| (*v).clone()).collect()),
    }
}

fn check_template(s: &str, what: &str, known: impl Fn(&str) -> bool) -> Result<(), String> {
    probe(s).map_err(|e| format!("{what}: {e}"))?;
    for ph in crate::manifest::placeholders(s) {
        if !known(&ph) {
            return Err(format!(
                "placeholder '{{{ph}}}' in {what} does not match any output field"
            ));
        }
    }
    Ok(())
}

/// Fill `{field}` from output values: text as-is, arrays comma-separated,
/// missing and null values empty.
fn fill_template<'a>(s: &str, get: impl Fn(&str) -> Option<&'a Value>) -> String {
    fn show(v: &Value) -> String {
        match v {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join(", "),
            other => other.to_string(),
        }
    }
    substitute(s, |n| Ok(get(n).map(show).unwrap_or_default())).unwrap_or_else(|_| s.to_string())
}

/// Replace each `{name}` in `s` with `f(name)`.
fn substitute(s: &str, mut f: impl FnMut(&str) -> ToolResult<String>) -> ToolResult<String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let j = rest[i..]
            .find('}')
            .ok_or_else(|| ToolError::failed("unterminated placeholder"))?;
        out.push_str(&f(&rest[i + 1..i + j])?);
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn probe(s: &str) -> Result<String, String> {
    substitute(s, |_| Ok("x".into())).map_err(|_| format!("unterminated placeholder in '{s}'"))
}

/// The host of `url`, which must be written out: no placeholders in the
/// scheme or host, no credentials.
fn literal_host(url: &str) -> Result<String, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or("[http] url must start with https:// or http://")?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return Err("[http] url must start with https:// or http://".into());
    }
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    if authority.contains(['{', '}']) {
        return Err(
            "[http] url: the scheme and host must be written out (no placeholders there)".into(),
        );
    }
    if authority.contains('@') {
        return Err(
            "[http] url must not contain credentials; send a secret setting in a header instead"
                .into(),
        );
    }
    let parsed =
        url::Url::parse(&probe(url)?).map_err(|e| format!("[http] url is invalid: {e}"))?;
    parsed
        .host_str()
        .filter(|h| !h.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "[http] url needs a host".into())
}

fn compile_path(what: &str, src: &str) -> Result<JsonPath, String> {
    JsonPath::parse(src).map_err(|e| format!("{what}: invalid JSON path '{src}': {e}"))
}

impl HttpRequest {
    pub(crate) fn parse(
        http: HttpSection,
        out: HttpOutputDecl,
        params: &[ParamSpec],
        settings: &[ParamSpec],
        tool_timeout_secs: u64,
    ) -> Result<Self, String> {
        let check = |s: &str, what: &str| -> Result<(), String> {
            for ph in crate::manifest::placeholders(s)
                .iter()
                .flat_map(|ph| ph.split('|'))
            {
                if let Some(name) = ph.strip_prefix("settings.") {
                    if settings.iter().any(|p| p.name == name) {
                        continue;
                    }
                    return Err(format!(
                        "placeholder '{{{ph}}}' in {what} does not match any setting"
                    ));
                }
                match params.iter().find(|p| p.name == ph) {
                    Some(p) if matches!(p.kind, ParamKind::File { .. } | ParamKind::Path { .. }) => {
                        return Err(format!(
                            "placeholder '{{{ph}}}' in {what}: file and path parameters cannot be sent in a request"
                        ))
                    }
                    Some(_) => {}
                    None => {
                        return Err(format!(
                            "placeholder '{{{ph}}}' in {what} does not match any parameter or setting"
                        ))
                    }
                }
            }
            Ok(())
        };
        let host = literal_host(&http.url)?;
        check(&http.url, "[http] url")?;
        let mut headers = vec![];
        for (name, value) in http.headers {
            let what = format!("header '{name}'");
            let hn = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| format!("[http] {what} is not a valid header name"))?;
            if hn == reqwest::header::HOST {
                return Err("[http] headers cannot set Host".into());
            }
            check(&value, &what)?;
            HeaderValue::from_str(&probe(&value)?)
                .map_err(|_| format!("[http] {what} has an invalid value"))?;
            headers.push((hn, value));
        }
        if http.timeout_secs == 0 {
            return Err("[http] timeout_secs must be at least 1".into());
        }
        let timeout = http
            .timeout_secs
            .min(tool_timeout_secs.saturating_sub(1))
            .max(1);
        let label_sets: BTreeMap<&str, Labels> = out
            .labels
            .iter()
            .map(|(k, v)| (k.as_str(), Arc::new(v.clone())))
            .collect();
        let labels = |what: &str, set: Option<&String>| -> Result<Option<Labels>, String> {
            set.map(|n| {
                label_sets.get(n.as_str()).cloned().ok_or_else(|| {
                    format!("{what}: labels '{n}' does not match any [output.labels] set")
                })
            })
            .transpose()
        };
        let picks = out
            .json
            .iter()
            .map(|(name, decl)| {
                let what = format!("[output] json field '{name}'");
                let (src, set) = match decl {
                    PickDecl::Path(src) => (src, None),
                    PickDecl::Labeled { path, labels } => (path, labels.as_ref()),
                };
                Ok(Pick {
                    name: name.clone(),
                    src: src.clone(),
                    path: compile_path(&what, src)?,
                    labels: labels(&what, set)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let table = match out.table {
            None => None,
            Some(t) => {
                if t.columns.is_empty() {
                    return Err("[output.table] needs at least one column".into());
                }
                if picks.iter().any(|p| p.name == "table") {
                    return Err("[output] json field 'rows' clashes with [output.table]".into());
                }
                let mut columns: Vec<Pick> = vec![];
                for c in t.columns {
                    if columns.iter().any(|p| p.name == c.name) {
                        return Err(format!(
                            "[output.table] column '{}' is declared twice",
                            c.name
                        ));
                    }
                    let what = format!("[output.table] column '{}'", c.name);
                    let path = compile_path(&what, &c.path)?;
                    columns.push(Pick {
                        labels: labels(&what, c.labels.as_ref())?,
                        name: c.name,
                        src: c.path,
                        path,
                    });
                }
                if t.columnar && t.rows.is_some() {
                    return Err("[output.table] takes rows or columnar, not both".into());
                }
                if let Some(text) = &t.text {
                    check_template(text, "[output.table] text", |n| {
                        columns.iter().any(|c| c.name == n) || picks.iter().any(|p| p.name == n)
                    })?;
                }
                let rows = t
                    .rows
                    .map(|src| compile_path("[output.table] rows", &src).map(|p| (src, p)))
                    .transpose()?;
                Some(Table {
                    rows,
                    columnar: t.columnar,
                    columns,
                    text: t.text,
                })
            }
        };
        if out.body.is_some() && (!picks.is_empty() || table.is_some()) {
            return Err("[output] body cannot be combined with json or table".into());
        }
        if let Some(text) = &out.text {
            if picks.is_empty() && table.is_none() {
                return Err("[output] text needs json or table fields to fill it".into());
            }
            check_template(text, "[output] text", |n| picks.iter().any(|p| p.name == n))?;
        }
        Ok(Self {
            url: http.url,
            host,
            headers,
            timeout: Duration::from_secs(timeout),
            body_mime: out.body,
            picks,
            table,
            text: out.text,
        })
    }

    fn returns_data(&self) -> bool {
        !self.picks.is_empty() || self.table.is_some()
    }

    fn has_text(&self) -> bool {
        self.text.is_some() || self.table.as_ref().is_some_and(|t| t.text.is_some())
    }

    pub(crate) fn output_kind(&self) -> OutputKind {
        if self.returns_data() {
            OutputKind::Data
        } else {
            OutputKind::Text
        }
    }

    pub(crate) fn text_mime(&self) -> Option<String> {
        if self.has_text() {
            Some("text/plain".into())
        } else if self.returns_data() {
            None
        } else {
            self.body_mime.clone()
        }
    }

    fn program(&self) -> String {
        format!("GET {}", self.host)
    }

    /// Send the request, with `lookup` giving the raw value of a placeholder
    /// (`lat`, `settings.key`).
    pub(crate) async fn run(
        &self,
        ctx: &ToolContext,
        recorded: Option<&RecordedResponse>,
        lookup: &(dyn Fn(&str) -> ToolResult<String> + Sync),
    ) -> ToolResult<ToolOutput> {
        let split = self.url.find('?').unwrap_or(self.url.len());
        let (path_part, query_part) = self.url.split_at(split);
        let encode = |ph: &str, in_path: bool| -> ToolResult<String> {
            let v = lookup(ph)?;
            if in_path && (v == "." || v == "..") {
                return Err(ToolError::invalid(
                    ph,
                    "cannot be '.' or '..' in the URL path",
                ));
            }
            Ok(utf8_percent_encode(&v, COMPONENT).to_string())
        };
        let raw = substitute(path_part, |ph| encode(ph, true))?
            + &substitute(query_part, |ph| encode(ph, false))?;
        let url = crate::net::check_url(ctx.net_policy(), &raw)?;
        if url.host_str() != Some(self.host.as_str()) {
            return Err(ToolError::failed(
                "request URL does not keep the manifest's host",
            ));
        }
        let mut headers = HeaderMap::new();
        for (name, template) in &self.headers {
            let mut value =
                HeaderValue::from_str(&substitute(template, lookup)?).map_err(|_| {
                    ToolError::invalid(name.as_str(), "header value contains invalid characters")
                })?;
            value.set_sensitive(true);
            headers.insert(name.clone(), value);
        }
        let limit = ctx.limits().max_stdout_bytes;
        let mut client = ctx.http_client()?;
        if !self.headers.is_empty() {
            client = crate::net::guarded_client_same_origin(ctx.net_policy().clone())?;
        }
        let (status, content_type, body) = match recorded {
            Some(r) => {
                if let Ok(mut reqs) = r.requests.lock() {
                    reqs.push(url.to_string());
                }
                if r.body.len() > limit {
                    return Err(too_large(r.body.len() as u64, limit));
                }
                (r.status, r.content_type.clone(), r.body.clone())
            }
            None => self.send(client, url, headers, limit).await?,
        };
        if !(200..300).contains(&status) {
            let reason = reqwest::StatusCode::from_u16(status)
                .ok()
                .and_then(|s| s.canonical_reason())
                .map(|r| format!(" {r}"))
                .unwrap_or_default();
            return Err(ToolError::External {
                program: self.program(),
                exit_code: Some(i32::from(status)),
                stderr: format!("HTTP {status}{reason}: {}", excerpt(&body)),
            });
        }
        if !self.returns_data() {
            let mime = self
                .body_mime
                .clone()
                .or_else(|| {
                    content_type
                        .as_deref()
                        .and_then(|c| c.split(';').next())
                        .map(|c| c.trim().to_string())
                        .filter(|c| !c.is_empty())
                })
                .unwrap_or_else(|| "text/plain".into());
            let charset = content_type.as_deref().and_then(|c| {
                c.split(';')
                    .skip(1)
                    .find_map(|p| p.trim().strip_prefix("charset="))
                    .and_then(|l| encoding_rs::Encoding::for_label(l.trim_matches('"').as_bytes()))
            });
            let text = charset.unwrap_or(encoding_rs::UTF_8).decode(&body).0;
            return Ok(ToolOutput::new().with_text(text, &mime));
        }
        let doc: Value = serde_json::from_slice(&body).map_err(|e| {
            ToolError::failed(format!(
                "{} did not return JSON ({e}): {}",
                self.program(),
                excerpt(&body)
            ))
        })?;
        let mut data = Map::new();
        for p in &self.picks {
            data.insert(p.name.clone(), self.pick(p, &doc, "[output] json field")?);
        }
        let mut lines = vec![];
        if let Some(text) = &self.text {
            lines.push(fill_template(text, |n| data.get(n)));
        }
        if let Some(t) = &self.table {
            let rows = if t.columnar {
                t.columnar_rows(&doc)
            } else {
                self.table_rows(t, &doc)?
            };
            if let Some(text) = &t.text {
                for row in &rows {
                    lines.push(fill_template(text, |n| {
                        match t.columns.iter().position(|c| c.name == n) {
                            Some(i) => row.get(i),
                            None => data.get(n),
                        }
                    }));
                }
            }
            let rows: Vec<Value> = rows.into_iter().map(Value::Array).collect();
            let columns: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
            data.insert("table".into(), json!({ "columns": columns, "rows": rows }));
        }
        let out = ToolOutput::new().with_data(Value::Object(data));
        Ok(if self.has_text() {
            out.with_text(lines.join("\n"), "text/plain")
        } else {
            out
        })
    }

    fn table_rows(&self, t: &Table, doc: &Value) -> ToolResult<Vec<Vec<Value>>> {
        let rows: Vec<&Value> = match &t.rows {
            None => match doc {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            },
            Some((src, path)) => {
                let found = path.query(doc).all();
                match found.as_slice() {
                    [] => return Err(self.missing("[output.table] rows", src)),
                    [Value::Array(a)] if !selects_many(src) => a.iter().collect(),
                    _ => found,
                }
            }
        };
        Ok(rows
            .into_iter()
            .map(|row| {
                t.columns
                    .iter()
                    .map(|c| match c.path.query(row).all().as_slice() {
                        [] => Value::Null,
                        found => c.apply_labels(collect(found)),
                    })
                    .collect()
            })
            .collect())
    }

    fn pick(&self, p: &Pick, doc: &Value, what: &str) -> ToolResult<Value> {
        match p.path.query(doc).all().as_slice() {
            [] => Err(self.missing(&format!("{what} '{}'", p.name), &p.src)),
            found => Ok(p.apply_labels(collect(found))),
        }
    }

    fn missing(&self, what: &str, src: &str) -> ToolError {
        ToolError::failed(format!(
            "{what}: JSON path '{src}' matched nothing in the response from {}",
            self.host
        ))
    }

    async fn send(
        &self,
        client: reqwest::Client,
        url: url::Url,
        headers: HeaderMap,
        limit: usize,
    ) -> ToolResult<(u16, Option<String>, Vec<u8>)> {
        let mut resp = client
            .get(url)
            .headers(headers)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| self.transport_error(e))?;
        let status = resp.status().as_u16();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        if let Some(len) = resp.content_length() {
            if len > limit as u64 {
                return Err(too_large(len, limit));
            }
        }
        let mut body = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|e| self.transport_error(e))? {
            if body.len() + chunk.len() > limit {
                return Err(too_large((body.len() + chunk.len()) as u64, limit));
            }
            body.extend_from_slice(&chunk);
        }
        Ok((status, content_type, body))
    }

    fn transport_error(&self, e: reqwest::Error) -> ToolError {
        // Expanded URLs can carry secret settings.
        let e = e.without_url();
        if e.is_timeout() {
            return ToolError::External {
                program: self.program(),
                exit_code: None,
                stderr: format!("timed out after {}s", self.timeout.as_secs()),
            };
        }
        let mut chain = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            chain.push_str(": ");
            chain.push_str(&s.to_string());
            src = s.source();
        }
        if chain.contains("network policy") || chain.contains("redirect refused") {
            ToolError::denied(format!("{}: {chain}", self.program()))
        } else {
            ToolError::failed(format!("{} failed: {chain}", self.program()))
        }
    }
}

/// Whether a JSON path can select several nodes (each a row) rather than
/// one array of rows.
fn selects_many(src: &str) -> bool {
    let mut syntax = String::new();
    let mut quote = None;
    let mut chars = src.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(_), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, c) => syntax.push(c),
        }
    }
    syntax.contains(['*', '?', ':', ',']) || syntax.contains("..")
}

fn too_large(len: u64, limit: usize) -> ToolError {
    ToolError::limit(format!(
        "response is {} (limit {})",
        crate::util::human_size(len),
        crate::util::human_size(limit as u64)
    ))
}

fn excerpt(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return "(empty body)".into();
    }
    let mut short: String = flat.chars().take(EXCERPT_CHARS).collect();
    if flat.chars().count() > EXCERPT_CHARS {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{parse_manifest, ExternalTool};
    use crate::spec::{Execution, Permission};
    use crate::testing::TestHarness;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const WEATHER: &str = r#"
[tool]
id = "ext.weather"
name = "Weather"
category = "data"

[http]
url = "https://api.example.com/v1/forecast?lat={settings.lat}&lon={settings.lon}&q={city}"
headers = { Authorization = "Bearer {settings.key}" }

[[params]]
name = "city"
type = "text"
default = ""

[[settings]]
name = "lat"
type = "text"
default = "52.52"

[[settings]]
name = "lon"
type = "text"
default = "13.41"

[[settings]]
name = "key"
type = "text"
secret = true

[output]
json = { temp = "$.current.temperature_2m", unit = "$.current_units.temperature_2m" }
"#;

    fn manifest(http: &str, extra: &str) -> String {
        format!(
            "[tool]\nid = \"ext.t\"\nname = \"T\"\ncategory = \"data\"\n\n[http]\n{http}\n\n[[params]]\nname = \"q\"\ntype = \"text\"\ndefault = \"\"\n\n[[params]]\nname = \"file\"\ntype = \"file\"\nrequired = false\n{extra}"
        )
    }

    fn weather() -> ExternalTool {
        parse_manifest(WEATHER, "w")
            .unwrap()
            .with_setting_values(json!({ "key": "k" }).as_object().unwrap())
            .unwrap()
    }

    fn harness(tool: ExternalTool) -> TestHarness {
        TestHarness::new(vec![Arc::new(tool)])
    }

    #[test]
    fn http_manifest_is_a_network_tool() {
        let t = parse_manifest(WEATHER, "w").unwrap();
        assert!(t.is_http());
        let spec = crate::tool::Tool::spec(&t);
        assert_eq!(spec.execution, Execution::Network);
        assert!(spec.permissions.contains(&Permission::Network));
        assert_eq!(spec.output.kind, OutputKind::Data);
    }

    #[test]
    fn rejects_bad_http_manifests() {
        let both = format!("{WEATHER}\n[command]\nprogram = \"curl\"\nargs = []\n");
        assert!(parse_manifest(&both, "x")
            .map(|_| ())
            .unwrap_err()
            .contains("never both"));
        let neither = "[tool]\nid = \"ext.n\"\nname = \"N\"\ncategory = \"data\"\n";
        assert!(parse_manifest(neither, "x")
            .map(|_| ())
            .unwrap_err()
            .contains("[command] or an [http]"));
        let cases = [
            ("url = \"https://{q}.example.com/\"", "", "scheme and host"),
            ("url = \"{q}://example.com/\"", "", "https://"),
            ("url = \"https://example.com:{q}/\"", "", "scheme and host"),
            ("url = \"https://u:p@example.com/\"", "", "credentials"),
            ("url = \"ftp://example.com/\"", "", "https://"),
            ("url = \"https://example.com/{nope}\"", "", "does not match"),
            (
                "url = \"https://example.com/{settings.nope}\"",
                "",
                "does not match any setting",
            ),
            ("url = \"https://example.com/{file}\"", "", "cannot be sent"),
            (
                "url = \"https://example.com/\"\nmethod = \"POST\"",
                "",
                "unknown field",
            ),
            (
                "url = \"https://example.com/\"\nheaders = { Host = \"x\" }",
                "",
                "Host",
            ),
            (
                "url = \"https://example.com/\"\nheaders = { A = \"{nope}\" }",
                "",
                "does not match",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output]\nfile = \"a.txt\"\n",
                "[command] manifests",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output]\njson = { a = \"$[\" }\n",
                "invalid JSON path",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output]\nbody = \"text/plain\"\njson = { a = \"$.a\" }\n",
                "cannot be combined",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output]\ntext = \"{a}\"\n",
                "needs json or table",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output]\ntext = \"{b}\"\njson = { a = \"$.a\" }\n",
                "does not match any output field",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output]\njson = { a = { path = \"$.a\", labels = \"x\" } }\n",
                "does not match any [output.labels] set",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output.table]\ncolumnar = true\nrows = \"$.r\"\ncolumns = [{ name = \"a\", path = \"$.a\" }]\n",
                "rows or columnar",
            ),
            (
                "url = \"https://example.com/\"",
                "\n[output.table]\ncolumns = [{ name = \"a\", path = \"$.a\" }]\ntext = \"{b}\"\n",
                "does not match any output field",
            ),
            (
                "url = \"https://example.com/{q|settings.nope}\"",
                "",
                "does not match any setting",
            ),
        ];
        for (http, extra, want) in cases {
            let err = parse_manifest(&manifest(http, extra), "x")
                .map(|_| ())
                .unwrap_err();
            assert!(err.contains(want), "{http} {extra}: {err}");
        }
        let cmd_json = "[tool]\nid = \"ext.c\"\nname = \"C\"\ncategory = \"data\"\n[command]\nprogram = \"echo\"\nargs = []\n[output]\njson = { a = \"$.a\" }\n";
        assert!(parse_manifest(cmd_json, "x")
            .map(|_| ())
            .unwrap_err()
            .contains("[http] manifests"));
    }

    #[tokio::test]
    async fn maps_json_into_data_with_encoded_url_and_settings() {
        let rec = RecordedResponse::json(&json!({
            "current": { "temperature_2m": 21.5 },
            "current_units": { "temperature_2m": "°C" }
        }));
        let t = parse_manifest(WEATHER, "w")
            .unwrap()
            .with_setting_values(
                json!({ "lat": "40.0", "lon": "-105.2", "key": "k" })
                    .as_object()
                    .unwrap(),
            )
            .unwrap()
            .with_recorded_response(rec.clone());
        let out = harness(t)
            .run("ext.weather", json!({ "city": "São Paulo & co/x?y" }))
            .await
            .unwrap();
        assert_eq!(out.data, json!({ "temp": 21.5, "unit": "°C" }));
        assert_eq!(
            rec.requests(),
            vec!["https://api.example.com/v1/forecast?lat=40.0&lon=-105.2&q=S%C3%A3o%20Paulo%20%26%20co%2Fx%3Fy"]
        );
    }

    const FORECAST: &str = r#"
[tool]
id = "ext.forecast"
name = "Forecast"
category = "data"

[http]
url = "https://api.example.com/f?lat={lat|settings.lat}&unit={unit|settings.unit}"

[[params]]
name = "lat"
type = "number"

[[params]]
name = "unit"
type = "text"

[[settings]]
name = "lat"
type = "number"

[[settings]]
name = "unit"
type = "text"
default = "c"

[output]
text = "Now {sky}, {temp}{unit_label}"
json = { temp = "$.now.t", unit_label = "$.u", sky = { path = "$.now.code", labels = "sky" } }

[output.table]
columnar = true
columns = [{ name = "day", path = "$.days.d" }, { name = "sky", path = "$.days.code", labels = "sky" }, { name = "high", path = "$.days.hi" }]
text = "{day}: {sky}, {high}{unit_label}"

[output.labels.sky]
"0" = "clear"
"3" = "cloudy"
"#;

    #[tokio::test]
    async fn text_template_labels_and_columnar_table() {
        let rec = RecordedResponse::json(&json!({
            "now": { "t": 21.5, "code": 0 },
            "u": "°C",
            "days": { "d": ["mon", "tue", "wed"], "code": [0, 3, 9], "hi": [22, 18] }
        }));
        let t = parse_manifest(FORECAST, "f")
            .unwrap()
            .with_setting_values(json!({ "lat": 40.0 }).as_object().unwrap())
            .unwrap()
            .with_recorded_response(rec.clone());
        let spec = crate::tool::Tool::spec(&t).clone();
        assert_eq!(spec.output.text_mime.as_deref(), Some("text/plain"));
        let h = harness(t);
        let out = h.run("ext.forecast", json!({})).await.unwrap();
        assert_eq!(
            out.text.as_deref(),
            Some("Now clear, 21.5°C\nmon: clear, 22°C\ntue: cloudy, 18°C\nwed: 9, °C")
        );
        assert_eq!(out.data["sky"], json!("clear"));
        assert_eq!(
            out.data["table"],
            json!({
                "columns": ["day", "sky", "high"],
                "rows": [["mon", "clear", 22], ["tue", "cloudy", 18], ["wed", 9, null]]
            })
        );
        h.run("ext.forecast", json!({ "lat": -33.9, "unit": "f" }))
            .await
            .unwrap();
        assert_eq!(
            rec.requests(),
            vec![
                "https://api.example.com/f?lat=40.0&unit=c",
                "https://api.example.com/f?lat=-33.9&unit=f"
            ]
        );
    }

    #[tokio::test]
    async fn alternatives_fall_back_to_a_setup_hint() {
        let rec = RecordedResponse::json(&json!({}));
        let t = parse_manifest(FORECAST, "f")
            .unwrap()
            .with_setting_values(&Map::new())
            .unwrap()
            .with_recorded_response(rec.clone());
        let err = harness(t).run("ext.forecast", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "not_configured", "{err}");
        assert!(err.to_string().contains("missing: lat"), "{err}");
        assert!(rec.requests().is_empty());
    }

    #[tokio::test]
    async fn missing_json_path_is_a_clear_error() {
        let rec = RecordedResponse::json(&json!({ "current": {} }));
        let t = weather().with_recorded_response(rec);
        let err = harness(t)
            .run("ext.weather", json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("'temp'") || err.contains("'unit'"), "{err}");
        assert!(err.contains("matched nothing"), "{err}");
    }

    #[tokio::test]
    async fn non_2xx_is_external_with_status_and_excerpt() {
        let body = format!("{{\"error\":\"bad key\"}} {}", "x".repeat(500));
        let t = weather().with_recorded_response(RecordedResponse::new(401, body));
        let err = harness(t).run("ext.weather", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "external_failure");
        let ToolError::External {
            exit_code, stderr, ..
        } = &err
        else {
            panic!("{err:?}")
        };
        assert_eq!(*exit_code, Some(401));
        assert!(
            stderr.starts_with("HTTP 401 Unauthorized: {\"error\":\"bad key\"}"),
            "{stderr}"
        );
        assert!(stderr.chars().count() < 260, "{stderr}");
    }

    #[tokio::test]
    async fn body_and_table_outputs() {
        let text = manifest(
            "url = \"https://example.com/feed\"",
            "\n[output]\nbody = \"application/rss+xml\"\n",
        );
        let t = parse_manifest(&text, "x")
            .unwrap()
            .with_recorded_response(RecordedResponse::new(200, "<rss/>"));
        let spec = crate::tool::Tool::spec(&t).clone();
        assert_eq!(spec.output.kind, OutputKind::Text);
        let out = harness(t).run("ext.t", json!({})).await.unwrap();
        assert_eq!(out.text.as_deref(), Some("<rss/>"));
        assert_eq!(out.text_mime.as_deref(), Some("application/rss+xml"));

        let plain = manifest("url = \"https://example.com/\"", "");
        let t = parse_manifest(&plain, "x").unwrap().with_recorded_response(
            RecordedResponse::new(200, "{}").content_type("application/json; charset=utf-8"),
        );
        let out = harness(t).run("ext.t", json!({})).await.unwrap();
        assert_eq!(out.text_mime.as_deref(), Some("application/json"));

        let table = manifest(
            "url = \"https://example.com/\"",
            "\n[output]\njson = { total = \"$.total\" }\n[output.table]\nrows = \"$.items\"\ncolumns = [{ name = \"title\", path = \"$.t\" }, { name = \"n\", path = \"$.n\" }]\n",
        );
        let rec = RecordedResponse::json(
            &json!({ "total": 2, "items": [{ "t": "a", "n": 1 }, { "t": "b" }] }),
        );
        let t = parse_manifest(&table, "x")
            .unwrap()
            .with_recorded_response(rec);
        let out = harness(t).run("ext.t", json!({})).await.unwrap();
        assert_eq!(
            out.data,
            json!({ "total": 2, "table": { "columns": ["title", "n"], "rows": [["a", 1], ["b", null]] } })
        );

        // A wildcard selects rows itself, even when it matches one array row.
        let arrays = manifest(
            "url = \"https://example.com/\"",
            "\n[output.table]\nrows = \"$.items[*]\"\ncolumns = [{ name = \"a\", path = \"$[0]\" }]\n",
        );
        let rec = RecordedResponse::json(&json!({ "items": [["x", 1]] }));
        let t = parse_manifest(&arrays, "x")
            .unwrap()
            .with_recorded_response(rec);
        let out = harness(t).run("ext.t", json!({})).await.unwrap();
        assert_eq!(out.data["table"]["rows"], json!([["x"]]));

        let latin1 = RecordedResponse::new(200, b"caf\xe9".to_vec())
            .content_type("text/plain; charset=ISO-8859-1");
        let t = parse_manifest(&plain, "x")
            .unwrap()
            .with_recorded_response(latin1);
        let out = harness(t).run("ext.t", json!({})).await.unwrap();
        assert_eq!(out.text.as_deref(), Some("café"));
    }

    #[tokio::test]
    async fn refuses_oversized_private_and_denied_requests() {
        let text = manifest("url = \"https://example.com/\"", "");
        let t = parse_manifest(&text, "x")
            .unwrap()
            .with_recorded_response(RecordedResponse::new(200, vec![b'a'; 2048]));
        let mut h = harness(t);
        h.engine.config.limits.max_stdout_bytes = 1024;
        let err = h.run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "limit_exceeded", "{err}");

        let private = manifest("url = \"http://127.0.0.1:9/x\"", "");
        let rec = RecordedResponse::new(200, "never");
        let t = parse_manifest(&private, "x")
            .unwrap()
            .with_recorded_response(rec.clone());
        let mut h = harness(t);
        h.engine.config.net.allow_private = false;
        let err = h.run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "permission_denied", "{err}");
        assert!(rec.requests().is_empty());

        let t = parse_manifest(&text, "x")
            .unwrap()
            .with_recorded_response(RecordedResponse::new(200, "never"));
        let mut h = harness(t);
        h.engine.config.net.deny_hosts = vec!["example.com".into()];
        let err = h.run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "permission_denied", "{err}");
    }

    #[tokio::test]
    async fn missing_setting_asks_for_setup() {
        let text = manifest(
            "url = \"https://example.com/?k={settings.key}\"",
            "\n[[settings]]\nname = \"key\"\ntype = \"text\"\nsecret = true\nrequired = false\n",
        );
        let t = parse_manifest(&text, "x")
            .unwrap()
            .with_recorded_response(RecordedResponse::new(200, "{}"));
        let err = harness(t).run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "not_configured", "{err}");
    }

    /// Serve `reply` (raw HTTP) once on 127.0.0.1; `None` never replies.
    #[test]
    fn multi_select_paths() {
        for p in ["$.items[*]", "$..t", "$.a[?@.x]", "$.a[0:2]", "$.a[0,1]"] {
            assert!(selects_many(p), "{p}");
        }
        for p in ["$.items", "$['data:items']", "$[\"a*b\"]", "$['it\\'s,x']"] {
            assert!(!selects_many(p), "{p}");
        }
    }

    #[tokio::test]
    async fn dot_segments_and_secret_urls_stay_out() {
        let text = manifest("url = \"https://example.com/items/{q}?also={q}\"", "");
        let rec = RecordedResponse::new(200, "{}");
        let t = parse_manifest(&text, "x")
            .unwrap()
            .with_recorded_response(rec.clone());
        let h = harness(t);
        for v in [".", ".."] {
            let err = h.run("ext.t", json!({ "q": v })).await.unwrap_err();
            assert_eq!(err.code(), "invalid_input", "{err}");
        }
        h.run("ext.t", json!({ "q": "a.b" })).await.unwrap();
        assert_eq!(
            rec.requests(),
            vec!["https://example.com/items/a.b?also=a.b"]
        );

        let leak = manifest(
            "url = \"http://127.0.0.1:1/x?key={settings.key}\"",
            "\n[[settings]]\nname = \"key\"\ntype = \"text\"\nsecret = true\n",
        );
        let t = parse_manifest(&leak, "x")
            .unwrap()
            .with_setting_values(json!({ "key": "s3cret" }).as_object().unwrap())
            .unwrap();
        let err = harness(t).run("ext.t", json!({})).await.unwrap_err();
        assert!(!format!("{err} {err:?}").contains("s3cret"), "{err:?}");
    }

    async fn serve_once(reply: Option<String>) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            match reply {
                Some(r) => {
                    let _ = sock.write_all(r.as_bytes()).await;
                    let _ = sock.shutdown().await;
                }
                None => tokio::time::sleep(Duration::from_secs(30)).await,
            }
            req
        });
        (format!("http://{addr}"), task)
    }

    fn live(base: &str, http_extra: &str, extra: &str) -> ExternalTool {
        let http = format!("url = \"{base}/data?q={{q}}\"\nheaders = {{ X-Key = \"{{settings.key}}\" }}\n{http_extra}");
        let settings = "\n[[settings]]\nname = \"key\"\ntype = \"text\"\nsecret = true\n";
        parse_manifest(&manifest(&http, &format!("{settings}{extra}")), "x")
            .unwrap()
            .with_setting_values(json!({ "key": "s3cret" }).as_object().unwrap())
            .unwrap()
    }

    #[tokio::test]
    async fn live_get_sends_secret_header_and_reads_json() {
        let body = r#"{"a":{"b":7}}"#;
        let (base, task) = serve_once(Some(format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )))
        .await;
        let t = live(&base, "", "\n[output]\njson = { b = \"$.a.b\" }\n");
        let out = harness(t)
            .run("ext.t", json!({ "q": "a b" }))
            .await
            .unwrap();
        assert_eq!(out.data, json!({ "b": 7 }));
        let req = task.await.unwrap();
        assert!(req.starts_with("GET /data?q=a%20b HTTP/1.1"), "{req}");
        assert!(req.to_ascii_lowercase().contains("x-key: s3cret"), "{req}");
    }

    #[tokio::test]
    async fn live_timeout_and_oversized_and_redirect() {
        let (base, _task) = serve_once(None).await;
        let t = live(&base, "timeout_secs = 1", "");
        let err = harness(t).run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "external_failure", "{err}");
        assert!(err.to_string().contains("timed out after 1s"), "{err}");

        let (base, _task) = serve_once(Some(
            "HTTP/1.1 200 OK\r\ncontent-length: 5000\r\nconnection: close\r\n\r\n".into(),
        ))
        .await;
        let mut h = harness(live(&base, "", ""));
        h.engine.config.limits.max_stdout_bytes = 1000;
        let err = h.run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "limit_exceeded", "{err}");

        let (base, _task) = serve_once(Some(
            "HTTP/1.1 302 Found\r\nlocation: http://blocked.example/\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into(),
        ))
        .await;
        let mut h = harness(live(&base, "", ""));
        h.engine.config.net.deny_hosts = vec!["blocked.example".into()];
        let err = h.run("ext.t", json!({})).await.unwrap_err();
        assert_eq!(err.code(), "permission_denied", "{err}");

        // Manifest headers never follow a redirect to another origin.
        let (other, other_task) = serve_once(Some(
            "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}".into(),
        ))
        .await;
        let other = other.replace("127.0.0.1", "localhost");
        let (base, _task) = serve_once(Some(format!(
            "HTTP/1.1 302 Found\r\nlocation: {other}/\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
        )))
        .await;
        let err = harness(live(&base, "", ""))
            .run("ext.t", json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "permission_denied", "{err}");
        assert!(err.to_string().contains("cross-origin"), "{err}");
        other_task.abort();
    }
}
