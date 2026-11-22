//! `dogubako store`: search, install and update tools from the official store,
//! and build/test the store index from the repository's `store/` folder.

use crate::admin::{extensions_dir, print_report};
use crate::render::Out;
use clap::{Arg, ArgAction, ArgMatches, Command};
use dogubako_core::extensions as ext;
use dogubako_core::store::{self, LoadedIndex, StoreTool};
use serde_json::json;
use std::path::PathBuf;

pub fn store_cmd() -> Command {
    let dir = Arg::new("dir")
        .long("dir")
        .global(true)
        .help("Extensions directory [default: per-user extensions folder]");
    let url = Arg::new("url").long("url").global(true).help(
        "Store index URL [default: $DOGUBAKO_STORE_URL or the official store; empty turns it off]",
    );
    let src = Arg::new("store")
        .long("store")
        .default_value("store")
        .help("Folder of store tools (<id>/manifest.toml)");
    Command::new("store")
        .about("Find and install tools from the official Dogubako tool store")
        .subcommand_required(true)
        .arg(dir)
        .arg(url)
        .subcommand(
            Command::new("search")
                .about("Search the store (uses the cached index when offline)")
                .arg(Arg::new("query").num_args(0..)),
        )
        .subcommand(
            Command::new("list")
                .about("List tools installed from the store, with available updates"),
        )
        .subcommand(
            Command::new("install")
                .about(
                    "Install a store tool: check its SHA-256 against the index, then validate, \
                     trust, reload and test it",
                )
                .arg(Arg::new("id").required(true))
                .arg(
                    Arg::new("yes")
                        .long("yes")
                        .short('y')
                        .action(ArgAction::SetTrue)
                        .help("Accepted for scripts; store tools are reviewed and SHA-pinned"),
                ),
        )
        .subcommand(
            Command::new("update")
                .about("Update store tools that have a newer version (all, or one id)")
                .arg(Arg::new("id")),
        )
        .subcommand(
            Command::new("build")
                .about("Build the store index and manifests from a store folder")
                .arg(src.clone())
                .arg(
                    Arg::new("out")
                        .long("out")
                        .default_value("site/public/store")
                        .help("Where to write index.json and <id>/manifest.toml"),
                )
                .arg(
                    Arg::new("check")
                        .long("check")
                        .action(ArgAction::SetTrue)
                        .help("Write nothing; fail if the published index is stale"),
                ),
        )
        .subcommand(
            Command::new("test")
                .about("Install every store tool into a scratch folder and run its tests")
                .arg(src),
        )
}

fn url_of(m: &ArgMatches) -> Option<String> {
    match m.get_one::<String>("url") {
        Some(u) => store::resolve_url(Some(u)),
        None => store::local_url(),
    }
}

pub async fn run(m: &ArgMatches, out: &Out) -> i32 {
    match m.subcommand() {
        Some(("build", s)) => return build(s, out),
        Some(("test", s)) => return test_all(s, out).await,
        _ => {}
    }
    let Some(url) = url_of(m) else {
        return out.error(&store::disabled_error().to_string(), 3);
    };
    let dir = extensions_dir(m);
    let engine = crate::build_engine_with(dogubako_core::tool::Environment::Cli, dir.clone());
    let index = match store::load_index(&url, &dir, &engine.config.net).await {
        Ok(i) => i,
        Err(e) => return out.error(&e.to_string(), e.exit_code()),
    };
    if index.offline && !out.json && !out.quiet {
        eprintln!(
            "offline: using the store index cached {} ({})",
            ago(index.fetched_at),
            index.error.as_deref().unwrap_or("")
        );
    }
    match m.subcommand() {
        Some(("search", s)) => {
            let q: Vec<String> = s
                .get_many::<String>("query")
                .map(|v| v.cloned().collect())
                .unwrap_or_default();
            let hits = store::search(store::annotate(&index.index, &engine, &dir), &q.join(" "));
            print_tools(out, &index, &hits, "no store tools match");
            0
        }
        Some(("list", _)) => {
            let installed: Vec<StoreTool> = store::annotate(&index.index, &engine, &dir)
                .into_iter()
                .filter(|t| t.installed)
                .collect();
            print_tools(out, &index, &installed, "no store tools installed");
            0
        }
        Some(("install", s)) => {
            let id = s.get_one::<String>("id").unwrap();
            let replace = std::fs::metadata(dir.join(format!("{id}.toml"))).is_ok()
                && store::annotate(&index.index, &engine, &dir)
                    .iter()
                    .any(|t| t.entry.id == *id && t.installed);
            match store::install(&engine, &dir, &index, id, replace).await {
                Ok(r) => print_report(out, &r),
                Err(e) => print_report(
                    out,
                    &ext::ExtensionReport {
                        id: id.clone(),
                        error: Some(e.to_string()),
                        ..Default::default()
                    },
                ),
            }
        }
        Some(("update", s)) => {
            let id = s.get_one::<String>("id").map(String::as_str);
            match store::update(&engine, &dir, &index, id).await {
                Ok(reports) if reports.is_empty() => {
                    if out.json {
                        println!("[]");
                    } else {
                        println!("everything from the store is up to date");
                    }
                    0
                }
                Ok(reports) => {
                    if out.json {
                        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
                        return if reports.iter().all(|r| r.passed) {
                            0
                        } else {
                            1
                        };
                    }
                    reports
                        .iter()
                        .map(|r| print_report(out, r))
                        .max()
                        .unwrap_or(0)
                }
                Err(e) => out.error(&e.to_string(), e.exit_code()),
            }
        }
        _ => 2,
    }
}

fn ago(t: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(t);
    let s = now.saturating_sub(t);
    match s {
        0..=119 => "just now".into(),
        120..=7199 => format!("{} min ago", s / 60),
        7200..=172_799 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

fn print_tools(out: &Out, index: &LoadedIndex, tools: &[StoreTool], empty: &str) {
    if out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "index": index, "tools": tools })).unwrap()
        );
        return;
    }
    if tools.is_empty() {
        println!("{empty}");
    }
    for t in tools {
        let state = match (&t.installed_version, t.update_available) {
            (Some(v), true) => format!("  [installed {v}, update to {}]", t.entry.version),
            (Some(v), false) => format!("  [installed {v}]"),
            _ => String::new(),
        };
        println!("{} {}{state}", t.entry.id, t.entry.version);
        println!("    {}", t.entry.description);
    }
}

fn build(m: &ArgMatches, out: &Out) -> i32 {
    let src = PathBuf::from(m.get_one::<String>("store").unwrap());
    let dst = PathBuf::from(m.get_one::<String>("out").unwrap());
    let built = match store::build_index(&src) {
        Ok(b) => b,
        Err(e) => return out.error(&e, 1),
    };
    if m.get_flag("check") {
        let stale = built.stale(&dst);
        if stale.is_empty() {
            if !out.quiet {
                println!(
                    "store index is up to date ({} tools)",
                    built.index.tools.len()
                );
            }
            return 0;
        }
        eprintln!(
            "store index in {} is stale; run `dogubako store build` and commit:\n  {}",
            dst.display(),
            stale.join("\n  ")
        );
        return 1;
    }
    if let Err(e) = built.write(&dst) {
        return out.error(&format!("writing {}: {e}", dst.display()), 1);
    }
    if !out.quiet {
        println!(
            "wrote {} ({} tools)",
            dst.join("index.json").display(),
            built.index.tools.len()
        );
    }
    0
}

/// Install each store tool into its own scratch extensions folder and run
/// its tests; fails when any tool fails to load or a test fails.
async fn test_all(m: &ArgMatches, out: &Out) -> i32 {
    let src = PathBuf::from(m.get_one::<String>("store").unwrap());
    let built = match store::build_index(&src) {
        Ok(b) => b,
        Err(e) => return out.error(&e, 1),
    };
    let mut reports = vec![];
    for (rel, text) in &built.manifests {
        let id = rel.trim_end_matches("/manifest.toml").to_string();
        let scratch = match tempfile::Builder::new()
            .prefix("dogubako-store-test-")
            .tempdir()
        {
            Ok(d) => d,
            Err(e) => return out.error(&format!("creating a scratch folder: {e}"), 1),
        };
        let dir = scratch.path().to_path_buf();
        let engine = crate::build_engine_with(dogubako_core::tool::Environment::Cli, dir.clone());
        let r = match ext::install(&engine, &dir, text, ext::InstallOptions::default()).await {
            Ok(mut r) => {
                if !r.tests.iter().any(|t| t.status == ext::TestStatus::Passed) {
                    r.passed = false;
                    r.error.get_or_insert_with(|| {
                        "store tools need at least one offline test that runs and passes".into()
                    });
                }
                r
            }
            Err(e) => ext::ExtensionReport {
                id,
                error: Some(e.to_string()),
                ..Default::default()
            },
        };
        reports.push(r);
    }
    let ok = reports.iter().all(|r| r.passed);
    if out.json {
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    } else {
        for r in &reports {
            println!("== {}", r.id);
            print_report(out, r);
        }
        println!(
            "{} of {} store tools passed",
            reports.iter().filter(|r| r.passed).count(),
            reports.len()
        );
    }
    if ok {
        0
    } else {
        1
    }
}
