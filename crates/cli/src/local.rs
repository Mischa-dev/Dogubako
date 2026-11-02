//! Local model management.

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
                    if s.installed && !s.verified { " (no upstream checksum)" } else { "" }
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
                            if total > 0 {
                                b.set_position(done * 1000 / total);
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
