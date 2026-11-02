//! Natural-language assistant relay (`/api/v1/assistant/*`).
//!
//! The assistant's tool-calling loop runs in the browser and calls tools
//! through the normal `/tools/{id}/run` API, so every tool call is authorized
//! exactly like a manual run. These routes only relay the model requests to
//! an OpenAI-compatible endpoint, because the browser cannot reach it itself
//! (CSP `connect-src 'self'`, missing CORS headers on local runtimes).
//!
//! * Human users only: agents bring their own model and use MCP.
//! * The endpoint is checked against the network policy (public addresses
//!   only, unless `allow_private_network` or `assistant_allow_private`).
//! * The user's API key is forwarded with the request and never stored.
//! * An administrator may configure a server endpoint and key
//!   (`assistant_base_url`, `assistant_api_key`); users select it as the
//!   "This server" provider and never see the key.

use crate::authz::{Principal, Role};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use dogubako_core::llm::{self, Endpoint, LlmError, LlmErrorKind};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/assistant/config", get(config))
        .route("/assistant/models", post(models))
        .route(
            "/assistant/chat",
            post(chat).layer(DefaultBodyLimit::max(llm::MAX_REQUEST_BYTES + 64 * 1024)),
        )
}

/// Which endpoint a request targets.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Target {
    /// `"server"` uses the administrator's endpoint and key.
    provider: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct ChatReq {
    #[serde(flatten)]
    target: Target,
    body: Value,
}

fn check_access(s: &AppState, p: &Principal) -> ApiResult<()> {
    if !s.cfg.assistant_enabled {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "assistant_disabled",
            "the assistant is turned off on this server",
        ));
    }
    if p.role == Role::Agent {
        return Err(ApiError::forbidden(
            "agents connect their own model through MCP; the assistant relay is for people",
        ));
    }
    if !s.assistant_limiter.check(&p.user_id) {
        return Err(ApiError::too_many(
            "too many assistant requests; wait a few minutes",
        ));
    }
    Ok(())
}

fn endpoint(s: &AppState, t: Target) -> ApiResult<Endpoint> {
    if t.provider.as_deref() == Some("server") {
        return s
            .cfg
            .assistant_server_endpoint()
            .ok_or_else(|| ApiError::bad_request("this server has no model endpoint configured"));
    }
    Ok(Endpoint {
        base_url: t.base_url.unwrap_or_default(),
        api_key: t.api_key,
    })
}

fn relay_error(e: LlmError) -> ApiError {
    let (status, code) = match e.kind {
        LlmErrorKind::Invalid => (StatusCode::BAD_REQUEST, "assistant_invalid"),
        LlmErrorKind::Unreachable => (StatusCode::BAD_GATEWAY, "assistant_unreachable"),
        LlmErrorKind::Upstream => (StatusCode::BAD_GATEWAY, "assistant_upstream"),
        LlmErrorKind::BadResponse => (StatusCode::BAD_GATEWAY, "assistant_bad_response"),
    };
    let mut err = ApiError::new(status, code, e.message.clone());
    err.extra = Some(Box::new(llm::error_json(&e)));
    err
}

/// What the UI needs to show the "This server" provider.
async fn config(State(s): State<AppState>, p: Principal) -> Json<Value> {
    let server = s.cfg.assistant_server_endpoint().map(|_| {
        json!({
            "model": s.cfg.assistant_model,
        })
    });
    Json(json!({
        "enabled": s.cfg.assistant_enabled && p.role != Role::Agent,
        "server_provider": server,
        "private_network": s.cfg.allow_private_network || s.cfg.assistant_allow_private,
    }))
}

async fn models(
    State(s): State<AppState>,
    p: Principal,
    Json(t): Json<Target>,
) -> ApiResult<Json<Value>> {
    check_access(&s, &p)?;
    let ep = endpoint(&s, t)?;
    let list = llm::list_models(s.cfg.assistant_net_policy(), &ep)
        .await
        .map_err(relay_error)?;
    Ok(Json(json!({ "models": list })))
}

async fn chat(
    State(s): State<AppState>,
    p: Principal,
    Json(r): Json<ChatReq>,
) -> ApiResult<Json<Value>> {
    check_access(&s, &p)?;
    let server = r.target.provider.as_deref() == Some("server");
    let ep = endpoint(&s, r.target)?;
    let mut body = r.body;
    // The server provider's default model applies when the user picked none.
    if server
        && body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .is_empty()
    {
        if let (Some(m), Some(obj)) = (s.cfg.assistant_model.clone(), body.as_object_mut()) {
            obj.insert("model".into(), Value::String(m));
        }
    }
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(120)
        .collect::<String>();
    // Only the host goes to the audit log, never the path or key.
    let host: String = ep
        .base_url
        .trim()
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    let res = llm::chat(s.cfg.assistant_net_policy(), &ep, body).await;
    let outcome = match &res {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("{:?}", e.kind).to_lowercase(),
    };
    s.db.audit(
        &p.username,
        "assistant.chat",
        &model,
        &outcome,
        "",
        &if server {
            "server provider".to_string()
        } else {
            host
        },
    );
    res.map(Json).map_err(relay_error)
}
