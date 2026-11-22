//! Dogubako desktop: the shared React UI in a Tauri webview, calling the
//! tool engine in-process (no HTTP server). See docs/FRONTEND_CONTRACT.md.

use dogubako_core::deps::DepRegistry;
use dogubako_core::engine::{Engine, EngineConfig, RunRequest};
use dogubako_core::engines::{default_bundled_root, ManagedEngines};
use dogubako_core::error::ToolError;
use dogubako_core::params::LocalFileResolver;
use dogubako_core::paths::AppDirs;
use dogubako_core::policy::PermissionSet;
use dogubako_core::tool::{Environment, ProgressEvent, ProgressFn};
use dogubako_mcp::{ArtifactView, JobStatus, JobView, ResultView};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_util::sync::CancellationToken;

struct Desktop {
    engine: Arc<Engine>,
    dirs: AppDirs,
    output_dir: PathBuf,
    staging_dir: PathBuf,
    runs: Mutex<HashMap<String, CancellationToken>>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn valid_run_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

const TOOLS_CHANGED: &str = "dogubako://tools-changed";

/// Rebuild the tool registry (builtins, trusted extensions, enabled packs)
/// without restarting; runs in progress keep the tool they started with.
#[tauri::command]
async fn reload_tools(app: tauri::AppHandle, state: State<'_, Desktop>) -> Result<Value, String> {
    let engine = state.engine.clone();
    let report = tauri::async_runtime::spawn_blocking(move || engine.reload()).await.map_err(|e| e.to_string())?;
    if report.tool_set_changed() {
        let _ = app.emit(TOOLS_CHANGED, &report);
    }
    Ok(json!({ "reload": report, "extensions": *state.engine.extensions() }))
}

/// Reload when the extensions folder changes (a manifest added, removed or
/// edited, or the trust file updated), and tell the UI.
fn watch_extensions(app: tauri::AppHandle, engine: Arc<Engine>, dir: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let mut last = dogubako_tools::extensions_stamp(&dir);
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let stamp = dogubako_tools::extensions_stamp(&dir);
            if stamp == last {
                continue;
            }
            last = stamp;
            let e = engine.clone();
            if let Ok(report) = tauri::async_runtime::spawn_blocking(move || e.reload()).await {
                if report.tool_set_changed() {
                    let _ = app.emit(TOOLS_CHANGED, &report);
                }
            }
        }
    });
}

#[tauri::command]
fn app_info(state: State<'_, Desktop>) -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": dogubako_core::spec::Platform::current(),
        "tools": state.engine.registry().len(),
        "output_dir": state.output_dir,
        "data_dir": state.dirs.data,
        "mode": "desktop",
    })
}

/// Built-in tool packs (same shape as `GET /api/v1/packs` entries).
#[tauri::command]
fn list_packs(state: State<'_, Desktop>) -> Vec<dogubako_core::packs::PackInfo> {
    state.engine.packs()
}

fn packs_view(engine: &Engine) -> Value {
    let unused = engine.unused_downloads();
    let unused_bytes: u64 = unused.iter().map(|u| u.bytes).sum();
    json!({ "packs": engine.packs(), "unused": unused, "unused_bytes": unused_bytes })
}

/// Packs plus downloads no turned-on pack uses, and whether the install
/// choice (Everything or Custom) has been made yet.
#[tauri::command]
fn packs_status(state: State<'_, Desktop>) -> Value {
    let mut v = packs_view(&state.engine);
    v["chosen"] = json!(dogubako_core::packs::packs_file(&state.dirs.config).exists());
    v
}

/// Save the pack selection (`null` = every pack) and apply it now.
#[tauri::command]
async fn set_packs(app: tauri::AppHandle, state: State<'_, Desktop>, enabled: Option<Vec<String>>) -> Result<Value, String> {
    use dogubako_core::packs::{packs_file, PackSelection};
    let sel = match enabled {
        None => PackSelection::all(),
        Some(ids) => {
            let sel = PackSelection::only(ids);
            if let Some(bad) = sel.unknown().first() {
                return Err(format!("unknown pack '{bad}'"));
            }
            sel
        }
    };
    let engine = state.engine.clone();
    let file = packs_file(&state.dirs.config);
    let report = tauri::async_runtime::spawn_blocking(move || {
        engine.edit_packs(|_| sel, |next| next.save(&file))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    if report.tool_set_changed() {
        let _ = app.emit(TOOLS_CHANGED, &report);
    }
    let mut v = packs_view(&state.engine);
    v["reload"] = json!(report);
    Ok(v)
}

/// Delete engines and models no turned-on pack uses (all when `ids` is null).
#[tauri::command]
fn packs_cleanup(state: State<'_, Desktop>, ids: Option<Vec<String>>) -> Result<Value, String> {
    if state.runs.lock().unwrap().values().any(|t| !t.is_cancelled()) {
        return Err("Tools are running; delete unused downloads once they finish".into());
    }
    let removed = state.engine.remove_unused(ids.as_deref()).map_err(|e| e.to_string())?;
    let freed: u64 = removed.iter().map(|u| u.bytes).sum();
    let mut v = packs_view(&state.engine);
    v["removed"] = json!(removed);
    v["freed_bytes"] = json!(freed);
    Ok(v)
}

#[tauri::command]
fn list_tools(state: State<'_, Desktop>) -> Vec<Value> {
    state
        .engine
        .list()
        .into_iter()
        .map(|info| {
            let mut v = serde_json::to_value(&info).unwrap_or_default();
            v["permitted"] = json!(true);
            v["input_schema"] = dogubako_core::schema::input_schema(&info.spec, "path");
            v
        })
        .collect()
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[tauri::command]
async fn run_tool(app: AppHandle, state: State<'_, Desktop>, run_id: String, tool_id: String, params: Map<String, Value>) -> Result<JobView, String> {
    if !valid_run_id(&run_id) {
        return Err("invalid run id".into());
    }
    // cancel_run may have arrived first (both commands are async IPC): reuse
    // its already-cancelled token instead of replacing it.
    let cancel = state.runs.lock().unwrap().entry(run_id.clone()).or_default().clone();
    let out_dir = state.output_dir.join(&run_id);
    let rid = run_id.clone();
    let app2 = app.clone();
    let progress: ProgressFn = Arc::new(move |ev: ProgressEvent| {
        let _ = app2.emit("dogubako://progress", ProgressPayload { run_id: rid.clone(), progress: ev.fraction, message: ev.message });
    });
    let engine = state.engine.clone();
    let created = now();
    let resolver = LocalFileResolver::unrestricted();
    let res = engine
        .run(RunRequest {
            tool_id: &tool_id,
            params: params.clone(),
            resolver: &resolver,
            out_dir: out_dir.clone(),
            permissions: PermissionSet::all(),
            authorize: None,
            progress: Some(progress),
            cancel,
            timeout: None,
            fs: None,
        })
        .await;
    state.runs.lock().unwrap().remove(&run_id);
    let mut view = JobView {
        id: run_id,
        tool_id,
        status: JobStatus::Succeeded,
        progress: Some(1.0),
        message: None,
        result: None,
        error: None,
        created_at: created,
        finished_at: Some(now()),
        params: Value::Object(params),
        owner: None,
    };
    match res {
        Ok((out, stats)) => {
            if out.artifacts.is_empty() {
                let _ = std::fs::remove_dir(&out_dir);
            } else {
                let _ = app.asset_protocol_scope().allow_directory(&out_dir, true);
            }
            view.result = Some(ResultView {
                text: out.text,
                text_mime: out.text_mime,
                data: out.data,
                artifacts: out
                    .artifacts
                    .into_iter()
                    .map(|a| ArtifactView { id: None, name: a.name, mime: a.mime, size: a.size, label: a.label, url: None, path: Some(a.path.to_string_lossy().to_string()), rel_path: a.rel_path })
                    .collect(),
                warnings: out.warnings,
                duration_ms: stats.duration_ms,
            });
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&out_dir);
            view.status = if matches!(e, ToolError::Cancelled) { JobStatus::Cancelled } else { JobStatus::Failed };
            view.progress = None;
            view.error = Some(e);
        }
    }
    Ok(view)
}

/// Pre-run check of a tool's inputs; `None` when they look fine.
#[tauri::command]
async fn check_tool(state: State<'_, Desktop>, tool_id: String, params: Map<String, Value>) -> Result<Option<ToolError>, String> {
    let resolver = LocalFileResolver::unrestricted();
    let res = state
        .engine
        .check(RunRequest {
            tool_id: &tool_id,
            params,
            resolver: &resolver,
            out_dir: state.output_dir.clone(),
            permissions: PermissionSet::all(),
            authorize: None,
            progress: None,
            cancel: Default::default(),
            timeout: None,
            fs: None,
        })
        .await;
    Ok(res.err())
}

#[tauri::command]
fn extension_settings(state: State<'_, Desktop>) -> Vec<dogubako_core::manifest::ExtensionSettings> {
    dogubako_core::manifest::extension_settings(&state.dirs.extensions)
}

#[tauri::command]
fn configure_extension(state: State<'_, Desktop>, id: String, values: Map<String, Value>) -> Result<dogubako_core::manifest::ExtensionSettings, String> {
    dogubako_core::manifest::configure_extension(&state.dirs.extensions, &id, &values).map_err(|e| e.to_string())
}

/// Install a manifest (file path or http(s) URL) in one step: validate,
/// copy, trust, reload and run its tests.
#[tauri::command]
async fn install_extension(app: tauri::AppHandle, state: State<'_, Desktop>, source: String, replace: Option<bool>) -> Result<dogubako_core::extensions::ExtensionReport, String> {
    use dogubako_core::extensions as ext;
    let text = ext::fetch_manifest(&source, &state.engine.config.net).await.map_err(|e| e.to_string())?;
    let opts = ext::InstallOptions { replace: replace.unwrap_or(false) };
    let report = ext::install(&state.engine, &state.dirs.extensions, &text, opts).await.map_err(|e| e.to_string())?;
    let _ = app.emit(TOOLS_CHANGED, json!({ "added": [report.id.clone()] }));
    Ok(report)
}

/// Remove an extension and its trust entry; settings are kept unless `purge`.
#[tauri::command]
fn uninstall_extension(app: tauri::AppHandle, state: State<'_, Desktop>, id: String, purge: Option<bool>) -> Result<dogubako_core::extensions::UninstallReport, String> {
    let report = dogubako_core::extensions::uninstall(&state.engine, &state.dirs.extensions, &id, purge.unwrap_or(false)).map_err(|e| e.to_string())?;
    let _ = app.emit(TOOLS_CHANGED, json!({ "removed": [id] }));
    Ok(report)
}

/// Install a manifest into the dev slot: untrusted, reloaded when edited,
/// hidden from agents until promoted.
#[tauri::command]
async fn install_dev_extension(app: tauri::AppHandle, state: State<'_, Desktop>, source: String, replace: Option<bool>) -> Result<dogubako_core::extensions::ExtensionReport, String> {
    use dogubako_core::extensions as ext;
    let text = ext::fetch_manifest(&source, &state.engine.config.net).await.map_err(|e| e.to_string())?;
    let opts = ext::InstallOptions { replace: replace.unwrap_or(false) };
    let author = dogubako_core::manifest::LOCAL_AUTHOR;
    let report = ext::install_dev(&state.engine, &state.dirs.extensions, &text, author, opts).await.map_err(|e| e.to_string())?;
    let _ = app.emit(TOOLS_CHANGED, json!({ "added": [report.id.clone()] }));
    Ok(report)
}

/// Promote a dev tool to trusted (the desktop user is the owner).
#[tauri::command]
async fn promote_extension(app: tauri::AppHandle, state: State<'_, Desktop>, id: String) -> Result<dogubako_core::extensions::ExtensionReport, String> {
    let report = dogubako_core::extensions::promote(&state.engine, &state.dirs.extensions, &id).await.map_err(|e| e.to_string())?;
    let _ = app.emit(TOOLS_CHANGED, json!({ "changed": [id] }));
    Ok(report)
}

/// Run an installed extension's tests without reinstalling it.
#[tauri::command]
async fn test_extension(state: State<'_, Desktop>, id: String) -> Result<dogubako_core::extensions::ExtensionReport, String> {
    dogubako_core::extensions::test(&state.engine, &state.dirs.extensions, &id).await.map_err(|e| e.to_string())
}

async fn store_index(state: &Desktop) -> Result<dogubako_core::store::LoadedIndex, String> {
    let url = dogubako_core::store::local_url().ok_or_else(|| dogubako_core::store::disabled_error().to_string())?;
    dogubako_core::store::load_index(&url, &state.dirs.extensions, &state.engine.config.net).await.map_err(|e| e.to_string())
}

/// Tools not installed here: turned-off packs and store tools from the cached
/// index (refreshed in the background when stale).
#[tauri::command]
async fn available_tools(state: State<'_, Desktop>) -> Result<Value, String> {
    use dogubako_core::store;
    let ext = &state.dirs.extensions;
    let url = store::local_url();
    let index = url.as_deref().and_then(|u| store::cached_index(u, ext));
    if let Some(url) = url.filter(|u| store::cache_is_stale(u, ext, 3600)) {
        let (ext, net) = (ext.clone(), state.engine.config.net.clone());
        tauri::async_runtime::spawn(async move {
            let _ = store::load_index(&url, &ext, &net).await;
        });
    }
    let tools: Vec<Value> = store::available(&state.engine, index.as_ref(), |_| true)
        .into_iter()
        .map(|t| {
            let mut v = serde_json::to_value(&t).unwrap_or_default();
            v["can_install"] = json!(true);
            v
        })
        .collect();
    Ok(json!({ "tools": tools }))
}

/// Search the tool store (cached index when offline).
#[tauri::command]
async fn store_search(state: State<'_, Desktop>, query: Option<String>) -> Result<Value, String> {
    use dogubako_core::store;
    let index = store_index(&state).await?;
    let tools = store::search(store::annotate(&index.index, &state.engine, &state.dirs.extensions), query.as_deref().unwrap_or(""));
    Ok(json!({ "index": index, "tools": tools }))
}

/// Tools installed from the store with their version and available updates.
#[tauri::command]
async fn store_installed(state: State<'_, Desktop>) -> Result<Value, String> {
    use dogubako_core::store;
    let index = store_index(&state).await?;
    let tools: Vec<_> = store::annotate(&index.index, &state.engine, &state.dirs.extensions).into_iter().filter(|t| t.installed).collect();
    Ok(json!({ "index": index, "tools": tools }))
}

/// Install a store tool after checking its SHA-256 against the index.
#[tauri::command]
async fn store_install(app: tauri::AppHandle, state: State<'_, Desktop>, id: String) -> Result<dogubako_core::extensions::ExtensionReport, String> {
    let index = store_index(&state).await?;
    let report = dogubako_core::store::install(&state.engine, &state.dirs.extensions, &index, &id, false).await.map_err(|e| e.to_string())?;
    let _ = app.emit(TOOLS_CHANGED, json!({ "added": [report.id.clone()] }));
    Ok(report)
}

/// Update store tools with a newer version in the index (one, or all).
#[tauri::command]
async fn store_update(app: tauri::AppHandle, state: State<'_, Desktop>, id: Option<String>) -> Result<Value, String> {
    let index = store_index(&state).await?;
    let reports = dogubako_core::store::update(&state.engine, &state.dirs.extensions, &index, id.as_deref()).await.map_err(|e| e.to_string())?;
    let changed: Vec<String> = reports.iter().map(|r| r.id.clone()).collect();
    let _ = app.emit(TOOLS_CHANGED, json!({ "changed": changed }));
    Ok(json!({ "updated": reports }))
}

#[tauri::command]
fn cancel_run(state: State<'_, Desktop>, run_id: String) {
    // The web app only cancels runs it has started, so a missing entry means
    // run_tool has not registered yet: leave a cancelled token for it.
    if valid_run_id(&run_id) {
        state.runs.lock().unwrap().entry(run_id).or_default().cancel();
    }
}

#[tauri::command]
fn stage_file(app: AppHandle, state: State<'_, Desktop>, name: String, bytes: Vec<u8>) -> Result<Value, String> {
    let dir = state.staging_dir.join(dogubako_core::util::random_id(6));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let clean = dogubako_core::policy::sanitize_filename(&name);
    let path = dir.join(&clean);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    let _ = app.asset_protocol_scope().allow_file(&path);
    Ok(json!({"path": path, "name": clean, "size": bytes.len()}))
}

#[tauri::command]
fn file_info(app: AppHandle, path: String) -> Result<Value, String> {
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("{path}: {e}"))?;
    if meta.is_file() {
        // Files the user explicitly chose may be previewed.
        let _ = app.asset_protocol_scope().allow_file(&p);
    }
    Ok(json!({
        "path": p,
        "name": p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        "size": meta.len(),
        "is_dir": meta.is_dir(),
        "mime": dogubako_core::util::mime_for_path(&p),
    }))
}

#[tauri::command]
fn save_artifact(state: State<'_, Desktop>, src: String, dest: String) -> Result<(), String> {
    let src = std::fs::canonicalize(&src).map_err(|e| e.to_string())?;
    let out = std::fs::canonicalize(&state.output_dir).map_err(|e| e.to_string())?;
    if !src.starts_with(&out) {
        return Err("can only save files produced by Dogubako".into());
    }
    std::fs::copy(&src, &dest).map(|_| ()).map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
struct SaveItem {
    path: String,
    name: String,
    rel_path: Option<String>,
}

/// Copy a whole result set into a folder the user picked, keeping the
/// folders in `rel_path`. Returns how many files were saved.
#[tauri::command]
fn save_artifacts(state: State<'_, Desktop>, items: Vec<SaveItem>, dest_dir: String) -> Result<usize, String> {
    let out = std::fs::canonicalize(&state.output_dir).map_err(|e| e.to_string())?;
    let mut pairs = Vec::with_capacity(items.len());
    for it in items {
        let src = std::fs::canonicalize(&it.path).map_err(|e| e.to_string())?;
        if !src.starts_with(&out) {
            return Err("can only save files produced by Dogubako".into());
        }
        pairs.push((src, it.rel_path.unwrap_or(it.name)));
    }
    dogubako_core::util::copy_files_into(&pairs, std::path::Path::new(&dest_dir))
        .map(|w| w.len())
        .map_err(|e| e.to_string())
}

/// Remove the output folder of one run (plain-words Undo). Only run folders
/// directly inside the output directory can be removed.
#[tauri::command]
fn discard_run_outputs(state: State<'_, Desktop>, run_id: String) -> Result<(), String> {
    if !valid_run_id(&run_id) {
        return Err("invalid run id".into());
    }
    let dir = state.output_dir.join(&run_id);
    match std::fs::symlink_metadata(&dir) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(&dir).map_err(|e| e.to_string()),
        Ok(_) => Err("not a run folder".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn deps_status(state: State<'_, Desktop>) -> Vec<Value> {
    let mut used_by: HashMap<String, Vec<String>> = HashMap::new();
    for s in state.engine.registry().specs() {
        for d in &s.requires {
            used_by.entry(d.clone()).or_default().push(s.id.clone());
        }
    }
    state
        .engine
        .deps
        .all()
        .into_iter()
        .map(|d| {
            let mut v = serde_json::to_value(&d).unwrap_or_default();
            v["used_by"] = json!(used_by.get(&d.id).cloned().unwrap_or_default());
            v
        })
        .collect()
}

#[tauri::command]
fn deps_refresh(state: State<'_, Desktop>) {
    state.engine.deps.refresh();
}

#[tauri::command]
fn models_list(state: State<'_, Desktop>) -> Value {
    json!({"models": state.engine.models.list(), "downloads": []})
}

#[tauri::command]
async fn model_download(app: AppHandle, state: State<'_, Desktop>, id: String) -> Result<(), String> {
    let engine = state.engine.clone();
    let app2 = app.clone();
    let mid = id.clone();
    engine
        .models
        .download(
            &id,
            move |done, total| {
                let _ = app2.emit("dogubako://model-progress", json!({"id": mid, "done": done, "total": total}));
            },
            CancellationToken::new(),
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Natural-language assistant: relay one chat completion to the endpoint the
/// user picked. The desktop app is the user's own machine, so local runtimes
/// (Ollama, LM Studio) on loopback or the LAN are allowed.
#[tauri::command]
async fn assistant_chat(base_url: String, api_key: Option<String>, body: Value) -> Result<Value, Value> {
    let ep = dogubako_core::llm::Endpoint { base_url, api_key };
    dogubako_core::llm::chat(dogubako_core::policy::NetPolicy::default(), &ep, body)
        .await
        .map_err(|e| dogubako_core::llm::error_json(&e))
}

/// The endpoint's model list, for the model picker.
#[tauri::command]
async fn assistant_models(base_url: String, api_key: Option<String>) -> Result<Value, Value> {
    let ep = dogubako_core::llm::Endpoint { base_url, api_key };
    dogubako_core::llm::list_models(dogubako_core::policy::NetPolicy::default(), &ep)
        .await
        .map(|m| json!({ "models": m }))
        .map_err(|e| dogubako_core::llm::error_json(&e))
}

#[tauri::command]
fn engines_list(state: State<'_, Desktop>) -> Value {
    let m = state.engine.deps.managed();
    json!({"mode": m.mode(), "engines": m.list()})
}

/// Download an on-demand engine into the app's own folder (pinned version,
/// checksum-verified), then re-detect dependencies.
#[tauri::command]
async fn engine_install(app: AppHandle, state: State<'_, Desktop>, id: String) -> Result<(), String> {
    let engine = state.engine.clone();
    let eid = id.clone();
    let result = engine
        .deps
        .managed()
        .install(
            &id,
            move |done, total| {
                let _ = app.emit("dogubako://engine-progress", json!({"id": eid, "done": done, "total": total}));
            },
            CancellationToken::new(),
        )
        .await
        .map(|_| ())
        .map_err(|e| e.to_string());
    engine.deps.refresh();
    result
}

#[tauri::command]
fn engine_remove(state: State<'_, Desktop>, id: String) -> Result<(), String> {
    let r = state.engine.deps.managed().remove(&id).map_err(|e| e.to_string());
    state.engine.deps.refresh();
    r
}

#[tauri::command]
fn model_remove(state: State<'_, Desktop>, id: String) -> Result<(), String> {
    state.engine.models.remove(&id).map_err(|e| e.to_string())
}

pub fn run() {
    // Started as a tool worker (killable jq evaluation etc.)?
    if let Some(code) = dogubako_tools::worker_main_from_args() {
        std::process::exit(code);
    }
    let dirs = AppDirs::default_local();
    let mut cfg = EngineConfig::local(Environment::Desktop);
    cfg.models_dir = dirs.models.clone();
    cfg.temp_root = std::env::temp_dir().join("dogubako");
    cfg.worker_exe = std::env::current_exe().ok();
    cfg.packs = dogubako_core::packs::PackSelection::local(&dirs);
    // Pinned engines shipped in the app bundle, plus on-demand ones it
    // downloads into its own folder (docs/ENGINES.md).
    let managed = ManagedEngines::load(default_bundled_root(), dirs.data.join("engines"));
    let deps = DepRegistry::with_managed(vec![dirs.bin.clone()], managed);
    let ext_dir = dirs.extensions.clone();
    let engine = Arc::new(Engine::from_loader(cfg, deps, Arc::new(move || dogubako_tools::build_registry(Some(&ext_dir)))));
    let watched = (engine.clone(), dirs.extensions.clone());
    let output_dir = dirs.data.join("outputs");
    let staging_dir = std::env::temp_dir().join("dogubako-staging");
    let _ = std::fs::create_dir_all(&output_dir);
    let _ = std::fs::create_dir_all(&staging_dir);
    let state = Desktop { engine, dirs, output_dir: output_dir.clone(), staging_dir: staging_dir.clone(), runs: Mutex::new(HashMap::new()) };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(move |app| {
            let scope = app.asset_protocol_scope();
            let _ = scope.allow_directory(&output_dir, true);
            let _ = scope.allow_directory(&staging_dir, true);
            watch_extensions(app.handle().clone(), watched.0.clone(), watched.1.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            available_tools,
            store_search,
            store_installed,
            store_install,
            store_update,
            list_tools,
            list_packs,
            packs_status,
            set_packs,
            packs_cleanup,
            run_tool,
            check_tool,
            cancel_run,
            stage_file,
            file_info,
            save_artifact,
            save_artifacts,
            reveal_path,
            deps_status,
            deps_refresh,
            models_list,
            model_download,
            model_remove,
            engines_list,
            engine_install,
            engine_remove,
            assistant_chat,
            assistant_models,
            extension_settings,
            configure_extension,
            install_extension,
            uninstall_extension,
            test_extension,
            install_dev_extension,
            promote_extension,
            discard_run_outputs,
            reload_tools
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dogubako");
}
