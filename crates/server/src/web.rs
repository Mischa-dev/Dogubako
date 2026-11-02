//! Embedded web UI (built from `web/` with Vite). In debug builds files are
//! read from disk; release builds embed them into the binary.

use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let candidate = if path.is_empty() { "index.html" } else { path };
    if let Some(f) = Assets::get(candidate) {
        let mime = mime_guess::from_path(candidate).first_or_octet_stream();
        let cache = if candidate.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
        return ([(header::CONTENT_TYPE, mime.as_ref().to_string()), (header::CACHE_CONTROL, cache.to_string())], f.data.into_owned()).into_response();
    }
    // SPA fallback for client-side routes.
    if !candidate.contains('.') {
        if let Some(f) = Assets::get("index.html") {
            return ([(header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()), (header::CACHE_CONTROL, "no-cache".to_string())], f.data.into_owned()).into_response();
        }
        return Html(
            "<!doctype html><title>Dogubako</title><body style=\"font-family:system-ui;padding:2rem\"><h1>Dogubako</h1><p>The web UI has not been built. Run <code>npm install && npm run build</code> in <code>web/</code>, then rebuild the server. The API is available at <code>/api/v1</code> and MCP at <code>/mcp</code>.</p>",
        )
        .into_response();
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}
