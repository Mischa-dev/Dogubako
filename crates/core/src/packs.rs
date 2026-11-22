//! Tool packs: named groups of built-in tools that an installation can turn
//! off. Built-ins stay compiled in; a turned-off pack only hides its tools
//! (see [`crate::engine::Engine`]).

use crate::spec::Category;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Environment variable holding the pack selection (e.g. for Docker).
pub const PACKS_ENV: &str = "DOGUBAKO_PACKS";
/// File in the config directory holding the pack selection.
pub const PACKS_FILE: &str = "packs.json";

/// A built-in pack.
#[derive(Debug, Clone, Serialize)]
pub struct Pack {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// The built-in packs: one per [`Category`].
pub fn builtin_packs() -> Vec<Pack> {
    Category::ALL
        .iter()
        .map(|c| Pack {
            id: c.id(),
            name: c.label(),
            description: c.description(),
        })
        .collect()
}

pub fn builtin_pack(id: &str) -> Option<Pack> {
    builtin_packs().into_iter().find(|p| p.id == id)
}

/// Which built-in packs are turned on. Packs that are not built-in (an
/// extension's tools) are always on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackSelection {
    /// `None`: every pack (the default install).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<BTreeSet<String>>,
}

impl PackSelection {
    pub fn all() -> Self {
        PackSelection { enabled: None }
    }

    pub fn only<I: IntoIterator<Item = S>, S: Into<String>>(packs: I) -> Self {
        PackSelection {
            enabled: Some(packs.into_iter().map(Into::into).collect()),
        }
    }

    pub fn is_all(&self) -> bool {
        self.enabled.is_none()
    }

    pub fn is_on(&self, pack: &str) -> bool {
        match &self.enabled {
            None => true,
            Some(set) => set.contains(pack) || builtin_pack(pack).is_none(),
        }
    }

    /// Parse a list such as `image,video,pdf`, `all`, or `all,-speech`.
    /// Entries starting with `-` turn a pack off; a list of only those starts
    /// from every pack.
    pub fn parse(list: &str) -> Self {
        let items: Vec<&str> = list
            .split([',', ';', ' '])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let all_ids = || builtin_packs().into_iter().map(|p| p.id.to_string());
        let base_all = items.is_empty()
            || items
                .iter()
                .all(|i| i.starts_with('-') || *i == "all" || *i == "*")
            || items.iter().any(|i| *i == "all" || *i == "*");
        let mut set: BTreeSet<String> = if base_all {
            all_ids().collect()
        } else {
            BTreeSet::new()
        };
        for i in items {
            if let Some(off) = i.strip_prefix('-') {
                set.remove(&off.to_ascii_lowercase());
            } else if i != "all" && i != "*" {
                set.insert(i.to_ascii_lowercase());
            }
        }
        if all_ids().all(|id| set.contains(&id)) {
            PackSelection::all()
        } else {
            PackSelection { enabled: Some(set) }
        }
    }

    /// Entries that are not built-in pack ids (likely typos).
    pub fn unknown(&self) -> Vec<String> {
        self.enabled
            .iter()
            .flatten()
            .filter(|p| builtin_pack(p).is_none())
            .cloned()
            .collect()
    }

    /// Read a saved selection; a missing or unreadable file means every pack.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)
    }

    /// `DOGUBAKO_PACKS` when set, otherwise the file at `path`.
    pub fn from_env_or_file(path: &Path) -> Self {
        match std::env::var(PACKS_ENV) {
            Ok(v) if !v.trim().is_empty() => Self::parse(&v),
            _ => Self::load(path),
        }
    }

    /// Selection of a local install (CLI, desktop): `DOGUBAKO_PACKS` or
    /// `<config>/packs.json`.
    pub fn local(dirs: &crate::paths::AppDirs) -> Self {
        Self::from_env_or_file(&packs_file(&dirs.config))
    }
}

pub fn packs_file(config_dir: &Path) -> PathBuf {
    config_dir.join(PACKS_FILE)
}

/// An engine or model a pack's tools use.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackEngine {
    pub id: String,
    pub name: String,
}

/// A pack as listed to clients.
#[derive(Debug, Clone, Serialize)]
pub struct PackInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// Number of tools in the pack (whether or not it is on).
    pub tools: usize,
    pub engines: Vec<PackEngine>,
    /// Model groups (see [`crate::models`]).
    pub models: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lists() {
        assert!(PackSelection::parse("").is_all());
        assert!(PackSelection::parse("all").is_all());
        let s = PackSelection::parse("image, PDF");
        assert!(s.is_on("image") && s.is_on("pdf") && !s.is_on("video"));
        let s = PackSelection::parse("-speech,-video");
        assert!(s.is_on("image") && !s.is_on("speech") && !s.is_on("video"));
        let s = PackSelection::parse("all,-speech");
        assert!(!s.is_on("speech") && s.is_on("audio"));
        let every: Vec<_> = builtin_packs().iter().map(|p| p.id).collect();
        assert!(PackSelection::parse(&every.join(",")).is_all());
        assert_eq!(PackSelection::parse("image,imgae").unknown(), vec!["imgae"]);
    }

    #[test]
    fn non_builtin_packs_stay_on() {
        let s = PackSelection::only(["image"]);
        assert!(s.is_on("extension:foo"));
        assert!(!s.is_on("text"));
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let f = packs_file(dir.path());
        assert!(PackSelection::load(&f).is_all());
        let s = PackSelection::only(["image", "text"]);
        s.save(&f).unwrap();
        assert_eq!(PackSelection::load(&f), s);
        PackSelection::all().save(&f).unwrap();
        assert!(PackSelection::load(&f).is_all());
    }
}
