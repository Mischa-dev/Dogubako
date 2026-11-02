//! Text, developer and data utilities.

use dogubako_core::prelude::*;

mod data;
pub(crate) mod dev;
mod escape;
mod sheet;
mod text;
mod util;

/// Run a named worker (see `ToolContext::worker`); `None` if unknown.
pub fn run_worker(name: &str) -> Option<i32> {
    match name {
        "jq" => Some(dev::query::jq_worker_main()),
        _ => None,
    }
}

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = Vec::new();
    v.extend(text::tools());
    v.extend(dev::tools());
    v.extend(data::tools());
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn unique_ids_and_categories() {
        let tools = super::tools();
        let mut seen = std::collections::HashSet::new();
        for t in &tools {
            let s = t.spec();
            assert!(seen.insert(s.id.clone()), "duplicate id {}", s.id);
            assert!(
                s.id.starts_with(&format!("{}.", s.category.id())),
                "{} has wrong category",
                s.id
            );
            assert!(!s.description.is_empty(), "{} lacks a description", s.id);
        }
    }
}
