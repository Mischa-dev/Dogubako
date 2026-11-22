//! Local model and pack management.

use crate::render::Out;
use clap::ArgMatches;
use dogubako_core::engine::Engine;
use dogubako_core::util::human_size;
use serde_json::json;

pub async fn models(engine: &Engine, m: &ArgMatches, out: &Out) -> i32 {
    match m.subcommand() {
        Some(("list", _)) => {
            let list = engine.models.list();
            if out.json {
                println!("{}", serde_json::to_string_pretty(&list).unwrap());
                return 0;
            }
            println!("Models directory: {}\n", engine.models.dir().display());
            for s in list {
                println!(
                    "  {} {:<32} {:>9}  {}{}",
                    if s.installed { "✓" } else { " " },
                    s.spec.id,
                    human_size(s.spec.size_bytes),
                    s.spec.description,
                    if s.installed && !s.verified {
                        " (no upstream checksum)"
                    } else {
                        ""
                    }
                );
            }
            0
        }
        Some(("download", s)) => {
            let id = s.get_one::<String>("id").unwrap();
            let bar = out.progress_bar(id);
            let b2 = bar.clone();
            let res = engine
                .models
                .download(
                    id,
                    move |done, total| {
                        if let Some(b) = &b2 {
                            if let Some(pos) = (done * 1000).checked_div(total) {
                                b.set_position(pos);
                            }
                            b.set_message(format!("{} / {}", human_size(done), human_size(total)));
                        }
                    },
                    Default::default(),
                )
                .await;
            if let Some(b) = bar {
                b.finish_and_clear();
            }
            match res {
                Ok(p) => {
                    if out.json {
                        println!("{}", json!({"ok": true, "path": p}));
                    } else {
                        out.info(&format!("downloaded {id} → {}", p.display()));
                    }
                    0
                }
                Err(e) => out.tool_error(&e),
            }
        }
        Some(("remove", s)) => {
            let id = s.get_one::<String>("id").unwrap();
            match engine.models.remove(id) {
                Ok(()) => {
                    out.info(&format!("removed {id}"));
                    0
                }
                Err(e) => out.tool_error(&e),
            }
        }
        _ => 2,
    }
}

pub fn packs(engine: &Engine, m: &ArgMatches, out: &Out) -> i32 {
    use dogubako_core::packs::{builtin_pack, packs_file, PackSelection, PACKS_ENV};
    let list = |engine: &Engine| {
        if out.json {
            println!("{}", serde_json::to_string_pretty(&engine.packs()).unwrap());
            return;
        }
        for p in engine.packs() {
            println!(
                "  {} {:<12} {:>4} tools  {:>9} download  {}",
                if p.enabled { "✓" } else { " " },
                p.id,
                p.tools,
                human_size(p.download_bytes),
                p.description
            );
        }
    };
    let (pack, on) = match m.subcommand() {
        Some(("list", _)) => {
            list(engine);
            return 0;
        }
        Some(("enable", s)) => (s.get_one::<String>("pack").unwrap(), true),
        Some(("disable", s)) => (s.get_one::<String>("pack").unwrap(), false),
        _ => return 2,
    };
    if builtin_pack(pack).is_none() {
        let known: Vec<_> = dogubako_core::packs::builtin_packs()
            .iter()
            .map(|p| p.id)
            .collect();
        return out.error(
            &format!("unknown pack '{pack}' (packs: {})", known.join(", ")),
            2,
        );
    }
    let file = packs_file(&dogubako_core::paths::AppDirs::default_local().config);
    let next = PackSelection::load(&file).with(pack, on);
    if let Err(e) = next.save(&file) {
        return out.error(&format!("could not save {}: {e}", file.display()), 1);
    }
    if std::env::var(PACKS_ENV).is_ok_and(|v| !v.trim().is_empty()) {
        out.info(&format!(
            "note: {PACKS_ENV} is set and overrides the saved packs"
        ));
    }
    engine.set_packs(engine.pack_selection().with(pack, on));
    let remove = !on && m.subcommand().unwrap().1.get_flag("remove-unused");
    let unused = if on {
        vec![]
    } else {
        engine.unused_downloads()
    };
    let removed = if remove {
        match engine.remove_unused(None) {
            Ok(r) => r,
            Err(e) => return out.tool_error(&e),
        }
    } else {
        vec![]
    };
    let unused_bytes: u64 = unused.iter().map(|u| u.bytes).sum();
    let freed: u64 = removed.iter().map(|u| u.bytes).sum();
    if out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "pack": pack,
                "enabled": on,
                "tools": engine.registry().len(),
                "unused": if remove { vec![] } else { unused },
                "unused_bytes": if remove { 0 } else { unused_bytes },
                "removed": removed,
                "freed_bytes": freed,
            }))
            .unwrap()
        );
        return 0;
    }
    out.info(&format!(
        "turned {} {pack}; {} tools installed",
        if on { "on" } else { "off" },
        engine.registry().len()
    ));
    if remove {
        for u in &removed {
            out.info(&format!(
                "removed {} {} ({})",
                u.kind,
                u.id,
                human_size(u.bytes)
            ));
        }
        out.info(&format!("freed {}", human_size(freed)));
    } else if !unused.is_empty() {
        for u in &unused {
            out.info(&format!(
                "  no longer used: {} {} ({})",
                u.kind,
                u.id,
                human_size(u.bytes)
            ));
        }
        out.info(&format!(
            "run `dogubako packs disable {pack} --remove-unused` to free {}",
            human_size(unused_bytes)
        ));
    }
    0
}
