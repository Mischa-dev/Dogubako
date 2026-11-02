//! Platform-appropriate application directories.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub models: PathBuf,
    pub bin: PathBuf,
    pub extensions: PathBuf,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

impl AppDirs {
    /// Per-user directories, overridable with `DOGUBAKO_HOME`.
    pub fn default_local() -> Self {
        if let Some(h) = std::env::var_os("DOGUBAKO_HOME") {
            return Self::under(PathBuf::from(h));
        }
        let base = if cfg!(target_os = "macos") {
            home().join("Library/Application Support/Dogubako")
        } else if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(home).join("Dogubako")
        } else {
            std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("dogubako")
        };
        Self::under(base)
    }

    pub fn under(base: PathBuf) -> Self {
        AppDirs {
            config: base.join("config"),
            models: base.join("models"),
            bin: base.join("bin"),
            extensions: base.join("extensions"),
            data: base,
        }
    }
}
