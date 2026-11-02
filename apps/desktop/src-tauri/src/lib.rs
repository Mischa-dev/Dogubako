//! Dogubako desktop: the shared React UI in a Tauri webview, calling the
//! tool engine in-process (no HTTP server). See docs/FRONTEND_CONTRACT.md.

use dogubako_core::engine::{Engine, EngineConfig, RunRequest};
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

#[tauri::command]
fn app_info(state: State<'_, Desktop>) -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "platform": dogubako_core::spec::Platform::current(),
        "tools": state.engine.registry.len(),
        "output_dir": state.output_dir,
        "data_dir": state.dirs.data,
        "mode": "desktop",
    })
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
    let cancel = CancellationToken::new();
    state.runs.lock().unwrap().insert(run_id.clone(), cancel.clone());
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
                    .map(|a| ArtifactView { id: None, name: a.name, mime: a.mime, size: a.size, label: a.label, url: None, path: Some(a.path.to_string_lossy().to_string()) })
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

#[tauri::command]
fn cancel_run(state: State<'_, Desktop>, run_id: String) {
    if let Some(c) = state.runs.lock().unwrap().get(&run_id) {
        c.cancel();
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

#[tauri::command]
fn reveal_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn deps_status(state: State<'_, Desktop>) -> Vec<Value> {
    let mut used_by: HashMap<String, Vec<String>> = HashMap::new();
    for s in state.engine.registry.specs() {
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
    let (registry, _ext) = dogubako_tools::build_registry(Some(&dirs.extensions));
    let mut cfg = EngineConfig::local(Environment::Desktop);
    cfg.models_dir = dirs.models.clone();
    cfg.temp_root = std::env::temp_dir().join("dogubako");
    cfg.worker_exe = std::env::current_exe().ok();
    let engine = Arc::new(Engine::new(registry, cfg, vec![dirs.bin.clone()]));
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_tools,
            run_tool,
            cancel_run,
            stage_file,
            file_info,
            save_artifact,
            reveal_path,
            deps_status,
            deps_refresh,
            models_list,
            model_download,
            model_remove
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dogubako");
}
