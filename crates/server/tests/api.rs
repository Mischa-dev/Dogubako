//! End-to-end tests of the HTTP API, authentication, per-agent authorization,
//! file isolation and the MCP endpoint, using small test tools.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use dogubako_core::prelude::*;
use dogubako_core::registry::Registry;
use dogubako_server::config::{AuthMode, ServerConfig};
use dogubako_server::{app, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

fn test_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        sync_tool(
            ToolSpec::new("text.upper", "Upper", Category::Text)
                .describe("Uppercase text")
                .param(ParamSpec::textarea("text").required())
                .text_output("text/plain"),
            |_c, p| Ok(ToolOutput::text(p.text("text").to_uppercase())),
        ),
        sync_tool(
            ToolSpec::new("image.copy", "Copy file", Category::Image)
                .describe("Copies the input file to an output artifact")
                .param(ParamSpec::file("input").required())
                .output(OutputKind::Files),
            |c, p| {
                let f = p.file("input")?;
                let out = c.output_path(&suffixed(&f.name, "copy", None));
                std::fs::copy(&f.path, &out)?;
                Ok(ToolOutput::files(vec![c.artifact(&out)?]))
            },
        ),
        sync_tool(
            ToolSpec::new("data.fill", "Fill", Category::Data)
                .describe("Writes an output file of the requested size")
                .param(ParamSpec::integer("kib").default_int(1))
                .output(OutputKind::Files),
            |c, p| {
                let out = c.output_path("fill.bin");
                std::fs::write(&out, vec![0u8; p.int_or("kib", 1) as usize * 1024])?;
                Ok(ToolOutput::files(vec![c.artifact(&out)?]))
            },
        ),
        sync_tool(
            ToolSpec::new("system.secret_info", "Secret info", Category::System)
                .describe("Needs system_info")
                .permission(Permission::SystemInfo),
            |_c, _p| Ok(ToolOutput::data(json!({"host": "x"}))),
        ),
        async_tool(
            ToolSpec::new("dev.sleep", "Sleep", Category::Dev)
                .describe("Sleeps")
                .param(ParamSpec::number("seconds").default_num(5.0))
                .progress(),
            |c, p| async move {
                let secs = p.num_or("seconds", 5.0);
                let steps = (secs * 10.0) as u32;
                for i in 0..steps {
                    c.check_cancelled()?;
                    c.progress(i as f32 / steps as f32, "sleeping");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Ok(ToolOutput::text("awake"))
            },
        ),
    ]
}

#[allow(dead_code)]
struct T {
    app: axum::Router,
    state: AppState,
    _dir: tempfile::TempDir,
}

fn setup(auth: AuthMode) -> T {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        data_dir: dir.path().to_path_buf(),
        auth,
        setup_token: Some("setup-secret".into()),
        ..Default::default()
    };
    let mut reg = Registry::new();
    reg.register_all(test_tools());
    let state = AppState::build_with(cfg, reg, vec![]).unwrap();
    T {
        app: app(state.clone()),
        state,
        _dir: dir,
    }
}

struct Resp {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
    raw: Vec<u8>,
}

async fn call(
    app: &axum::Router,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> Resp {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
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
    let headers = resp.headers().clone();
    let raw = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    let body = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    Resp {
        status,
        headers,
        body,
        raw,
    }
}

/// Set up the owner and return (cookie header, csrf).
async fn owner_session(t: &T) -> (String, String) {
    let r = call(&t.app, Method::POST, "/api/v1/setup", &[], Some(json!({"setup_token": "setup-secret", "username": "owner", "password": "a-strong-password"}))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let cookie = r
        .headers
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    (cookie, r.body["csrf_token"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn setup_login_and_csrf() {
    let t = setup(AuthMode::Required);
    let s = call(&t.app, Method::GET, "/api/v1/status", &[], None).await;
    assert_eq!(s.body["setup_required"], true);
    // Wrong setup token
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/setup",
        &[],
        Some(json!({"setup_token": "nope", "username": "x", "password": "a-strong-password"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let (cookie, csrf) = owner_session(&t).await;
    // Setup can't be repeated
    let r = call(&t.app, Method::POST, "/api/v1/setup", &[], Some(json!({"setup_token": "setup-secret", "username": "y", "password": "a-strong-password"}))).await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    // Unauthenticated access rejected
    let r = call(&t.app, Method::GET, "/api/v1/tools", &[], None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    // Session works for GET
    let r = call(
        &t.app,
        Method::GET,
        "/api/v1/tools",
        &[("cookie", &cookie)],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["count"], 5);
    // POST without CSRF header is rejected
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/text.upper/run",
        &[("cookie", &cookie)],
        Some(json!({"params": {"text": "hi"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/text.upper/run",
        &[("cookie", &cookie), ("x-csrf-token", &csrf)],
        Some(json!({"params": {"text": "hi"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_eq!(r.body["result"]["text"], "HI");
    // Bad login
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/auth/login",
        &[],
        Some(json!({"username": "owner", "password": "wrong-password"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/auth/login",
        &[],
        Some(json!({"username": "owner", "password": "a-strong-password"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    // Audit log has entries
    let r = call(
        &t.app,
        Method::GET,
        "/api/v1/admin/audit",
        &[("cookie", &cookie)],
        None,
    )
    .await;
    assert!(r.body["entries"].as_array().unwrap().len() >= 3);
}

#[tokio::test]
async fn login_rate_limited() {
    let t = setup(AuthMode::Required);
    owner_session(&t).await;
    let mut last = StatusCode::OK;
    for _ in 0..15 {
        last = call(
            &t.app,
            Method::POST,
            "/api/v1/auth/login",
            &[],
            Some(json!({"username": "owner", "password": "bad-password-x"})),
        )
        .await
        .status;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
}

async fn create_agent_token(t: &T, cookie: &str, csrf: &str, name: &str, policy: Value) -> String {
    let h = [("cookie", cookie), ("x-csrf-token", csrf)];
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/admin/users",
        &h,
        Some(json!({"username": name, "role": "agent", "policy": policy})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let uid = r.body["id"].as_str().unwrap().to_string();
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/admin/tokens",
        &h,
        Some(json!({"user_id": uid, "name": "test"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    r.body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn agent_permissions_enforced() {
    let t = setup(AuthMode::Required);
    let (cookie, csrf) = owner_session(&t).await;
    let img_token = create_agent_token(
        &t,
        &cookie,
        &csrf,
        "imgbot",
        json!({"allow": ["image.*"], "permissions": []}),
    )
    .await;
    let bearer = format!("Bearer {img_token}");
    let auth = [("authorization", bearer.as_str())];

    // Discovery only shows permitted tools
    let r = call(&t.app, Method::GET, "/api/v1/tools", &auth, None).await;
    let ids: Vec<&str> = r.body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["image.copy"]);
    // The scheme name is case-insensitive.
    let upper = format!("BEARER {img_token}");
    let r = call(
        &t.app,
        Method::GET,
        "/api/v1/tools",
        &[("authorization", upper.as_str())],
        None,
    )
    .await;
    assert_eq!(r.body["tools"].as_array().unwrap().len(), 1, "{}", r.body);
    // Invocation of a non-permitted tool is refused even though the agent knows its id
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/text.upper/run",
        &auth,
        Some(json!({"params": {"text": "x"}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Agent can't access admin endpoints
    let r = call(&t.app, Method::GET, "/api/v1/admin/users", &auth, None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Agent can't mint tokens with its token
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tokens",
        &auth,
        Some(json!({"name": "x"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Agent allowed the system tool by pattern but without the system_info capability: denied by the engine
    let sys_token = create_agent_token(
        &t,
        &cookie,
        &csrf,
        "sysbot",
        json!({"allow": ["system.*"], "permissions": []}),
    )
    .await;
    let b2 = format!("Bearer {sys_token}");
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/system.secret_info/run",
        &[("authorization", &b2)],
        Some(json!({"params": {}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["status"], "failed");
    assert_eq!(r.body["error"]["code"], "permission_denied");

    // Revoked token stops working
    let r = call(
        &t.app,
        Method::GET,
        "/api/v1/admin/tokens",
        &[("cookie", &cookie)],
        None,
    )
    .await;
    let tid = r.body["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["username"] == "imgbot")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/tokens/{tid}"),
        &[("cookie", &cookie), ("x-csrf-token", &csrf)],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let r = call(&t.app, Method::GET, "/api/v1/tools", &auth, None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    // Invalid token
    let r = call(
        &t.app,
        Method::GET,
        "/api/v1/tools",
        &[("authorization", "Bearer dgb_0000")],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

async fn upload(t: &T, auth: &[(&str, &str)], name: &str, data: &[u8]) -> Value {
    let mut b = Request::builder()
        .method(Method::PUT)
        .uri(format!("/api/v1/files/raw?name={name}"));
    for (k, v) in auth {
        b = b.header(*k, *v);
    }
    let resp = t
        .app
        .clone()
        .oneshot(b.body(Body::from(data.to_vec())).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn files_isolated_between_principals() {
    let t = setup(AuthMode::Required);
    let (cookie, csrf) = owner_session(&t).await;
    let a = create_agent_token(&t, &cookie, &csrf, "agent-a", json!({"allow": ["image.*"]})).await;
    let b = create_agent_token(&t, &cookie, &csrf, "agent-b", json!({"allow": ["image.*"]})).await;
    let ba = format!("Bearer {a}");
    let bb = format!("Bearer {b}");
    let f = upload(&t, &[("authorization", &ba)], "secret.txt", b"top secret").await;
    let fid = f["id"].as_str().unwrap();
    assert!(fid.starts_with("f_"));

    // A can run a tool on its file and download the artifact
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/image.copy/run",
        &[("authorization", &ba)],
        Some(json!({"params": {"input": fid}})),
    )
    .await;
    assert_eq!(r.body["status"], "succeeded", "{:?}", r.body);
    let art = r.body["result"]["artifacts"][0].clone();
    assert_eq!(art["name"], "secret-copy.txt");
    let url = art["url"].as_str().unwrap();
    let d = call(&t.app, Method::GET, url, &[("authorization", &ba)], None).await;
    assert_eq!(d.raw, b"top secret");
    assert!(d.headers[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment"));
    let d = call(
        &t.app,
        Method::GET,
        &format!("{url}?inline=1"),
        &[("authorization", &ba)],
        None,
    )
    .await;
    assert!(d.headers[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("inline"));
    assert!(d.headers["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("sandbox"));

    // B cannot read A's file, its artifacts, or use it as tool input
    assert_eq!(
        call(
            &t.app,
            Method::GET,
            &format!("/api/v1/files/{fid}/content"),
            &[("authorization", &bb)],
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(&t.app, Method::GET, url, &[("authorization", &bb)], None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/image.copy/run",
        &[("authorization", &bb)],
        Some(json!({"params": {"input": fid}})),
    )
    .await;
    assert_eq!(r.body["status"], "failed");
    assert_eq!(r.body["error"]["code"], "invalid_input");
    // Host paths are never accepted as file inputs on the server
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/image.copy/run",
        &[("authorization", &bb)],
        Some(json!({"params": {"input": "/etc/passwd"}})),
    )
    .await;
    assert_eq!(r.body["error"]["code"], "invalid_input");
    // B can't see A's job
    let jid = call(
        &t.app,
        Method::GET,
        "/api/v1/jobs",
        &[("authorization", &ba)],
        None,
    )
    .await
    .body["jobs"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        call(
            &t.app,
            Method::GET,
            &format!("/api/v1/jobs/{jid}"),
            &[("authorization", &bb)],
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // Path traversal in upload names is neutralized
    let f2 = upload(&t, &[("authorization", &ba)], "..%2F..%2Fevil.sh", b"x").await;
    assert_eq!(f2["name"], "evil.sh");
}

#[tokio::test]
async fn validation_errors_and_cancellation() {
    let t = setup(AuthMode::None);
    // Unknown parameter
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/text.upper/run",
        &[],
        Some(json!({"params": {"txt": "x"}})),
    )
    .await;
    assert_eq!(r.body["error"]["code"], "invalid_input");
    // Missing required
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/text.upper/run",
        &[],
        Some(json!({"params": {}})),
    )
    .await;
    assert_eq!(r.body["error"]["code"], "invalid_input");
    // Ephemeral (live) runs execute but stay out of job history
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/text.upper/run",
        &[],
        Some(json!({"params": {"text": "live"}, "ephemeral": true})),
    )
    .await;
    assert_eq!(r.body["result"]["text"], "LIVE");
    let jobs = call(&t.app, Method::GET, "/api/v1/jobs", &[], None).await;
    assert!(jobs.body["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|j| j["params"]["text"] != "live"));
    // Unknown tool
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/nope.nope/run",
        &[],
        Some(json!({"params": {}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Long job: returns 202, then cancel
    let r = call(
        &t.app,
        Method::POST,
        "/api/v1/tools/dev.sleep/run",
        &[],
        Some(json!({"params": {"seconds": 30}, "wait_seconds": 0})),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    let id = r.body["id"].as_str().unwrap().to_string();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let r = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/jobs/{id}"),
        &[],
        None,
    )
    .await;
    assert_eq!(r.body["status"], "running");
    let r = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/jobs/{id}/cancel"),
        &[],
        None,
    )
    .await;
    assert_eq!(r.body["status"], "cancelled", "{:?}", r.body);
}

#[tokio::test]
async fn no_auth_mode_refuses_public_bind() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ServerConfig {
        data_dir: dir.path().to_path_buf(),
        auth: AuthMode::None,
        bind: "0.0.0.0:8420".parse().unwrap(),
        ..Default::default()
    };
    assert!(AppState::build_with(cfg, Registry::new(), vec![]).is_err());
}

#[tokio::test]
async fn mcp_endpoint() {
    let t = setup(AuthMode::Required);
    let (cookie, csrf) = owner_session(&t).await;
    let token = create_agent_token(
        &t,
        &cookie,
        &csrf,
        "mcpbot",
        json!({"allow": ["text.*", "image.*"]}),
    )
    .await;
    let bearer = format!("Bearer {token}");
    // No auth
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &[],
        Some(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    let h = [
        ("authorization", bearer.as_str()),
        ("accept", "application/json, text/event-stream"),
    ];
    let r = call(&t.app, Method::POST, "/mcp", &h, Some(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}))).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["result"]["protocolVersion"], "2025-06-18");
    let sid = r
        .headers
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let hs = [
        ("authorization", bearer.as_str()),
        ("mcp-session-id", sid.as_str()),
    ];
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &hs,
        Some(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::ACCEPTED);
    // A 3 MiB upload (4 MiB as base64) exceeds axum's default 2 MiB body
    // limit; /mcp is sized for the advertised upload allowance.
    let b64 = "AAAA".repeat(1024 * 1024);
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &hs,
        Some(json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "dogubako_upload_file", "arguments": {"name": "zeros.bin", "content_base64": b64}}})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    assert_ne!(r.body["result"]["isError"], true, "{:?}", r.body);
    // tools/list shows only permitted tools (plus meta tools)
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &hs,
        Some(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})),
    )
    .await;
    let names: Vec<String> = r.body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"text_upper".to_string()));
    assert!(names.contains(&"dogubako_search_tools".to_string()));
    assert!(!names.contains(&"system_secret_info".to_string()));
    assert!(!names.contains(&"dev_sleep".to_string()));
    // Calling a permitted tool
    let r = call(&t.app, Method::POST, "/mcp", &hs, Some(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "text_upper", "arguments": {"text": "mcp"}}}))).await;
    assert_eq!(r.body["result"]["isError"], false, "{:?}", r.body);
    assert_eq!(r.body["result"]["structuredContent"]["text"], "MCP");
    // Calling a hidden tool directly is refused
    let r = call(&t.app, Method::POST, "/mcp", &hs, Some(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "system_secret_info", "arguments": {}}}))).await;
    assert_eq!(r.body["result"]["isError"], true);
    // ... also through the generic runner
    let r = call(&t.app, Method::POST, "/mcp", &hs, Some(json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "dogubako_run_tool", "arguments": {"tool": "dev.sleep", "arguments": {}}}}))).await;
    assert_eq!(r.body["result"]["isError"], true);
    // Upload via MCP + run + resource read
    let r = call(&t.app, Method::POST, "/mcp", &hs, Some(json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "dogubako_upload_file", "arguments": {"name": "a.txt", "content_base64": "aGVsbG8="}}}))).await;
    let fid = r.body["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(&t.app, Method::POST, "/mcp", &hs, Some(json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "image_copy", "arguments": {"input": fid}}}))).await;
    let link = r.body["result"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "resource_link")
        .unwrap()["uri"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &hs,
        Some(
            json!({"jsonrpc": "2.0", "id": 8, "method": "resources/read", "params": {"uri": link}}),
        ),
    )
    .await;
    assert_eq!(r.body["result"]["contents"][0]["text"], "hello");
    // Files over the inline limit are refused without being read into memory.
    let big = upload(
        &t,
        &[("authorization", &bearer)],
        "big.txt",
        &vec![b'x'; (16 << 20) + 1],
    )
    .await;
    let big_id = big["id"].as_str().unwrap();
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &hs,
        Some(json!({"jsonrpc": "2.0", "id": 81, "method": "resources/read", "params": {"uri": format!("dogubako://files/{big_id}")}})),
    )
    .await;
    assert_eq!(r.body["error"]["code"], -32603, "{:?}", r.body);
    assert!(r.body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("too large"));
    // Search meta-tool respects permissions
    let r = call(&t.app, Method::POST, "/mcp", &hs, Some(json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "dogubako_search_tools", "arguments": {"query": "secret"}}}))).await;
    assert_eq!(r.body["result"]["structuredContent"]["total"], 0);
    // Compact discovery
    let r = call(
        &t.app,
        Method::POST,
        "/mcp?discovery=compact",
        &hs,
        Some(json!({"jsonrpc": "2.0", "id": 10, "method": "tools/list"})),
    )
    .await;
    assert!(r.body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["name"].as_str().unwrap().starts_with("dogubako_")));
    // Session of another principal is rejected
    let other = create_agent_token(&t, &cookie, &csrf, "other", json!({"allow": ["*"]})).await;
    let bo = format!("Bearer {other}");
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &[("authorization", &bo), ("mcp-session-id", &sid)],
        Some(json!({"jsonrpc": "2.0", "id": 11, "method": "tools/list"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    // Foreign Origin rejected (DNS rebinding protection)
    let r = call(
        &t.app,
        Method::POST,
        "/mcp",
        &[
            ("authorization", &bearer),
            ("origin", "http://evil.example"),
            ("host", "localhost:8420"),
        ],
        Some(json!({"jsonrpc": "2.0", "id": 12, "method": "ping"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn job_outputs_count_against_quota() {
    let t = setup(AuthMode::Required);
    let (cookie, csrf) = owner_session(&t).await;
    let token = create_agent_token(
        &t,
        &cookie,
        &csrf,
        "fillbot",
        json!({"allow": ["data.*"], "permissions": [], "quota_mb": 1}),
    )
    .await;
    let bearer = format!("Bearer {token}");
    let auth = [("authorization", bearer.as_str())];
    let run = |kib: i64| {
        call(
            &t.app,
            Method::POST,
            "/api/v1/tools/data.fill/run",
            &auth,
            Some(json!({"params": {"kib": kib}, "wait_seconds": 10})),
        )
    };
    // Fits: stored.
    let r = run(600).await;
    assert_eq!(r.body["status"], "succeeded", "{:?}", r.body);
    // Would exceed the 1 MiB quota (600 + 600 KiB): refused, nothing stored.
    let r = run(600).await;
    assert_eq!(r.body["status"], "failed", "{:?}", r.body);
    assert!(
        r.body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("quota"),
        "{:?}",
        r.body
    );
    let r = call(&t.app, Method::GET, "/api/v1/files", &auth, None).await;
    assert_eq!(r.body["files"].as_array().unwrap().len(), 1, "{:?}", r.body);
    assert_eq!(r.body["used"], 600 * 1024);
    // Uploads go through the same quota check: 600 KiB more does not fit.
    let resp = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/files/raw?name=big.bin")
                .header("authorization", bearer.as_str())
                .body(Body::from(vec![0u8; 600 * 1024]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let r = call(&t.app, Method::GET, "/api/v1/files", &auth, None).await;
    assert_eq!(r.body["used"], 600 * 1024);
}

#[tokio::test]
async fn scoped_tokens_never_carry_admin_rights() {
    let t = setup(AuthMode::Required);
    let (cookie, csrf) = owner_session(&t).await;
    let h = [("cookie", cookie.as_str()), ("x-csrf-token", csrf.as_str())];
    let mint = |scope: Option<Value>| {
        let mut body = json!({"name": "t"});
        if let Some(s) = scope {
            body["scope"] = s;
        }
        call(&t.app, Method::POST, "/api/v1/tokens", &h, Some(body))
    };
    // Even a wildcard scope with a deny list is a restriction, not admin.
    let r = mint(Some(json!({"allow": ["*"], "deny": ["system.*"]}))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
    let scoped = format!("Bearer {}", r.body["token"].as_str().unwrap());
    let a = [("authorization", scoped.as_str())];
    for uri in ["/api/v1/admin/users", "/api/v1/admin/tokens"] {
        let r = call(&t.app, Method::GET, uri, &a, None).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{uri}");
    }
    // An unscoped owner token keeps the owner's admin rights.
    let r = mint(None).await;
    let full = format!("Bearer {}", r.body["token"].as_str().unwrap());
    let r = call(
        &t.app,
        Method::GET,
        "/api/v1/admin/users",
        &[("authorization", full.as_str())],
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn enabling_auth_after_no_auth_offers_setup() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = |auth| ServerConfig {
        data_dir: dir.path().to_path_buf(),
        auth,
        setup_token: Some("setup-secret".into()),
        ..Default::default()
    };
    // First run without authentication creates the implicit `local` owner.
    let s = AppState::build_with(cfg(AuthMode::None), Registry::new(), vec![]).unwrap();
    assert!(!s.setup_required());
    drop(s);
    // Restarted with authentication on the same data dir: setup is offered.
    let s = AppState::build_with(cfg(AuthMode::Required), Registry::new(), vec![]).unwrap();
    assert!(s.setup_required());
    let a = app(s);
    let r = call(
        &a,
        Method::POST,
        "/api/v1/setup",
        &[],
        Some(json!({"setup_token": "setup-secret", "username": "owner", "password": "a-strong-password"})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.body);
}
