//! One-step extension lifecycle: install (validate, copy, pin, reload, test),
//! uninstall and test. Shared by the CLI, the server and the desktop app.

use crate::engine::{Engine, RunRequest};
use crate::error::{ToolError, ToolResult};
use crate::manifest::{
    json_path_get, load_dev_extensions, load_extensions, parse_manifest, sha256_hex, DevAuthors,
    ExtensionStatus, ManifestTest, SettingsStore, TrustStore, DEV_DIR, LOCAL_AUTHOR,
};
use crate::params::LocalFileResolver;
use crate::policy::NetPolicy;
use crate::policy::{FsPolicy, PermissionSet};
use crate::registry::Registry;
use crate::spec::ToolSpec;
use crate::tool::{Tool, ToolOutput};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Largest manifest accepted by install.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// Upper bound for one manifest test run.
pub const TEST_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    /// Not run: the tool is unavailable here (missing program, unset
    /// required setting, unsupported platform). Never counted as passing.
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Result of install, test or uninstall.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExtensionReport {
    pub id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    pub permissions: Vec<String>,
    pub requires: Vec<String>,
    /// Required programs that are not installed here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    /// Required settings that are not set yet.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unset_settings: Vec<String>,
    pub trusted: bool,
    pub loaded: bool,
    /// In the dev slot: untrusted, usable only by its author.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub dev: bool,
    /// Install replaced an earlier copy of the same extension.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub replaced: bool,
    pub tests: Vec<TestResult>,
    /// True when nothing failed: no error and no failing test.
    pub passed: bool,
    pub error: Option<String>,
}

impl ExtensionReport {
    fn from_spec(spec: &ToolSpec, engine: &Engine) -> Self {
        let avail = engine.availability(spec);
        ExtensionReport {
            id: spec.id.clone(),
            name: spec.name.clone(),
            version: spec.version.clone(),
            permissions: spec.permissions.iter().map(|p| p.id().into()).collect(),
            requires: spec.requires.clone(),
            missing: avail.missing.iter().map(|m| m.id.clone()).collect(),
            unset_settings: avail.unset_settings.clone(),
            ..Default::default()
        }
    }

    fn finish(mut self) -> Self {
        self.passed =
            self.error.is_none() && !self.tests.iter().any(|t| t.status == TestStatus::Failed);
        self
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct UninstallReport {
    pub id: String,
    pub removed: Vec<String>,
    pub purged: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InstallOptions {
    /// Allow replacing an installed extension with the same id.
    pub replace: bool,
}

/// Read a manifest from a local path or an http(s) URL (fetched through
/// `net`). Returns the text and a file-name hint.
pub async fn fetch_manifest(source: &str, net: &NetPolicy) -> ToolResult<String> {
    let source = source.trim();
    if source.starts_with("http://") || source.starts_with("https://") {
        let url = crate::net::check_url(net, source)?;
        let client = crate::net::guarded_client(net.clone())?;
        let resp = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::failed(format!("downloading manifest: {e}")))?;
        if !resp.status().is_success() {
            return Err(ToolError::failed(format!(
                "downloading manifest: HTTP {}",
                resp.status()
            )));
        }
        if resp.content_length().unwrap_or(0) > MAX_MANIFEST_BYTES as u64 {
            return Err(too_big());
        }
        let mut resp = resp;
        let mut bytes = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| ToolError::failed(format!("downloading manifest: {e}")))?
        {
            if bytes.len() + chunk.len() > MAX_MANIFEST_BYTES {
                return Err(too_big());
            }
            bytes.extend_from_slice(&chunk);
        }
        String::from_utf8(bytes)
            .map_err(|_| ToolError::invalid("manifest", "manifest is not UTF-8 text"))
    } else {
        let meta = std::fs::metadata(source)
            .map_err(|e| ToolError::invalid("source", format!("cannot read '{source}': {e}")))?;
        if meta.len() > MAX_MANIFEST_BYTES as u64 {
            return Err(too_big());
        }
        std::fs::read_to_string(source)
            .map_err(|e| ToolError::invalid("source", format!("cannot read '{source}': {e}")))
    }
}

/// Serializes changes to extension directories (manifests and `trusted.toml`),
/// so concurrent installs and uninstalls cannot overwrite each other's pins.
static LIFECYCLE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lifecycle_lock() -> std::sync::MutexGuard<'static, ()> {
    LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner())
}

fn too_big() -> ToolError {
    ToolError::limit(format!(
        "manifest is larger than {} KiB",
        MAX_MANIFEST_BYTES / 1024
    ))
}

/// Validate a manifest without changing anything: what install would add.
pub fn preview(
    engine: &Engine,
    dir: &Path,
    text: &str,
    opts: InstallOptions,
) -> ToolResult<ExtensionReport> {
    let (tool, existing) = validate(engine, dir, text, opts)?;
    let mut r = ExtensionReport::from_spec(tool.spec(), engine);
    r.sha256 = tool.sha256().to_string();
    r.replaced = !existing.is_empty();
    Ok(r)
}

/// Install a manifest: validate, copy to `<dir>/<id>.toml`, pin its SHA-256,
/// reload the engine and run the manifest's tests. Invalid manifests and id
/// collisions fail before anything changes; a failing test leaves the
/// extension installed and reports it failing.
pub async fn install(
    engine: &Engine,
    dir: &Path,
    text: &str,
    opts: InstallOptions,
) -> ToolResult<ExtensionReport> {
    let guard = lifecycle_lock();
    let (tool, existing) = validate(engine, dir, text, opts)?;
    let id = tool.spec().id.clone();
    check_id(&id)?;
    let sha = tool.sha256().to_string();
    std::fs::create_dir_all(dir)
        .map_err(|e| ToolError::failed(format!("creating {}: {e}", dir.display())))?;
    let target = dir.join(format!("{id}.toml"));
    let trust_path = dir.join("trusted.toml");
    let mut trust = TrustStore::load(&trust_path);
    let old_trust = trust.trusted.get(&id).cloned();
    let backups: Vec<(PathBuf, String)> = existing
        .iter()
        .filter_map(|p| Some((p.clone(), std::fs::read_to_string(p).ok()?)))
        .collect();
    let rollback = |trust: &mut TrustStore| {
        let _ = std::fs::remove_file(&target);
        for (p, text) in &backups {
            let _ = std::fs::write(p, text);
        }
        match &old_trust {
            Some(s) => trust.trusted.insert(id.clone(), s.clone()),
            None => trust.trusted.remove(&id),
        };
        let _ = trust.save(&trust_path);
    };

    let write = (|| -> ToolResult<()> {
        for p in &existing {
            if *p != target {
                std::fs::remove_file(p)
                    .map_err(|e| ToolError::failed(format!("removing {}: {e}", p.display())))?;
            }
        }
        let tmp = dir.join(format!(".{id}.toml.tmp"));
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, &target))
            .map_err(|e| ToolError::failed(format!("writing {}: {e}", target.display())))?;
        trust.trusted.insert(id.clone(), sha.clone());
        trust
            .save(&trust_path)
            .map_err(|e| ToolError::failed(format!("saving trust: {e}")))
    })();
    if let Err(e) = write {
        rollback(&mut trust);
        return Err(e);
    }

    engine.reload();
    let file = target.file_name().unwrap().to_string_lossy().to_string();
    let status = engine.extensions().iter().find(|s| s.file == file).cloned();
    if !status.as_ref().is_some_and(|s| s.loaded) {
        let why = status
            .and_then(|s| s.error)
            .unwrap_or_else(|| "extension did not load".into());
        rollback(&mut trust);
        engine.reload();
        return Err(ToolError::failed(format!("'{id}' did not load: {why}")));
    }
    let Some(loaded) = engine.registry().get(&id).cloned() else {
        rollback(&mut trust);
        engine.reload();
        return Err(ToolError::failed(format!(
            "'{id}' is not available after reload"
        )));
    };
    let mut r = ExtensionReport::from_spec(loaded.spec(), engine);
    r.file = Some(file.clone());
    r.sha256 = sha;
    r.trusted = true;
    r.loaded = true;
    r.replaced = !existing.is_empty();
    drop(guard);
    let src = Source {
        dir,
        file: &file,
        text,
    };
    r.tests = run_tests(engine, &src, loaded.spec(), tool.tests()).await;
    Ok(r.finish())
}

/// Run an installed extension's tests without reinstalling it.
pub async fn test(engine: &Engine, dir: &Path, id: &str) -> ToolResult<ExtensionReport> {
    check_id(id)?;
    let statuses = engine.extensions();
    let status = statuses
        .iter()
        .filter(|s| s.id.as_deref() == Some(id))
        .max_by_key(|s| s.loaded)
        .ok_or_else(|| ToolError::NotFound { id: id.to_string() })?;
    let trusted = !status.dev
        && TrustStore::load(&dir.join("trusted.toml"))
            .trusted
            .get(id)
            .is_some_and(|s| *s == status.sha256);
    let text = std::fs::read_to_string(dir.join(&status.file))
        .map_err(|e| ToolError::failed(format!("reading {}: {e}", status.file)))?;
    let parsed =
        parse_manifest(&text, &status.file).map_err(|e| ToolError::invalid("manifest", e))?;
    let loaded = engine.registry().get(id).cloned();
    let spec = loaded
        .as_ref()
        .map(|t| t.spec().clone())
        .unwrap_or_else(|| parsed.spec().clone());
    let mut r = ExtensionReport::from_spec(&spec, engine);
    r.file = Some(status.file.clone());
    r.sha256 = status.sha256.clone();
    r.trusted = trusted;
    r.dev = status.dev;
    r.loaded = status.loaded && loaded.is_some();
    if sha256_hex(text.as_bytes()) != status.sha256 {
        r.error = Some(format!(
            "{} changed since it was loaded; reload or reinstall it",
            status.file
        ));
    } else if !r.loaded {
        r.error = Some(
            status
                .error
                .clone()
                .unwrap_or_else(|| format!("'{id}' is not loaded")),
        );
    } else {
        let src = Source {
            dir,
            file: &status.file,
            text: &text,
        };
        r.tests = run_tests(engine, &src, &spec, parsed.tests()).await;
    }
    Ok(r.finish())
}

/// Remove an extension's manifest(s) and trust entry and reload. Settings
/// are kept unless `purge`.
pub fn uninstall(
    engine: &Engine,
    dir: &Path,
    id: &str,
    purge: bool,
) -> ToolResult<UninstallReport> {
    check_id(id)?;
    let _guard = lifecycle_lock();
    if let Some(t) = engine.catalog().get(id) {
        if !is_extension(t.spec()) {
            return Err(ToolError::invalid(
                "id",
                format!("'{id}' is a built-in tool, not an extension"),
            ));
        }
    }
    let mut files = manifests_with_id(dir, id);
    let dev_files = manifests_with_id(&dir.join(DEV_DIR), id);
    files.extend(dev_files.iter().cloned());
    let trust_path = dir.join("trusted.toml");
    let mut trust = TrustStore::load(&trust_path);
    let was_trusted = trust.trusted.remove(id).is_some();
    if !dev_files.is_empty() {
        let mut authors = dev_authors(dir)?;
        if authors.authors.remove(id).is_some() {
            authors
                .save(dir)
                .map_err(|e| ToolError::failed(format!("saving dev authors: {e}")))?;
        }
    }
    if files.is_empty() && !was_trusted {
        return Err(ToolError::NotFound { id: id.to_string() });
    }
    let mut removed = vec![];
    for p in &files {
        std::fs::remove_file(p)
            .map_err(|e| ToolError::failed(format!("removing {}: {e}", p.display())))?;
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        removed.push(match p.parent() == Some(dir) {
            true => name,
            false => format!("{DEV_DIR}/{name}"),
        });
    }
    trust
        .save(&trust_path)
        .map_err(|e| ToolError::failed(format!("saving trust: {e}")))?;
    if purge {
        SettingsStore::for_extensions(dir).remove(id)?;
    }
    crate::store::forget(dir, id);
    engine.reload();
    Ok(UninstallReport {
        id: id.to_string(),
        removed,
        purged: purge,
    })
}

/// Load an extensions folder into `reg`: trusted manifests first, then the
/// dev slot. A dev tool never shadows a built-in or an installed extension
/// (trusted or not), by id or by MCP name.
pub fn load_into(reg: &mut Registry, dir: &Path) -> Vec<ExtensionStatus> {
    let (tools, mut statuses) = load_extensions(dir, false);
    register(reg, tools, &mut statuses);
    let (dev, mut dev_st) = load_dev_extensions(dir);
    let installed: Vec<String> = statuses.iter().filter_map(|s| s.id.clone()).collect();
    for t in dev {
        let spec = t.spec();
        let clash = if installed.contains(&spec.id) {
            Some(format!("'{}' is already an installed extension", spec.id))
        } else if let Some(other) = reg.get(&spec.id) {
            Some(format!(
                "'{}' is already the tool '{}'",
                spec.id,
                other.spec().source
            ))
        } else {
            reg.find(&spec.mcp_name())
                .map(|o| format!("its MCP name clashes with '{}'", o.spec().id))
        };
        match clash {
            Some(e) => mark_failed(&mut dev_st, spec, format!("dev tool cannot shadow: {e}")),
            // Registered now, so later drafts are checked against it.
            None => register(reg, vec![t.clone()], &mut dev_st),
        }
    }
    statuses.extend(dev_st);
    statuses
}

fn register(reg: &mut Registry, tools: Vec<Arc<dyn Tool>>, statuses: &mut [ExtensionStatus]) {
    for t in tools {
        let spec = t.spec().clone();
        if let Err(e) = reg.register(t) {
            mark_failed(statuses, &spec, e.to_string());
        }
    }
}

fn mark_failed(statuses: &mut [ExtensionStatus], spec: &ToolSpec, error: String) {
    let file = spec.source.strip_prefix("extension:").unwrap_or_default();
    for s in statuses.iter_mut().filter(|s| s.file == file) {
        s.loaded = false;
        s.error = Some(error.clone());
    }
}

/// Install a manifest into the dev slot (`<dir>/dev/<id>.toml`) for
/// `author`: no trust pin, visible only to its author, reloaded on edit.
/// It may not take an id or MCP name already used by a built-in, an
/// installed extension or another author's dev tool.
pub async fn install_dev(
    engine: &Engine,
    dir: &Path,
    text: &str,
    author: &str,
    opts: InstallOptions,
) -> ToolResult<ExtensionReport> {
    let guard = lifecycle_lock();
    if text.len() > MAX_MANIFEST_BYTES {
        return Err(too_big());
    }
    let tool = parse_manifest(text, "install").map_err(|e| ToolError::invalid("manifest", e))?;
    Registry::new()
        .register(Arc::new(
            parse_manifest(text, "install").map_err(|e| ToolError::invalid("manifest", e))?,
        ))
        .map_err(|e| ToolError::invalid("manifest", e.to_string()))?;
    let id = tool.spec().id.clone();
    check_id(&id)?;
    let mcp = tool.spec().mcp_name();
    let mut authors = dev_authors(dir)?;
    let owner = |o: &ToolSpec| o.author.as_deref().unwrap_or(LOCAL_AUTHOR).to_string();
    for t in engine.catalog().iter() {
        let other = t.spec();
        if other.id != id && other.mcp_name() != mcp {
            continue;
        }
        let why = if !is_extension(other) {
            format!("the built-in tool '{}'", other.id)
        } else if !other.dev {
            format!("the installed extension '{}'", other.id)
        } else if other.id != id {
            format!("the dev tool '{}'", other.id)
        } else if owner(other) != author {
            format!("another author's dev tool '{id}'")
        } else {
            continue;
        };
        return Err(ToolError::invalid(
            "manifest",
            format!("'{id}' cannot shadow {why}"),
        ));
    }
    if !manifests_with_id(dir, &id).is_empty() {
        return Err(ToolError::invalid(
            "manifest",
            format!("'{id}' cannot shadow the installed extension '{id}'"),
        ));
    }
    let dev = dir.join(DEV_DIR);
    let existing = manifests_with_id(&dev, &id);
    if let Some(a) = authors.authors.get(&id).filter(|a| *a != author) {
        if !existing.is_empty() {
            return Err(ToolError::invalid(
                "manifest",
                format!("'{id}' cannot shadow another author's dev tool ({a})"),
            ));
        }
    }
    if !existing.is_empty() && !opts.replace {
        return Err(ToolError::invalid(
            "manifest",
            format!("a dev tool with id '{id}' is already installed; install with replace"),
        ));
    }
    let target = dev.join(format!("{id}.toml"));
    let occupied = std::fs::read_to_string(&target)
        .ok()
        .and_then(|t| parse_manifest(&t, "").ok())
        .is_some_and(|t| t.spec().id != id);
    if occupied {
        return Err(ToolError::invalid(
            "manifest",
            format!("dev/{id}.toml holds a different tool; rename or remove it first"),
        ));
    }
    std::fs::create_dir_all(&dev)
        .map_err(|e| ToolError::failed(format!("creating {}: {e}", dev.display())))?;
    let backups: Vec<(PathBuf, String)> = existing
        .iter()
        .filter_map(|p| Some((p.clone(), std::fs::read_to_string(p).ok()?)))
        .collect();
    let old_author = authors.authors.get(&id).cloned();
    let rollback = |authors: &mut DevAuthors| {
        let _ = std::fs::remove_file(&target);
        for (p, text) in &backups {
            let _ = std::fs::write(p, text);
        }
        match &old_author {
            Some(a) => authors.authors.insert(id.clone(), a.clone()),
            None => authors.authors.remove(&id),
        };
        let _ = authors.save(dir);
    };
    let write = (|| -> ToolResult<()> {
        for p in &existing {
            if *p != target {
                std::fs::remove_file(p)
                    .map_err(|e| ToolError::failed(format!("removing {}: {e}", p.display())))?;
            }
        }
        authors.authors.insert(id.clone(), author.to_string());
        authors
            .save(dir)
            .map_err(|e| ToolError::failed(format!("saving dev authors: {e}")))?;
        let tmp = dev.join(format!(".{id}.toml.tmp"));
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, &target))
            .map_err(|e| ToolError::failed(format!("writing {}: {e}", target.display())))
    })();
    if let Err(e) = write {
        rollback(&mut authors);
        return Err(e);
    }
    engine.reload();
    let file = format!("{DEV_DIR}/{id}.toml");
    let status = engine.extensions().iter().find(|s| s.file == file).cloned();
    let loaded = engine.registry().get(&id).cloned();
    let (Some(loaded), Some(true)) = (loaded, status.as_ref().map(|s| s.loaded)) else {
        let why = status
            .and_then(|s| s.error)
            .unwrap_or_else(|| "extension did not load".into());
        rollback(&mut authors);
        engine.reload();
        return Err(ToolError::failed(format!("'{id}' did not load: {why}")));
    };
    let mut r = ExtensionReport::from_spec(loaded.spec(), engine);
    r.file = Some(file.clone());
    r.sha256 = tool.sha256().to_string();
    r.loaded = true;
    r.dev = true;
    r.replaced = !existing.is_empty();
    drop(guard);
    let src = Source {
        dir,
        file: &file,
        text,
    };
    r.tests = run_tests(engine, &src, loaded.spec(), tool.tests()).await;
    Ok(r.finish())
}

/// Promote a dev tool to a trusted extension: move `dev/<id>.toml` to
/// `<id>.toml`, pin its SHA-256, reload and run its tests. Callers must make
/// sure a human owner or admin asked for it.
pub async fn promote(engine: &Engine, dir: &Path, id: &str) -> ToolResult<ExtensionReport> {
    check_id(id)?;
    let guard = lifecycle_lock();
    let dev = dir.join(DEV_DIR);
    let files = manifests_with_id(&dev, id);
    let source = match files.as_slice() {
        [] => return Err(ToolError::NotFound { id: id.to_string() }),
        [one] => one.clone(),
        _ => {
            return Err(ToolError::invalid(
                "id",
                format!("more than one dev manifest declares '{id}'; keep one and retry"),
            ))
        }
    };
    let text = std::fs::read_to_string(&source)
        .map_err(|e| ToolError::failed(format!("reading {}: {e}", source.display())))?;
    let tool = parse_manifest(&text, "promote").map_err(|e| ToolError::invalid("manifest", e))?;
    if !manifests_with_id(dir, id).is_empty() {
        return Err(ToolError::invalid(
            "id",
            format!("an extension with id '{id}' is already installed; uninstall it first"),
        ));
    }
    let sha = tool.sha256().to_string();
    let target = dir.join(format!("{id}.toml"));
    if target.exists() {
        return Err(ToolError::invalid(
            "id",
            format!("{id}.toml holds a different extension; rename or remove it first"),
        ));
    }
    let trust_path = dir.join("trusted.toml");
    let mut trust = TrustStore::load(&trust_path);
    let old_trust = trust.trusted.get(id).cloned();
    let mut authors = dev_authors(dir)?;
    let old_author = authors.authors.remove(id);
    let backups: Vec<(PathBuf, String)> = files
        .iter()
        .filter_map(|p| Some((p.clone(), std::fs::read_to_string(p).ok()?)))
        .collect();
    let rollback = |trust: &mut TrustStore, authors: &mut DevAuthors| {
        let _ = std::fs::remove_file(&target);
        for (p, text) in &backups {
            let _ = std::fs::write(p, text);
        }
        match &old_trust {
            Some(s) => trust.trusted.insert(id.to_string(), s.clone()),
            None => trust.trusted.remove(id),
        };
        let _ = trust.save(&trust_path);
        if let Some(a) = &old_author {
            authors.authors.insert(id.to_string(), a.clone());
            let _ = authors.save(dir);
        }
    };
    let write = (|| -> ToolResult<()> {
        let tmp = dir.join(format!(".{id}.toml.tmp"));
        std::fs::write(&tmp, &text)
            .and_then(|_| std::fs::rename(&tmp, &target))
            .map_err(|e| ToolError::failed(format!("writing {}: {e}", target.display())))?;
        trust.trusted.insert(id.to_string(), sha.clone());
        trust
            .save(&trust_path)
            .map_err(|e| ToolError::failed(format!("saving trust: {e}")))?;
        for p in &files {
            std::fs::remove_file(p)
                .map_err(|e| ToolError::failed(format!("removing {}: {e}", p.display())))?;
        }
        authors
            .save(dir)
            .map_err(|e| ToolError::failed(format!("saving dev authors: {e}")))
    })();
    if let Err(e) = write {
        rollback(&mut trust, &mut authors);
        engine.reload();
        return Err(e);
    }
    engine.reload();
    let file = format!("{id}.toml");
    let status = engine.extensions().iter().find(|s| s.file == file).cloned();
    let loaded = engine.registry().get(id).cloned();
    let (Some(loaded), Some(true)) = (loaded, status.as_ref().map(|s| s.loaded)) else {
        let why = status
            .and_then(|s| s.error)
            .unwrap_or_else(|| "extension did not load".into());
        rollback(&mut trust, &mut authors);
        engine.reload();
        return Err(ToolError::failed(format!("'{id}' did not load: {why}")));
    };
    let mut r = ExtensionReport::from_spec(loaded.spec(), engine);
    r.file = Some(file.clone());
    r.sha256 = sha;
    r.trusted = true;
    r.loaded = true;
    drop(guard);
    let src = Source {
        dir,
        file: &file,
        text: &text,
    };
    r.tests = run_tests(engine, &src, loaded.spec(), tool.tests()).await;
    Ok(r.finish())
}

fn dev_authors(dir: &Path) -> ToolResult<DevAuthors> {
    DevAuthors::load(dir).map_err(|e| ToolError::failed(format!("reading dev authors: {e}")))
}

fn is_extension(spec: &ToolSpec) -> bool {
    spec.source.starts_with("extension:")
}

/// Parse and check a manifest for install; returns it with the paths of
/// installed manifests it would replace.
fn validate(
    engine: &Engine,
    dir: &Path,
    text: &str,
    opts: InstallOptions,
) -> ToolResult<(crate::manifest::ExternalTool, Vec<PathBuf>)> {
    if text.len() > MAX_MANIFEST_BYTES {
        return Err(too_big());
    }
    let tool = parse_manifest(text, "install").map_err(|e| ToolError::invalid("manifest", e))?;
    let spec = tool.spec().clone();
    let id = spec.id.clone();
    // The registry's own checks (id format, parameter specs) up front.
    Registry::new()
        .register(Arc::new(
            parse_manifest(text, "install").map_err(|e| ToolError::invalid("manifest", e))?,
        ))
        .map_err(|e| ToolError::invalid("manifest", e.to_string()))?;
    let catalog = engine.catalog();
    let mcp = spec.mcp_name();
    for t in catalog.iter() {
        let other = t.spec();
        if other.id != id && other.mcp_name() != mcp {
            continue;
        }
        if !is_extension(other) {
            return Err(ToolError::invalid(
                "manifest",
                format!("'{id}' collides with the built-in tool '{}'", other.id),
            ));
        }
        if other.id != id {
            return Err(ToolError::invalid(
                "manifest",
                format!("'{id}' collides with the extension '{}'", other.id),
            ));
        }
    }
    let existing = manifests_with_id(dir, &id);
    if !existing.is_empty() && !opts.replace {
        return Err(ToolError::invalid(
            "manifest",
            format!(
                "an extension with id '{id}' is already installed ({}); uninstall it or install with replace",
                existing
                    .iter()
                    .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    let target = dir.join(format!("{id}.toml"));
    if target.exists() && !existing.contains(&target) {
        return Err(ToolError::invalid(
            "manifest",
            format!(
                "{} exists and is not a manifest for '{id}'",
                target.display()
            ),
        ));
    }
    Ok((tool, existing))
}

/// An id whose `<id>.toml` is a plain file name inside the extension
/// folder: only letters, digits, `.`, `_` and `-`, so no path separators.
fn check_id(id: &str) -> ToolResult<()> {
    let ok = !id.is_empty()
        && id != "."
        && id != ".."
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(ToolError::invalid(
            "id",
            format!("'{id}' is not a valid extension id"),
        ))
    }
}

/// Manifest files in `dir` whose id is `id`, plus an unparsable `<id>.toml`.
fn manifests_with_id(dir: &Path, id: &str) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().is_some_and(|e| e == "toml")
                && p.file_name().is_some_and(|n| n != "trusted.toml")
        })
        .filter(|p| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|t| parse_manifest(&t, "").ok())
                .is_some_and(|t| t.spec().id == id)
        })
        .collect();
    // A manifest at `<id>.toml` that no longer parses is still this id's.
    let target = dir.join(format!("{id}.toml"));
    let unparsable = std::fs::read_to_string(&target)
        .ok()
        .is_some_and(|t| parse_manifest(&t, "").is_err());
    if unparsable && !out.contains(&target) {
        out.push(target);
    }
    out.sort();
    out
}

/// The installed manifest a test run comes from, so a test with a recorded
/// HTTP response can run its own copy of the tool against it.
struct Source<'a> {
    dir: &'a Path,
    file: &'a str,
    text: &'a str,
}

async fn run_tests(
    engine: &Engine,
    src: &Source<'_>,
    spec: &ToolSpec,
    tests: &[ManifestTest],
) -> Vec<TestResult> {
    let installed = engine.find(&spec.id);
    let mut out = vec![];
    for t in tests {
        out.push(run_test(engine, src, spec, installed.as_ref(), t).await);
    }
    out
}

async fn run_test(
    engine: &Engine,
    src: &Source<'_>,
    spec: &ToolSpec,
    installed: Result<&Arc<dyn Tool>, &ToolError>,
    t: &ManifestTest,
) -> TestResult {
    let started = Instant::now();
    let result = |status, error: Option<String>| TestResult {
        name: t.name.clone(),
        status,
        error,
        duration_ms: started.elapsed().as_millis() as u64,
    };
    let tool: Arc<dyn Tool> = if t.http.is_some() || t.settings.is_some() {
        let own = parse_manifest(src.text, src.file)
            .map_err(|e| ToolError::invalid("manifest", e))
            .and_then(|tool| {
                let tool = tool.with_settings_store(SettingsStore::for_extensions(src.dir));
                let tool = match &t.http {
                    Some(fixture) => tool.with_recorded_response(fixture.response()),
                    None => tool,
                };
                match &t.settings {
                    Some(values) => tool.with_setting_values(values),
                    None => Ok(tool),
                }
            });
        match own {
            Ok(tool) => Arc::new(tool),
            Err(e) => return result(TestStatus::Failed, Some(e.to_string())),
        }
    } else {
        match installed {
            Ok(tool) => tool.clone(),
            Err(e) => return result(TestStatus::Failed, Some(e.to_string())),
        }
    };
    let mut avail = engine.availability(spec);
    avail.unset_settings = tool.unset_settings();
    avail.available = avail.platform_supported
        && avail.missing.is_empty()
        && avail.missing_model.is_none()
        && avail.unset_settings.is_empty();
    if !avail.available {
        let mut why = vec![];
        if !avail.platform_supported {
            why.push("not supported on this platform".to_string());
        }
        if !avail.missing.is_empty() {
            let names: Vec<_> = avail.missing.iter().map(|m| m.name.as_str()).collect();
            why.push(format!("missing {}", names.join(", ")));
        }
        if !avail.unset_settings.is_empty() {
            why.push(format!(
                "unset settings: {}",
                avail.unset_settings.join(", ")
            ));
        }
        if why.is_empty() {
            why.push("tool is unavailable".into());
        }
        return result(TestStatus::Skipped, Some(why.join("; ")));
    }
    let work = match std::fs::create_dir_all(&engine.config.temp_root).and_then(|_| {
        tempfile::Builder::new()
            .prefix("dgb-test-")
            .tempdir_in(&engine.config.temp_root)
    }) {
        Ok(w) => w,
        Err(e) => return result(TestStatus::Failed, Some(format!("temp dir: {e}"))),
    };
    let mut params = t.params.clone();
    let inputs = work.path().join("inputs");
    for (param, contents) in &t.files {
        let name = params
            .get(param)
            .and_then(Value::as_str)
            .and_then(|v| Path::new(v).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{param}.txt"));
        let path = inputs.join(name);
        if let Err(e) =
            std::fs::create_dir_all(&inputs).and_then(|_| std::fs::write(&path, contents))
        {
            return result(
                TestStatus::Failed,
                Some(format!("writing sample file: {e}")),
            );
        }
        params.insert(param.clone(), Value::String(path.to_string_lossy().into()));
    }
    // Tests only see their own sample files, never other host paths.
    let fs = FsPolicy::roots(vec![work.path().to_path_buf()]);
    let resolver = LocalFileResolver { policy: fs.clone() };
    let run = engine
        .run_tool(
            tool,
            RunRequest {
                tool_id: &spec.id,
                params,
                resolver: &resolver,
                out_dir: work.path().join("out"),
                permissions: PermissionSet::all(),
                authorize: None,
                progress: None,
                cancel: CancellationToken::new(),
                timeout: Some(Duration::from_secs(TEST_TIMEOUT_SECS)),
                fs: Some(fs),
            },
        )
        .await;
    if let Err(err @ ToolError::MissingDependency { .. }) = &run {
        if t.expect.error.as_deref() != Some(err.code()) {
            return result(TestStatus::Skipped, Some(err.to_string()));
        }
    }
    match check(t, run.map(|(o, _)| o)) {
        Ok(()) => result(TestStatus::Passed, None),
        Err(e) => result(TestStatus::Failed, Some(e)),
    }
}

/// Compare a run's result with a test's expectations.
pub fn check(t: &ManifestTest, run: ToolResult<ToolOutput>) -> Result<(), String> {
    let e = &t.expect;
    let out = match (run, &e.error) {
        (Err(err), Some(code)) if err.code() == code => return Ok(()),
        (Err(err), Some(code)) => {
            return Err(format!(
                "expected error '{code}', got '{}': {err}",
                err.code()
            ))
        }
        (Err(err), None) => return Err(err.to_string()),
        (Ok(_), Some(code)) => {
            return Err(format!("expected error '{code}', but the run succeeded"))
        }
        (Ok(o), None) => o,
    };
    let text = out.text.clone().unwrap_or_default();
    if let Some(want) = &e.text {
        if text.trim_end() != want.trim_end() {
            return Err(format!("expected text {want:?}, got {:?}", truncate(&text)));
        }
    }
    for s in &e.contains {
        if !text.contains(s.as_str()) {
            return Err(format!(
                "output does not contain {s:?}: {:?}",
                truncate(&text)
            ));
        }
    }
    if !e.json.is_empty() {
        let doc = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| out.data.clone());
        for (path, want) in &e.json {
            match json_path_get(&doc, path) {
                Some(got) if json_eq(got, want) => {}
                Some(got) => return Err(format!("{path}: expected {want}, got {got}")),
                None => return Err(format!("{path}: not found in output")),
            }
        }
    }
    if let Some(n) = e.files {
        if out.artifacts.len() != n {
            return Err(format!(
                "expected {n} output file(s), got {}",
                out.artifacts.len()
            ));
        }
    }
    Ok(())
}

/// Equal, treating integer and float forms of the same number as equal.
/// JSON equality where `1` equals `1.0`; integers compare exactly.
fn json_eq(a: &Value, b: &Value) -> bool {
    let int = |n: &serde_json::Number| {
        n.as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
    };
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (int(x), int(y)) {
            (Some(p), Some(q)) => p == q,
            _ => x.as_f64() == y.as_f64(),
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

fn truncate(s: &str) -> String {
    let mut t: String = s.chars().take(200).collect();
    if t.len() < s.len() {
        t.push('…');
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineConfig;

    use crate::params::Params;
    use crate::spec::Category;
    use crate::tool::{Environment, ToolContext};

    struct Builtin(ToolSpec);

    #[async_trait::async_trait]
    impl Tool for Builtin {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        async fn run(&self, _: ToolContext, _: Params) -> ToolResult<ToolOutput> {
            Ok(ToolOutput::new())
        }
    }

    fn engine(dir: &Path) -> Engine {
        let d = dir.to_path_buf();
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.temp_root = dir.join(".tmp");
        Engine::from_loader(
            cfg,
            crate::deps::DepRegistry::new(vec![]),
            Arc::new(move || {
                let mut reg = Registry::new();
                reg.register_all(vec![Arc::new(Builtin(ToolSpec::new(
                    "text.builtin_x",
                    "Builtin",
                    Category::Text,
                )))]);
                let st = load_into(&mut reg, &d);
                (reg, st)
            }),
        )
    }

    const M: &str = r#"
[tool]
id = "text.ext_echo"
name = "Echo"
category = "text"
version = "1.2.0"
[command]
program = "echo"
args = ["{word}"]
[output]
stdout = "text/plain"
[[params]]
name = "word"
type = "text"
default = "x"
[[tests]]
name = "says hi"
params = { word = "hi" }
expect = { text = "hi", contains = ["h"] }
[[tests]]
params = { word = '{"a":[1,{"b":"c"}]}' }
expect = { json = { "$.a[0]" = 1, "a[1].b" = "c" } }
"#;

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| n != ".tmp")
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn install_pins_reloads_and_runs_tests() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let r = install(&e, dir.path(), M, InstallOptions::default())
            .await
            .unwrap();
        assert!(r.trusted && r.loaded && r.passed, "{r:?}");
        assert_eq!(
            (r.id.as_str(), r.version.as_str()),
            ("text.ext_echo", "1.2.0")
        );
        assert_eq!(r.tests.len(), 2);
        assert!(
            r.tests.iter().all(|t| t.status == TestStatus::Passed),
            "{:?}",
            r.tests
        );
        assert_eq!(r.tests[1].name, "test 2");
        assert_eq!(files(dir.path()), ["text.ext_echo.toml", "trusted.toml"]);
        let trust = TrustStore::load(&dir.path().join("trusted.toml"));
        assert_eq!(trust.trusted["text.ext_echo"], sha256_hex(M.as_bytes()));
        assert!(e.registry().get("text.ext_echo").is_some());

        let again = test(&e, dir.path(), "text.ext_echo").await.unwrap();
        assert!(again.passed && again.tests.len() == 2, "{again:?}");
    }

    const HTTP_M: &str = r#"
[tool]
id = "data.ext_weather"
name = "Weather"
version = "1.0.0"
category = "data"

[http]
url = "https://api.example.com/v1/forecast?lat={settings.lat}&q={city}"

[output]
json = { temp = "$.current.temp" }

[[params]]
name = "city"
type = "text"

[[settings]]
name = "lat"
type = "text"
required = true

[[tests]]
name = "maps the recorded response"
params = { city = "Boulder" }
settings = { lat = "40.0" }
http = { json = { current = { temp = 21.5 } } }
expect = { json = { "$.temp" = 21.5 } }

[[tests]]
name = "fails on a 500"
params = { city = "x" }
settings = { lat = "1" }
http = { status = 500, body = "down" }
expect = { error = "external_failure" }

[[tests]]
name = "needs the user's settings"
params = { city = "x" }
http = { json = {} }
"#;

    #[test]
    fn json_eq_is_exact_for_integers_and_recursive() {
        use serde_json::json;
        assert!(json_eq(&json!(1), &json!(1.0)));
        assert!(json_eq(
            &json!({"a": [1, {"b": 2.0}]}),
            &json!({"a": [1.0, {"b": 2}]})
        ));
        assert!(!json_eq(
            &json!(9007199254740993_u64),
            &json!(9007199254740992_u64)
        ));
        assert!(!json_eq(&json!([1]), &json!([1, 2])));
    }

    #[tokio::test]
    async fn ids_cannot_reach_outside_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("ext");
        std::fs::create_dir(&dir).unwrap();
        let outside = root.path().join("victim.toml");
        std::fs::write(&outside, "not a manifest").unwrap();
        let e = engine(&dir);
        for id in ["../victim", "..", "a/b", "a\\b", "a:b", ""] {
            assert!(uninstall(&e, &dir, id, true).is_err(), "{id}");
            assert!(test(&e, &dir, id).await.is_err(), "{id}");
        }
        assert!(outside.exists());
    }

    #[tokio::test]
    async fn uninstall_and_replace_handle_a_broken_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        install(&e, dir.path(), HTTP_M, InstallOptions::default())
            .await
            .unwrap();
        let file = dir.path().join("data.ext_weather.toml");
        std::fs::write(&file, "not [valid").unwrap();
        let replaced = install(&e, dir.path(), HTTP_M, InstallOptions { replace: true })
            .await
            .unwrap();
        assert!(replaced.loaded && replaced.replaced, "{replaced:?}");
        std::fs::write(&file, "not [valid").unwrap();
        let r = uninstall(&e, dir.path(), "data.ext_weather", false).unwrap();
        assert_eq!(r.removed, ["data.ext_weather.toml"]);
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn http_tests_use_recorded_responses_and_their_own_settings() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let r = install(&e, dir.path(), HTTP_M, InstallOptions::default())
            .await
            .unwrap();
        assert!(r.loaded && r.trusted, "{r:?}");
        let status: Vec<_> = r.tests.iter().map(|t| t.status).collect();
        assert_eq!(
            status,
            [TestStatus::Passed, TestStatus::Passed, TestStatus::Skipped],
            "{:?}",
            r.tests
        );
        assert!(r.tests[2].error.as_deref().unwrap().contains("lat"));
        assert!(r.passed);
    }

    #[test]
    fn http_fixtures_are_only_for_http_manifests() {
        let cmd = M.replacen("[[tests]]", "[[tests]]\nhttp = { body = \"x\" }", 1);
        assert!(parse_manifest(&cmd, "x")
            .map(|_| ())
            .unwrap_err()
            .contains("only for [http] manifests"));
        let both = HTTP_M.replace("http = { json = {} }", "http = { json = {}, body = \"x\" }");
        assert!(parse_manifest(&both, "x")
            .map(|_| ())
            .unwrap_err()
            .contains("not both"));
        let bad = HTTP_M.replace("settings = { lat = \"1\" }", "settings = { nope = \"1\" }");
        assert!(parse_manifest(&bad, "x")
            .map(|_| ())
            .unwrap_err()
            .contains("unknown setting 'nope'"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_failing_test_stays_installed_and_reports_failing() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let m = M.replace(r#"text = "hi""#, r#"text = "bye""#);
        let r = install(&e, dir.path(), &m, InstallOptions::default())
            .await
            .unwrap();
        assert!(r.loaded && r.trusted);
        assert!(!r.passed);
        assert_eq!(r.tests[0].status, TestStatus::Failed);
        assert!(r.tests[0].error.as_deref().unwrap().contains("bye"));
        assert!(e.registry().get("text.ext_echo").is_some());
        let again = test(&e, dir.path(), "text.ext_echo").await.unwrap();
        assert!(!again.passed);
        assert_eq!(again.tests[0].status, TestStatus::Failed);
    }

    #[tokio::test]
    async fn unavailable_tools_skip_tests_instead_of_passing() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let m = format!("{M}[[settings]]\nname = \"key\"\ntype = \"text\"\nrequired = true\n");
        let r = install(&e, dir.path(), &m, InstallOptions::default())
            .await
            .unwrap();
        assert!(r.loaded && r.passed);
        assert_eq!(r.unset_settings, ["key"]);
        assert!(r.tests.iter().all(|t| t.status == TestStatus::Skipped));
        assert!(r.tests[0].error.as_deref().unwrap().contains("key"));
    }

    #[tokio::test]
    async fn collisions_and_invalid_manifests_change_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let opts = InstallOptions::default();
        let builtin = M.replace("text.ext_echo", "text.builtin_x");
        let err = install(&e, dir.path(), &builtin, opts).await.unwrap_err();
        assert!(err.to_string().contains("built-in"), "{err}");
        let bad = M.replace("program = \"echo\"", "program = \"bash\"");
        assert!(install(&e, dir.path(), &bad, opts).await.is_err());
        let unknown = M.replace("params = { word = \"hi\" }", "params = { nope = 1 }");
        let err = install(&e, dir.path(), &unknown, opts).await.unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
        assert!(files(dir.path()).is_empty());

        install(&e, dir.path(), M, opts).await.unwrap();
        let before = files(dir.path());
        let trust = std::fs::read_to_string(dir.path().join("trusted.toml")).unwrap();
        let v2 = M.replace("1.2.0", "1.3.0");
        let err = install(&e, dir.path(), &v2, opts).await.unwrap_err();
        assert!(err.to_string().contains("already installed"), "{err}");
        assert_eq!(files(dir.path()), before);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("trusted.toml")).unwrap(),
            trust
        );

        let r = install(&e, dir.path(), &v2, InstallOptions { replace: true })
            .await
            .unwrap();
        assert!(r.replaced && r.version == "1.3.0");
        assert_eq!(
            e.registry().get("text.ext_echo").unwrap().spec().version,
            "1.3.0"
        );
    }

    #[tokio::test]
    async fn uninstall_keeps_settings_unless_purged() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(dir.path());
        let settings = dir.path().join("settings").join("text.ext_echo.toml");
        for purge in [false, true] {
            install(&e, dir.path(), M, InstallOptions::default())
                .await
                .unwrap();
            std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
            std::fs::write(&settings, "word = \"x\"\n").unwrap();
            let r = uninstall(&e, dir.path(), "text.ext_echo", purge).unwrap();
            assert_eq!(r.removed, ["text.ext_echo.toml"]);
            assert!(e.registry().get("text.ext_echo").is_none());
            let trust = TrustStore::load(&dir.path().join("trusted.toml"));
            assert!(!trust.trusted.contains_key("text.ext_echo"));
            assert_eq!(settings.exists(), !purge);
        }
        assert!(matches!(
            uninstall(&e, dir.path(), "text.ext_echo", false),
            Err(ToolError::NotFound { .. })
        ));
        let err = uninstall(&e, dir.path(), "text.builtin_x", false).unwrap_err();
        assert!(err.to_string().contains("built-in"));
    }

    #[test]
    fn checks_compare_text_json_files_and_errors() {
        let t = |expect: &str| -> ManifestTest {
            toml::from_str(&format!("expect = {expect}")).unwrap()
        };
        let out = |text: &str| {
            let mut o = ToolOutput::new();
            o.text = Some(text.into());
            Ok(o)
        };
        assert!(check(&t(r#"{ text = "a" }"#), out("a\n")).is_ok());
        assert!(check(&t(r#"{ text = "a" }"#), out("b")).is_err());
        assert!(check(&t(r#"{ json = { "$.n" = 2.0 } }"#), out(r#"{"n":2}"#)).is_ok());
        assert!(check(&t(r#"{ json = { "$.m" = 2 } }"#), out(r#"{"n":2}"#))
            .unwrap_err()
            .contains("not found"));
        assert!(check(&t("{ files = 1 }"), out("")).is_err());
        let failed = || Err(ToolError::invalid("x", "bad"));
        assert!(check(&t(r#"{ error = "invalid_input" }"#), failed()).is_ok());
        assert!(check(&t("{}"), failed()).is_err());
        assert!(check(&t(r#"{ error = "invalid_input" }"#), out("")).is_err());
    }

    #[test]
    fn json_paths() {
        use crate::manifest::{json_path_segments, JsonStep};
        assert_eq!(json_path_segments("$").unwrap(), []);
        assert_eq!(
            json_path_segments("$.a[1][2].b").unwrap(),
            [
                JsonStep::Key("a".into()),
                JsonStep::Index(1),
                JsonStep::Index(2),
                JsonStep::Key("b".into())
            ]
        );
        assert!(json_path_segments("a..b").is_err());
        assert!(json_path_segments("a[x]").is_err());
        assert!(json_path_segments("a[1]b").is_err());
        let doc = serde_json::json!({"a": [{"b": true}]});
        assert_eq!(json_path_get(&doc, "a[0].b"), Some(&Value::Bool(true)));
        assert_eq!(json_path_get(&doc, "a[3]"), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dev_slot_loads_untrusted_reloads_and_promotes() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let e = engine(dir);
        let opts = InstallOptions::default();
        let r = install_dev(&e, dir, M, "token:t1", opts).await.unwrap();
        assert!(r.dev && r.loaded && !r.trusted && r.passed, "{r:?}");
        assert!(dir.join("dev/text.ext_echo.toml").exists());
        assert!(TrustStore::load(&dir.join("trusted.toml"))
            .trusted
            .is_empty());
        let spec = e.registry().get("text.ext_echo").unwrap().spec().clone();
        assert!(spec.dev);
        assert_eq!(spec.author.as_deref(), Some("token:t1"));
        let st = e.extensions().iter().find(|s| s.dev).cloned().unwrap();
        assert_eq!(st.file, "dev/text.ext_echo.toml");
        assert_eq!(st.author.as_deref(), Some("token:t1"));

        // Edited in place: the next reload picks it up with no trust step.
        let edited = M.replace("Echo", "Echo v2");
        std::fs::write(dir.join("dev/text.ext_echo.toml"), &edited).unwrap();
        assert!(e.reload().changed.contains(&"text.ext_echo".to_string()));
        assert!(e
            .registry()
            .get("text.ext_echo")
            .unwrap()
            .spec()
            .name
            .contains("v2"));

        // Another author cannot take the id; the same author replaces it.
        let err = install_dev(&e, dir, M, "token:t2", InstallOptions { replace: true })
            .await
            .unwrap_err();
        assert!(err.to_string().contains("another author"), "{err}");
        assert!(install_dev(&e, dir, M, "token:t1", opts).await.is_err());
        install_dev(&e, dir, M, "token:t1", InstallOptions { replace: true })
            .await
            .unwrap();

        // Built-ins are never shadowed.
        let err = install_dev(
            &e,
            dir,
            &M.replace("text.ext_echo", "text.builtin_x"),
            "local",
            opts,
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("cannot shadow the built-in"),
            "{err}"
        );

        // Promote: moved to the trusted slot and pinned.
        let r = promote(&e, dir, "text.ext_echo").await.unwrap();
        assert!(r.trusted && r.loaded && !r.dev && r.passed, "{r:?}");
        assert!(dir.join("text.ext_echo.toml").exists());
        assert!(!dir.join("dev/text.ext_echo.toml").exists());
        let trust = TrustStore::load(&dir.join("trusted.toml"));
        assert_eq!(trust.trusted.get("text.ext_echo"), Some(&r.sha256));
        assert!(DevAuthors::load(dir).unwrap().authors.is_empty());
        let spec = e.registry().get("text.ext_echo").unwrap().spec().clone();
        assert!(!spec.dev && spec.author.is_none());
        assert!(
            promote(&e, dir, "text.ext_echo").await.is_err(),
            "nothing left to promote"
        );

        // A dev copy may not shadow the trusted extension, even dropped in by hand.
        let err = install_dev(&e, dir, M, "token:t1", opts).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot shadow the installed extension"),
            "{err}"
        );
        std::fs::write(dir.join("dev/text.ext_echo.toml"), M).unwrap();
        e.reload();
        let st = e.extensions().iter().find(|s| s.dev).cloned().unwrap();
        assert!(!st.loaded);
        assert!(st.error.unwrap().contains("cannot shadow"));
        assert!(!e.registry().get("text.ext_echo").unwrap().spec().dev);

        // Uninstall removes both copies.
        let u = uninstall(&e, dir, "text.ext_echo", false).unwrap();
        assert_eq!(
            u.removed,
            vec!["text.ext_echo.toml", "dev/text.ext_echo.toml"]
        );
    }

    #[test]
    fn hand_placed_dev_manifests_belong_to_the_local_owner() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("dev")).unwrap();
        std::fs::write(dir.join("dev/wip.toml"), M).unwrap();
        let e = engine(dir);
        let spec = e.registry().get("text.ext_echo").unwrap().spec().clone();
        assert!(spec.dev);
        assert_eq!(spec.author.as_deref(), Some(LOCAL_AUTHOR));
        assert_eq!(spec.source, "extension:dev/wip.toml");
    }

    #[tokio::test]
    async fn dev_drafts_cannot_clash_or_be_handed_out() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::create_dir_all(dir.join("dev")).unwrap();
        std::fs::write(dir.join("dev/a.toml"), M).unwrap();
        // Same MCP name (`text_ext_echo`), different id.
        std::fs::write(
            dir.join("dev/b.toml"),
            M.replace("text.ext_echo", "text.ext-echo"),
        )
        .unwrap();
        let e = engine(dir);
        assert!(e.registry().get("text.ext_echo").is_some());
        assert!(e.registry().get("text.ext-echo").is_none());
        let b = e
            .extensions()
            .iter()
            .find(|s| s.file == "dev/b.toml")
            .cloned();
        assert!(b.is_some_and(|s| !s.loaded && s.error.unwrap().contains("clashes")));

        // Two drafts declaring one id: promotion refuses to pick.
        std::fs::write(dir.join("dev/b.toml"), M).unwrap();
        let err = promote(&e, dir, "text.ext_echo").await.unwrap_err();
        assert!(err.to_string().contains("more than one"), "{err}");
        std::fs::remove_file(dir.join("dev/b.toml")).unwrap();

        // A different extension already sits at the promotion target.
        std::fs::write(
            dir.join("text.ext_echo.toml"),
            M.replace("text.ext_echo", "text.other"),
        )
        .unwrap();
        assert!(promote(&e, dir, "text.ext_echo").await.is_err());
        assert!(dir.join("dev/a.toml").exists());
        std::fs::remove_file(dir.join("text.ext_echo.toml")).unwrap();

        // A damaged ownership file hides drafts from everyone.
        std::fs::write(dir.join("dev/authors.toml"), "authors = [").unwrap();
        e.reload();
        let spec = e.registry().get("text.ext_echo").unwrap().spec().clone();
        assert_eq!(
            spec.author.as_deref(),
            Some(crate::manifest::UNKNOWN_AUTHOR)
        );
    }
}
