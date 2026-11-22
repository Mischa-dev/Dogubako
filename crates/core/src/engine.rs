//! The execution engine: the single code path through which every interface
//! (desktop, web, CLI, HTTP API, MCP) runs tools.
//!
//! It enforces, in order: tool lookup, caller authorization, platform
//! support, permission grants, dependency availability, parameter
//! validation, isolated scratch directories, timeouts and cancellation.

use crate::deps::{DepRegistry, Unmet};
use crate::error::{ToolError, ToolResult};
use crate::models::ModelRegistry;
use crate::packs::{PackEngine, PackInfo, PackSelection, UnusedDownload};
use crate::params::{validate, FileResolver, Params};
use crate::policy::{ExecLimits, FsPolicy, NetPolicy, PermissionSet};
use crate::registry::{fingerprint, Registry};
use crate::spec::{ParamKind, Requirement, ToolSpec};
use crate::tool::{ContextBuilder, Environment, ProgressFn, Tool, ToolContext, ToolOutput};

/// Upper bound for a tool's pre-run [`Tool::check`].
const CHECK_TIMEOUT_SECS: u64 = 30;
use crate::manifest::ExtensionStatus;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub limits: ExecLimits,
    pub fs: FsPolicy,
    pub net: NetPolicy,
    /// Where per-run scratch directories are created.
    pub temp_root: PathBuf,
    pub models_dir: PathBuf,
    pub env: Environment,
    /// Executable that handles `__dogubako-worker <name>` (normally the host
    /// binary itself); see [`crate::tool::ToolContext::worker`].
    pub worker_exe: Option<PathBuf>,
    /// Built-in packs turned on; tools of other packs are hidden.
    pub packs: PackSelection,
}

impl EngineConfig {
    pub fn local(env: Environment) -> Self {
        let dirs = crate::paths::AppDirs::default_local();
        EngineConfig {
            limits: ExecLimits::default(),
            fs: FsPolicy::unrestricted(),
            net: NetPolicy::default(),
            temp_root: std::env::temp_dir(),
            models_dir: dirs.models,
            env,
            worker_exe: None,
            packs: PackSelection::all(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingDep {
    pub id: String,
    pub name: String,
    pub hint: String,
    /// Set when the app can download this program itself: the engine id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_bytes: Option<u64>,
}

/// The model to offer when a tool's model group has none installed.
#[derive(Debug, Clone, Serialize)]
pub struct ModelOffer {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub available: bool,
    pub platform_supported: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<MissingDep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_model: Option<String>,
    /// Required extension settings that are not set yet.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unset_settings: Vec<String>,
    /// With `missing_model`: the model group the tool needs and the one to install.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_model: Option<ModelOffer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Spec plus host availability, as listed to clients.
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    #[serde(flatten)]
    pub spec: ToolSpec,
    pub availability: Availability,
}

pub struct RunRequest<'a> {
    pub tool_id: &'a str,
    pub params: Map<String, Value>,
    pub resolver: &'a dyn FileResolver,
    /// Directory receiving output artifacts (must exist).
    pub out_dir: PathBuf,
    pub permissions: PermissionSet,
    /// Caller-specific authorization (per-tool / per-category ACLs).
    pub authorize: Option<&'a (dyn Fn(&ToolSpec) -> bool + Send + Sync)>,
    pub progress: Option<ProgressFn>,
    pub cancel: CancellationToken,
    /// Optional override of the timeout (capped by the tool & limits).
    pub timeout: Option<Duration>,
    /// Optional per-caller filesystem policy override (e.g. stricter roots).
    pub fs: Option<FsPolicy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunStats {
    pub duration_ms: u64,
}

/// What a tool with unset required settings tells the user.
pub fn setup_message(tool_name: &str) -> String {
    format!("Set up {tool_name} in Settings")
}

/// Builds a fresh registry (builtins, trusted extensions, enabled packs) for
/// [`Engine::reload`].
pub type RegistryLoader = Arc<dyn Fn() -> (Registry, Vec<ExtensionStatus>) + Send + Sync>;

/// What a registry reload changed, by tool id.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct ReloadReport {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Same id, different code or manifest.
    pub changed: Vec<String>,
    /// Tools registered after the reload.
    pub tools: usize,
    /// Bumped by every reload that changed the tool set.
    pub generation: u64,
}

impl ReloadReport {
    pub fn tool_set_changed(&self) -> bool {
        !(self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty())
    }
}

/// One consistent view of the tools, swapped whole by a reload.
#[derive(Clone)]
pub struct Registries {
    /// Installed tools: those whose pack is turned on. Every interface lists
    /// and runs tools from here.
    pub installed: Arc<Registry>,
    /// Every registered tool, including those of turned-off packs.
    pub catalog: Arc<Registry>,
}

impl Registries {
    /// An installed tool by id or MCP name; a known tool of a turned-off pack
    /// is [`ToolError::NotInstalled`].
    pub fn find(&self, id: &str) -> ToolResult<Arc<dyn Tool>> {
        if let Some(t) = self.installed.find(id) {
            return Ok(t.clone());
        }
        Err(self
            .not_installed(id)
            .unwrap_or_else(|| ToolError::NotFound { id: id.to_string() }))
    }

    /// The error for a tool that exists but whose pack is turned off.
    pub fn not_installed(&self, id: &str) -> Option<ToolError> {
        if self.installed.find(id).is_some() {
            return None;
        }
        self.catalog
            .find(id)
            .map(|t| ToolError::not_installed(&t.spec().id, &t.spec().pack))
    }
}

pub struct Engine {
    tools: RwLock<Registries>,
    extensions: RwLock<Arc<Vec<ExtensionStatus>>>,
    loader: RwLock<Option<RegistryLoader>>,
    /// Serializes reloads so two cannot interleave their diff and swap.
    reloading: Mutex<()>,
    generation: tokio::sync::watch::Sender<u64>,
    /// Packs turned on now; starts as `config.packs`, changed by
    /// [`Engine::set_packs`].
    packs: RwLock<PackSelection>,
    pack_edit: Mutex<()>,
    pub deps: Arc<DepRegistry>,
    pub models: Arc<ModelRegistry>,
    pub config: EngineConfig,
}

impl Engine {
    pub fn new(registry: Registry, config: EngineConfig, extra_bin_dirs: Vec<PathBuf>) -> Self {
        Self::with_deps(registry, config, DepRegistry::new(extra_bin_dirs))
    }

    /// With a prepared dependency registry, e.g. one that uses the app's own
    /// engines ([`DepRegistry::with_managed`]).
    pub fn with_deps(registry: Registry, config: EngineConfig, deps: DepRegistry) -> Self {
        let installed = registry.filtered(|s| config.packs.is_on(&s.pack));
        Engine {
            tools: RwLock::new(Registries {
                installed: Arc::new(installed),
                catalog: Arc::new(registry),
            }),
            extensions: RwLock::new(Arc::new(vec![])),
            loader: RwLock::new(None),
            reloading: Mutex::new(()),
            generation: tokio::sync::watch::Sender::new(0),
            packs: RwLock::new(config.packs.clone()),
            pack_edit: Mutex::new(()),
            deps: Arc::new(deps),
            models: Arc::new(ModelRegistry::new(config.models_dir.clone())),
            config,
        }
    }

    /// Build the registry with `loader` now, and again on every [`Engine::reload`].
    pub fn from_loader(config: EngineConfig, deps: DepRegistry, loader: RegistryLoader) -> Self {
        let (registry, extensions) = loader();
        let engine = Self::with_deps(registry, config, deps).with_extensions(extensions);
        engine.set_loader(loader);
        engine
    }

    /// The extension statuses that came with the initial registry.
    pub fn with_extensions(mut self, extensions: Vec<ExtensionStatus>) -> Self {
        *self.extensions.get_mut().unwrap() = Arc::new(extensions);
        self
    }

    pub fn set_loader(&self, loader: RegistryLoader) {
        *self.loader.write().unwrap() = Some(loader);
    }

    /// The current tool set. Hold the returned snapshot for the length of one
    /// request; a reload swaps in a new one without affecting it.
    pub fn registry(&self) -> Arc<Registry> {
        self.tools.read().unwrap().installed.clone()
    }

    /// Every registered tool, including those of turned-off packs.
    pub fn catalog(&self) -> Arc<Registry> {
        self.tools.read().unwrap().catalog.clone()
    }

    /// Installed and catalog tools from the same load; use it when a request
    /// reads both, so a reload in between cannot mix two versions.
    pub fn snapshot(&self) -> Registries {
        self.tools.read().unwrap().clone()
    }

    /// Extension manifests found by the last load, trusted or not.
    pub fn extensions(&self) -> Arc<Vec<ExtensionStatus>> {
        self.extensions.read().unwrap().clone()
    }

    /// Bumped every time a reload changes the tool set.
    pub fn generation(&self) -> u64 {
        *self.generation.borrow()
    }

    /// Notified with the new [`Engine::generation`] when the tool set changes.
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.generation.subscribe()
    }

    /// Rebuild the registry with the loader and swap it in. Runs already
    /// started keep the tool they started with. Without a loader the
    /// registry stays as it is.
    pub fn reload(&self) -> ReloadReport {
        let loader = self.loader.read().unwrap().clone();
        match loader {
            Some(load) => {
                let _guard = self.reloading.lock().unwrap();
                let (registry, extensions) = load();
                self.swap(registry, extensions)
            }
            None => ReloadReport {
                tools: self.registry().len(),
                generation: self.generation(),
                ..Default::default()
            },
        }
    }

    /// Swap in an already built registry (embedders, tests).
    pub fn replace_registry(
        &self,
        registry: Registry,
        extensions: Vec<ExtensionStatus>,
    ) -> ReloadReport {
        let _guard = self.reloading.lock().unwrap();
        self.swap(registry, extensions)
    }

    fn swap(&self, catalog: Registry, extensions: Vec<ExtensionStatus>) -> ReloadReport {
        let packs = self.pack_selection();
        let registry = catalog.filtered(|s| packs.is_on(&s.pack));
        let old = self.registry();
        let before: HashMap<&str, &Arc<dyn Tool>> =
            old.iter().map(|t| (t.spec().id.as_str(), t)).collect();
        let mut report = ReloadReport {
            tools: registry.len(),
            ..Default::default()
        };
        for t in registry.iter() {
            let id = t.spec().id.as_str();
            match before.get(id) {
                None => report.added.push(id.to_string()),
                Some(prev) => {
                    if fingerprint(prev.as_ref()) != fingerprint(t.as_ref()) {
                        report.changed.push(id.to_string());
                    }
                }
            }
        }
        report.removed = before
            .keys()
            .filter(|id| registry.get(id).is_none())
            .map(|id| id.to_string())
            .collect();
        report.added.sort();
        report.removed.sort();
        report.changed.sort();
        *self.tools.write().unwrap() = Registries {
            installed: Arc::new(registry),
            catalog: Arc::new(catalog),
        };
        *self.extensions.write().unwrap() = Arc::new(extensions);
        if report.tool_set_changed() {
            self.generation.send_modify(|g| *g += 1);
        }
        report.generation = self.generation();
        report
    }

    pub fn availability(&self, spec: &ToolSpec) -> Availability {
        let unset_settings = self
            .registry()
            .get(&spec.id)
            .map(|t| t.unset_settings())
            .unwrap_or_default();
        self.availability_with(spec, unset_settings)
    }

    /// [`Engine::availability`] with the unset settings of a specific tool
    /// version, e.g. the one a job resolved before a reload.
    fn availability_with(&self, spec: &ToolSpec, unset_settings: Vec<String>) -> Availability {
        let platform_supported = spec.supports_current_platform();
        let missing: Vec<MissingDep> = spec
            .requires
            .iter()
            .map(|d| self.deps.status(d))
            .filter(|s| !s.found)
            .map(|s| MissingDep {
                id: s.id,
                name: s.name,
                hint: s.install_hint,
                downloadable: s.downloadable,
                download_bytes: s.download_bytes,
            })
            .collect();
        let model_group = spec
            .requires_model
            .clone()
            .filter(|g| !self.models.group_available(g));
        let missing_model = model_group.as_ref().map(|g| {
            format!("No '{g}' model installed. Download one in Settings → Models or with `dogubako models download <id>`.")
        });
        let recommended_model = model_group
            .as_deref()
            .and_then(crate::models::recommended_model)
            .map(|m| ModelOffer {
                id: m.id.to_string(),
                name: m.name.to_string(),
                size_bytes: m.size_bytes,
            });
        let reason = if !platform_supported {
            Some(format!(
                "Not supported on {:?}",
                crate::spec::Platform::current()
            ))
        } else if let Some(m) = missing.first() {
            Some(format!("Requires {}", m.name))
        } else if missing_model.is_some() {
            missing_model.clone()
        } else if !unset_settings.is_empty() {
            Some(setup_message(&spec.name))
        } else {
            None
        };
        Availability {
            available: platform_supported
                && missing.is_empty()
                && missing_model.is_none()
                && unset_settings.is_empty(),
            platform_supported,
            missing,
            missing_model,
            unset_settings,
            model_group,
            recommended_model,
            reason,
        }
    }

    pub fn info(&self, spec: &ToolSpec) -> ToolInfo {
        let mut spec = spec.clone();
        for p in &mut spec.params {
            p.unavailable = p
                .requires
                .as_ref()
                .and_then(|r| self.deps.check(r).err())
                .map(|u| u.message());
            if let ParamKind::Enum { options, .. } = &mut p.kind {
                for o in options {
                    o.unavailable = o
                        .requires
                        .as_ref()
                        .and_then(|r| self.deps.check(r).err())
                        .map(|u| u.message());
                }
            }
        }
        ToolInfo {
            availability: self.availability(&spec),
            spec,
        }
    }

    /// Reject chosen options this install cannot do, before the tool runs.
    fn check_requirements(&self, spec: &ToolSpec, params: &Params) -> ToolResult<()> {
        let unmet = |param: &str, req: &Requirement| match self.deps.check(req) {
            Ok(()) => Ok(()),
            Err(Unmet::MissingDependency {
                dependency, hint, ..
            }) => Err(ToolError::MissingDependency { dependency, hint }),
            Err(Unmet::Unsupported(m)) => Err(ToolError::invalid(param, m)),
        };
        for p in &spec.params {
            let Some(v) = params.get(&p.name) else {
                continue;
            };
            if let Some(cond) = &p.show_if {
                let cur = params.get(&cond.param);
                let shown = cond.values.iter().any(|want| match cur {
                    Some(Value::Array(a)) => a.iter().any(|c| loose_eq(c, want)),
                    Some(c) => loose_eq(c, want),
                    None => false,
                });
                if !shown {
                    continue;
                }
            }
            if let Some(req) = &p.requires {
                if !matches!(v, Value::Bool(false)) && v.as_str() != Some("") {
                    unmet(&p.name, req)?;
                }
            }
            if let ParamKind::Enum { options, .. } = &p.kind {
                let chosen: Vec<&str> = match v {
                    Value::Array(a) => a.iter().filter_map(|x| x.as_str()).collect(),
                    other => other.as_str().into_iter().collect(),
                };
                for o in options
                    .iter()
                    .filter(|o| chosen.contains(&o.value.as_str()))
                {
                    if let Some(req) = &o.requires {
                        unmet(&p.name, req)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<ToolInfo> {
        self.registry().specs().map(|s| self.info(s)).collect()
    }

    /// An installed tool by id or MCP name; a known tool of a turned-off pack
    /// is [`ToolError::NotInstalled`].
    pub fn find(&self, id: &str) -> ToolResult<Arc<dyn Tool>> {
        self.snapshot().find(id)
    }

    /// The error for a tool that exists but whose pack is turned off.
    pub fn not_installed(&self, id: &str) -> Option<ToolError> {
        self.snapshot().not_installed(id)
    }

    /// The packs turned on now.
    pub fn pack_selection(&self) -> PackSelection {
        self.packs.read().unwrap().clone()
    }

    /// Turn packs on or off now: re-filters the current catalog and swaps
    /// it in like a reload. The caller saves the selection.
    pub fn set_packs(&self, packs: PackSelection) -> ReloadReport {
        let _guard = self.reloading.lock().unwrap();
        *self.packs.write().unwrap() = packs;
        let catalog = self.catalog().filtered(|_| true);
        let extensions = (*self.extensions()).clone();
        self.swap(catalog, extensions)
    }

    /// Turn one pack on or off as a single step: read the current
    /// selection, `save` the new one, then apply it. Concurrent edits are
    /// serialized so none is lost.
    pub fn edit_pack<E>(
        &self,
        pack: &str,
        on: bool,
        save: impl FnOnce(&PackSelection) -> Result<(), E>,
    ) -> Result<ReloadReport, E> {
        self.edit_packs(|cur| cur.with(pack, on), save)
    }

    /// Like [`Engine::edit_pack`] for any change of the selection.
    pub fn edit_packs<E>(
        &self,
        change: impl FnOnce(&PackSelection) -> PackSelection,
        save: impl FnOnce(&PackSelection) -> Result<(), E>,
    ) -> Result<ReloadReport, E> {
        let _edit = self.pack_edit.lock().unwrap();
        let next = change(&self.pack_selection());
        save(&next)?;
        Ok(self.set_packs(next))
    }

    /// Engines and model groups a tool uses.
    fn uses(spec: &ToolSpec) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut deps: BTreeSet<String> = spec.requires.iter().cloned().collect();
        for r in spec.params.iter().filter_map(|p| p.requires.as_ref()) {
            deps.insert(match r {
                Requirement::MagickWrite { .. } => "imagemagick".to_string(),
                Requirement::FfmpegEncoder { .. } => "ffmpeg".to_string(),
                Requirement::Program { dependency, .. } => dependency.clone(),
            });
        }
        (deps, spec.requires_model.iter().cloned().collect())
    }

    /// Every built-in pack, with its tool count, state, what it uses and
    /// what it can download.
    pub fn packs(&self) -> Vec<PackInfo> {
        let catalog = self.catalog();
        let selection = self.pack_selection();
        let managed = self.deps.managed().list();
        crate::packs::builtin_packs()
            .into_iter()
            .map(|p| {
                let specs: Vec<&ToolSpec> = catalog.specs().filter(|s| s.pack == p.id).collect();
                let mut deps = BTreeSet::new();
                let mut models = BTreeSet::new();
                for s in &specs {
                    let (d, m) = Self::uses(s);
                    deps.extend(d);
                    models.extend(m);
                }
                let mut download_bytes = 0;
                let mut disk_bytes = 0;
                for e in managed
                    .iter()
                    .filter(|e| e.tier == "on_demand" && e.deps.iter().any(|d| deps.contains(d)))
                {
                    download_bytes += e.download_bytes.unwrap_or(0);
                    disk_bytes += e.installed_bytes.unwrap_or(0);
                }
                for m in models
                    .iter()
                    .filter_map(|g| crate::models::recommended_model(g))
                {
                    download_bytes += m.size_bytes;
                    disk_bytes += m.size_bytes;
                }
                PackInfo {
                    id: p.id.to_string(),
                    name: p.name.to_string(),
                    description: p.description.to_string(),
                    enabled: selection.is_on(p.id),
                    tools: specs.len(),
                    engines: deps
                        .into_iter()
                        .map(|id| PackEngine {
                            name: self.deps.status(&id).name,
                            id,
                        })
                        .collect(),
                    models: models.into_iter().collect(),
                    download_bytes,
                    disk_bytes,
                }
            })
            .collect()
    }

    /// Downloaded engines and models that only turned-off tools use: what
    /// turning a pack off can free. Anything a turned-on tool (or an
    /// extension) uses is never listed.
    pub fn unused_downloads(&self) -> Vec<UnusedDownload> {
        let tools = self.snapshot();
        let (mut used_deps, mut used_models) = (BTreeSet::new(), BTreeSet::new());
        for s in tools.installed.specs() {
            let (d, m) = Self::uses(s);
            used_deps.extend(d);
            used_models.extend(m);
        }
        // Format converters other tools fall back to without declaring them.
        if !tools.installed.is_empty() {
            used_deps.extend(SHARED_ENGINES.iter().map(|d| d.to_string()));
        }
        let (mut off_deps, mut off_models) = (BTreeSet::new(), BTreeSet::new());
        for s in tools
            .catalog
            .specs()
            .filter(|s| tools.installed.get(&s.id).is_none())
        {
            let (d, m) = Self::uses(s);
            off_deps.extend(d);
            off_models.extend(m);
        }
        let mut out = vec![];
        for e in self.deps.managed().list() {
            if e.tier == "on_demand"
                && e.installed
                && e.deps.iter().any(|d| off_deps.contains(d))
                && !e.deps.iter().any(|d| used_deps.contains(d))
            {
                let bytes = dir_size(&self.deps.managed().install_root().join(&e.id));
                out.push(UnusedDownload {
                    kind: "engine",
                    name: e
                        .deps
                        .first()
                        .map(|d| self.deps.status(d).name)
                        .unwrap_or_else(|| e.id.clone()),
                    id: e.id,
                    bytes,
                });
            }
        }
        for m in self.models.list() {
            if m.installed
                && off_models.contains(m.spec.group)
                && !used_models.contains(m.spec.group)
            {
                let bytes = model_bytes(self.models.dir(), &m.spec);
                out.push(UnusedDownload {
                    kind: "model",
                    id: m.spec.id.to_string(),
                    name: m.spec.name.to_string(),
                    bytes,
                });
            }
        }
        out
    }

    /// Remove the given unused downloads (all of them when `ids` is
    /// `None`); ids that a turned-on tool still uses are skipped. Returns
    /// what was removed.
    pub fn remove_unused(&self, ids: Option<&[String]>) -> ToolResult<Vec<UnusedDownload>> {
        let mut removed = vec![];
        for u in self.unused_downloads() {
            if ids.is_some_and(|ids| !ids.contains(&u.id)) {
                continue;
            }
            let path = self.deps.managed().install_root().join(&u.id);
            let left = match u.kind {
                "engine" => {
                    self.deps.managed().remove(&u.id)?;
                    dir_size(&path)
                }
                _ => {
                    self.models.remove(&u.id)?;
                    model_spec(&u.id).map_or(0, |m| model_bytes(self.models.dir(), m))
                }
            };
            removed.push(UnusedDownload {
                bytes: u.bytes.saturating_sub(left),
                ..u
            });
        }
        if removed.iter().any(|u| u.kind == "engine") {
            self.deps.refresh();
        }
        Ok(removed)
    }

    /// Checks shared by [`Engine::run`] and [`Engine::check`]: the tool,
    /// authorization, platform, permissions, dependencies and params.
    fn prepare(&self, req: &RunRequest<'_>) -> ToolResult<(Arc<dyn Tool>, Params, FsPolicy)> {
        let tool = self.find(req.tool_id)?;
        self.prepare_tool(tool, req)
    }

    fn prepare_tool(
        &self,
        tool: Arc<dyn Tool>,
        req: &RunRequest<'_>,
    ) -> ToolResult<(Arc<dyn Tool>, Params, FsPolicy)> {
        let spec = tool.spec().clone();

        if let Some(auth) = req.authorize {
            if !auth(&spec) {
                return Err(ToolError::denied(format!(
                    "not authorized to use '{}'",
                    spec.id
                )));
            }
        }
        if !spec.supports_current_platform() {
            return Err(ToolError::unsupported(format!(
                "'{}' is not supported on this platform",
                spec.id
            )));
        }
        let missing_perms = req.permissions.missing(&spec.permissions);
        if !missing_perms.is_empty() {
            let names: Vec<&str> = missing_perms.iter().map(|p| p.id()).collect();
            return Err(ToolError::denied(format!(
                "'{}' requires permission(s): {}",
                spec.id,
                names.join(", ")
            )));
        }
        let avail = self.availability_with(&spec, tool.unset_settings());
        if let Some(m) = avail.missing.first() {
            return Err(ToolError::MissingDependency {
                dependency: m.id.clone(),
                hint: m.hint.clone(),
            });
        }
        if let Some(m) = avail.missing_model {
            return Err(ToolError::MissingModel {
                message: m,
                model: avail.recommended_model.map(|r| r.id),
            });
        }
        if !avail.unset_settings.is_empty() {
            return Err(ToolError::NotConfigured {
                message: format!(
                    "{} (missing: {})",
                    setup_message(&spec.name),
                    avail.unset_settings.join(", ")
                ),
            });
        }

        let fs = req.fs.clone().unwrap_or_else(|| self.config.fs.clone());
        let params = validate(&spec, &req.params, req.resolver, &fs)?;
        self.check_requirements(&spec, &params)?;
        Ok((tool, params, fs))
    }

    fn context(
        &self,
        req: &RunRequest<'_>,
        work_dir: PathBuf,
        out_dir: PathBuf,
        fs: FsPolicy,
        params: &Params,
    ) -> ToolContext {
        ContextBuilder {
            work_dir,
            out_dir,
            models_dir: self.config.models_dir.clone(),
            cancel: req.cancel.clone(),
            progress: req.progress.clone(),
            deps: self.deps.clone(),
            models: self.models.clone(),
            limits: self.config.limits.clone(),
            fs,
            net: self.config.net.clone(),
            env: self.config.env,
            permissions: req.permissions.clone(),
            input_paths: params.all_files().map(|f| f.path.clone()).collect(),
            worker_exe: self.config.worker_exe.clone(),
        }
        .build()
    }

    /// Check a tool's inputs before running it: everything [`Engine::run`]
    /// rejects up front, plus the tool's own [`Tool::check`]. Writes nothing
    /// but a scratch directory, removed afterwards.
    pub async fn check(&self, req: RunRequest<'_>) -> ToolResult<()> {
        let (tool, params, fs) = self.prepare(&req)?;
        if !tool.spec().prechecks {
            return Ok(());
        }
        std::fs::create_dir_all(&self.config.temp_root)?;
        let work = tempfile::Builder::new()
            .prefix("dgb-check-")
            .tempdir_in(&self.config.temp_root)?;
        let dir = work.path().to_path_buf();
        let ctx = self.context(&req, dir.clone(), dir, fs, &params);
        let timeout =
            Duration::from_secs(CHECK_TIMEOUT_SECS).min(req.timeout.unwrap_or(Duration::MAX));
        tokio::select! {
            biased;
            _ = req.cancel.cancelled() => Err(ToolError::Cancelled),
            r = tool.check(ctx, params) => r,
            _ = tokio::time::sleep(timeout) => Err(ToolError::Timeout { seconds: timeout.as_secs() }),
        }
    }

    /// Run a tool. All checks happen here, regardless of the interface.
    pub async fn run(&self, req: RunRequest<'_>) -> ToolResult<(ToolOutput, RunStats)> {
        let tool = self.find(req.tool_id)?;
        self.run_tool(tool, req).await
    }

    /// Run a tool looked up earlier (e.g. when a job was queued), so a reload
    /// in between cannot swap it for another version. All of [`Engine::run`]'s
    /// checks still apply.
    pub async fn run_tool(
        &self,
        tool: Arc<dyn Tool>,
        req: RunRequest<'_>,
    ) -> ToolResult<(ToolOutput, RunStats)> {
        let started = Instant::now();
        let (tool, params, fs) = self.prepare_tool(tool, &req)?;
        let spec = tool.spec().clone();

        std::fs::create_dir_all(&self.config.temp_root)?;
        let work = tempfile::Builder::new()
            .prefix("dgb-run-")
            .tempdir_in(&self.config.temp_root)?;
        std::fs::create_dir_all(&req.out_dir)?;
        let out_dir = std::fs::canonicalize(&req.out_dir)?;
        let ctx = self.context(
            &req,
            work.path().to_path_buf(),
            out_dir.clone(),
            fs,
            &params,
        );

        let mut timeout =
            Duration::from_secs(spec.timeout_secs.min(self.config.limits.max_timeout_secs));
        if let Some(t) = req.timeout {
            timeout = timeout.min(t);
        }
        let cancel = req.cancel.clone();
        let blocking = ctx.inner.blocking.clone();
        let mut fut = Box::pin(tool.run(ctx, params));
        // Cancellation is checked first, so a run cancelled before it
        // started never does its work (even a tool that finishes on its
        // first poll).
        let interrupted = tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(ToolError::Cancelled),
            r = &mut fut => Ok(r),
            _ = tokio::time::sleep(timeout) => Err(ToolError::Timeout { seconds: timeout.as_secs() }),
        };
        let result = match interrupted {
            Ok(r) => r,
            Err(e) => {
                // Signal the tool and drop its future (this stops async work
                // and kills child processes), then wait for blocking workers,
                // which cannot be aborted. Until they finish, the scratch
                // directory and the caller's concurrency slot stay in use.
                cancel.cancel();
                drop(fut);
                blocking.close();
                blocking.wait().await;
                Err(e)
            }
        };
        let mut output = result?;

        // Artifacts must live inside the output directory.
        for a in &mut output.artifacts {
            let canon = std::fs::canonicalize(&a.path)
                .map_err(|e| ToolError::failed(format!("artifact '{}' missing: {e}", a.name)))?;
            if !canon.starts_with(&out_dir) {
                return Err(ToolError::failed(format!(
                    "tool returned an artifact outside its output directory: {}",
                    a.name
                )));
            }
            a.path = canon;
            a.size = std::fs::metadata(&a.path)
                .map(|m| m.len())
                .unwrap_or(a.size);
        }
        if output.data.is_null() {
            output.data = Value::Object(Map::new());
        }
        drop(work);
        Ok((
            output,
            RunStats {
                duration_ms: started.elapsed().as_millis() as u64,
            },
        ))
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    a == b
        || match (a, b) {
            (Value::String(x), y) | (y, Value::String(x)) => match y {
                Value::Number(n) => x.parse::<f64>().ok() == n.as_f64(),
                Value::Bool(v) => x.parse::<bool>().ok() == Some(*v),
                _ => false,
            },
            _ => false,
        }
}

/// Engines tools may use to read or write formats without listing them in
/// `requires`; kept while any tool is on.
use crate::models::model_spec;

const SHARED_ENGINES: &[&str] = &["imagemagick", "ffmpeg"];

fn dir_size(p: &std::path::Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p)
            .map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

fn model_bytes(dir: &std::path::Path, spec: &crate::models::ModelSpec) -> u64 {
    std::iter::once(spec.file_name)
        .chain(spec.extra.iter().map(|(_, f)| *f))
        .map(|f| dir_size(&dir.join(f)))
        .sum()
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::testing::TestHarness;

    static STUBBORN_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    fn tools() -> Vec<Arc<dyn Tool>> {
        vec![
            async_tool(
                ToolSpec::new("dev.slow_async", "Slow async", Category::Dev)
                    .describe("async work that never checks cancellation")
                    .timeout(1),
                |_c, _p| async move {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    Ok(ToolOutput::text("never"))
                },
            ),
            sync_tool(
                ToolSpec::new("dev.stubborn", "Stubborn", Category::Dev)
                    .describe("blocking work that ignores cancellation")
                    .timeout(1),
                |_c, _p| {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    STUBBORN_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(ToolOutput::text("late"))
                },
            ),
            async_tool(
                ToolSpec::new("dev.slow", "Slow", Category::Dev)
                    .describe("sleeps")
                    .timeout(1),
                |ctx, _p| async move {
                    #[cfg(unix)]
                    ctx.command_path("sleep".into(), "sleep")
                        .arg("30")
                        .run()
                        .await?;
                    #[cfg(not(unix))]
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    let _ = ctx;
                    Ok(ToolOutput::text("never"))
                },
            ),
            sync_tool(
                ToolSpec::new("dev.escape", "Escape", Category::Dev)
                    .describe("returns artifact outside out dir"),
                |ctx, _p| {
                    let p = ctx.work_dir().join("x.txt");
                    std::fs::write(&p, "x")?;
                    Ok(ToolOutput::files(vec![ctx.artifact(&p)?]))
                },
            ),
            sync_tool(
                ToolSpec::new("network.fake", "Net", Category::Network)
                    .describe("needs network")
                    .permission(Permission::Network),
                |_c, _p| Ok(ToolOutput::text("ok")),
            ),
            sync_tool(
                ToolSpec::new("video.fake", "Needs dep", Category::Video)
                    .describe("needs a missing dep")
                    .requires(&["definitely-not-a-dep"]),
                |_c, _p| Ok(ToolOutput::text("ok")),
            ),
            sync_tool(
                ToolSpec::new("dev.caps", "Caps", Category::Dev)
                    .describe("options that need missing engines")
                    .param(
                        ParamSpec::choice_of("mode", &["a", "b"])
                            .default_str("a")
                            .option_needs("b", Requirement::program("definitely-not-a-dep", "x")),
                    )
                    .param(
                        ParamSpec::boolean("extra")
                            .show_if("mode", &["a"])
                            .needs(Requirement::program("definitely-not-a-dep", "y")),
                    ),
                |_c, p| Ok(ToolOutput::text(p.str("mode")?)),
            ),
            sync_tool(
                ToolSpec::new("dev.capped", "Capped", Category::Dev)
                    .describe("per-mode maximum")
                    .param(ParamSpec::choice_of("mode", &["a", "b"]).default_str("a"))
                    .param(ParamSpec::integer("q").range(0.0, 9.0))
                    .param(
                        ParamSpec::integer("n")
                            .range(1.0, 640.0)
                            .max_if("mode", &["a"], 320.0)
                            .max_unless_set("q"),
                    ),
                |_c, p| Ok(ToolOutput::data(json!({"n": p.opt_int("n")}))),
            ),
            sync_tool(
                ToolSpec::new("dev.types", "Types", Category::Dev)
                    .describe("coercion")
                    .param(ParamSpec::integer("n").range(1.0, 10.0).required())
                    .param(ParamSpec::boolean("flag"))
                    .param(ParamSpec::choice_of("mode", &["a", "b"]).default_str("a"))
                    .param(ParamSpec::color("c").default_str("#abc")),
                |_c, p| {
                    Ok(ToolOutput::data(
                        json!({"n": p.int("n")?, "flag": p.bool("flag"), "mode": p.str("mode")?, "c": p.str("c")?}),
                    ))
                },
            ),
        ]
    }

    #[tokio::test]
    async fn options_needing_missing_engines_are_marked_and_rejected() {
        let h = TestHarness::new(tools());
        let spec = h.engine.registry().get("dev.caps").unwrap().spec().clone();
        let info = h.engine.info(&spec);
        assert!(info.availability.available);
        let mode = info.spec.params.iter().find(|p| p.name == "mode").unwrap();
        let crate::spec::ParamKind::Enum { options, .. } = &mode.kind else {
            panic!("mode is an enum");
        };
        assert_eq!(options[0].unavailable, None);
        assert_eq!(
            options[1].unavailable.as_deref(),
            Some("Requires definitely-not-a-dep")
        );
        let extra = info.spec.params.iter().find(|p| p.name == "extra").unwrap();
        assert!(extra.unavailable.is_some());

        let ok = h.run("dev.caps", json!({"mode": "a"})).await.unwrap();
        assert_eq!(ok.text.as_deref(), Some("a"));
        let e = h.run("dev.caps", json!({"mode": "b"})).await.unwrap_err();
        assert_eq!(e.code(), "missing_dependency");
        let e = h
            .run("dev.caps", json!({"mode": "a", "extra": true}))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "missing_dependency");
        // Turned off, or hidden by show_if, it doesn't block the run.
        h.run("dev.caps", json!({"mode": "a", "extra": false}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn max_if_caps_a_param_while_another_has_a_value() {
        let h = TestHarness::new(tools());
        let e = h.run("dev.capped", json!({"n": 321})).await.unwrap_err();
        assert_eq!(e.code(), "invalid_input");
        assert!(e.to_string().contains("at most 320 when mode is a"), "{e}");
        let e = h
            .run("dev.capped", json!({"mode": "a", "n": "640"}))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "invalid_input");
        h.run("dev.capped", json!({"n": 320})).await.unwrap();
        h.run("dev.capped", json!({"mode": "b", "n": 640}))
            .await
            .unwrap();
        let e = h
            .run("dev.capped", json!({"mode": "b", "n": 641}))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("at most 640"), "{e}");
        h.run("dev.capped", json!({})).await.unwrap();
        // Setting the lifting param removes the cap; null doesn't count as set.
        h.run("dev.capped", json!({"n": 640, "q": 2}))
            .await
            .unwrap();
        let e = h
            .run("dev.capped", json!({"n": 640, "q": null}))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("unless q is set"), "{e}");
    }

    #[tokio::test]
    async fn run_cancelled_before_start_does_not_run() {
        let h = TestHarness::new(tools());
        let resolver = crate::params::LocalFileResolver::unrestricted();
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        for _ in 0..20 {
            let r = h
                .engine
                .run(crate::engine::RunRequest {
                    tool_id: "network.fake",
                    params: Default::default(),
                    resolver: &resolver,
                    out_dir: h.path("o"),
                    permissions: crate::policy::PermissionSet::all(),
                    authorize: None,
                    progress: None,
                    cancel: cancel.clone(),
                    timeout: None,
                    fs: None,
                })
                .await;
            assert_eq!(r.unwrap_err().code(), "cancelled");
        }
    }

    #[tokio::test]
    async fn timeout_kills_tool() {
        let h = TestHarness::new(tools());
        let t = std::time::Instant::now();
        let e = h.run("dev.slow", json!({})).await.unwrap_err();
        assert_eq!(e.code(), "timeout");
        assert!(t.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn interrupted_async_work_is_dropped() {
        let h = TestHarness::new(tools());
        let t = std::time::Instant::now();
        let e = h.run("dev.slow_async", json!({})).await.unwrap_err();
        assert_eq!(e.code(), "timeout");
        assert!(t.elapsed().as_secs() < 5);
    }

    #[tokio::test]
    async fn interrupted_blocking_work_is_awaited() {
        let h = TestHarness::new(tools());
        let e = h.run("dev.stubborn", json!({})).await.unwrap_err();
        assert_eq!(e.code(), "timeout");
        // The engine only returns once the blocking worker has stopped, so
        // its scratch dir and the caller's concurrency slot stay accounted.
        assert!(STUBBORN_DONE.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn artifacts_must_stay_in_out_dir() {
        let h = TestHarness::new(tools());
        let e = h.run("dev.escape", json!({})).await.unwrap_err();
        assert!(e.to_string().contains("outside"));
    }

    #[tokio::test]
    async fn permissions_and_dependencies_enforced() {
        let h = TestHarness::new(tools());
        let resolver = crate::params::LocalFileResolver::unrestricted();
        let r = h
            .engine
            .run(crate::engine::RunRequest {
                tool_id: "network.fake",
                params: Default::default(),
                resolver: &resolver,
                out_dir: h.path("o"),
                permissions: crate::policy::PermissionSet::none(),
                authorize: None,
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await;
        assert_eq!(r.unwrap_err().code(), "permission_denied");
        let deny = |_: &ToolSpec| false;
        let r = h
            .engine
            .run(crate::engine::RunRequest {
                tool_id: "dev.types",
                params: Default::default(),
                resolver: &resolver,
                out_dir: h.path("o"),
                permissions: crate::policy::PermissionSet::all(),
                authorize: Some(&deny),
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await;
        assert_eq!(r.unwrap_err().code(), "permission_denied");
        assert_eq!(
            h.run("video.fake", json!({})).await.unwrap_err().code(),
            "missing_dependency"
        );
        assert_eq!(
            h.run("nope.nope", json!({})).await.unwrap_err().code(),
            "not_found"
        );
    }

    #[test]
    fn missing_model_offers_the_recommended_one() {
        use super::{Engine, EngineConfig};
        use crate::registry::Registry;
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.models_dir = dir.path().to_path_buf();
        let engine = Engine::new(Registry::new(), cfg, vec![]);
        let spec = ToolSpec::new("speech.fake", "Fake", Category::Audio)
            .describe("needs a whisper model")
            .requires(&["definitely-not-a-dep"])
            .requires_model("whisper");
        let a = engine.availability(&spec);
        assert!(!a.available);
        assert_eq!(a.model_group.as_deref(), Some("whisper"));
        let m = a.recommended_model.expect("recommended model");
        assert_eq!(m.id, "whisper-base");
        assert_eq!(m.size_bytes, 147_951_465);
        assert_eq!(a.missing[0].downloadable, None);
        let v = serde_json::to_value(engine.info(&spec).availability).unwrap();
        assert_eq!(v["recommended_model"]["name"], "Whisper base");
        assert!(v["missing"][0].get("downloadable").is_none());

        let e = ToolError::MissingModel {
            message: "x".into(),
            model: Some(m.id.clone()),
        };
        assert_eq!(serde_json::to_value(&e).unwrap()["model"], "whisper-base");

        let ok = ToolSpec::new("text.fake", "Fine", Category::Text).describe("needs nothing");
        let v = serde_json::to_value(engine.availability(&ok)).unwrap();
        assert!(v.get("model_group").is_none() && v.get("recommended_model").is_none());
    }

    #[tokio::test]
    async fn validation_and_coercion() {
        let h = TestHarness::new(tools());
        let o = h
            .run("dev.types", json!({"n": "5", "flag": "yes", "mode": "B"}))
            .await
            .unwrap();
        assert_eq!(
            o.data,
            json!({"n": 5, "flag": true, "mode": "b", "c": "#aabbcc"})
        );
        for bad in [
            json!({"n": 11}),
            json!({"n": "x"}),
            json!({}),
            json!({"n": 1, "mode": "z"}),
            json!({"n": 1, "c": "red"}),
            json!({"n": 1, "extra": 1}),
        ] {
            assert_eq!(
                h.run("dev.types", bad.clone()).await.unwrap_err().code(),
                "invalid_input",
                "{bad}"
            );
        }
        let json_schema = crate::schema::input_schema(
            h.engine.registry().get("dev.types").unwrap().spec(),
            "path",
        );
        assert_eq!(json_schema["required"], json!(["n"]));
        assert_eq!(json_schema["properties"]["mode"]["enum"], json!(["a", "b"]));
    }

    fn manifest(id: &str, description: &str) -> String {
        format!(
            "[tool]\nid = \"{id}\"\nname = \"{id}\"\ncategory = \"text\"\ndescription = \"{description}\"\n[command]\nprogram = \"echo\"\nargs = [\"hi\"]\n[output]\nstdout = \"text/plain\"\n"
        )
    }

    fn loader_for(dir: std::path::PathBuf) -> crate::engine::RegistryLoader {
        Arc::new(move || {
            let mut reg = crate::registry::Registry::new();
            reg.register_all(tools());
            let (ext, st) = crate::manifest::load_extensions(&dir, true);
            reg.register_all(ext);
            (reg, st)
        })
    }

    #[test]
    fn reload_reports_added_removed_and_changed_tools() {
        let dir = tempfile::tempdir().unwrap();
        let ext = dir.path().to_path_buf();
        std::fs::write(ext.join("a.toml"), manifest("text.ext_a", "first")).unwrap();
        let engine = crate::engine::Engine::from_loader(
            crate::engine::EngineConfig::local(Environment::Cli),
            crate::deps::DepRegistry::new(vec![]),
            loader_for(ext.clone()),
        );
        let mut changes = engine.subscribe();
        assert!(engine.registry().get("text.ext_a").is_some());
        assert_eq!(engine.extensions().len(), 1);

        let r = engine.reload();
        assert!(!r.tool_set_changed(), "{r:?}");
        assert_eq!(r.generation, 0);
        assert!(!changes.has_changed().unwrap());

        std::fs::write(ext.join("b.toml"), manifest("text.ext_b", "second")).unwrap();
        let r = engine.reload();
        assert_eq!(r.added, vec!["text.ext_b"]);
        assert!(r.removed.is_empty() && r.changed.is_empty());
        assert_eq!(r.generation, 1);
        assert!(changes.has_changed().unwrap());
        assert_eq!(*changes.borrow_and_update(), 1);
        assert!(engine.list().iter().any(|t| t.spec.id == "text.ext_b"));

        std::fs::write(ext.join("a.toml"), manifest("text.ext_a", "edited")).unwrap();
        let r = engine.reload();
        assert_eq!(r.changed, vec!["text.ext_a"]);
        assert!(r.added.is_empty() && r.removed.is_empty());
        assert_eq!(
            engine
                .registry()
                .get("text.ext_a")
                .unwrap()
                .spec()
                .description,
            "edited"
        );

        std::fs::remove_file(ext.join("b.toml")).unwrap();
        let r = engine.reload();
        assert_eq!(r.removed, vec!["text.ext_b"]);
        assert!(engine.registry().get("text.ext_b").is_none());
        assert_eq!(engine.extensions().len(), 1);
        assert_eq!(r.generation, 3);
        assert_eq!(r.tools, engine.registry().len());
    }

    #[test]
    fn reload_without_loader_keeps_registry() {
        let h = TestHarness::new(tools());
        let n = h.engine.registry().len();
        let r = h.engine.reload();
        assert!(!r.tool_set_changed());
        assert_eq!(r.tools, n);
    }

    fn gate(
        text: &'static str,
        started: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    ) -> Arc<dyn Tool> {
        async_tool(
            ToolSpec::new("dev.gate", "Gate", Category::Dev).describe(text),
            move |_c, _p| {
                let (started, release) = (started.clone(), release.clone());
                async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(ToolOutput::text(text))
                }
            },
        )
    }

    #[tokio::test]
    async fn run_started_before_reload_keeps_its_tool() {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let h = TestHarness::new(vec![gate("v1", started.clone(), release.clone())]);
        let (out, report) = tokio::join!(h.run("dev.gate", json!({})), async {
            started.notified().await;
            let mut reg = crate::registry::Registry::new();
            reg.register_all(vec![gate("v2", started.clone(), release.clone())]);
            let report = h.engine.replace_registry(reg, vec![]);
            release.notify_one();
            report
        });
        assert_eq!(report.changed, vec!["dev.gate"]);
        assert_eq!(out.unwrap().text.as_deref(), Some("v1"));
        release.notify_one();
        let out = h.run("dev.gate", json!({})).await.unwrap();
        assert_eq!(out.text.as_deref(), Some("v2"));
    }

    use super::{Engine, EngineConfig, PackSelection, Registry};
    use crate::tool::Environment;

    fn packs_engine(packs: PackSelection) -> Engine {
        let mut reg = Registry::new();
        reg.register_all(vec![
            sync_tool(
                ToolSpec::new("text.upper", "Upper", Category::Text).describe("upper"),
                |_c, _p| Ok(ToolOutput::text("A")),
            ),
            sync_tool(
                ToolSpec::new("image.blur", "Blur", Category::Image)
                    .describe("blur")
                    .requires(&["imagemagick"])
                    .requires_model("upscale"),
                |_c, _p| Ok(ToolOutput::text("b")),
            ),
        ]);
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.packs = packs;
        Engine::new(reg, cfg, vec![])
    }

    #[test]
    fn turned_off_pack_tools_are_available_not_installed() {
        let e = packs_engine(PackSelection::only(["text"]));
        let avail = crate::store::available(&e, None, |_| true);
        assert_eq!(avail.len(), 1);
        assert_eq!(avail[0].id, "image.blur");
        assert_eq!(avail[0].source, "pack");
        assert_eq!(
            avail[0].pack.as_deref(),
            Some(e.catalog().get("image.blur").unwrap().spec().pack.as_str())
        );
        assert_eq!(avail[0].requires, vec!["imagemagick".to_string()]);
        assert!(crate::store::available(&e, None, |s| s.id != "image.blur").is_empty());
        assert!(
            crate::store::available(&packs_engine(PackSelection::all()), None, |_| true).is_empty()
        );
    }

    #[test]
    fn every_pack_on_by_default() {
        let e = packs_engine(PackSelection::default());
        assert_eq!(e.list().len(), 2);
        assert!(e.find("text.upper").is_ok() && e.find("image.blur").is_ok());
        assert!(e.packs().iter().all(|p| p.enabled));
    }

    #[test]
    fn turned_off_pack_hides_and_rejects_tools() {
        let e = packs_engine(PackSelection::only(["image"]));
        let ids: Vec<String> = e.list().into_iter().map(|t| t.spec.id).collect();
        assert_eq!(ids, vec!["image.blur"]);
        assert!(e.registry().search("upper").is_empty());
        assert_eq!(e.registry().by_category(Category::Text).count(), 0);
        let err = e.find("text.upper").err().unwrap();
        assert_eq!(err.code(), "not_installed");
        assert!(err.to_string().contains("Text pack ('text')"), "{err}");
        assert!(matches!(
            e.find("text.nope"),
            Err(ToolError::NotFound { .. })
        ));
        assert!(e.not_installed("image.blur").is_none());
    }

    #[test]
    fn packs_report_counts_engines_and_models() {
        let e = packs_engine(PackSelection::only(["image"]));
        let packs = e.packs();
        assert_eq!(packs.len(), Category::ALL.len());
        let image = packs.iter().find(|p| p.id == "image").unwrap();
        assert!(image.enabled);
        assert_eq!(image.tools, 1);
        assert_eq!(image.engines[0].id, "imagemagick");
        assert_eq!(image.models, vec!["upscale"]);
        let text = packs.iter().find(|p| p.id == "text").unwrap();
        assert!(!text.enabled);
        assert_eq!(text.tools, 1);
    }

    #[test]
    fn reload_keeps_turned_off_packs_hidden() {
        let e = packs_engine(PackSelection::only(["image"]));
        let mut reg = Registry::new();
        reg.register_all(vec![
            sync_tool(
                ToolSpec::new("text.lower", "Lower", Category::Text).describe("lower"),
                |_c, _p| Ok(ToolOutput::text("a")),
            ),
            sync_tool(
                ToolSpec::new("image.sharpen", "Sharpen", Category::Image).describe("sharpen"),
                |_c, _p| Ok(ToolOutput::text("s")),
            ),
        ]);
        let r = e.replace_registry(reg, vec![]);
        assert_eq!(r.added, vec!["image.sharpen"]);
        assert_eq!(r.removed, vec!["image.blur"]);
        assert_eq!(r.tools, 1);
        assert_eq!(
            e.not_installed("text.lower").unwrap().code(),
            "not_installed"
        );
        assert_eq!(e.packs().iter().find(|p| p.id == "text").unwrap().tools, 1);
    }

    #[test]
    fn set_packs_applies_without_restart() {
        let e = packs_engine(PackSelection::all());
        let before = e.generation();
        let r = e.set_packs(PackSelection::only(["image"]));
        assert_eq!(r.removed, vec!["text.upper"]);
        assert!(e.generation() > before);
        assert_eq!(
            e.not_installed("text.upper").unwrap().code(),
            "not_installed"
        );
        assert!(!e.packs().iter().find(|p| p.id == "text").unwrap().enabled);
        let r = e.set_packs(e.pack_selection().with("text", true));
        assert_eq!(r.added, vec!["text.upper"]);
        assert!(e.pack_selection().is_on("text"));
        e.set_packs(PackSelection::all());
        assert_eq!(e.registry().len(), 2);
    }

    fn speech_engine() -> Engine {
        let mut reg = Registry::new();
        reg.register_all(vec![
            sync_tool(
                ToolSpec::new("text.upper", "Upper", Category::Text).describe("upper"),
                |_c, _p| Ok(ToolOutput::text("A")),
            ),
            sync_tool(
                ToolSpec::new("audio.transcribe", "Transcribe", Category::Audio)
                    .describe("transcribe")
                    .requires_model("whisper"),
                |_c, _p| Ok(ToolOutput::text("t")),
            ),
        ]);
        Engine::new(reg, EngineConfig::local(Environment::Cli), vec![])
    }

    #[test]
    fn packs_report_download_sizes() {
        let e = speech_engine();
        let packs = e.packs();
        let model = crate::models::recommended_model("whisper").unwrap();
        let audio = packs.iter().find(|p| p.id == "audio").unwrap();
        assert!(audio.download_bytes >= model.size_bytes);
        assert!(audio.disk_bytes >= model.size_bytes);
        assert_eq!(
            packs
                .iter()
                .find(|p| p.id == "text")
                .unwrap()
                .download_bytes,
            0
        );
    }

    #[test]
    fn unused_downloads_only_lists_what_no_turned_on_pack_uses() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = speech_engine();
        e.models = Arc::new(crate::models::ModelRegistry::new(dir.path().to_path_buf()));
        let model = crate::models::recommended_model("whisper").unwrap();
        for f in std::iter::once(model.file_name).chain(model.extra.iter().map(|(_, f)| *f)) {
            std::fs::write(dir.path().join(f), b"12345").unwrap();
        }
        assert!(e.models.is_installed(model.id));
        assert!(e.unused_downloads().is_empty(), "audio pack still on");

        e.set_packs(PackSelection::only(["text"]));
        let unused = e.unused_downloads();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].kind, "model");
        assert_eq!(unused[0].id, model.id);
        assert_eq!(unused[0].bytes, 5 * (1 + model.extra.len() as u64));

        assert!(e
            .remove_unused(Some(&["other".to_string()]))
            .unwrap()
            .is_empty());
        let removed = e.remove_unused(None).unwrap();
        assert_eq!(removed, unused);
        assert!(!e.models.is_installed(model.id));
        assert!(e.unused_downloads().is_empty());
    }

    #[test]
    fn concurrent_pack_edits_are_not_lost() {
        let e = Arc::new(packs_engine(PackSelection::all()));
        let saved = Arc::new(std::sync::Mutex::new(PackSelection::all()));
        let hs: Vec<_> = ["text", "image"]
            .into_iter()
            .map(|p| {
                let (e, saved) = (e.clone(), saved.clone());
                std::thread::spawn(move || {
                    e.edit_pack(p, false, |n| {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        *saved.lock().unwrap() = n.clone();
                        Ok::<_, ()>(())
                    })
                    .unwrap();
                })
            })
            .collect();
        hs.into_iter().for_each(|h| h.join().unwrap());
        let live = e.pack_selection();
        assert!(!live.is_on("text") && !live.is_on("image"));
        assert_eq!(*saved.lock().unwrap(), live);
    }
}
