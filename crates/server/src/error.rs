use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use dogubako_core::error::ToolError;
use serde_json::json;

/// JSON API error: `{"error": {"code", "message", ...}}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub extra: Option<Box<serde_json::Value>>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        ApiError {
            status,
            code,
            message: message.into(),
            extra: None,
        }
    }
    pub fn bad_request(m: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", m)
    }
    pub fn unauthorized(m: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", m)
    }
    pub fn forbidden(m: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", m)
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", m)
    }
    pub fn conflict(m: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", m)
    }
    pub fn too_large(m: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "too_large", m)
    }
    pub fn too_many(m: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", m)
    }
    pub fn internal(m: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", m)
    }
}

impl From<ToolError> for ApiError {
    fn from(e: ToolError) -> Self {
        let status =
            StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
        ApiError {
            status,
            code: e.code(),
            message: e.to_string(),
            extra: serde_json::to_value(&e).ok().map(Box::new),
        }
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(f, _) = &e {
            if f.code == rusqlite::ErrorCode::ConstraintViolation {
                return ApiError::conflict("already exists");
            }
        }
        tracing::error!("database error: {e}");
        ApiError::internal("database error")
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!("io error: {e}");
        ApiError::internal("storage error")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({"error": {"code": self.code, "message": self.message}});
        if let Some(x) = self.extra {
            body["error"]["detail"] = *x;
        }
        let mut resp = (self.status, Json(body)).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            resp.headers_mut().insert(
                "www-authenticate",
                axum::http::HeaderValue::from_static("Bearer realm=\"dogubako\""),
            );
        }
        resp
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
