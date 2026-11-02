//! Test harness for tool crates: run a tool through the real engine with
//! local paths, full permissions and temporary directories.

use crate::engine::{Engine, EngineConfig, RunRequest};
use crate::error::ToolResult;
use crate::params::LocalFileResolver;
use crate::policy::PermissionSet;
use crate::registry::Registry;
use crate::tool::{Environment, Tool, ToolOutput};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct TestHarness {
    pub engine: Engine,
    pub dir: tempfile::TempDir,
}

impl TestHarness {
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Self {
        let mut reg = Registry::new();
        reg.register_all(tools);
        let dir = tempfile::tempdir().expect("tempdir");
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.temp_root = dir.path().join("tmp");
        if let Some(m) = std::env::var_os("DOGUBAKO_TEST_MODELS") {
            cfg.models_dir = PathBuf::from(m);
        }
        TestHarness { engine: Engine::new(reg, cfg, vec![]), dir }
    }

    /// Path for a fixture file inside the harness directory.
    pub fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// True if all deps of a tool are present (tests skip otherwise).
    pub fn available(&self, id: &str) -> bool {
        let spec = self.engine.registry.get(id).expect("tool registered").spec().clone();
        let a = self.engine.availability(&spec);
        if !a.available {
            eprintln!("skipping {id}: {:?}", a.reason);
        }
        a.available
    }

    pub async fn run(&self, id: &str, params: Value) -> ToolResult<ToolOutput> {
        let out_dir = self.dir.path().join(format!("out-{}", crate::util::random_id(4)));
        let map: Map<String, Value> = params.as_object().cloned().unwrap_or_default();
        let resolver = LocalFileResolver::unrestricted();
        self.engine
            .run(RunRequest {
                tool_id: id,
                params: map,
                resolver: &resolver,
                out_dir,
                permissions: PermissionSet::all(),
                authorize: None,
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await
            .map(|(o, _)| o)
    }
}

pub fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}
