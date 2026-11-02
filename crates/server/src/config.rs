//! Server configuration from environment variables and an optional TOML file.
//!
//! Precedence: defaults < config file (`DOGUBAKO_CONFIG` or
//! `<data_dir>/dogubako.toml`) < environment variables < CLI flags.

use dogubako_core::policy::SandboxMode;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// Accounts required (default).
    Required,
    /// No authentication; everyone is the owner. Only allowed on loopback
    /// unless explicitly overridden.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    /// Public base URL used for absolute links (e.g. https://tools.example.ts.net).
    pub public_url: Option<String>,
    pub instance_name: String,
    pub auth: AuthMode,
    pub allow_insecure_no_auth: bool,
    /// Mark session cookies `Secure` (enable behind HTTPS).
    pub secure_cookies: bool,
    pub session_hours: u64,
    /// Max size of a single upload (MiB).
    pub max_upload_mb: u64,
    /// Default per-user storage quota (MiB).
    pub user_quota_mb: u64,
    /// Hours to keep uploads and artifacts.
    pub file_retention_hours: u64,
    /// Days to keep job records.
    pub job_retention_days: u64,
    pub max_concurrent_jobs: usize,
    pub per_user_concurrency: usize,
    /// Host folders tools may access via path parameters (empty = none).
    pub fs_roots: Vec<PathBuf>,
    /// Allow network tools to reach private/loopback addresses (SSRF risk).
    pub allow_private_network: bool,
    pub mcp_enabled: bool,
    /// Default MCP discovery mode: "full" or "compact".
    pub mcp_discovery: String,
    /// Extra allowed Origins for browser requests / MCP (besides same-origin).
    pub allowed_origins: Vec<String>,
    pub sandbox: SandboxMode,
    /// Worker executable for killable in-process engines (set by the host
    /// binary, not configurable).
    #[serde(skip)]
    pub worker_exe: Option<PathBuf>,
    pub exec_memory_mb: Option<u64>,
    pub exec_cpu_secs: Option<u64>,
    pub max_job_seconds: u64,
    pub extensions_dir: Option<PathBuf>,
    pub models_dir: Option<PathBuf>,
    /// Pre-set setup token (otherwise random, printed at startup).
    pub setup_token: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            bind: "127.0.0.1:8420".parse().unwrap(),
            data_dir: PathBuf::from("./data"),
            public_url: None,
            instance_name: "Dogubako".into(),
            auth: AuthMode::Required,
            allow_insecure_no_auth: false,
            secure_cookies: false,
            session_hours: 24 * 14,
            max_upload_mb: 2048,
            user_quota_mb: 10240,
            file_retention_hours: 24,
            job_retention_days: 14,
            max_concurrent_jobs: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(2)
                .clamp(1, 16),
            per_user_concurrency: 2,
            fs_roots: vec![],
            allow_private_network: false,
            mcp_enabled: true,
            mcp_discovery: "full".into(),
            allowed_origins: vec![],
            sandbox: SandboxMode::None,
            worker_exe: None,
            exec_memory_mb: Some(4096),
            exec_cpu_secs: Some(3600),
            max_job_seconds: 3600,
            extensions_dir: None,
            models_dir: None,
            setup_token: None,
        }
    }
}

fn env(k: &str) -> Option<String> {
    std::env::var(format!("DOGUBAKO_{k}"))
        .ok()
        .filter(|v| !v.is_empty())
}

fn env_bool(k: &str) -> Option<bool> {
    env(k).map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

fn env_num<T: std::str::FromStr>(k: &str) -> Option<T> {
    env(k).and_then(|v| v.parse().ok())
}

fn env_list(k: &str) -> Option<Vec<String>> {
    env(k).map(|v| {
        v.split([',', ';'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

impl ServerConfig {
    /// Load config file (if any) then apply environment overrides.
    pub fn load(explicit_file: Option<PathBuf>) -> anyhow_lite::Result<Self> {
        let data_dir = env("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("./data"));
        let file = explicit_file
            .or_else(|| env("CONFIG").map(PathBuf::from))
            .or_else(|| {
                let p = data_dir.join("dogubako.toml");
                p.is_file().then_some(p)
            });
        let mut cfg = match &file {
            Some(f) => {
                let text = std::fs::read_to_string(f)
                    .map_err(|e| anyhow_lite::Error(format!("reading {}: {e}", f.display())))?;
                toml::from_str::<ServerConfig>(&text)
                    .map_err(|e| anyhow_lite::Error(format!("parsing {}: {e}", f.display())))?
            }
            None => ServerConfig {
                data_dir: data_dir.clone(),
                ..Default::default()
            },
        };
        if let Some(v) = env("BIND") {
            cfg.bind = v
                .parse()
                .map_err(|e| anyhow_lite::Error(format!("DOGUBAKO_BIND: {e}")))?;
        }
        if let Some(v) = env("DATA_DIR") {
            cfg.data_dir = PathBuf::from(v);
        }
        if let Some(v) = env("PUBLIC_URL") {
            cfg.public_url = Some(v.trim_end_matches('/').to_string());
        }
        if let Some(v) = env("INSTANCE_NAME") {
            cfg.instance_name = v;
        }
        if let Some(v) = env("AUTH") {
            cfg.auth = match v.as_str() {
                "none" | "disabled" | "off" => AuthMode::None,
                _ => AuthMode::Required,
            };
        }
        if let Some(v) = env_bool("ALLOW_INSECURE_NO_AUTH") {
            cfg.allow_insecure_no_auth = v;
        }
        if let Some(v) = env_bool("SECURE_COOKIES") {
            cfg.secure_cookies = v;
        }
        if let Some(v) = env_num("SESSION_HOURS") {
            cfg.session_hours = v;
        }
        if let Some(v) = env_num("MAX_UPLOAD_MB") {
            cfg.max_upload_mb = v;
        }
        if let Some(v) = env_num("USER_QUOTA_MB") {
            cfg.user_quota_mb = v;
        }
        if let Some(v) = env_num("FILE_RETENTION_HOURS") {
            cfg.file_retention_hours = v;
        }
        if let Some(v) = env_num("JOB_RETENTION_DAYS") {
            cfg.job_retention_days = v;
        }
        if let Some(v) = env_num("MAX_CONCURRENT_JOBS") {
            cfg.max_concurrent_jobs = v;
        }
        if let Some(v) = env_num("PER_USER_CONCURRENCY") {
            cfg.per_user_concurrency = v;
        }
        if let Some(v) = env_list("FS_ROOTS") {
            cfg.fs_roots = v.into_iter().map(PathBuf::from).collect();
        }
        if let Some(v) = env_bool("ALLOW_PRIVATE_NETWORK") {
            cfg.allow_private_network = v;
        }
        if let Some(v) = env_bool("MCP_ENABLED") {
            cfg.mcp_enabled = v;
        }
        if let Some(v) = env("MCP_DISCOVERY") {
            cfg.mcp_discovery = v;
        }
        if let Some(v) = env_list("ALLOWED_ORIGINS") {
            cfg.allowed_origins = v;
        }
        if let Some(v) = env("SANDBOX") {
            cfg.sandbox = if v == "bwrap" {
                SandboxMode::Bwrap
            } else {
                SandboxMode::None
            };
        }
        if let Some(v) = env("EXEC_MEMORY_MB") {
            cfg.exec_memory_mb = v.parse().ok().filter(|n: &u64| *n > 0);
        }
        if let Some(v) = env("EXEC_CPU_SECS") {
            cfg.exec_cpu_secs = v.parse().ok().filter(|n: &u64| *n > 0);
        }
        if let Some(v) = env_num("MAX_JOB_SECONDS") {
            cfg.max_job_seconds = v;
        }
        if let Some(v) = env("EXTENSIONS_DIR") {
            cfg.extensions_dir = Some(PathBuf::from(v));
        }
        if let Some(v) = env("MODELS_DIR") {
            cfg.models_dir = Some(PathBuf::from(v));
        }
        if let Some(v) = env("SETUP_TOKEN") {
            cfg.setup_token = Some(v);
        }
        Ok(cfg)
    }

    pub fn validate(&self) -> anyhow_lite::Result<()> {
        if self.auth == AuthMode::None
            && !self.bind.ip().is_loopback()
            && !self.allow_insecure_no_auth
        {
            return Err(anyhow_lite::Error(format!(
                "refusing to run without authentication on non-loopback address {}. Use DOGUBAKO_AUTH=required, bind to 127.0.0.1, or set DOGUBAKO_ALLOW_INSECURE_NO_AUTH=1 if you really mean it.",
                self.bind
            )));
        }
        if !["full", "compact"].contains(&self.mcp_discovery.as_str()) {
            return Err(anyhow_lite::Error(
                "mcp_discovery must be 'full' or 'compact'".into(),
            ));
        }
        Ok(())
    }

    pub fn models_dir(&self) -> PathBuf {
        self.models_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("models"))
    }
    pub fn extensions_dir(&self) -> PathBuf {
        self.extensions_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("extensions"))
    }
    pub fn files_dir(&self) -> PathBuf {
        self.data_dir.join("files")
    }
    pub fn jobs_dir(&self) -> PathBuf {
        self.data_dir.join("jobs")
    }
    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }
}

/// Minimal error type to avoid an anyhow dependency.
pub mod anyhow_lite {
    #[derive(Debug)]
    pub struct Error(pub String);
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl std::error::Error for Error {}
    pub type Result<T> = std::result::Result<T, Error>;
    impl From<std::io::Error> for Error {
        fn from(e: std::io::Error) -> Self {
            Error(e.to_string())
        }
    }
    impl From<rusqlite::Error> for Error {
        fn from(e: rusqlite::Error) -> Self {
            Error(format!("database: {e}"))
        }
    }
}
