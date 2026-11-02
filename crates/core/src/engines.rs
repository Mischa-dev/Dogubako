//! App-managed engines: pinned copies of the external programs that ship
//! inside the desktop app ("bundled") or that it downloads into its own
//! folder the first time a tool needs them ("on-demand").
//!
//! The list of engines, their versions and checksums comes from the
//! `manifest.json` that `packaging/engines/assemble.py` writes into the
//! bundled engines folder, so every app release carries exactly the engine
//! versions it was tested with. See docs/ENGINES.md.
//!
//! Without a manifest (server, Docker, CLI, development builds) nothing here
//! is active and dependencies are found on the system as before.

use crate::error::{ToolError, ToolResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// How engines are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMode {
    /// Only system-installed programs (no manifest, or `DOGUBAKO_ENGINES=system`).
    System,
    /// Engines listed in the manifest come only from the app's own copies;
    /// anything the manifest doesn't cover is still found on the system.
    Managed,
    /// App copies first, then the system for everything (`DOGUBAKO_ENGINES=prefer`).
    Prefer,
}

impl EngineMode {
    /// `DOGUBAKO_ENGINES` = `managed` | `prefer` | `system`; `default` otherwise.
    pub fn from_env(default: EngineMode) -> EngineMode {
        match std::env::var("DOGUBAKO_ENGINES").ok().as_deref() {
            Some("system") => EngineMode::System,
            Some("prefer") => EngineMode::Prefer,
            Some("managed") => EngineMode::Managed,
            _ => default,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundledEngine {
    pub id: String,
    /// Dependency ids (deps.rs) this engine provides.
    pub deps: Vec<String>,
    pub version: String,
    pub binaries: Vec<String>,
    /// Environment for processes of this engine; values are paths relative to
    /// the engine folder (`:`-separated lists allowed).
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub license: String,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnginePack {
    pub id: String,
    pub deps: Vec<String>,
    pub version: String,
    pub binaries: Vec<String>,
    pub license: String,
    pub url: String,
    pub sha256: String,
    pub download_bytes: u64,
    pub installed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineManifest {
    pub schema: u32,
    pub target: String,
    pub lock_id: String,
    #[serde(default)]
    pub bundled: Vec<BundledEngine>,
    #[serde(default)]
    pub on_demand: Vec<EnginePack>,
}

/// Where a managed dependency stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Found in the app's own folders.
    Found { path: PathBuf, origin: Origin },
    /// Managed, on-demand and not downloaded yet.
    Downloadable { engine: String, bytes: u64 },
    /// Bundled with the app but the file is gone (damaged install).
    Missing { engine: String },
    /// Not covered by the manifest (or managed engines are off).
    Unmanaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Bundled,
    Downloaded,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub id: String,
    pub deps: Vec<String>,
    pub version: String,
    pub tier: &'static str,
    pub installed: bool,
    pub license: String,
    pub download_bytes: Option<u64>,
    pub installed_bytes: Option<u64>,
}

pub struct ManagedEngines {
    mode: EngineMode,
    bundled_root: Option<PathBuf>,
    install_root: PathBuf,
    manifest: Option<EngineManifest>,
}

const ENGINE_JSON: &str = "engine.json";

impl ManagedEngines {
    /// Nothing managed: every lookup is [`Lookup::Unmanaged`].
    pub fn disabled() -> Self {
        ManagedEngines {
            mode: EngineMode::System,
            bundled_root: None,
            install_root: PathBuf::new(),
            manifest: None,
        }
    }

    /// Load the manifest from `bundled_root/manifest.json`. Without one the
    /// result behaves like [`ManagedEngines::disabled`].
    pub fn load(bundled_root: Option<PathBuf>, install_root: PathBuf) -> Self {
        let manifest = bundled_root.as_ref().and_then(|r| {
            let text = std::fs::read_to_string(r.join("manifest.json")).ok()?;
            match serde_json::from_str::<EngineManifest>(&text) {
                Ok(m) => Some(m),
                Err(e) => {
                    tracing::warn!(error = %e, "ignoring unreadable engines manifest");
                    None
                }
            }
        });
        let default = if manifest.is_some() {
            EngineMode::Managed
        } else {
            EngineMode::System
        };
        ManagedEngines {
            mode: EngineMode::from_env(default),
            bundled_root,
            install_root,
            manifest,
        }
    }

    pub fn mode(&self) -> EngineMode {
        if self.manifest.is_none() {
            EngineMode::System
        } else {
            self.mode
        }
    }

    pub fn install_root(&self) -> &Path {
        &self.install_root
    }

    fn manifest(&self) -> Option<&EngineManifest> {
        match self.mode() {
            EngineMode::System => None,
            _ => self.manifest.as_ref(),
        }
    }

    /// The folder an on-demand engine lives in once installed. Versioned, so
    /// an app update that pins a new version never runs a stale copy.
    fn pack_dir(&self, pack: &EnginePack) -> PathBuf {
        self.install_root.join(&pack.id).join(&pack.version)
    }

    fn pack_installed(&self, pack: &EnginePack) -> bool {
        self.pack_dir(pack).join(ENGINE_JSON).is_file()
    }

    /// Find one of `names` for dependency `dep_id` in the app's own folders.
    pub fn lookup(&self, dep_id: &str, names: &[&str]) -> Lookup {
        let Some(m) = self.manifest() else {
            return Lookup::Unmanaged;
        };
        let find = |bin: &Path, listed: &[String]| {
            names
                .iter()
                .filter(|n| listed.iter().any(|b| b == *n))
                .map(|n| bin.join(exe_name(n)))
                .find(|p| p.is_file())
        };
        if let (Some(root), Some(e)) = (
            &self.bundled_root,
            m.bundled
                .iter()
                .find(|e| e.deps.iter().any(|d| d == dep_id)),
        ) {
            return match find(&root.join("bin"), &e.binaries) {
                Some(path) => Lookup::Found {
                    path,
                    origin: Origin::Bundled,
                },
                None => Lookup::Missing {
                    engine: e.id.clone(),
                },
            };
        }
        if let Some(p) = m
            .on_demand
            .iter()
            .find(|p| p.deps.iter().any(|d| d == dep_id))
        {
            if self.pack_installed(p) {
                if let Some(path) = find(&self.pack_dir(p).join("bin"), &p.binaries) {
                    return Lookup::Found {
                        path,
                        origin: Origin::Downloaded,
                    };
                }
            }
            return Lookup::Downloadable {
                engine: p.id.clone(),
                bytes: p.download_bytes,
            };
        }
        Lookup::Unmanaged
    }

    /// Whether a missing dependency must not be looked up on the system.
    pub fn exclusive(&self, lookup: &Lookup) -> bool {
        self.mode() == EngineMode::Managed && !matches!(lookup, Lookup::Unmanaged)
    }

    /// Extra environment for running `program`, when it is one of the app's
    /// engines: the engine's own variables plus every managed `bin/` on PATH
    /// (ImageMagick calls `gs` by name, for example).
    pub fn env_for(&self, program: &Path) -> Vec<(String, OsString)> {
        let Some(m) = self.manifest() else {
            return vec![];
        };
        let mut roots: Vec<(PathBuf, HashMap<String, String>)> = vec![];
        if let Some(root) = &self.bundled_root {
            let mut env = HashMap::new();
            for e in &m.bundled {
                env.extend(e.env.clone());
            }
            roots.push((root.clone(), env));
        }
        for p in m.on_demand.iter().filter(|p| self.pack_installed(p)) {
            let dir = self.pack_dir(p);
            let env = std::fs::read_to_string(dir.join(ENGINE_JSON))
                .ok()
                .and_then(|t| serde_json::from_str::<BundledEngine>(&t).ok())
                .map(|e| e.env)
                .unwrap_or_default();
            roots.push((dir, env));
        }
        let Some((root, env)) = roots.iter().find(|(r, _)| program.starts_with(r)) else {
            return vec![];
        };
        let mut out: Vec<(String, OsString)> = env
            .iter()
            .map(|(k, v)| {
                let joined = v
                    .split(':')
                    .map(|part| root.join(part).to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(":");
                (k.clone(), OsString::from(joined))
            })
            .collect();
        let mut path: Vec<PathBuf> = roots.iter().map(|(r, _)| r.join("bin")).collect();
        if let Some(sys) = std::env::var_os("PATH") {
            path.extend(std::env::split_paths(&sys));
        }
        if let Ok(joined) = std::env::join_paths(path) {
            out.push(("PATH".into(), joined));
        }
        out.sort();
        out
    }

    pub fn list(&self) -> Vec<EngineStatus> {
        let Some(m) = self.manifest() else {
            return vec![];
        };
        let bundled = m.bundled.iter().map(|e| EngineStatus {
            id: e.id.clone(),
            deps: e.deps.clone(),
            version: e.version.clone(),
            tier: "bundled",
            installed: true,
            license: e.license.clone(),
            download_bytes: None,
            installed_bytes: None,
        });
        let packs = m.on_demand.iter().map(|p| EngineStatus {
            id: p.id.clone(),
            deps: p.deps.clone(),
            version: p.version.clone(),
            tier: "on_demand",
            installed: self.pack_installed(p),
            license: p.license.clone(),
            download_bytes: Some(p.download_bytes),
            installed_bytes: Some(p.installed_bytes),
        });
        bundled.chain(packs).collect()
    }

    fn pack(&self, id: &str) -> ToolResult<&EnginePack> {
        self.manifest()
            .and_then(|m| m.on_demand.iter().find(|p| p.id == id))
            .ok_or_else(|| ToolError::input(format!("'{id}' is not a downloadable engine")))
    }

    /// Remove a downloaded engine (all versions).
    pub fn remove(&self, id: &str) -> ToolResult<()> {
        let pack = self.pack(id)?;
        let dir = self.install_root.join(&pack.id);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }

    /// Download, verify and unpack an on-demand engine into the app's folder.
    /// Other versions of the same engine are removed afterwards. The caller
    /// should refresh dependency detection.
    pub async fn install(
        &self,
        id: &str,
        progress: impl Fn(u64, u64) + Send + Sync,
        cancel: tokio_util::sync::CancellationToken,
    ) -> ToolResult<PathBuf> {
        use futures::StreamExt;
        use tokio::io::AsyncWriteExt;

        let pack = self.pack(id)?.clone();
        let final_dir = self.pack_dir(&pack);
        if self.pack_installed(&pack) {
            return Ok(final_dir);
        }
        let engine_dir = self.install_root.join(&pack.id);
        std::fs::create_dir_all(&engine_dir)?;
        let client = reqwest::Client::builder()
            .user_agent(concat!("dogubako/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ToolError::failed(e.to_string()))?;
        let resp = client
            .get(&pack.url)
            .send()
            .await
            .map_err(|e| ToolError::failed(format!("download failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ToolError::failed(format!(
                "download failed: HTTP {} for {}",
                resp.status(),
                pack.url
            )));
        }
        let total = resp.content_length().unwrap_or(pack.download_bytes);
        let part = engine_dir.join(format!(".{}.download", pack.version));
        let mut out = tokio::fs::File::create(&part).await?;
        let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
        let mut done = 0u64;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                drop(out);
                let _ = std::fs::remove_file(&part);
                return Err(ToolError::Cancelled);
            }
            let chunk =
                chunk.map_err(|e| ToolError::failed(format!("download interrupted: {e}")))?;
            sha2::Digest::update(&mut hasher, &chunk);
            out.write_all(&chunk).await?;
            done += chunk.len() as u64;
            progress(done, total);
        }
        out.flush().await?;
        drop(out);
        let got = hex::encode(sha2::Digest::finalize(hasher));
        if !got.eq_ignore_ascii_case(&pack.sha256) {
            let _ = std::fs::remove_file(&part);
            return Err(ToolError::failed(format!(
                "checksum mismatch for engine {id}: expected {}, got {got}",
                pack.sha256
            )));
        }
        let staging = engine_dir.join(format!(".{}.unpack", pack.version));
        let unpack = {
            let (part, staging) = (part.clone(), staging.clone());
            tokio::task::spawn_blocking(move || unpack_tar_gz(&part, &staging))
        };
        let result = unpack
            .await
            .map_err(|e| ToolError::failed(e.to_string()))
            .and_then(|r| r);
        let _ = std::fs::remove_file(&part);
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        check_unpacked(&staging, &pack).inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&staging);
        })?;
        if final_dir.exists() {
            std::fs::remove_dir_all(&final_dir)?;
        }
        std::fs::rename(&staging, &final_dir)?;
        // Keep only the pinned version.
        for entry in std::fs::read_dir(&engine_dir)?.flatten() {
            if entry.file_name() != std::ffi::OsStr::new(&pack.version) {
                let p = entry.path();
                let _ = if p.is_dir() {
                    std::fs::remove_dir_all(p)
                } else {
                    std::fs::remove_file(p)
                };
            }
        }
        Ok(final_dir)
    }
}

/// Unpack a pack, rejecting absolute paths, `..`, links and special files.
fn unpack_tar_gz(archive: &Path, dest: &Path) -> ToolResult<()> {
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    tar.set_preserve_permissions(true);
    for entry in tar.entries().map_err(bad_pack)? {
        let mut entry = entry.map_err(bad_pack)?;
        let path = entry.path().map_err(bad_pack)?.into_owned();
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            return Err(ToolError::failed(format!(
                "engine pack has an unsafe path: {}",
                path.display()
            )));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(ToolError::failed(format!(
                "engine pack has an unsupported entry: {}",
                path.display()
            )));
        }
        entry.unpack_in(dest).map_err(bad_pack)?;
    }
    Ok(())
}

fn bad_pack(e: std::io::Error) -> ToolError {
    ToolError::failed(format!("engine pack is damaged: {e}"))
}

fn check_unpacked(dir: &Path, pack: &EnginePack) -> ToolResult<()> {
    let text = std::fs::read_to_string(dir.join(ENGINE_JSON))
        .map_err(|_| ToolError::failed("engine pack has no engine.json"))?;
    let meta: BundledEngine =
        serde_json::from_str(&text).map_err(|e| ToolError::failed(e.to_string()))?;
    if meta.id != pack.id || meta.version != pack.version {
        return Err(ToolError::failed(format!(
            "engine pack is {} {}, expected {} {}",
            meta.id, meta.version, pack.id, pack.version
        )));
    }
    for b in &pack.binaries {
        if !dir.join("bin").join(exe_name(b)).is_file() {
            return Err(ToolError::failed(format!("engine pack is missing bin/{b}")));
        }
    }
    Ok(())
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Where the desktop app's bundled engines live: `DOGUBAKO_BUNDLED_ENGINES`,
/// else next to the executable (`Contents/Resources/engines` on macOS,
/// `engines/` beside the binary elsewhere).
pub fn default_bundled_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DOGUBAKO_BUNDLED_ENGINES") {
        return Some(PathBuf::from(p));
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidates = [dir.join("../Resources/engines"), dir.join("engines")];
    candidates
        .into_iter()
        .find(|c| c.join("manifest.json").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_exe(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"#!/bin/sh\n").unwrap();
    }

    fn pack_entry(sha256: &str, url: &str) -> EnginePack {
        EnginePack {
            id: "ghostscript".into(),
            deps: vec!["ghostscript".into()],
            version: "10.08.0".into(),
            binaries: vec!["gs".into()],
            license: "AGPL-3.0-or-later".into(),
            url: url.into(),
            sha256: sha256.into(),
            download_bytes: 10,
            installed_bytes: 20,
        }
    }

    fn setup(packs: Vec<EnginePack>) -> (tempfile::TempDir, ManagedEngines) {
        let t = tempfile::tempdir().unwrap();
        let bundled = t.path().join("bundled");
        write_exe(&bundled.join("bin").join(exe_name("ffmpeg")));
        let manifest = EngineManifest {
            schema: 1,
            target: "darwin-arm64".into(),
            lock_id: "test".into(),
            bundled: vec![BundledEngine {
                id: "ffmpeg".into(),
                deps: vec!["ffmpeg".into(), "ffprobe".into()],
                version: "9.0.2".into(),
                binaries: vec!["ffmpeg".into(), "ffprobe".into()],
                env: HashMap::from([("FFREPORT".into(), "share/x".into())]),
                license: "GPL".into(),
                source_url: None,
            }],
            on_demand: packs,
        };
        std::fs::write(
            bundled.join("manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let mut m = ManagedEngines::load(Some(bundled), t.path().join("installed"));
        m.mode = EngineMode::Managed; // independent of the test environment
        (t, m)
    }

    #[test]
    fn no_manifest_means_system() {
        let t = tempfile::tempdir().unwrap();
        let m = ManagedEngines::load(Some(t.path().into()), t.path().join("i"));
        assert_eq!(m.mode(), EngineMode::System);
        assert_eq!(m.lookup("ffmpeg", &["ffmpeg"]), Lookup::Unmanaged);
        assert!(m.list().is_empty());
    }

    #[test]
    fn bundled_on_demand_and_unmanaged() {
        let (_t, m) = setup(vec![pack_entry("00", "http://127.0.0.1:9/x")]);
        match m.lookup("ffmpeg", &["ffmpeg"]) {
            Lookup::Found { origin, .. } => assert_eq!(origin, Origin::Bundled),
            other => panic!("{other:?}"),
        }
        // Listed but the binary isn't in the folder: not found, and managed
        // mode must not fall back to the system for it.
        let probe = m.lookup("ffprobe", &["ffprobe"]);
        assert_eq!(
            probe,
            Lookup::Missing {
                engine: "ffmpeg".into()
            }
        );
        assert!(m.exclusive(&probe));
        let gs = m.lookup("ghostscript", &["gs"]);
        assert_eq!(
            gs,
            Lookup::Downloadable {
                engine: "ghostscript".into(),
                bytes: 10
            }
        );
        assert!(m.exclusive(&gs));
        assert_eq!(m.lookup("libreoffice", &["soffice"]), Lookup::Unmanaged);
        assert!(!m.exclusive(&Lookup::Unmanaged));
    }

    #[test]
    fn env_applies_only_to_managed_programs() {
        let (_t, m) = setup(vec![]);
        let Lookup::Found { path, .. } = m.lookup("ffmpeg", &["ffmpeg"]) else {
            panic!()
        };
        let env = m.env_for(&path);
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let root = m.bundled_root.clone().unwrap();
        assert_eq!(get("FFREPORT").unwrap(), root.join("share/x").as_os_str());
        let path_var = get("PATH").unwrap();
        assert_eq!(
            std::env::split_paths(&path_var).next().unwrap(),
            root.join("bin")
        );
        assert!(m.env_for(Path::new("/usr/bin/true")).is_empty());
    }

    fn make_pack(dir: &Path, meta: &BundledEngine) -> Vec<u8> {
        let mut buf = vec![];
        {
            let gz = flate2::write::GzEncoder::new(&mut buf, flate2::Compression::fast());
            let mut tar = tar::Builder::new(gz);
            let json = serde_json::to_vec(meta).unwrap();
            let mut h = tar::Header::new_gnu();
            h.set_size(json.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, ENGINE_JSON, &json[..]).unwrap();
            let body = b"#!/bin/sh\necho gs\n";
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            tar.append_data(&mut h, format!("bin/{}", exe_name("gs")), &body[..])
                .unwrap();
            tar.into_inner().unwrap().finish().unwrap();
        }
        let _ = dir;
        buf
    }

    /// Serve `body` once per connection on a local port.
    fn serve(body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut s = stream;
                let mut req = [0u8; 1024];
                let _ = std::io::Read::read(&mut s, &mut req);
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}/gs.tar.gz")
    }

    fn sha(b: &[u8]) -> String {
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(b))
    }

    #[tokio::test]
    async fn install_verifies_and_unpacks() {
        let meta = BundledEngine {
            id: "ghostscript".into(),
            deps: vec!["ghostscript".into()],
            version: "10.08.0".into(),
            binaries: vec!["gs".into()],
            env: HashMap::from([("GS_LIB".into(), "share/ghostscript/lib".into())]),
            license: "AGPL-3.0-or-later".into(),
            source_url: None,
        };
        let body = make_pack(Path::new("."), &meta);
        let url = serve(body.clone());
        let (_t, m) = setup(vec![pack_entry(&sha(&body), &url)]);
        // A stale version is cleaned up after install.
        std::fs::create_dir_all(m.install_root.join("ghostscript/9.0")).unwrap();
        let dir = m
            .install("ghostscript", |_, _| {}, Default::default())
            .await
            .unwrap();
        assert!(dir.ends_with("ghostscript/10.08.0"));
        assert!(!m.install_root.join("ghostscript/9.0").exists());
        let Lookup::Found { path, origin } = m.lookup("ghostscript", &["gs"]) else {
            panic!()
        };
        assert_eq!(origin, Origin::Downloaded);
        let env = m.env_for(&path);
        assert!(env
            .iter()
            .any(|(k, v)| k == "GS_LIB" && Path::new(v).starts_with(&dir)));
        assert!(m
            .list()
            .iter()
            .any(|e| e.id == "ghostscript" && e.installed));
        m.remove("ghostscript").unwrap();
        assert!(matches!(
            m.lookup("ghostscript", &["gs"]),
            Lookup::Downloadable { .. }
        ));
    }

    #[tokio::test]
    async fn install_rejects_checksum_mismatch() {
        let meta = BundledEngine {
            id: "ghostscript".into(),
            deps: vec![],
            version: "10.08.0".into(),
            binaries: vec!["gs".into()],
            env: HashMap::new(),
            license: String::new(),
            source_url: None,
        };
        let url = serve(make_pack(Path::new("."), &meta));
        let (_t, m) = setup(vec![pack_entry(&"0".repeat(64), &url)]);
        let err = m
            .install("ghostscript", |_, _| {}, Default::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"), "{err}");
        assert!(!m.install_root.join("ghostscript/10.08.0").exists());
    }

    #[test]
    fn unpack_rejects_traversal_and_links() {
        let t = tempfile::tempdir().unwrap();
        for (name, kind) in [
            ("../evil", tar::EntryType::Regular),
            ("bin/link", tar::EntryType::Symlink),
        ] {
            let archive = t.path().join("bad.tar.gz");
            {
                let f = std::fs::File::create(&archive).unwrap();
                let gz = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
                let mut tar = tar::Builder::new(gz);
                let mut h = tar::Header::new_gnu();
                h.set_entry_type(kind);
                h.set_size(0);
                // Header::set_path refuses "..", so write the raw name.
                let raw = &mut h.as_old_mut().name;
                raw[..name.len()].copy_from_slice(name.as_bytes());
                if kind == tar::EntryType::Symlink {
                    h.set_link_name("/etc/passwd").unwrap();
                }
                h.set_cksum();
                tar.append(&h, std::io::empty()).unwrap();
                tar.into_inner().unwrap().finish().unwrap();
            }
            let dest = t.path().join("out");
            assert!(unpack_tar_gz(&archive, &dest).is_err(), "{name}");
            assert!(!t.path().join("evil").exists());
        }
    }
}
