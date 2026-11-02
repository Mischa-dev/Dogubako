//! Versioned HTTP API (`/api/v1`).

use crate::auth::{self, Admin, ClientIp, LOCAL_USER_ID};
use crate::authz::{Policy, Principal, Role};
use crate::config::AuthMode;
use crate::db::{now, UserRow};
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, DownloadState};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use dogubako_core::engine::ToolInfo;
use dogubako_core::spec::{Category, Permission};
use dogubako_mcp::{JobView, ResultView};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub fn router(state: AppState) -> Router<AppState> {
    let upload_limit = (state.cfg.max_upload_mb as usize).saturating_mul(1024 * 1024).saturating_add(1024 * 1024);
    Router::new()
        .route("/status", get(status))
        .route("/setup", post(setup))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/password", post(change_password))
        .route("/tools", get(list_tools))
        .route("/tools/{id}", get(get_tool))
        .route("/tools/{id}/run", post(run_tool))
        .route("/categories", get(categories))
        .route("/jobs", get(list_jobs))
        .route("/jobs/{id}", get(get_job).delete(delete_job))
        .route("/jobs/{id}/events", get(job_events))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/jobs/{id}/artifacts.zip", get(job_zip))
        .route("/files", get(list_files).post(upload_multipart).layer(DefaultBodyLimit::max(upload_limit)))
        .route("/files/raw", put(upload_raw).layer(DefaultBodyLimit::max(upload_limit)))
        .route("/files/{id}", get(get_file).delete(delete_file))
        .route("/files/{id}/content", get(file_content))
        .route("/tokens", get(my_tokens).post(create_my_token))
        .route("/tokens/{id}", delete(revoke_token))
        .route("/deps", get(deps))
        .route("/deps/refresh", post(deps_refresh))
        .route("/models", get(models))
        .route("/models/{id}/download", post(model_download))
        .route("/models/{id}", delete(model_delete))
        .route("/extensions", get(extensions))
        .route("/admin/users", get(admin_users).post(admin_create_user))
        .route("/admin/users/{id}", axum::routing::patch(admin_update_user).delete(admin_delete_user))
        .route("/admin/users/{id}/password", post(admin_set_password))
        .route("/admin/tokens", get(admin_tokens).post(admin_create_token))
        .route("/admin/audit", get(admin_audit))
        .route("/admin/config", get(admin_config))
        .route("/openapi.json", get(openapi))
}

// ---------------------------------------------------------------- status & auth

async fn status(State(s): State<AppState>) -> Json<Value> {
    Json(json!({
        "name": "dogubako",
        "instance_name": s.cfg.instance_name,
        "version": env!("CARGO_PKG_VERSION"),
        "auth": if s.cfg.auth == AuthMode::None { "none" } else { "required" },
        "setup_required": s.setup_required(),
        "mcp_enabled": s.cfg.mcp_enabled,
        "tools": s.engine.registry.len(),
        "mode": "server",
        "max_upload_mb": s.cfg.max_upload_mb,
    }))
}

#[derive(Deserialize)]
struct SetupReq {
    setup_token: String,
    username: String,
    password: String,
}

async fn setup(State(s): State<AppState>, ClientIp(ip): ClientIp, Json(r): Json<SetupReq>) -> ApiResult<Response> {
    if !s.login_limiter.check(&format!("setup:{ip}")) {
        return Err(ApiError::too_many("too many attempts; try again later"));
    }
    let expected = s.setup_token.lock().unwrap().clone().ok_or_else(|| ApiError::conflict("setup already completed"))?;
    if !auth::constant_time_eq(r.setup_token.trim(), &expected) {
        s.db.audit(&r.username, "setup", "", "denied", &ip, "bad setup token");
        return Err(ApiError::forbidden("invalid setup token (see the server log)"));
    }
    auth::validate_username(&r.username)?;
    auth::validate_password(&r.password)?;
    let hash = auth::hash_password(&r.password).map_err(ApiError::internal)?;
    let id = format!("u_{}", auth::random_token(8));
    s.db.create_user(&id, &r.username, Some(&hash), Role::Owner, &Policy::for_role(Role::Owner), "Instance owner")?;
    *s.setup_token.lock().unwrap() = None;
    s.db.audit(&r.username, "setup", &id, "ok", &ip, "owner account created");
    start_session(&s, &id, &ip, "")
}

fn start_session(s: &AppState, user_id: &str, ip: &str, ua: &str) -> ApiResult<Response> {
    let sid = auth::random_token(32);
    let csrf = auth::random_token(16);
    let ttl = (s.cfg.session_hours as i64) * 3600;
    s.db.create_session(&auth::sha256_hex(&sid), user_id, &csrf, ttl, ip, ua)?;
    let user = s.db.user(user_id).ok_or_else(|| ApiError::internal("user vanished"))?;
    let mut resp = Json(json!({"ok": true, "csrf_token": csrf, "user": user_json(&user)})).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&auth::session_cookie(&sid, ttl, s.cfg.secure_cookies)).unwrap());
    Ok(resp)
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

async fn login(State(s): State<AppState>, ClientIp(ip): ClientIp, headers: HeaderMap, Json(r): Json<LoginReq>) -> ApiResult<Response> {
    if s.cfg.auth == AuthMode::None {
        return Err(ApiError::bad_request("authentication is disabled on this instance"));
    }
    let ukey = format!("user:{}", r.username.to_lowercase());
    if !s.login_limiter.check(&format!("ip:{ip}")) || !s.login_limiter.check(&ukey) {
        s.db.audit(&r.username, "login", "", "rate_limited", &ip, "");
        return Err(ApiError::too_many("too many login attempts; try again in a few minutes"));
    }
    let user = s.db.user_by_name(&r.username);
    let ok = match &user {
        Some(u) if !u.disabled && u.role != Role::Agent => u.password_hash.as_deref().map(|h| auth::verify_password(&r.password, h)).unwrap_or(false),
        _ => {
            auth::dummy_verify();
            false
        }
    };
    if !ok {
        s.db.audit(&r.username, "login", "", "denied", &ip, "");
        return Err(ApiError::unauthorized("invalid username or password"));
    }
    let user = user.unwrap();
    s.login_limiter.reset(&ukey);
    s.db.touch_user(&user.id);
    s.db.audit(&user.username, "login", &user.id, "ok", &ip, "");
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").chars().take(200).collect::<String>();
    start_session(&s, &user.id, &ip, &ua)
}

async fn logout(State(s): State<AppState>, headers: HeaderMap, p: Principal) -> ApiResult<Response> {
    if let Some(c) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        for kv in c.split(';') {
            if let Some(v) = kv.trim().strip_prefix(&format!("{}=", auth::SESSION_COOKIE)) {
                s.db.delete_session(&auth::sha256_hex(v));
            }
        }
    }
    s.db.audit(&p.username, "logout", &p.user_id, "ok", "", "");
    let mut resp = Json(json!({"ok": true})).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&auth::session_cookie("", 0, s.cfg.secure_cookies)).unwrap());
    Ok(resp)
}

fn user_json(u: &UserRow) -> Value {
    json!({
        "id": u.id, "username": u.username, "role": u.role, "policy": u.policy,
        "disabled": u.disabled, "created_at": u.created_at, "last_seen_at": u.last_seen_at,
        "description": u.description, "kind": if u.role == Role::Agent { "agent" } else { "user" },
    })
}

async fn me(State(s): State<AppState>, p: Principal) -> Json<Value> {
    let used = s.db.storage_used(&p.user_id);
    Json(json!({
        "user": {"id": p.user_id, "username": p.username, "role": p.role, "is_admin": p.is_admin(), "via": p.via},
        "csrf_token": p.csrf,
        "permissions": p.permissions(s.host_fs_enabled()).0,
        "policy": p.policy,
        "token_scope": p.token_scope,
        "storage": {"used": used, "quota": p.quota_bytes(s.cfg.user_quota_mb)},
        "max_upload": p.max_upload_bytes(s.cfg.max_upload_mb),
    }))
}

#[derive(Deserialize)]
struct PwChange {
    current_password: String,
    new_password: String,
}

async fn change_password(State(s): State<AppState>, p: Principal, ClientIp(ip): ClientIp, Json(r): Json<PwChange>) -> ApiResult<Json<Value>> {
    let u = s.db.user(&p.user_id).ok_or_else(|| ApiError::not_found("user"))?;
    if p.via != "session" || !u.password_hash.as_deref().map(|h| auth::verify_password(&r.current_password, h)).unwrap_or(false) {
        return Err(ApiError::forbidden("current password is incorrect"));
    }
    auth::validate_password(&r.new_password)?;
    s.db.set_password(&u.id, &auth::hash_password(&r.new_password).map_err(ApiError::internal)?)?;
    s.db.audit(&p.username, "password.change", &u.id, "ok", &ip, "");
    Ok(Json(json!({"ok": true, "relogin": true})))
}

// ---------------------------------------------------------------- tools

#[derive(Deserialize)]
struct ToolQuery {
    q: Option<String>,
    category: Option<String>,
    #[serde(default)]
    all: bool,
}

fn tool_json(s: &AppState, p: &Principal, info: &ToolInfo) -> Value {
    let mut v = serde_json::to_value(info).unwrap_or_default();
    let perms = p.permissions(s.host_fs_enabled());
    let missing: Vec<&str> = perms.missing(&info.spec.permissions).iter().map(|x| x.id()).collect();
    v["permitted"] = json!(p.can_use(&info.spec) && missing.is_empty());
    if !missing.is_empty() {
        v["missing_permissions"] = json!(missing);
    }
    v["input_schema"] = dogubako_core::schema::input_schema(&info.spec, "id");
    v
}

async fn list_tools(State(s): State<AppState>, p: Principal, Query(q): Query<ToolQuery>) -> Json<Value> {
    let specs: Vec<_> = match q.q.as_deref().filter(|x| !x.trim().is_empty()) {
        Some(query) => s.engine.registry.search(query).into_iter().cloned().collect(),
        None => s.engine.registry.specs().cloned().collect(),
    };
    let tools: Vec<Value> = specs
        .iter()
        .filter(|spec| q.category.as_deref().map(|c| spec.category.id() == c).unwrap_or(true))
        .filter(|spec| q.all && p.is_admin() || p.can_use(spec))
        .map(|spec| tool_json(&s, &p, &s.engine.info(spec)))
        .collect();
    Json(json!({"tools": tools, "count": tools.len()}))
}

async fn get_tool(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let t = s.engine.registry.find(&id).ok_or_else(|| ApiError::not_found("unknown tool"))?;
    if !p.can_use(t.spec()) && !p.is_admin() {
        return Err(ApiError::not_found("unknown tool"));
    }
    Ok(Json(tool_json(&s, &p, &s.engine.info(t.spec()))))
}

async fn categories(State(s): State<AppState>, p: Principal) -> Json<Value> {
    let cats: Vec<Value> = Category::ALL
        .iter()
        .map(|c| {
            let n = s.engine.registry.by_category(*c).filter(|t| p.can_use(t)).count();
            json!({"id": c.id(), "label": c.label(), "description": c.description(), "count": n})
        })
        .collect();
    Json(json!({"categories": cats}))
}

#[derive(Deserialize)]
struct RunReq {
    #[serde(default)]
    params: Map<String, Value>,
    /// Seconds to wait for completion before returning (default 30, max 600).
    wait_seconds: Option<u64>,
}

fn job_out(s: &AppState, mut j: JobView) -> Value {
    if let Some(r) = j.result.as_mut() {
        for a in &mut r.artifacts {
            if let Some(u) = &a.url {
                a.url = Some(s.absolute(u));
            }
        }
    }
    serde_json::to_value(j).unwrap_or_default()
}

async fn run_tool(State(s): State<AppState>, p: Principal, Path(id): Path<String>, Json(r): Json<RunReq>) -> ApiResult<Response> {
    let job = s.jobs.submit(&p, &id, r.params, if p.via == "token" { "api" } else { "web" }, s.cfg.user_quota_mb)?;
    let wait = Duration::from_secs(r.wait_seconds.unwrap_or(30).min(600));
    let job = s.jobs.wait(&p.user_id, &job.id, wait).await.unwrap_or(job);
    let code = if job.status.is_terminal() { StatusCode::OK } else { StatusCode::ACCEPTED };
    Ok((code, Json(job_out(&s, job))).into_response())
}

// ---------------------------------------------------------------- jobs

#[derive(Deserialize)]
struct JobsQuery {
    limit: Option<i64>,
    #[serde(default)]
    all: bool,
}

async fn list_jobs(State(s): State<AppState>, p: Principal, Query(q): Query<JobsQuery>) -> Json<Value> {
    let owner = if q.all && p.is_admin() { None } else { Some(p.user_id.as_str()) };
    let jobs: Vec<Value> = s
        .db
        .jobs(owner, q.limit.unwrap_or(50).clamp(1, 500))
        .into_iter()
        .map(|j| {
            let live = j.owner.as_deref().and_then(|o| s.jobs.get(o, &j.id)).unwrap_or(j);
            job_out(&s, live)
        })
        .collect();
    Json(json!({"jobs": jobs}))
}

#[derive(Deserialize)]
struct WaitQuery {
    wait_seconds: Option<u64>,
}

async fn get_job(State(s): State<AppState>, p: Principal, Path(id): Path<String>, Query(q): Query<WaitQuery>) -> ApiResult<Json<Value>> {
    let wait = Duration::from_secs(q.wait_seconds.unwrap_or(0).min(600));
    let j = s.jobs.wait(&p.user_id, &id, wait).await.ok_or_else(|| ApiError::not_found("job not found"))?;
    Ok(Json(job_out(&s, j)))
}

async fn cancel_job(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.jobs.cancel(&p.user_id, &id).ok_or_else(|| ApiError::not_found("job not found"))?;
    let j = s.jobs.wait(&p.user_id, &id, Duration::from_secs(5)).await.ok_or_else(|| ApiError::not_found("job not found"))?;
    Ok(Json(job_out(&s, j)))
}

async fn delete_job(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let j = s.jobs.get(&p.user_id, &id).ok_or_else(|| ApiError::not_found("job not found"))?;
    if !j.status.is_terminal() {
        return Err(ApiError::conflict("job is still running; cancel it first"));
    }
    for f in s.db.files_for_job(&id) {
        if f.owner_id == p.user_id {
            s.storage.delete(&f)?;
        }
    }
    s.db.delete_job(&id)?;
    Ok(Json(json!({"ok": true})))
}

async fn job_events(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Response> {
    let initial = s.jobs.get(&p.user_id, &id).ok_or_else(|| ApiError::not_found("job not found"))?;
    let rx = s.jobs.subscribe(&p.user_id, &id);
    let state = s.clone();
    let stream = async_stream_job(state, initial, rx);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))).into_response())
}

fn async_stream_job(
    s: AppState,
    initial: JobView,
    rx: Option<tokio::sync::watch::Receiver<JobView>>,
) -> impl futures::Stream<Item = Result<Event, std::convert::Infallible>> {
    let first = futures::stream::once({
        let s = s.clone();
        let v = initial.clone();
        async move { Ok(Event::default().event("job").data(job_out(&s, v).to_string())) }
    });
    let rest = match rx {
        Some(rx) if !initial.status.is_terminal() => tokio_stream::wrappers::WatchStream::from_changes(rx)
            .scan(false, |done, v: JobView| {
                if *done {
                    return futures::future::ready(None);
                }
                *done = v.status.is_terminal();
                futures::future::ready(Some(v))
            })
            .map(move |v| Ok(Event::default().event("job").data(job_out(&s, v).to_string())))
            .boxed(),
        _ => futures::stream::empty().boxed(),
    };
    first.chain(rest)
}

async fn job_zip(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Response> {
    let _ = s.jobs.get(&p.user_id, &id).ok_or_else(|| ApiError::not_found("job not found"))?;
    let files: Vec<_> = s.db.files_for_job(&id).into_iter().filter(|f| f.owner_id == p.user_id).collect();
    if files.is_empty() {
        return Err(ApiError::not_found("job has no output files"));
    }
    let tmp = tempfile::NamedTempFile::new_in(s.cfg.tmp_dir())?;
    let path = tmp.path().to_path_buf();
    let files2 = files.clone();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let f = std::fs::File::create(&path)?;
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut used = std::collections::HashSet::new();
        for file in &files2 {
            let mut name = file.view.name.clone();
            let mut i = 2;
            while !used.insert(name.clone()) {
                name = dogubako_core::util::suffixed(&file.view.name, &i.to_string(), None);
                i += 1;
            }
            z.start_file(name, opts).map_err(std::io::Error::other)?;
            let mut src = std::fs::File::open(&file.path)?;
            std::io::copy(&mut src, &mut z)?;
        }
        z.finish().map_err(std::io::Error::other)?;
        Ok(())
    })
    .await
    .map_err(|e| ApiError::internal(e.to_string()))??;
    let file = tokio::fs::File::open(tmp.path()).await?;
    let len = file.metadata().await?.len();
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file));
    // tmp is deleted when dropped; the open handle keeps data readable on Unix.
    #[cfg(unix)]
    drop(tmp);
    #[cfg(not(unix))]
    std::mem::forget(tmp);
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_LENGTH, len)
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"dogubako-{id}.zip\""))
        .body(body)
        .unwrap())
}

// ---------------------------------------------------------------- files

async fn list_files(State(s): State<AppState>, p: Principal) -> Json<Value> {
    let used = s.db.storage_used(&p.user_id);
    Json(json!({"files": s.storage.list(&p.user_id), "used": used, "quota": p.quota_bytes(s.cfg.user_quota_mb)}))
}

async fn check_quota(s: &AppState, p: &Principal, incoming: u64) -> ApiResult<()> {
    let used = s.db.storage_used(&p.user_id);
    if used + incoming > p.quota_bytes(s.cfg.user_quota_mb) {
        return Err(ApiError::too_large("storage quota exceeded"));
    }
    Ok(())
}

async fn upload_multipart(State(s): State<AppState>, p: Principal, mut mp: Multipart) -> ApiResult<Json<Value>> {
    let max = p.max_upload_bytes(s.cfg.max_upload_mb);
    let mut out = vec![];
    while let Some(mut field) = mp.next_field().await.map_err(|e| ApiError::bad_request(format!("invalid multipart body: {e}")))? {
        let Some(name) = field.file_name().map(|n| n.to_string()) else { continue };
        check_quota(&s, &p, 0).await?;
        let tmp = tempfile::NamedTempFile::new_in(s.cfg.tmp_dir())?;
        let (std_file, tmp_path) = tmp.into_parts();
        let mut f = tokio::fs::File::from_std(std_file);
        let mut written: u64 = 0;
        while let Some(chunk) = field.chunk().await.map_err(|e| ApiError::bad_request(format!("upload interrupted: {e}")))? {
            written += chunk.len() as u64;
            if written > max {
                return Err(ApiError::too_large(format!("file exceeds the upload limit of {} MB", max / 1024 / 1024)));
            }
            f.write_all(&chunk).await?;
        }
        f.flush().await?;
        drop(f);
        check_quota(&s, &p, written).await?;
        let path = tmp_path.keep().map_err(|e| ApiError::internal(e.to_string()))?;
        let view = s.storage.store(&p.user_id, &name, &path, "upload", None)?;
        out.push(view);
    }
    if out.is_empty() {
        return Err(ApiError::bad_request("no file fields in upload"));
    }
    s.db.audit(&p.username, "file.upload", &out.iter().map(|f| f.id.clone()).collect::<Vec<_>>().join(","), "ok", "", "");
    Ok(Json(json!({"files": out})))
}

#[derive(Deserialize)]
struct RawQuery {
    name: String,
}

async fn upload_raw(State(s): State<AppState>, p: Principal, Query(q): Query<RawQuery>, body: Body) -> ApiResult<Json<Value>> {
    let max = p.max_upload_bytes(s.cfg.max_upload_mb);
    check_quota(&s, &p, 0).await?;
    let tmp = tempfile::NamedTempFile::new_in(s.cfg.tmp_dir())?;
    let (std_file, tmp_path) = tmp.into_parts();
    let mut f = tokio::fs::File::from_std(std_file);
    let mut stream = body.into_data_stream();
    let mut written = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ApiError::bad_request(format!("upload interrupted: {e}")))?;
        written += chunk.len() as u64;
        if written > max {
            return Err(ApiError::too_large("file exceeds the upload limit"));
        }
        f.write_all(&chunk).await?;
    }
    f.flush().await?;
    drop(f);
    check_quota(&s, &p, written).await?;
    let path = tmp_path.keep().map_err(|e| ApiError::internal(e.to_string()))?;
    let view = s.storage.store(&p.user_id, &q.name, &path, "upload", None)?;
    s.db.audit(&p.username, "file.upload", &view.id, "ok", "", "raw");
    Ok(Json(serde_json::to_value(view).unwrap()))
}

async fn get_file(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let f = s.storage.get(&p.user_id, &id).ok_or_else(|| ApiError::not_found("file not found"))?;
    Ok(Json(serde_json::to_value(f.view).unwrap()))
}

async fn delete_file(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let f = s.storage.get(&p.user_id, &id).ok_or_else(|| ApiError::not_found("file not found"))?;
    s.storage.delete(&f)?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct ContentQuery {
    #[serde(default)]
    inline: bool,
}

/// MIME types safe to render inline in the browser (no active content).
fn inline_safe(mime: &str) -> bool {
    (mime.starts_with("image/") && mime != "image/svg+xml")
        || mime.starts_with("audio/")
        || mime.starts_with("video/")
        || matches!(mime, "application/pdf" | "text/plain" | "image/svg+xml" | "application/json" | "text/csv" | "text/vtt")
}

async fn file_content(State(s): State<AppState>, p: Principal, Path(id): Path<String>, Query(q): Query<ContentQuery>, headers: HeaderMap) -> ApiResult<Response> {
    let f = s.storage.get(&p.user_id, &id).ok_or_else(|| ApiError::not_found("file not found"))?;
    let file = tokio::fs::File::open(&f.path).await.map_err(|_| ApiError::not_found("file content missing"))?;
    let len = file.metadata().await?.len();
    let inline = q.inline && inline_safe(&f.view.mime);
    // Text-like types are served as text/plain when inline to avoid script execution.
    let ctype = if inline && matches!(f.view.mime.as_str(), "application/json" | "text/csv" | "text/vtt") { "text/plain; charset=utf-8".to_string() } else { f.view.mime.clone() };
    let disp = format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        f.view.name.replace(['"', '\\', '\r', '\n'], "_"),
        percent_encode(&f.view.name)
    );
    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CONTENT_DISPOSITION, disp)
        .header("x-content-type-options", "nosniff")
        .header("content-security-policy", "default-src 'none'; img-src 'self' data:; media-src 'self'; style-src 'unsafe-inline'; sandbox")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "private, max-age=3600");
    // Simple single-range support for media seeking.
    if let Some(range) = headers.get(header::RANGE).and_then(|r| r.to_str().ok()).and_then(|r| parse_range(r, len)) {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let (start, end) = range;
        let mut file = file;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let take = file.take(end - start + 1);
        builder = builder
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
            .header(header::CONTENT_LENGTH, end - start + 1);
        return Ok(builder.body(Body::from_stream(tokio_util::io::ReaderStream::new(take))).unwrap());
    }
    Ok(builder.header(header::CONTENT_LENGTH, len).body(Body::from_stream(tokio_util::io::ReaderStream::new(file))).unwrap())
}

fn parse_range(h: &str, len: u64) -> Option<(u64, u64)> {
    let r = h.strip_prefix("bytes=")?;
    if r.contains(',') || len == 0 {
        return None;
    }
    let (a, b) = r.split_once('-')?;
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end: u64 = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
        (start, end)
    };
    (start <= end && start < len).then_some((start, end))
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

// ---------------------------------------------------------------- tokens

#[derive(Deserialize)]
struct TokenReq {
    name: String,
    /// Owner of the token (admin only; defaults to the caller).
    user_id: Option<String>,
    expires_days: Option<i64>,
    scope: Option<Policy>,
}

fn create_token_for(s: &AppState, actor: &Principal, target: &UserRow, r: &TokenReq) -> ApiResult<Value> {
    if r.name.trim().is_empty() || r.name.len() > 100 {
        return Err(ApiError::bad_request("token name must be 1-100 characters"));
    }
    if let Some(sc) = &r.scope {
        sc.validate().map_err(ApiError::bad_request)?;
    }
    let (plain, hash, prefix) = auth::new_api_token();
    let id = format!("t_{}", auth::random_token(8));
    let expires = r.expires_days.filter(|d| *d > 0).map(|d| now() + d * 86400);
    s.db.create_token(&id, &target.id, r.name.trim(), &hash, &prefix, r.scope.as_ref(), expires)?;
    s.db.audit(&actor.username, "token.create", &format!("{id} for {}", target.username), "ok", "", &r.name);
    Ok(json!({"id": id, "token": plain, "prefix": prefix, "expires_at": expires, "note": "Store this token now; it cannot be shown again."}))
}

async fn my_tokens(State(s): State<AppState>, p: Principal) -> Json<Value> {
    Json(json!({"tokens": s.db.tokens(Some(&p.user_id))}))
}

async fn create_my_token(State(s): State<AppState>, p: Principal, Json(r): Json<TokenReq>) -> ApiResult<Json<Value>> {
    if p.via == "token" {
        return Err(ApiError::forbidden("tokens cannot mint other tokens"));
    }
    if r.user_id.as_deref().is_some_and(|u| u != p.user_id) {
        return Err(ApiError::forbidden("use the admin endpoint to create tokens for others"));
    }
    let u = s.db.user(&p.user_id).ok_or_else(|| ApiError::not_found("user"))?;
    Ok(Json(create_token_for(&s, &p, &u, &r)?))
}

async fn revoke_token(State(s): State<AppState>, p: Principal, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let owner = s.db.token_owner(&id).ok_or_else(|| ApiError::not_found("token not found"))?;
    if owner != p.user_id && !p.is_admin() {
        return Err(ApiError::not_found("token not found"));
    }
    s.db.revoke_token(&id)?;
    s.db.audit(&p.username, "token.revoke", &id, "ok", "", "");
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------- deps, models, extensions

async fn deps(State(s): State<AppState>, _p: Principal) -> Json<Value> {
    let deps = s.engine.deps.all();
    let mut used_by: std::collections::HashMap<String, Vec<String>> = Default::default();
    for spec in s.engine.registry.specs() {
        for d in &spec.requires {
            used_by.entry(d.clone()).or_default().push(spec.id.clone());
        }
    }
    let list: Vec<Value> = deps
        .into_iter()
        .map(|d| {
            let mut v = serde_json::to_value(&d).unwrap();
            v["path"] = Value::Null; // don't disclose host paths
            v["used_by"] = json!(used_by.get(&d.id).cloned().unwrap_or_default());
            v
        })
        .collect();
    Json(json!({"dependencies": list, "platform": dogubako_core::spec::Platform::current()}))
}

async fn deps_refresh(State(s): State<AppState>, Admin(_): Admin) -> Json<Value> {
    s.engine.deps.refresh();
    Json(json!({"ok": true}))
}

async fn models(State(s): State<AppState>, _p: Principal) -> Json<Value> {
    let downloads: Vec<DownloadState> = s.downloads.lock().unwrap().values().cloned().collect();
    let models: Vec<Value> = s
        .engine
        .models
        .list()
        .into_iter()
        .map(|m| {
            let mut v = serde_json::to_value(&m).unwrap();
            v["path"] = Value::Null;
            v
        })
        .collect();
    Json(json!({"models": models, "downloads": downloads}))
}

async fn model_download(State(s): State<AppState>, Admin(p): Admin, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    if dogubako_core::models::model_spec(&id).is_none() {
        return Err(ApiError::not_found("unknown model"));
    }
    {
        let mut d = s.downloads.lock().unwrap();
        if d.get(&id).map(|x| x.status == "downloading").unwrap_or(false) {
            return Err(ApiError::conflict("already downloading"));
        }
        d.insert(id.clone(), DownloadState { model: id.clone(), done: 0, total: 0, status: "downloading".into(), error: None });
    }
    s.db.audit(&p.username, "model.download", &id, "started", "", "");
    let st = s.clone();
    tokio::spawn(async move {
        let st2 = st.clone();
        let mid = id.clone();
        let res = st
            .engine
            .models
            .download(
                &id,
                move |done, total| {
                    if let Some(x) = st2.downloads.lock().unwrap().get_mut(&mid) {
                        x.done = done;
                        x.total = total;
                    }
                },
                Default::default(),
            )
            .await;
        let mut d = st.downloads.lock().unwrap();
        if let Some(x) = d.get_mut(&id) {
            match res {
                Ok(_) => x.status = "done".into(),
                Err(e) => {
                    x.status = "failed".into();
                    x.error = Some(e.to_string());
                }
            }
        }
    });
    Ok(Json(json!({"ok": true})))
}

async fn model_delete(State(s): State<AppState>, Admin(p): Admin, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    s.engine.models.remove(&id)?;
    s.db.audit(&p.username, "model.delete", &id, "ok", "", "");
    Ok(Json(json!({"ok": true})))
}

async fn extensions(State(s): State<AppState>, Admin(_): Admin) -> Json<Value> {
    Json(json!({"extensions": *s.extensions, "dir": s.cfg.extensions_dir()}))
}

// ---------------------------------------------------------------- admin

#[derive(Deserialize)]
struct CreateUser {
    username: String,
    /// Required for human users; must be absent for agents.
    password: Option<String>,
    role: Role,
    policy: Option<Policy>,
    #[serde(default)]
    description: String,
}

async fn admin_users(State(s): State<AppState>, Admin(_): Admin) -> Json<Value> {
    let users: Vec<Value> = s
        .db
        .users()
        .iter()
        .filter(|u| u.id != LOCAL_USER_ID || s.cfg.auth == AuthMode::None)
        .map(|u| {
            let mut v = user_json(u);
            v["storage_used"] = json!(s.db.storage_used(&u.id));
            v
        })
        .collect();
    Json(json!({"users": users}))
}

async fn admin_create_user(State(s): State<AppState>, Admin(a): Admin, Json(r): Json<CreateUser>) -> ApiResult<Json<Value>> {
    auth::validate_username(&r.username)?;
    if r.role == Role::Owner && a.role != Role::Owner {
        return Err(ApiError::forbidden("only the owner can create owners"));
    }
    let hash = match (r.role, &r.password) {
        (Role::Agent, Some(_)) => return Err(ApiError::bad_request("agents authenticate with API tokens, not passwords")),
        (Role::Agent, None) => None,
        (_, Some(pw)) => {
            auth::validate_password(pw)?;
            Some(auth::hash_password(pw).map_err(ApiError::internal)?)
        }
        (_, None) => return Err(ApiError::bad_request("password is required")),
    };
    let policy = r.policy.unwrap_or_else(|| Policy::for_role(r.role));
    policy.validate().map_err(ApiError::bad_request)?;
    let prefix = if r.role == Role::Agent { "a" } else { "u" };
    let id = format!("{prefix}_{}", auth::random_token(8));
    s.db.create_user(&id, &r.username, hash.as_deref(), r.role, &policy, &r.description)?;
    s.db.audit(&a.username, "user.create", &format!("{} ({})", r.username, r.role.as_str()), "ok", "", &serde_json::to_string(&policy).unwrap_or_default());
    Ok(Json(user_json(&s.db.user(&id).unwrap())))
}

#[derive(Deserialize)]
struct UpdateUser {
    role: Option<Role>,
    policy: Option<Policy>,
    disabled: Option<bool>,
    description: Option<String>,
}

async fn admin_update_user(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<String>, Json(r): Json<UpdateUser>) -> ApiResult<Json<Value>> {
    let mut u = s.db.user(&id).ok_or_else(|| ApiError::not_found("user not found"))?;
    if (u.role == Role::Owner || r.role == Some(Role::Owner)) && a.role != Role::Owner {
        return Err(ApiError::forbidden("only the owner can modify owners"));
    }
    if id == a.user_id && (r.disabled == Some(true) || r.role.is_some_and(|x| x != u.role)) {
        return Err(ApiError::bad_request("you cannot disable yourself or change your own role"));
    }
    if let Some(role) = r.role {
        if (role == Role::Agent) != (u.role == Role::Agent) {
            return Err(ApiError::bad_request("cannot convert between agents and human users"));
        }
        u.role = role;
    }
    if let Some(pol) = r.policy {
        pol.validate().map_err(ApiError::bad_request)?;
        u.policy = pol;
    }
    if let Some(d) = r.disabled {
        u.disabled = d;
    }
    if let Some(d) = r.description {
        u.description = d.chars().take(500).collect();
    }
    s.db.update_user(&u)?;
    s.db.audit(&a.username, "user.update", &u.username, "ok", "", &serde_json::to_string(&u.policy).unwrap_or_default());
    Ok(Json(user_json(&u)))
}

async fn admin_delete_user(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let u = s.db.user(&id).ok_or_else(|| ApiError::not_found("user not found"))?;
    if id == a.user_id || u.role == Role::Owner && a.role != Role::Owner {
        return Err(ApiError::forbidden("cannot delete this account"));
    }
    for f in s.db.files_of_owner(&id) {
        let _ = s.storage.delete(&f);
    }
    s.db.delete_user(&id)?;
    s.db.audit(&a.username, "user.delete", &u.username, "ok", "", "");
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct SetPw {
    password: String,
}

async fn admin_set_password(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<String>, Json(r): Json<SetPw>) -> ApiResult<Json<Value>> {
    let u = s.db.user(&id).ok_or_else(|| ApiError::not_found("user not found"))?;
    if u.role == Role::Agent {
        return Err(ApiError::bad_request("agents use API tokens"));
    }
    if u.role == Role::Owner && a.role != Role::Owner {
        return Err(ApiError::forbidden("only the owner can reset an owner's password"));
    }
    auth::validate_password(&r.password)?;
    s.db.set_password(&id, &auth::hash_password(&r.password).map_err(ApiError::internal)?)?;
    s.db.audit(&a.username, "user.password_reset", &u.username, "ok", "", "");
    Ok(Json(json!({"ok": true})))
}

async fn admin_tokens(State(s): State<AppState>, Admin(_): Admin) -> Json<Value> {
    Json(json!({"tokens": s.db.tokens(None)}))
}

async fn admin_create_token(State(s): State<AppState>, Admin(a): Admin, Json(r): Json<TokenReq>) -> ApiResult<Json<Value>> {
    let uid = r.user_id.clone().unwrap_or_else(|| a.user_id.clone());
    let u = s.db.user(&uid).ok_or_else(|| ApiError::not_found("user not found"))?;
    if u.role == Role::Owner && a.role != Role::Owner {
        return Err(ApiError::forbidden("only the owner can mint tokens for owners"));
    }
    Ok(Json(create_token_for(&s, &a, &u, &r)?))
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
    before: Option<i64>,
}

async fn admin_audit(State(s): State<AppState>, Admin(_): Admin, Query(q): Query<AuditQuery>) -> Json<Value> {
    Json(json!({"entries": s.db.audit_log(q.limit.unwrap_or(200).clamp(1, 1000), q.before)}))
}

async fn admin_config(State(s): State<AppState>, Admin(_): Admin) -> Json<Value> {
    let mut c = serde_json::to_value(&*s.cfg).unwrap();
    c["setup_token"] = Value::Null;
    Json(json!({
        "config": c,
        "running_jobs": s.jobs.running_count(),
        "uptime_secs": s.started.elapsed().as_secs(),
        "permissions": Permission::ALL.iter().map(|p| json!({"id": p.id(), "description": p.describe()})).collect::<Vec<_>>(),
    }))
}

async fn openapi(State(s): State<AppState>) -> Json<Value> {
    Json(crate::openapi::spec(&s.engine))
}

#[allow(dead_code)]
fn _unused(_: ResultView) {}
