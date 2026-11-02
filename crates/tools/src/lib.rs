//! Aggregates all builtin tool crates (feature-gated) and trusted extensions
//! into a single registry.

use dogubako_core::manifest::{load_extensions, ExtensionStatus};
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
pub fn build_registry(extensions_dir: Option<&Path>) -> (Registry, Vec<ExtensionStatus>) {
    let mut reg = Registry::new();
    reg.register_all(builtin_tools());
    let mut statuses = vec![];
    if let Some(dir) = extensions_dir {
        let (tools, st) = load_extensions(dir, false);
        statuses = st;
        for t in tools {
            let id = t.spec().id.clone();
            if let Err(e) = reg.register(t) {
                statuses
                    .iter_mut()
                    .filter(|s| s.id.as_deref() == Some(id.as_str()))
                    .for_each(|s| {
                        s.loaded = false;
                        s.error = Some(e.to_string());
                    });
            }
        }
    }
    (reg, statuses)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_builds_without_conflicts() {
        let (reg, _) = build_registry(None);
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
        }
    }
}
