//! # dogubako-server
//!
//! Headless, self-hostable Dogubako service: web UI, versioned HTTP API,
//! MCP Streamable HTTP endpoint, accounts, API tokens, per-agent policies,
//! job management, access-controlled storage and cleanup.

pub mod api;
pub mod auth;
pub mod authz;
pub mod config;
pub mod db;
pub mod error;
pub mod jobs;
pub mod mcp_http;
pub mod openapi;
pub mod state;
pub mod storage;
pub mod web;

use axum::http::{header, HeaderValue, Request};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
pub use config::ServerConfig;
pub use state::AppState;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

async fn security_headers(req: Request<axum::body::Body>, next: Next) -> Response {
    let is_api = req.uri().path().starts_with("/api/") || req.uri().path() == "/mcp";
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.entry("x-content-type-options").or_insert(HeaderValue::from_static("nosniff"));
    h.entry("referrer-policy").or_insert(HeaderValue::from_static("no-referrer"));
    h.entry("x-frame-options").or_insert(HeaderValue::from_static("SAMEORIGIN"));
    if !is_api {
        h.entry("content-security-policy").or_insert(HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self'; frame-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'self'",
        ));
    } else {
        h.entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-store"));
    }
    resp
}

/// Build the full application router.
pub fn app(state: AppState) -> Router {
    Router::new()
        .nest("/api/v1", api::router(state.clone()))
        .route("/mcp", post(mcp_http::mcp_post).get(mcp_http::mcp_get).delete(mcp_http::mcp_delete))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(web::static_handler)
        .layer(middleware::from_fn(security_headers))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Periodic cleanup of expired files, jobs, sessions and audit entries.
pub fn spawn_cleanup(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            let files = state.storage.cleanup();
            let jobs = state.db.purge_jobs_before(db::now() - state.cfg.job_retention_days as i64 * 86400);
            let sessions = state.db.purge_sessions();
            let audit = state.db.purge_audit_before(db::now() - 180 * 86400);
            // Leftover temp files older than a day.
            if let Ok(rd) = std::fs::read_dir(state.cfg.tmp_dir()) {
                for e in rd.flatten() {
                    if e.metadata().and_then(|m| m.modified()).map(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(86400)).unwrap_or(false) {
                        let p = e.path();
                        let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                    }
                }
            }
            if files + jobs + sessions + audit > 0 {
                tracing::info!(files, jobs, sessions, audit, "cleanup");
            }
        }
    });
}

/// Run the server until Ctrl-C / SIGTERM.
pub async fn serve(cfg: ServerConfig) -> Result<(), config::anyhow_lite::Error> {
    let bind = cfg.bind;
    let state = AppState::build(cfg)?;
    if let Some(tok) = state.setup_token.lock().unwrap().clone() {
        let url = state.cfg.public_url.clone().unwrap_or_else(|| format!("http://{bind}"));
        eprintln!("\n  ┌─ Dogubako first-run setup ─────────────────────────────");
        eprintln!("  │ Open {url} and create the owner account.");
        eprintln!("  │ Setup token: {tok}");
        eprintln!("  └────────────────────────────────────────────────────────\n");
    }
    if state.cfg.auth == config::AuthMode::None {
        tracing::warn!("authentication is DISABLED (auth = none); anyone who can reach {bind} has full access");
    }
    let deps = state.engine.deps.all();
    let found: Vec<String> = deps.iter().filter(|d| d.found).map(|d| d.name.clone()).collect();
    tracing::info!(tools = state.engine.registry.len(), deps = %found.join(", "), "Dogubako server starting on http://{bind}");
    spawn_cleanup(state.clone());
    let jobs = state.jobs.clone();
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app(state).into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("shutting down; cancelling running jobs");
            jobs.cancel_all();
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
