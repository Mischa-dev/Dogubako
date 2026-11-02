//! The natural-language assistant relay: network policy, key handling,
//! error mapping, the server provider and who may use it. A tiny
//! OpenAI-compatible server on loopback stands in for the model.

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use dogubako_core::registry::Registry;
use dogubako_server::config::{AuthMode, ServerConfig};
use dogubako_server::{app, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct Seen {
    last: Arc<Mutex<Option<(String, Value)>>>,
}

/// Start a fake model endpoint; returns its base URL.
async fn fake_model(seen: Seen) -> String {
    async fn chat(
        State(s): State<Seen>,
        h: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let auth = h
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        *s.last.lock().unwrap() = Some((auth.clone(), body.clone()));
        if auth == "Bearer bad" {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": {"message": "Incorrect API key provided"}})),
            );
        }
        (
            StatusCode::OK,
            Json(
                json!({"choices": [{"message": {"role": "assistant", "content": format!("model={}", body["model"].as_str().unwrap_or(""))}}]}),
            ),
        )
    }
    async fn models() -> Json<Value> {
        Json(
            json!({"object": "list", "data": [{"id": "gpt-6-luna", "owned_by": "openai"}, {"id": "local-a"}]}),
        )
    }
    let router = Router::new()
        .route("/v1/chat/completions", post(chat))
        .route("/v1/models", get(models))
        .with_state(seen);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}/v1")
}

struct T {
    app: Router,
    _dir: tempfile::TempDir,
}

fn setup(f: impl FnOnce(&mut ServerConfig)) -> T {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = ServerConfig {
        data_dir: dir.path().to_path_buf(),
        auth: AuthMode::Required,
        setup_token: Some("setup-secret".into()),
        ..Default::default()
    };
    f(&mut cfg);
    let state = AppState::build_with(cfg, Registry::new(), vec![]).unwrap();
    T {
        app: app(state),
        _dir: dir,
    }
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    h: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in h {
        b = b.header(*k, *v);
    }
    let req = match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let raw = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&raw).unwrap_or(Value::Null))
}

async fn owner(t: &T) -> (String, String) {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/setup")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"setup_token": "setup-secret", "username": "owner", "password": "a-strong-password"}).to_string(),
        ))
        .unwrap();
    let resp = t.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let raw = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&raw).unwrap();
    (cookie, v["csrf_token"].as_str().unwrap().to_string())
}

fn chat_body(model: &str) -> Value {
    json!({"model": model, "messages": [{"role": "user", "content": "hi"}], "stream": true})
}

#[tokio::test]
async fn relay_blocks_private_endpoints_by_default() {
    let base = fake_model(Seen::default()).await;
    let t = setup(|_| {});
    let (c, x) = owner(&t).await;
    let h = [("cookie", c.as_str()), ("x-csrf-token", x.as_str())];
    let (s, v) = call(
        &t.app,
        Method::POST,
        "/api/v1/assistant/chat",
        &h,
        Some(json!({"base_url": base, "api_key": "k", "body": chat_body("m")})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    assert_eq!(v["error"]["code"], "assistant_invalid");
    let (_, cfg) = call(&t.app, Method::GET, "/api/v1/assistant/config", &h, None).await;
    assert_eq!(cfg["enabled"], true);
    assert_eq!(cfg["private_network"], false);
}

#[tokio::test]
async fn relay_forwards_chat_and_models() {
    let seen = Seen::default();
    let base = fake_model(seen.clone()).await;
    let t = setup(|c| c.assistant_allow_private = true);
    let (c, x) = owner(&t).await;
    let h = [("cookie", c.as_str()), ("x-csrf-token", x.as_str())];

    let (s, v) = call(&t.app, Method::POST, "/api/v1/assistant/chat", &h, Some(json!({"base_url": format!("{base}/"), "api_key": "user-key", "body": chat_body("gpt-6-luna")}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["choices"][0]["message"]["content"], "model=gpt-6-luna");
    let (auth, body) = seen.last.lock().unwrap().clone().unwrap();
    assert_eq!(auth, "Bearer user-key");
    assert_eq!(body["stream"], false, "the relay never streams");

    let (s, v) = call(
        &t.app,
        Method::POST,
        "/api/v1/assistant/models",
        &h,
        Some(json!({"base_url": base})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["models"][0]["id"], "gpt-6-luna");

    // A rejected key comes back readable, as a gateway error.
    let (s, v) = call(
        &t.app,
        Method::POST,
        "/api/v1/assistant/chat",
        &h,
        Some(json!({"base_url": base, "api_key": "bad", "body": chat_body("m")})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("rejected the API key"),
        "{v}"
    );

    // CSRF is required like any other session POST.
    let (s, _) = call(
        &t.app,
        Method::POST,
        "/api/v1/assistant/chat",
        &[("cookie", c.as_str())],
        Some(json!({"base_url": base, "body": chat_body("m")})),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn server_provider_uses_admin_key_without_revealing_it() {
    let seen = Seen::default();
    let base = fake_model(seen.clone()).await;
    let t = setup(|c| {
        c.assistant_allow_private = true;
        c.assistant_base_url = Some(base.clone());
        c.assistant_api_key = Some("admin-secret-key".into());
        c.assistant_model = Some("house-model".into());
    });
    let (c, x) = owner(&t).await;
    let h = [("cookie", c.as_str()), ("x-csrf-token", x.as_str())];

    let (_, cfg) = call(&t.app, Method::GET, "/api/v1/assistant/config", &h, None).await;
    assert_eq!(cfg["server_provider"]["model"], "house-model");
    assert!(!cfg.to_string().contains("admin-secret-key"));
    let (_, admin) = call(&t.app, Method::GET, "/api/v1/admin/config", &h, None).await;
    assert!(
        !admin.to_string().contains("admin-secret-key"),
        "admin config must not echo the key"
    );

    let (s, v) = call(&t.app, Method::POST, "/api/v1/assistant/chat", &h, Some(json!({"provider": "server", "base_url": "https://evil.example/v1", "body": chat_body("")}))).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["choices"][0]["message"]["content"], "model=house-model");
    let (auth, _) = seen.last.lock().unwrap().clone().unwrap();
    assert_eq!(auth, "Bearer admin-secret-key");
}

#[tokio::test]
async fn agents_and_disabled_servers_are_refused() {
    let t = setup(|c| c.assistant_enabled = false);
    let (c, x) = owner(&t).await;
    let h = [("cookie", c.as_str()), ("x-csrf-token", x.as_str())];
    let (s, v) = call(
        &t.app,
        Method::POST,
        "/api/v1/assistant/chat",
        &h,
        Some(json!({"base_url": "https://api.openai.com/v1", "body": chat_body("m")})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
    let (_, st) = call(&t.app, Method::GET, "/api/v1/status", &[], None).await;
    assert_eq!(st["assistant"], false);

    let t = setup(|_| {});
    let (c, x) = owner(&t).await;
    let h = [("cookie", c.as_str()), ("x-csrf-token", x.as_str())];
    let (s, v) = call(
        &t.app,
        Method::POST,
        "/api/v1/admin/users",
        &h,
        Some(json!({"username": "bot", "role": "agent", "policy": {"allow": ["*"]}})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, tok) = call(
        &t.app,
        Method::POST,
        "/api/v1/admin/tokens",
        &h,
        Some(json!({"user_id": v["id"], "name": "t"})),
    )
    .await;
    let bearer = format!("Bearer {}", tok["token"].as_str().unwrap());
    let (s, _) = call(
        &t.app,
        Method::POST,
        "/api/v1/assistant/chat",
        &[("authorization", bearer.as_str())],
        Some(json!({"base_url": "https://api.openai.com/v1", "body": chat_body("m")})),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
