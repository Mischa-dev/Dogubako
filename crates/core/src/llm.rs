//! A small client for OpenAI-compatible chat endpoints, used by the
//! natural-language assistant.
//!
//! The assistant's tool-calling loop runs in the UI; the hosts (server and
//! desktop) only relay two requests on its behalf:
//!
//! * `POST {base_url}/chat/completions` (non-streaming, with `tools`), and
//! * `GET {base_url}/models` for the model picker.
//!
//! Relaying keeps browsers out of CORS trouble (local runtimes such as Ollama
//! or LM Studio rarely send CORS headers, and the server's CSP only allows
//! same-origin requests) and lets the server apply its network policy, so a
//! user cannot point the relay at the server's private network unless the
//! administrator allows it.
//!
//! Nothing here is stored: the API key travels with each request and is only
//! sent to the endpoint it belongs to.

use crate::net::{check_url, guarded_client};
use crate::policy::NetPolicy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

/// Largest request body we relay (conversation + tool schemas).
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
/// Largest response we accept from the endpoint.
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Upper bound for one completion (reasoning models can be slow).
pub const CHAT_TIMEOUT: Duration = Duration::from_secs(300);

/// Where to send a request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Endpoint {
    /// Base URL including the version path, e.g. `https://api.openai.com/v1`.
    pub base_url: String,
    /// Bearer token; empty or absent for local runtimes that need none.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// A relay failure. `status` is the upstream HTTP status when there was one.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LlmError {
    pub kind: LlmErrorKind,
    pub status: Option<u16>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmErrorKind {
    /// The request (URL, body) is not acceptable.
    Invalid,
    /// The endpoint could not be reached (DNS, connect, TLS, timeout, policy).
    Unreachable,
    /// The endpoint answered with an error status.
    Upstream,
    /// The endpoint answered with something that is not usable JSON.
    BadResponse,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl LlmError {
    fn new(kind: LlmErrorKind, message: impl Into<String>) -> Self {
        LlmError {
            kind,
            status: None,
            message: message.into(),
        }
    }
    fn invalid(m: impl Into<String>) -> Self {
        Self::new(LlmErrorKind::Invalid, m)
    }
}

pub type LlmResult<T> = Result<T, LlmError>;

/// Normalize and validate a base URL: http(s), no credentials, no query,
/// trailing slash removed. Applies the network policy's host checks.
pub fn normalize_base_url(policy: &NetPolicy, raw: &str) -> LlmResult<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(LlmError::invalid("enter the endpoint's base URL"));
    }
    let url = check_url(policy, raw).map_err(|e| LlmError::invalid(e.to_string()))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(LlmError::invalid(
            "put the API key in the key field, not in the URL",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(LlmError::invalid(
            "the base URL must not have a query string or fragment",
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// Keep only what a chat completion needs and force a non-streaming call.
/// Unknown top-level fields are passed through so provider-specific options
/// (e.g. `reasoning_effort`) keep working.
pub fn sanitize_chat_body(body: Value) -> LlmResult<Value> {
    let Value::Object(mut map) = body else {
        return Err(LlmError::invalid("the request body must be a JSON object"));
    };
    match map.get("model") {
        Some(Value::String(m)) if !m.trim().is_empty() => {}
        _ => return Err(LlmError::invalid("choose a model first")),
    }
    match map.get("messages") {
        Some(Value::Array(a)) if !a.is_empty() => {}
        _ => return Err(LlmError::invalid("the conversation is empty")),
    }
    map.insert("stream".into(), Value::Bool(false));
    map.remove("stream_options");
    let out = Value::Object(map);
    let size = serde_json::to_vec(&out).map(|v| v.len()).unwrap_or(0);
    if size > MAX_REQUEST_BYTES {
        return Err(LlmError::invalid(format!(
            "the conversation is too long to send ({} MiB max); start a new chat",
            MAX_REQUEST_BYTES / (1024 * 1024)
        )));
    }
    Ok(out)
}

fn request(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    ep: &Endpoint,
) -> reqwest::RequestBuilder {
    let mut rb = client
        .request(method, url)
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(k) = ep
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        rb = rb.bearer_auth(k);
    }
    rb
}

fn transport_error(e: reqwest::Error, base: &str) -> LlmError {
    let msg = if e.is_timeout() {
        "the model took too long to answer".to_string()
    } else if e.is_connect() || e.is_request() {
        // The underlying cause carries policy denials and DNS errors.
        let mut cause = String::new();
        let mut src: Option<&dyn std::error::Error> = std::error::Error::source(&e);
        while let Some(s) = src {
            cause = s.to_string();
            src = s.source();
        }
        if cause.is_empty() {
            format!("could not reach {base}")
        } else {
            format!("could not reach {base}: {cause}")
        }
    } else {
        format!("request to {base} failed: {e}")
    };
    LlmError::new(LlmErrorKind::Unreachable, msg)
}

async fn read_json(resp: reqwest::Response) -> LlmResult<Value> {
    let status = resp.status();
    let mut resp = resp;
    let mut buf: Vec<u8> = Vec::new();
    loop {
        match resp.chunk().await {
            Ok(Some(c)) => {
                if buf.len() + c.len() > MAX_RESPONSE_BYTES {
                    return Err(LlmError::new(
                        LlmErrorKind::BadResponse,
                        "the endpoint's answer is too large",
                    ));
                }
                buf.extend_from_slice(&c);
            }
            Ok(None) => break,
            Err(e) => {
                return Err(LlmError::new(
                    LlmErrorKind::Unreachable,
                    format!("the connection dropped while reading the answer: {e}"),
                ))
            }
        }
    }
    let parsed: Option<Value> = serde_json::from_slice(&buf).ok();
    if !status.is_success() {
        let detail = parsed
            .as_ref()
            .and_then(upstream_message)
            .unwrap_or_else(|| {
                let t = String::from_utf8_lossy(&buf);
                let t = t.trim();
                if t.is_empty() || t.starts_with('<') {
                    status
                        .canonical_reason()
                        .unwrap_or("request failed")
                        .to_string()
                } else {
                    t.chars().take(300).collect()
                }
            });
        return Err(LlmError {
            kind: LlmErrorKind::Upstream,
            status: Some(status.as_u16()),
            message: friendly_status(status.as_u16(), &detail),
        });
    }
    parsed.ok_or_else(|| {
        LlmError::new(
            LlmErrorKind::BadResponse,
            "the endpoint did not answer with JSON; check the base URL (it usually ends in /v1)",
        )
    })
}

/// `{"error": {"message": …}}` (OpenAI), `{"error": "…"}` (Ollama and others),
/// or `{"message": …}`.
fn upstream_message(v: &Value) -> Option<String> {
    let e = v.get("error").unwrap_or(v);
    e.get("message")
        .and_then(Value::as_str)
        .or_else(|| e.as_str())
        .map(|s| s.chars().take(500).collect())
}

fn friendly_status(status: u16, detail: &str) -> String {
    let lead = match status {
        401 => "The endpoint rejected the API key",
        403 => "The API key may not use this model or endpoint",
        404 => "Not found (check the base URL and model id)",
        429 => "Rate limited or out of credits",
        s if s >= 500 => "The model provider had an error",
        _ => "The endpoint returned an error",
    };
    format!("{lead} (HTTP {status}): {detail}")
}

fn client(policy: NetPolicy) -> LlmResult<reqwest::Client> {
    guarded_client(policy).map_err(|e| LlmError::new(LlmErrorKind::Unreachable, e.to_string()))
}

/// Relay one non-streaming chat completion and return the endpoint's JSON.
pub async fn chat(policy: NetPolicy, ep: &Endpoint, body: Value) -> LlmResult<Value> {
    let base = normalize_base_url(&policy, &ep.base_url)?;
    let body = sanitize_chat_body(body)?;
    let url = format!("{base}/chat/completions");
    let resp = request(&client(policy)?, reqwest::Method::POST, &url, ep)
        .timeout(CHAT_TIMEOUT)
        .json(&body)
        .send()
        .await
        .map_err(|e| transport_error(e, &base))?;
    let v = read_json(resp).await?;
    if v.get("choices").and_then(Value::as_array).is_none() {
        return Err(LlmError::new(
            LlmErrorKind::BadResponse,
            v.get("error")
                .and_then(|_| upstream_message(&v))
                .unwrap_or_else(|| "the endpoint's answer has no choices".into()),
        ));
    }
    Ok(v)
}

/// One entry of `GET /models`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
}

/// List the endpoint's models (sorted by id). Accepts OpenAI's `{data: [...]}`
/// and the bare-array / `{models: [...]}` shapes some runtimes use.
pub async fn list_models(policy: NetPolicy, ep: &Endpoint) -> LlmResult<Vec<ModelEntry>> {
    let base = normalize_base_url(&policy, &ep.base_url)?;
    let url = format!("{base}/models");
    let resp = request(&client(policy)?, reqwest::Method::GET, &url, ep)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| transport_error(e, &base))?;
    let v = read_json(resp).await?;
    Ok(parse_models(&v))
}

pub fn parse_models(v: &Value) -> Vec<ModelEntry> {
    let list = v
        .get("data")
        .or_else(|| v.get("models"))
        .unwrap_or(v)
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out: Vec<ModelEntry> = list
        .iter()
        .filter_map(|m| {
            let id = m
                .get("id")
                .or_else(|| m.get("name"))
                .or_else(|| m.get("model"))
                .and_then(Value::as_str)
                .or_else(|| m.as_str())?
                .trim()
                .to_string();
            (!id.is_empty()).then(|| ModelEntry {
                id,
                owned_by: m.get("owned_by").and_then(Value::as_str).map(String::from),
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    out
}

/// JSON shape both hosts return for a relay failure.
pub fn error_json(e: &LlmError) -> Value {
    json!({"kind": e.kind, "status": e.status, "message": e.message})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_is_normalized_and_checked() {
        let open = NetPolicy::default();
        assert_eq!(
            normalize_base_url(&open, " https://api.openai.com/v1/ ").unwrap(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            normalize_base_url(&open, "http://localhost:11434/v1").unwrap(),
            "http://localhost:11434/v1"
        );
        assert!(normalize_base_url(&open, "ftp://x/v1").is_err());
        assert!(normalize_base_url(&open, "").is_err());
        assert!(normalize_base_url(&open, "https://u:p@api.example.com/v1").is_err());
        assert!(normalize_base_url(&open, "https://api.example.com/v1?key=1").is_err());
        let public = NetPolicy::public_only();
        assert!(normalize_base_url(&public, "http://127.0.0.1:11434/v1").is_err());
    }

    #[test]
    fn chat_body_is_sanitized() {
        let b = sanitize_chat_body(json!({
            "model": "gpt-6-luna",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
            "stream_options": {"include_usage": true},
            "reasoning_effort": "low"
        }))
        .unwrap();
        assert_eq!(b["stream"], false);
        assert!(b.get("stream_options").is_none());
        assert_eq!(b["reasoning_effort"], "low");
        assert!(sanitize_chat_body(json!({"messages": [{"role": "user"}]})).is_err());
        assert!(sanitize_chat_body(json!({"model": "m", "messages": []})).is_err());
        assert!(sanitize_chat_body(json!([1])).is_err());
    }

    #[test]
    fn models_parse_from_common_shapes() {
        let openai = json!({"object": "list", "data": [
            {"id": "gpt-6-luna", "owned_by": "openai"}, {"id": "a-model"}, {"id": "a-model"}
        ]});
        let ids: Vec<String> = parse_models(&openai).into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["a-model", "gpt-6-luna"]);
        let ollama = json!({"models": [{"name": "llama3.3:latest"}]});
        assert_eq!(parse_models(&ollama)[0].id, "llama3.3:latest");
        assert_eq!(parse_models(&json!(["x"]))[0].id, "x");
        assert!(parse_models(&json!({"nope": 1})).is_empty());
    }

    #[test]
    fn upstream_errors_are_readable() {
        assert_eq!(
            upstream_message(&json!({"error": {"message": "bad key"}})).as_deref(),
            Some("bad key")
        );
        assert_eq!(
            upstream_message(&json!({"error": "model not found"})).as_deref(),
            Some("model not found")
        );
        assert!(friendly_status(401, "x").starts_with("The endpoint rejected the API key"));
    }
}
