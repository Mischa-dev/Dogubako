//! Server configuration from environment variables and an optional TOML file.
//!
//! Precedence: defaults < config file (`DOGUBAKO_CONFIG` or
//! `<data_dir>/dogubako.toml`) < environment variables < CLI flags.

use dogubako_core::packs::PackSelection;
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
    /// Let admins install extension manifests over HTTP. Off by default:
    /// a manifest runs a host program, outside any sandbox unless one is set.
    pub allow_extension_install: bool,
    pub mcp_enabled: bool,
    /// Default MCP discovery mode: "full" or "compact".
    pub mcp_discovery: String,
    /// Built-in tool packs turned on (`DOGUBAKO_PACKS`, or `packs = [...]`
    /// in the config file); unset means every pack.
    #[serde(default, deserialize_with = "packs_de", serialize_with = "packs_ser")]
    pub packs: PackSelection,
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
    /// Natural-language assistant: relay chat requests to OpenAI-compatible
    /// endpoints that users configure in their browser.
    pub assistant_enabled: bool,
    /// Let the assistant relay reach loopback/private addresses (e.g. Ollama
    /// on this host) even when network tools may not.
    pub assistant_allow_private: bool,
    /// Optional endpoint offered to every user as the "This server" provider.
    pub assistant_base_url: Option<String>,
    /// API key for `assistant_base_url`. Never sent to clients.
    #[serde(skip_serializing)]
    pub assistant_api_key: Option<String>,
    /// Default model id for `assistant_base_url`.
    pub assistant_model: Option<String>,
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
            allow_extension_install: false,
            mcp_enabled: true,
            mcp_discovery: "full".into(),
            packs: PackSelection::all(),
            allowed_origins: vec![],
            sandbox: SandboxMode::None,
            worker_exe: None,
            exec_memory_mb: Some(4096),
            exec_cpu_secs: Some(3600),
            max_job_seconds: 3600,
            extensions_dir: None,
            models_dir: None,
            setup_token: None,
            assistant_enabled: true,
            assistant_allow_private: false,
            assistant_base_url: None,
            assistant_api_key: None,
            assistant_model: None,
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
        if let Some(v) = env_bool("ALLOW_EXTENSION_INSTALL") {
            cfg.allow_extension_install = v;
        }
        if let Some(v) = env_bool("MCP_ENABLED") {
            cfg.mcp_enabled = v;
        }
        if let Some(v) = env("MCP_DISCOVERY") {
            cfg.mcp_discovery = v;
        }
        if let Some(v) = env("PACKS") {
            cfg.packs = PackSelection::parse_checked(&v).map_err(|bad| {
                anyhow_lite::Error(format!("DOGUBAKO_PACKS: {}", unknown_packs(&bad)))
            })?;
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
        if let Some(v) = env_bool("ASSISTANT_ENABLED") {
            cfg.assistant_enabled = v;
        }
        if let Some(v) = env_bool("ASSISTANT_ALLOW_PRIVATE") {
            cfg.assistant_allow_private = v;
        }
        if let Some(v) = env("ASSISTANT_BASE_URL") {
            cfg.assistant_base_url = Some(v);
        }
        if let Some(v) = env("ASSISTANT_API_KEY") {
            cfg.assistant_api_key = Some(v);
        }
        if let Some(v) = env("ASSISTANT_MODEL") {
            cfg.assistant_model = Some(v);
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
        let unknown = self.packs.unknown();
        if !unknown.is_empty() {
            return Err(anyhow_lite::Error(unknown_packs(&unknown)));
        }
        if !["full", "compact"].contains(&self.mcp_discovery.as_str()) {
            return Err(anyhow_lite::Error(
                "mcp_discovery must be 'full' or 'compact'".into(),
            ));
        }
        Ok(())
    }

    /// The network policy for the assistant relay.
    pub fn assistant_net_policy(&self) -> dogubako_core::policy::NetPolicy {
        if self.allow_private_network || self.assistant_allow_private {
            dogubako_core::policy::NetPolicy::default()
        } else {
            dogubako_core::policy::NetPolicy::public_only()
        }
    }

    /// The server-provided assistant endpoint, when configured.
    pub fn assistant_server_endpoint(&self) -> Option<dogubako_core::llm::Endpoint> {
        let base = self.assistant_base_url.as_deref()?.trim();
        (!base.is_empty()).then(|| dogubako_core::llm::Endpoint {
            base_url: base.to_string(),
            api_key: self.assistant_api_key.clone(),
        })
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

fn unknown_packs(bad: &[String]) -> String {
    format!(
        "unknown tool pack(s) {}; known packs: {}",
        bad.join(", "),
        dogubako_core::packs::builtin_packs()
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// `packs` in the config file: a list (`["image", "pdf"]`, `[]` for none) or
/// a string (`"all,-speech"`).
fn packs_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PackSelection, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        List(Vec<String>),
        Text(String),
        Selection(PackSelection),
    }
    let checked = |t: &str| {
        PackSelection::parse_checked(t).map_err(|bad| serde::de::Error::custom(unknown_packs(&bad)))
    };
    match Raw::deserialize(d)? {
        Raw::List(v) if v.is_empty() => Ok(PackSelection::only(v)),
        Raw::List(v) => checked(&v.join(",")),
        Raw::Text(t) => checked(&t),
        Raw::Selection(s) => Ok(s),
    }
}

/// Written the way [`packs_de`] reads it, so `dogubako server config` output
/// loads back: `"all"` or the list of packs turned on.
fn packs_ser<S: serde::Serializer>(p: &PackSelection, s: S) -> Result<S::Ok, S::Error> {
    match &p.enabled {
        None => s.serialize_str("all"),
        Some(set) => s.collect_seq(set),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packs_from(toml_text: &str) -> Result<PackSelection, String> {
        toml::from_str::<ServerConfig>(toml_text)
            .map(|c| c.packs)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn packs_in_config_file() {
        assert!(packs_from("").unwrap().is_all());
        assert_eq!(
            packs_from("packs = []").unwrap(),
            PackSelection::only(Vec::<String>::new())
        );
        assert_eq!(
            packs_from("packs = [\"pdf\"]").unwrap(),
            PackSelection::only(["pdf"])
        );
        assert!(packs_from("packs = \"all,-spech\"")
            .unwrap_err()
            .contains("spech"));
    }

    #[test]
    fn packs_round_trip_through_toml() {
        for p in [
            PackSelection::all(),
            PackSelection::only(["image", "pdf"]),
            PackSelection::only(Vec::<String>::new()),
        ] {
            let cfg = ServerConfig {
                packs: p.clone(),
                ..Default::default()
            };
            let text = toml::to_string(&cfg).unwrap();
            assert_eq!(toml::from_str::<ServerConfig>(&text).unwrap().packs, p);
        }
    }
}
