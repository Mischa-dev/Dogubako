use crate::auth::{RateLimiter, LOCAL_USER_ID};
use crate::authz::{Policy, Role};
use crate::config::{AuthMode, ServerConfig};
use crate::db::Db;
use crate::jobs::JobManager;
use crate::storage::Storage;
use dogubako_core::engine::{Engine, EngineConfig};
use dogubako_core::manifest::ExtensionStatus;
use dogubako_core::policy::{ExecLimits, FsPolicy, NetPolicy};
use dogubako_core::tool::Environment;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct DownloadState {
    pub model: String,
    pub done: u64,
    pub total: u64,
    pub status: String,
    pub error: Option<String>,
}

pub struct McpSession {
    pub user_id: String,
    pub token_id: Option<String>,
    pub created: Instant,
    pub last_used: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<ServerConfig>,
    pub db: Arc<Db>,
    pub engine: Arc<Engine>,
    pub storage: Arc<Storage>,
    pub jobs: Arc<JobManager>,
    pub login_limiter: Arc<RateLimiter>,
    pub setup_token: Arc<Mutex<Option<String>>>,
    pub mcp_sessions: Arc<Mutex<HashMap<String, McpSession>>>,
    pub extensions: Arc<Vec<ExtensionStatus>>,
    pub downloads: Arc<Mutex<HashMap<String, DownloadState>>>,
    pub started: Instant,
}

impl AppState {
    pub fn build(cfg: ServerConfig) -> Result<AppState, crate::config::anyhow_lite::Error> {
        let (registry, extensions) = dogubako_tools::build_registry(Some(&cfg.extensions_dir()));
        Self::build_with(cfg, registry, extensions)
    }

    /// Build with an explicit registry (used by tests and embedders).
    pub fn build_with(
        cfg: ServerConfig,
        registry: dogubako_core::registry::Registry,
        extensions: Vec<ExtensionStatus>,
    ) -> Result<AppState, crate::config::anyhow_lite::Error> {
        cfg.validate()?;
        for d in [
            cfg.data_dir.clone(),
            cfg.files_dir(),
            cfg.jobs_dir(),
            cfg.tmp_dir(),
            cfg.models_dir(),
            cfg.extensions_dir(),
        ] {
            std::fs::create_dir_all(&d)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&cfg.data_dir, std::fs::Permissions::from_mode(0o700));
        }
        let db = Arc::new(Db::open(&cfg.data_dir.join("dogubako.db"))?);
        let orphaned = db.fail_orphaned_jobs();
        if orphaned > 0 {
            tracing::warn!("marked {orphaned} interrupted job(s) as failed");
        }
        // Stale job dirs from a crash.
        if let Ok(rd) = std::fs::read_dir(cfg.jobs_dir()) {
            for e in rd.flatten() {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
        let mut limits = ExecLimits::hosted();
        limits.max_memory_mb = cfg.exec_memory_mb;
        limits.max_cpu_secs = cfg.exec_cpu_secs;
        limits.max_timeout_secs = cfg.max_job_seconds;
        limits.sandbox = cfg.sandbox;
        let engine_cfg = EngineConfig {
            limits,
            fs: if cfg.fs_roots.is_empty() {
                FsPolicy::deny_all()
            } else {
                FsPolicy::roots(cfg.fs_roots.clone())
            },
            net: if cfg.allow_private_network {
                NetPolicy::default()
            } else {
                NetPolicy::public_only()
            },
            temp_root: cfg.tmp_dir(),
            models_dir: cfg.models_dir(),
            env: Environment::Server,
        };
        let engine = Arc::new(Engine::new(
            registry,
            engine_cfg,
            vec![cfg.data_dir.join("bin")],
        ));
        let storage = Arc::new(Storage::new(
            cfg.files_dir(),
            db.clone(),
            cfg.file_retention_hours,
        )?);
        let jobs = Arc::new(JobManager::new(
            db.clone(),
            engine.clone(),
            storage.clone(),
            cfg.jobs_dir(),
            cfg.max_concurrent_jobs,
            cfg.per_user_concurrency,
            !cfg.fs_roots.is_empty(),
            Duration::from_secs(cfg.max_job_seconds),
        ));
        if cfg.auth == AuthMode::None && db.user(LOCAL_USER_ID).is_none() {
            db.create_user(
                LOCAL_USER_ID,
                "local",
                None,
                Role::Owner,
                &Policy::for_role(Role::Owner),
                "Implicit user (authentication disabled)",
            )?;
        }
        let setup_token = if cfg.auth == AuthMode::Required && db.user_count() == 0 {
            Some(
                cfg.setup_token
                    .clone()
                    .unwrap_or_else(|| crate::auth::random_token(16)),
            )
        } else {
            None
        };
        Ok(AppState {
            cfg: Arc::new(cfg),
            db,
            engine,
            storage,
            jobs,
            login_limiter: Arc::new(RateLimiter::new(10, Duration::from_secs(300))),
            setup_token: Arc::new(Mutex::new(setup_token)),
            mcp_sessions: Arc::new(Mutex::new(HashMap::new())),
            extensions: Arc::new(extensions),
            downloads: Arc::new(Mutex::new(HashMap::new())),
            started: Instant::now(),
        })
    }

    pub fn host_fs_enabled(&self) -> bool {
        !self.cfg.fs_roots.is_empty()
    }

    pub fn setup_required(&self) -> bool {
        self.setup_token.lock().unwrap().is_some()
    }

    /// Absolute URL for a path, when a public URL is configured.
    pub fn absolute(&self, path: &str) -> String {
        match &self.cfg.public_url {
            Some(base) => format!("{base}{path}"),
            None => path.to_string(),
        }
    }
}
