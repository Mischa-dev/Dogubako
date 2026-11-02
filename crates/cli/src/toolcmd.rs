//! Tool commands generated from specs, plus `tools` and `run`.

use crate::render::{self, Out};
use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, ArgMatches, Command};
use dogubako_core::engine::{Availability, Engine, RunRequest};
use dogubako_core::error::ToolError;
use dogubako_core::params::LocalFileResolver;
use dogubako_core::policy::PermissionSet;
use dogubako_core::spec::{ParamKind, ParamSpec, ToolSpec};
use dogubako_core::tool::{ProgressEvent, ProgressFn, ToolOutput};
use serde_json::{json, Map, Value};
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const RESERVED: &[&str] = &["json", "quiet", "verbose", "out", "force", "help", "version"];

pub fn output_arg() -> Arg {
    Arg::new("__out").long("out").short('o').value_name("PATH").help("Where to save output files (file for a single output, otherwise a folder) [default: current folder]")
}

pub fn force_arg() -> Arg {
    Arg::new("__force").long("force").short('f').action(ArgAction::SetTrue).help("Overwrite existing output files")
}

fn long_name(p: &ParamSpec) -> String {
    let kebab = p.name.replace('_', "-");
    if RESERVED.contains(&kebab.as_str()) {
        format!("param-{kebab}")
    } else {
        kebab
    }
}

fn help_for(p: &ParamSpec) -> String {
    let mut h = p.label.clone();
    if !p.description.is_empty() {
        h = format!("{h}. {}", p.description);
    }
    if let Some(d) = &p.default {
        if !p.is_file() && !d.is_null() && d != &json!("") {
            h.push_str(&format!(" [default: {}]", d.as_str().map(|s| s.to_string()).unwrap_or_else(|| d.to_string())));
        }
    }
    h
}

pub fn command_for(spec: &ToolSpec, avail: &Availability) -> Command {
    let name = spec.id.split_once('.').map(|(_, r)| r).unwrap_or(&spec.id).replace('_', "-");
    let mut about = spec.name.clone();
    if !avail.available {
        about.push_str(&format!(" [unavailable: {}]", avail.reason.clone().unwrap_or_default()));
    }
    let mut long_about = format!("{}\n\n{}", spec.name, spec.description);
    if !spec.requires.is_empty() {
        long_about.push_str(&format!("\n\nRequires: {}", spec.requires.join(", ")));
    }
    long_about.push_str(&format!("\nTool id: {}", spec.id));
    let mut c = Command::new(name).about(about).long_about(long_about);
    let mut has_positional_text = false;
    for p in &spec.params {
        let mut a = Arg::new(p.name.clone()).help(help_for(p));
        match &p.kind {
            ParamKind::File { multiple, .. } if p.primary => {
                a = a.value_name("FILE");
                a = if *multiple { a.num_args(1..) } else { a.num_args(1) };
                if *multiple {
                    a = a.action(ArgAction::Append);
                }
            }
            ParamKind::Text { .. } if p.primary && !has_positional_text => {
                has_positional_text = true;
                a = a.value_name("TEXT").help(format!("{} (reads stdin when omitted)", help_for(p)));
            }
            ParamKind::File { multiple, .. } => {
                a = a.long(long_name(p)).value_name("FILE");
                if *multiple {
                    a = a.action(ArgAction::Append);
                }
            }
            ParamKind::Boolean => {
                a = a.long(long_name(p)).num_args(0..=1).default_missing_value("true").value_name("BOOL").require_equals(true);
            }
            ParamKind::Enum { options, multi } => {
                let vals: Vec<String> = options.iter().map(|o| o.value.clone()).collect();
                a = a.long(long_name(p)).value_name("CHOICE");
                if *multi {
                    a = a.action(ArgAction::Append).value_delimiter(',');
                }
                a = a.value_parser(PossibleValuesParser::new(vals));
            }
            ParamKind::Path { .. } => a = a.long(long_name(p)).value_name("PATH"),
            ParamKind::Json { .. } => a = a.long(long_name(p)).value_name("JSON"),
            ParamKind::Secret => a = a.long(long_name(p)).value_name("SECRET"),
            _ => a = a.long(long_name(p)).value_name(p.name.to_uppercase()),
        }
        if p.advanced {
            a = a.hide_short_help(true);
        }
        c = c.arg(a);
    }
    c.arg(output_arg()).arg(force_arg())
}

fn collect_params(spec: &ToolSpec, m: &ArgMatches) -> Result<Map<String, Value>, ToolError> {
    let mut map = Map::new();
    for p in &spec.params {
        let vals: Vec<String> = match m.get_many::<String>(&p.name) {
            Some(v) => v.cloned().collect(),
            None => vec![],
        };
        if vals.is_empty() {
            continue;
        }
        let v = match &p.kind {
            ParamKind::File { multiple: true, .. } | ParamKind::Enum { multi: true, .. } => Value::Array(vals.into_iter().map(Value::String).collect()),
            ParamKind::File { .. } => Value::String(absolutize(&vals[0])),
            ParamKind::Path { .. } => Value::String(absolutize(&vals[0])),
            _ => Value::String(vals[0].clone()),
        };
        let v = match (&p.kind, v) {
            (ParamKind::File { multiple: true, .. }, Value::Array(a)) => Value::Array(a.into_iter().map(|x| Value::String(absolutize(x.as_str().unwrap_or("")))).collect()),
            (_, v) => v,
        };
        map.insert(p.name.clone(), v);
    }
    // Primary text from stdin when not given.
    if let Some(p) = spec.params.iter().find(|p| p.primary && matches!(p.kind, ParamKind::Text { .. })) {
        if !map.contains_key(&p.name) && !std::io::stdin().is_terminal() {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).map_err(|e| ToolError::input(format!("reading stdin: {e}")))?;
            map.insert(p.name.clone(), Value::String(s));
        }
    }
    Ok(map)
}

fn absolutize(p: &str) -> String {
    let path = Path::new(p);
    if path.is_absolute() {
        p.to_string()
    } else {
        std::env::current_dir().map(|d| d.join(path).to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string())
    }
}

pub async fn run_generated(engine: Engine, id: &str, m: &ArgMatches, out: &Out) -> i32 {
    let Some(tool) = engine.registry.get(id) else {
        return out.error(&format!("unknown tool {id}"), 2);
    };
    let spec = tool.spec().clone();
    let params = match collect_params(&spec, m) {
        Ok(p) => p,
        Err(e) => return out.tool_error(&e),
    };
    execute(engine, &spec, params, m.get_one::<String>("__out").cloned(), m.get_flag("__force"), out).await
}

pub async fn run_generic(engine: Engine, m: &ArgMatches, out: &Out) -> i32 {
    let id = m.get_one::<String>("tool").unwrap().clone();
    let Some(tool) = engine.registry.find(&id) else {
        let sugg: Vec<String> = engine.registry.search(&id.replace(['.', '_'], " ")).iter().take(5).map(|s| s.id.clone()).collect();
        return out.error(&format!("unknown tool '{id}'{}", if sugg.is_empty() { String::new() } else { format!(" (did you mean: {})", sugg.join(", ")) }), 2);
    };
    let spec = tool.spec().clone();
    let mut params = Map::new();
    if let Some(j) = m.get_one::<String>("params") {
        match serde_json::from_str::<Value>(j) {
            Ok(Value::Object(o)) => params = o,
            _ => return out.error("--params must be a JSON object", 2),
        }
    }
    for kv in m.get_many::<String>("set").into_iter().flatten() {
        let Some((k, v)) = kv.split_once('=') else {
            return out.error(&format!("--set expects KEY=VALUE, got '{kv}'"), 2);
        };
        let k = k.trim().replace('-', "_");
        let pv = match spec.param_spec(&k).map(|p| &p.kind) {
            Some(ParamKind::File { multiple, .. }) => {
                if *multiple {
                    Value::Array(v.split(',').map(|s| Value::String(absolutize(s.trim()))).collect())
                } else {
                    Value::String(absolutize(v))
                }
            }
            Some(ParamKind::Path { .. }) => Value::String(absolutize(v)),
            _ => Value::String(v.to_string()),
        };
        params.insert(k, pv);
    }
    execute(engine, &spec, params, m.get_one::<String>("__out").cloned(), m.get_flag("__force"), out).await
}

async fn execute(engine: Engine, spec: &ToolSpec, params: Map<String, Value>, dest: Option<String>, force: bool, out: &Out) -> i32 {
    let tmp = match tempfile::Builder::new().prefix("dogubako-out-").tempdir() {
        Ok(t) => t,
        Err(e) => return out.error(&format!("cannot create temp dir: {e}"), 1),
    };
    let bar = out.progress_bar(&spec.name);
    let progress: Option<ProgressFn> = bar.clone().map(|b| {
        Arc::new(move |ev: ProgressEvent| {
            if let Some(f) = ev.fraction {
                b.set_position((f * 1000.0) as u64);
            }
            if let Some(m) = ev.message {
                b.set_message(m);
            }
        }) as ProgressFn
    });
    let cancel = tokio_util::sync::CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            c2.cancel();
        }
    });
    let resolver = LocalFileResolver::unrestricted();
    let res = engine
        .run(RunRequest {
            tool_id: &spec.id,
            params,
            resolver: &resolver,
            out_dir: tmp.path().to_path_buf(),
            permissions: PermissionSet::all(),
            authorize: None,
            progress,
            cancel,
            timeout: None,
            fs: None,
        })
        .await;
    if let Some(b) = bar {
        b.finish_and_clear();
    }
    match res {
        Ok((output, stats)) => match save_outputs(&output, dest.as_deref(), force) {
            Ok(saved) => {
                render::print_output(spec, &output, &saved, stats.duration_ms, out);
                0
            }
            Err(e) => out.error(&e, 1),
        },
        Err(e) => out.tool_error(&e),
    }
}

/// Move artifacts from the temp dir to their destination; returns final paths.
fn save_outputs(output: &ToolOutput, dest: Option<&str>, force: bool) -> Result<Vec<PathBuf>, String> {
    if output.artifacts.is_empty() {
        return Ok(vec![]);
    }
    if dest == Some("-") {
        if output.artifacts.len() != 1 {
            return Err("'-o -' (stdout) requires exactly one output file".into());
        }
        let bytes = std::fs::read(&output.artifacts[0].path).map_err(|e| e.to_string())?;
        use std::io::Write;
        std::io::stdout().write_all(&bytes).map_err(|e| e.to_string())?;
        return Ok(vec![PathBuf::from("-")]);
    }
    let dest_path = dest.map(PathBuf::from);
    let (dir, single_name): (PathBuf, Option<PathBuf>) = match &dest_path {
        None => (std::env::current_dir().map_err(|e| e.to_string())?, None),
        Some(p) if p.is_dir() || dest.unwrap().ends_with('/') || dest.unwrap().ends_with('\\') => (p.clone(), None),
        Some(p) if output.artifacts.len() == 1 => (p.parent().filter(|x| !x.as_os_str().is_empty()).map(|x| x.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")), Some(p.clone())),
        Some(p) => (p.clone(), None),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut saved = vec![];
    for a in &output.artifacts {
        let mut target = single_name.clone().unwrap_or_else(|| dir.join(&a.name));
        if target.exists() && !force {
            let stem = target.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = target.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            let parent = target.parent().unwrap_or(Path::new(".")).to_path_buf();
            let mut i = 1;
            while target.exists() {
                target = parent.join(format!("{stem} ({i}){ext}"));
                i += 1;
            }
        }
        if std::fs::rename(&a.path, &target).is_err() {
            std::fs::copy(&a.path, &target).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
        }
        saved.push(target);
    }
    Ok(saved)
}

pub fn tools_cmd(engine: &Engine, m: &ArgMatches, out: &Out) -> i32 {
    match m.subcommand() {
        Some(("list", s)) => {
            let cat = s.get_one::<String>("category");
            let only_avail = s.get_flag("available");
            let infos: Vec<_> = engine
                .list()
                .into_iter()
                .filter(|i| cat.map(|c| i.spec.category.id() == c).unwrap_or(true))
                .filter(|i| !only_avail || i.availability.available)
                .collect();
            render::tool_list(&infos, out);
            0
        }
        Some(("search", s)) => {
            let q: Vec<String> = s.get_many::<String>("query").unwrap().cloned().collect();
            let infos: Vec<_> = engine.registry.search(&q.join(" ")).into_iter().map(|s| engine.info(s)).collect();
            render::tool_list(&infos, out);
            if infos.is_empty() {
                1
            } else {
                0
            }
        }
        Some(("info", s)) => {
            let id = s.get_one::<String>("tool").unwrap();
            match engine.registry.find(id) {
                Some(t) => {
                    render::tool_info(&engine.info(t.spec()), out);
                    0
                }
                None => out.error(&format!("unknown tool '{id}'"), 2),
            }
        }
        _ => 2,
    }
}
