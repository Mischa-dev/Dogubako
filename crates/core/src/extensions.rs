//! One-step extension lifecycle: install (validate, copy, pin, reload, test),
//! uninstall and test. Shared by the CLI, the server and the desktop app.

use crate::engine::{Engine, RunRequest};
use crate::error::{ToolError, ToolResult};
use crate::manifest::{
    json_path_get, parse_manifest, sha256_hex, ManifestTest, SettingsStore, TrustStore,
};
use crate::params::LocalFileResolver;
use crate::policy::NetPolicy;
use crate::policy::PermissionSet;
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
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ToolError::failed(format!("downloading manifest: {e}")))?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(too_big());
        }
        String::from_utf8(bytes.to_vec())
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
    let (tool, existing) = validate(engine, dir, text, opts)?;
    let id = tool.spec().id.clone();
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
    let statuses = engine.extensions();
    let status = statuses
        .iter()
        .find(|s| s.id.as_deref() == Some(id))
        .ok_or_else(|| ToolError::NotFound { id: id.to_string() })?;
    let trusted = TrustStore::load(&dir.join("trusted.toml"))
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
    if let Some(t) = engine.catalog().get(id) {
        if !is_extension(t.spec()) {
            return Err(ToolError::invalid(
                "id",
                format!("'{id}' is a built-in tool, not an extension"),
            ));
        }
    }
    let files = manifests_with_id(dir, id);
    let trust_path = dir.join("trusted.toml");
    let mut trust = TrustStore::load(&trust_path);
    let was_trusted = trust.trusted.remove(id).is_some();
    if files.is_empty() && !was_trusted {
        return Err(ToolError::NotFound { id: id.to_string() });
    }
    let mut removed = vec![];
    for p in &files {
        std::fs::remove_file(p)
            .map_err(|e| ToolError::failed(format!("removing {}: {e}", p.display())))?;
        removed.push(p.file_name().unwrap().to_string_lossy().to_string());
    }
    trust
        .save(&trust_path)
        .map_err(|e| ToolError::failed(format!("saving trust: {e}")))?;
    if purge {
        SettingsStore::for_extensions(dir).remove(id)?;
    }
    engine.reload();
    Ok(UninstallReport {
        id: id.to_string(),
        removed,
        purged: purge,
    })
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

/// Manifest files in `dir` whose (parseable) id is `id`.
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
    let mut out = vec![];
    for t in tests {
        out.push(run_test(engine, src, spec, t).await);
    }
    out
}

async fn run_test(
    engine: &Engine,
    src: &Source<'_>,
    spec: &ToolSpec,
    t: &ManifestTest,
) -> TestResult {
    let started = Instant::now();
    let result = |status, error: Option<String>| TestResult {
        name: t.name.clone(),
        status,
        error,
        duration_ms: started.elapsed().as_millis() as u64,
    };
    let tool: Arc<dyn Tool> = if t.http.is_some() || !t.settings.is_empty() {
        let own = parse_manifest(src.text, src.file)
            .map_err(|e| ToolError::invalid("manifest", e))
            .and_then(|tool| {
                let tool = tool.with_settings_store(SettingsStore::for_extensions(src.dir));
                let tool = match &t.http {
                    Some(fixture) => tool.with_recorded_response(fixture.response()),
                    None => tool,
                };
                if t.settings.is_empty() {
                    Ok(tool)
                } else {
                    tool.with_setting_values(&t.settings)
                }
            });
        match own {
            Ok(tool) => Arc::new(tool),
            Err(e) => return result(TestStatus::Failed, Some(e.to_string())),
        }
    } else {
        match engine.find(&spec.id) {
            Ok(tool) => tool,
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
    let resolver = LocalFileResolver::unrestricted();
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
                fs: None,
            },
        )
        .await;
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
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
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
    use crate::manifest::load_extensions;
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
                let (tools, mut st) = load_extensions(&d, false);
                for t in tools {
                    let id = t.spec().id.clone();
                    if let Err(e) = reg.register(t) {
                        st.iter_mut()
                            .filter(|s| s.id.as_deref() == Some(id.as_str()))
                            .for_each(|s| {
                                s.loaded = false;
                                s.error = Some(e.to_string());
                            });
                    }
                }
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
}
