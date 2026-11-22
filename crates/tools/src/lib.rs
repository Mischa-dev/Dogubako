//! Aggregates all builtin tool crates (feature-gated) and trusted extensions
//! into a single registry.

use dogubako_core::manifest::ExtensionStatus;
use dogubako_core::registry::Registry;
use dogubako_core::tool::Tool;
use std::path::Path;
use std::sync::Arc;

/// All builtin tools enabled at compile time.
pub fn builtin_tools() -> Vec<Arc<dyn Tool>> {
    #[allow(unused_mut)]
    let mut v: Vec<Arc<dyn Tool>> = Vec::new();
    #[cfg(feature = "image")]
    v.extend(dogubako_tools_image::tools());
    #[cfg(feature = "media")]
    v.extend(dogubako_tools_media::tools());
    #[cfg(feature = "docs")]
    v.extend(dogubako_tools_docs::tools());
    #[cfg(feature = "files")]
    v.extend(dogubako_tools_files::tools());
    #[cfg(feature = "text")]
    v.extend(dogubako_tools_text::tools());
    #[cfg(feature = "net")]
    v.extend(dogubako_tools_net::tools());
    #[cfg(feature = "system")]
    v.extend(dogubako_tools_system::tools());
    v
}

/// Host binaries call this first: when started as a tool worker
/// (`<exe> __dogubako-worker <name>`), it runs the worker and returns its
/// exit code; otherwise `None` and the host continues normally.
pub fn worker_main_from_args() -> Option<i32> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some(dogubako_core::tool::WORKER_ARG) {
        return None;
    }
    let name = args.next().unwrap_or_default();
    #[cfg(feature = "text")]
    if let Some(code) = dogubako_tools_text::run_worker(&name) {
        return Some(code);
    }
    eprintln!("unknown worker '{name}'");
    Some(2)
}

/// Registry with builtins plus trusted extensions from `extensions_dir`.
/// Changes whenever a file in the extensions folder (manifests, the trust
/// file) is added, removed or rewritten; polled to reload installed tools.
pub fn extensions_stamp(dir: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    fn walk(
        dir: &Path,
        depth: u32,
        out: &mut Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)>,
    ) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                if depth > 0 {
                    walk(&e.path(), depth - 1, out);
                }
            } else {
                out.push((e.path(), md.len(), md.modified().ok()));
            }
        }
    }
    let mut files = vec![];
    walk(dir, 2, &mut files);
    files.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    files.hash(&mut h);
    h.finish()
}

pub fn build_registry(extensions_dir: Option<&Path>) -> (Registry, Vec<ExtensionStatus>) {
    let mut reg = Registry::new();
    reg.register_all(builtin_tools());
    let statuses = extensions_dir
        .map(|dir| dogubako_core::extensions::load_into(&mut reg, dir))
        .unwrap_or_default();
    (reg, statuses)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_builtin_tool_is_in_one_builtin_pack() {
        let packs: Vec<&str> = dogubako_core::packs::builtin_packs()
            .iter()
            .map(|p| p.id)
            .collect();
        for t in builtin_tools() {
            let s = t.spec();
            assert!(
                packs.contains(&s.pack.as_str()),
                "{} has no built-in pack (pack '{}')",
                s.id,
                s.pack
            );
        }
    }

    #[test]
    fn registry_builds_without_conflicts() {
        let (reg, _) = build_registry(None);
        let mut names = std::collections::HashMap::new();
        for spec in reg.specs() {
            if let Some(other) = names.insert(spec.name.to_lowercase(), spec.id.clone()) {
                panic!("{} and {} share the name '{}'", other, spec.id, spec.name);
            }
        }
        for spec in reg.specs() {
            assert!(
                !spec.description.is_empty(),
                "{} has no description",
                spec.id
            );
            assert!(
                spec.id.starts_with(spec.category.id()) || spec.id.contains('.'),
                "{}",
                spec.id
            );
            // Every show_if must reference an existing parameter, and a
            // boolean condition must test a boolean parameter (the web form
            // compares values loosely, so `""` never equals `false`).
            for p in &spec.params {
                for cap in &p.max_if {
                    for q in std::iter::once(&cap.param).chain(&cap.unless_set) {
                        assert!(
                            spec.params.iter().any(|x| &x.name == q),
                            "{}.{}: max_if on unknown '{}'",
                            spec.id,
                            p.name,
                            q
                        );
                    }
                }
                let Some(cond) = &p.show_if else { continue };
                let target = spec
                    .params
                    .iter()
                    .find(|q| q.name == cond.param)
                    .unwrap_or_else(|| {
                        panic!(
                            "{}.{}: show_if on unknown '{}'",
                            spec.id, p.name, cond.param
                        )
                    });
                if cond.values.iter().any(|v| v.is_boolean()) {
                    assert!(
                        matches!(target.kind, dogubako_core::spec::ParamKind::Boolean),
                        "{}.{}: boolean show_if on non-boolean '{}'",
                        spec.id,
                        p.name,
                        cond.param
                    );
                }
            }
        }
    }
}
