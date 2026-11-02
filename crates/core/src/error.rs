use serde::{Deserialize, Serialize};

/// Structured tool errors. Every interface maps these to its own
/// representation (HTTP status, MCP isError, CLI exit code).
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, PartialEq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ToolError {
    #[error("invalid input{}: {message}", param.as_ref().map(|p| format!(" for '{p}'")).unwrap_or_default())]
    InvalidInput { param: Option<String>, message: String },
    #[error("tool not found: {id}")]
    NotFound { id: String },
    #[error("missing dependency '{dependency}': {hint}")]
    MissingDependency { dependency: String, hint: String },
    #[error("missing model: {message}")]
    MissingModel { message: String },
    #[error("unsupported: {message}")]
    Unsupported { message: String },
    #[error("permission denied: {message}")]
    PermissionDenied { message: String },
    #[error("cancelled")]
    Cancelled,
    #[error("timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("limit exceeded: {message}")]
    LimitExceeded { message: String },
    #[error("{program} failed (exit {exit_code:?}): {stderr}")]
    External { program: String, exit_code: Option<i32>, stderr: String },
    #[error("{message}")]
    Failed { message: String },
}

impl ToolError {
    pub fn invalid(param: &str, message: impl Into<String>) -> Self {
        ToolError::InvalidInput { param: Some(param.to_string()), message: message.into() }
    }
    pub fn input(message: impl Into<String>) -> Self {
        ToolError::InvalidInput { param: None, message: message.into() }
    }
    pub fn failed(message: impl Into<String>) -> Self {
        ToolError::Failed { message: message.into() }
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        ToolError::Unsupported { message: message.into() }
    }
    pub fn denied(message: impl Into<String>) -> Self {
        ToolError::PermissionDenied { message: message.into() }
    }
    pub fn limit(message: impl Into<String>) -> Self {
        ToolError::LimitExceeded { message: message.into() }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ToolError::InvalidInput { .. } => "invalid_input",
            ToolError::NotFound { .. } => "not_found",
            ToolError::MissingDependency { .. } => "missing_dependency",
            ToolError::MissingModel { .. } => "missing_model",
            ToolError::Unsupported { .. } => "unsupported",
            ToolError::PermissionDenied { .. } => "permission_denied",
            ToolError::Cancelled => "cancelled",
            ToolError::Timeout { .. } => "timeout",
            ToolError::LimitExceeded { .. } => "limit_exceeded",
            ToolError::External { .. } => "external_failure",
            ToolError::Failed { .. } => "failed",
        }
    }

    /// Conventional process exit code for the CLI.
    pub fn exit_code(&self) -> i32 {
        match self {
            ToolError::InvalidInput { .. } | ToolError::NotFound { .. } => 2,
            ToolError::MissingDependency { .. } | ToolError::MissingModel { .. } | ToolError::Unsupported { .. } => 3,
            ToolError::PermissionDenied { .. } => 4,
            ToolError::Timeout { .. } | ToolError::LimitExceeded { .. } => 5,
            ToolError::Cancelled => 130,
            _ => 1,
        }
    }

    /// HTTP status code for the API.
    pub fn http_status(&self) -> u16 {
        match self {
            ToolError::InvalidInput { .. } => 400,
            ToolError::NotFound { .. } => 404,
            ToolError::PermissionDenied { .. } => 403,
            ToolError::MissingDependency { .. } | ToolError::MissingModel { .. } | ToolError::Unsupported { .. } => 501,
            ToolError::LimitExceeded { .. } => 413,
            ToolError::Timeout { .. } => 504,
            ToolError::Cancelled => 499,
            _ => 422,
        }
    }
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError::Failed { message: format!("I/O error: {e}") }
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(e: serde_json::Error) -> Self {
        ToolError::Failed { message: format!("JSON error: {e}") }
    }
}

pub type ToolResult<T> = Result<T, ToolError>;

/// Convenience: map any displayable error into `ToolError::Failed` with context.
pub trait Context<T> {
    fn ctx(self, what: &str) -> ToolResult<T>;
    /// Map to an invalid-input error for a parameter.
    fn bad_input(self, param: &str) -> ToolResult<T>;
}

impl<T, E: std::fmt::Display> Context<T> for Result<T, E> {
    fn ctx(self, what: &str) -> ToolResult<T> {
        self.map_err(|e| ToolError::Failed { message: format!("{what}: {e}") })
    }
    fn bad_input(self, param: &str) -> ToolResult<T> {
        self.map_err(|e| ToolError::invalid(param, e.to_string()))
    }
}
