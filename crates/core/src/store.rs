//! The official tool store: reviewed extension manifests kept in the
//! repository's `store/` folder, built into an index published with the docs
//! site. Apps fetch the index through the network policy, cache it for offline
//! search, and install entries through the one-step extension install after
//! checking each manifest's SHA-256 against the index.

use crate::engine::Engine;
use crate::error::{ToolError, ToolResult};
use crate::extensions::{self, ExtensionReport, InstallOptions};
use crate::manifest::{min_dogubako, parse_manifest, sha256_hex};
use crate::policy::NetPolicy;
use crate::tool::Tool;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the official index is published (with the docs site).
pub const OFFICIAL_INDEX_URL: &str = "https://docs.dogubako.app/store/index.json";
/// Index format version.
pub const INDEX_SCHEMA: u32 = 1;
/// Largest index accepted.
pub const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;
/// File name of a store tool's manifest inside `store/<id>/`.
pub const MANIFEST_FILE: &str = "manifest.toml";
/// The Dogubako version entries' `min_dogubako` is compared with.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One tool in the index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub category: String,
    pub version: String,
    /// SHA-256 of the manifest at `url`; install refuses anything else.
    pub sha256: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    /// The manifest, absolute or relative to the index URL.
    pub url: String,
    /// Oldest Dogubako that can load the manifest; older apps neither install
    /// nor update to this entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_dogubako: Option<String>,
}

impl StoreEntry {
    /// Why this app can't install the entry: it needs a newer Dogubako.
    pub fn unsupported(&self) -> Option<String> {
        self.unsupported_by(APP_VERSION)
    }

    fn unsupported_by(&self, app: &str) -> Option<String> {
        let min = self.min_dogubako.as_deref()?;
        needs_newer(min, app).then(|| {
            format!(
                "{} {} needs Dogubako {min} or newer (this is {app}); update Dogubako to install it",
                self.id, self.version
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoreIndex {
    pub schema: u32,
    pub tools: Vec<StoreEntry>,
}

/// The store URL to use: `setting` unset means the official index; an empty
/// value, `off`, `none` or `false` turns the store off.
pub fn resolve_url(setting: Option<&str>) -> Option<String> {
    match setting.map(str::trim) {
        None => Some(OFFICIAL_INDEX_URL.to_string()),
        Some("" | "off" | "none" | "false" | "0") => None,
        Some(u) => Some(u.to_string()),
    }
}

/// The store URL for local apps (CLI, desktop): `DOGUBAKO_STORE_URL`.
pub fn local_url() -> Option<String> {
    resolve_url(std::env::var("DOGUBAKO_STORE_URL").ok().as_deref())
}

pub fn disabled_error() -> ToolError {
    ToolError::Unsupported {
        message: "the tool store is turned off (store_url / DOGUBAKO_STORE_URL is empty)".into(),
    }
}

// ---------------------------------------------------------------- building

/// The index built from a `store/` folder, with the manifests to publish
/// next to it (`<id>/manifest.toml`).
#[derive(Debug, Clone)]
pub struct BuiltStore {
    pub index: StoreIndex,
    pub manifests: Vec<(String, String)>,
}

/// Build the index from `store/<id>/manifest.toml` folders. Every manifest
/// must parse and live in a folder named after its id.
pub fn build_index(store_dir: &Path) -> Result<BuiltStore, String> {
    let rd = std::fs::read_dir(store_dir)
        .map_err(|e| format!("reading {}: {e}", store_dir.display()))?;
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut tools = vec![];
    let mut manifests = vec![];
    let mut errors = vec![];
    for d in dirs {
        let folder = d.file_name().unwrap().to_string_lossy().to_string();
        let path = d.join(MANIFEST_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let tool = match parse_manifest(&text, &format!("{folder}/{MANIFEST_FILE}")) {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let spec = tool.spec();
        if spec.id != folder {
            errors.push(format!(
                "{}: id '{}' must match its folder name '{folder}'",
                path.display(),
                spec.id
            ));
            continue;
        }
        if text.len() > extensions::MAX_MANIFEST_BYTES {
            errors.push(format!("{}: manifest is too large", path.display()));
            continue;
        }
        let min = match min_dogubako(&text) {
            Ok(m) => m,
            Err(e) => {
                errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        tools.push(StoreEntry {
            id: spec.id.clone(),
            name: spec.name.clone(),
            description: spec.description.clone(),
            keywords: spec.keywords.clone(),
            category: spec.category.id().to_string(),
            version: spec.version.clone(),
            sha256: sha256_hex(text.as_bytes()),
            permissions: spec.permissions.iter().map(|p| p.id().into()).collect(),
            requires: spec.requires.clone(),
            platforms: spec
                .platforms
                .iter()
                .map(|p| {
                    serde_json::to_value(p)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default()
                })
                .collect(),
            url: format!("{}/{MANIFEST_FILE}", spec.id),
            min_dogubako: min,
        });
        manifests.push((format!("{}/{MANIFEST_FILE}", spec.id), text));
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(BuiltStore {
        index: StoreIndex {
            schema: INDEX_SCHEMA,
            tools,
        },
        manifests,
    })
}

impl BuiltStore {
    fn files(&self) -> Vec<(String, String)> {
        let mut out = vec![(
            "index.json".to_string(),
            serde_json::to_string_pretty(&self.index).unwrap() + "\n",
        )];
        out.extend(self.manifests.iter().cloned());
        out
    }

    /// Write `index.json` and the manifests into `out`, removing published
    /// tool folders (those holding a manifest) that are no longer in the store.
    pub fn write(&self, out: &Path) -> std::io::Result<()> {
        let keep: Vec<&str> = self.index.tools.iter().map(|t| t.id.as_str()).collect();
        if let Ok(rd) = std::fs::read_dir(out) {
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if p.join(MANIFEST_FILE).is_file() && !keep.contains(&name.as_str()) {
                    std::fs::remove_dir_all(&p)?;
                }
            }
        }
        for (rel, text) in self.files() {
            let p = out.join(&rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(p, text)?;
        }
        Ok(())
    }

    /// Files in `out` that differ from what [`BuiltStore::write`] would write.
    pub fn stale(&self, out: &Path) -> Vec<String> {
        let mut stale = vec![];
        for (rel, text) in self.files() {
            if std::fs::read_to_string(out.join(&rel)).ok().as_deref() != Some(text.as_str()) {
                stale.push(rel);
            }
        }
        let keep: Vec<&str> = self.index.tools.iter().map(|t| t.id.as_str()).collect();
        if let Ok(rd) = std::fs::read_dir(out) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if e.path().join(MANIFEST_FILE).is_file() && !keep.contains(&name.as_str()) {
                    stale.push(format!("{name}/ (no longer in the store)"));
                }
            }
        }
        stale.sort();
        stale
    }
}

// ---------------------------------------------------------------- fetching

/// An index as the app sees it: fresh, or the cached copy when offline.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedIndex {
    pub url: String,
    /// True when the index could not be fetched and the cached copy is used.
    pub offline: bool,
    /// When the index in use was fetched (Unix seconds).
    pub fetched_at: u64,
    /// Why fetching failed, when offline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Where the index was actually served from (after redirects).
    #[serde(skip)]
    pub base: String,
    #[serde(skip)]
    pub index: StoreIndex,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    url: String,
    #[serde(default)]
    base: String,
    fetched_at: u64,
    index: StoreIndex,
}

fn store_dir(ext_dir: &Path) -> PathBuf {
    ext_dir.join("store")
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_else(|| url.to_string())
}

/// The error for installs and updates without a reachable store.
pub fn offline_error(url: &str, why: &str) -> ToolError {
    ToolError::failed(format!(
        "the tool store is offline: cannot reach {} ({why}). Search uses the cached index; \
         installing and updating need the network",
        host_of(url)
    ))
}

/// Store traffic must be authenticated: `https`, or `http` only to this machine.
fn require_https(url: &url::Url) -> Result<(), String> {
    let loopback = matches!(
        url.host(),
        Some(url::Host::Domain("localhost"))
            | Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST))
            | Some(url::Host::Ipv6(std::net::Ipv6Addr::LOCALHOST))
    );
    match url.scheme() {
        "https" | "file" => Ok(()),
        "http" if loopback => Ok(()),
        s => Err(format!("the tool store must be served over https, not {s}")),
    }
}

/// Fetch `url` (redirects stay on its origin); returns the bytes and the
/// final URL, which relative manifest links resolve against.
async fn fetch_bytes(url: &str, net: &NetPolicy, max: usize) -> Result<(Vec<u8>, String), String> {
    require_https(&url::Url::parse(url).map_err(|e| e.to_string())?)?;
    let checked = crate::net::check_url(net, url).map_err(|e| e.to_string())?;
    let client = crate::net::guarded_client_same_origin(net.clone()).map_err(|e| e.to_string())?;
    let mut resp = client
        .get(checked)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let final_url = resp.url().to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > max {
            return Err(format!("response is larger than {} KiB", max / 1024));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((bytes, final_url))
}

fn read_file_url(url: &str) -> Result<(Vec<u8>, String), String> {
    let path = url::Url::parse(url)
        .ok()
        .and_then(|u| u.to_file_path().ok())
        .ok_or_else(|| format!("bad file url {url}"))?;
    let bytes = std::fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    Ok((bytes, url.to_string()))
}

fn parse_index(bytes: &[u8]) -> Result<StoreIndex, String> {
    let index: StoreIndex =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid store index: {e}"))?;
    if index.schema != INDEX_SCHEMA {
        return Err(format!(
            "store index schema {} is not supported (expected {INDEX_SCHEMA}); update Dogubako",
            index.schema
        ));
    }
    Ok(index)
}

fn read_cache(ext_dir: &Path, url: &str) -> Option<Cache> {
    let text = std::fs::read_to_string(store_dir(ext_dir).join("index.json")).ok()?;
    let c: Cache = serde_json::from_str(&text).ok()?;
    (c.url == url).then_some(c)
}

/// Fetch the index at `url` and cache it under `<extensions>/store/`; when
/// that fails, fall back to the cached copy (marked `offline`).
pub async fn load_index(url: &str, ext_dir: &Path, net: &NetPolicy) -> ToolResult<LoadedIndex> {
    let fetched = if url.starts_with("file://") {
        read_file_url(url)
    } else {
        fetch_bytes(url, net, MAX_INDEX_BYTES).await
    }
    .and_then(|(b, base)| Ok((parse_index(&b)?, base)));
    match fetched {
        Ok((index, base)) => {
            let cache = Cache {
                url: url.to_string(),
                base,
                fetched_at: now(),
                index,
            };
            let dir = store_dir(ext_dir);
            let _ = std::fs::create_dir_all(&dir).and_then(|_| {
                std::fs::write(
                    dir.join("index.json"),
                    serde_json::to_string(&cache).unwrap(),
                )
            });
            Ok(LoadedIndex {
                url: cache.url,
                base: cache.base,
                offline: false,
                fetched_at: cache.fetched_at,
                error: None,
                index: cache.index,
            })
        }
        Err(why) => match read_cache(ext_dir, url) {
            Some(c) => Ok(LoadedIndex {
                base: if c.base.is_empty() { c.url.clone() } else { c.base },
                url: c.url,
                offline: true,
                fetched_at: c.fetched_at,
                error: Some(why),
                index: c.index,
            }),
            None => Err(ToolError::failed(format!(
                "the tool store is offline: cannot reach {} ({why}) and there is no cached index yet",
                host_of(url)
            ))),
        },
    }
}

// ---------------------------------------------------------------- installed

/// What the store installed: id → the index entry's version and SHA-256.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Installed {
    #[serde(default)]
    tools: BTreeMap<String, InstalledTool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledTool {
    version: String,
    sha256: String,
}

fn installed_path(ext_dir: &Path) -> PathBuf {
    store_dir(ext_dir).join("installed.json")
}

/// Serializes read-modify-write of `installed.json`.
static INSTALLED_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_installed(ext_dir: &Path, f: impl FnOnce(&mut Installed) -> bool) -> ToolResult<()> {
    let _guard = INSTALLED_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut i = load_installed(ext_dir);
    if f(&mut i) {
        save_installed(ext_dir, &i)?;
    }
    Ok(())
}

fn load_installed(ext_dir: &Path) -> Installed {
    std::fs::read_to_string(installed_path(ext_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_installed(ext_dir: &Path, i: &Installed) -> ToolResult<()> {
    use std::io::Write;
    let p = installed_path(ext_dir);
    let dir = p.parent().unwrap();
    std::fs::create_dir_all(dir)
        .and_then(|_| {
            let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
            tmp.write_all(serde_json::to_string_pretty(i).unwrap().as_bytes())?;
            tmp.persist(&p).map(|_| ()).map_err(|e| e.error)
        })
        .map_err(|e| ToolError::failed(format!("writing {}: {e}", p.display())))
}

/// Drop the store record of an uninstalled extension.
pub fn forget(ext_dir: &Path, id: &str) {
    let _ = with_installed(ext_dir, |i| i.tools.remove(id).is_some());
}

/// A store tool with what is installed here.
#[derive(Debug, Clone, Serialize)]
pub struct StoreTool {
    #[serde(flatten)]
    pub entry: StoreEntry,
    pub installed: bool,
    /// The installed store tool's version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    /// The index has a newer version than the installed one, and this app
    /// can load it.
    pub update_available: bool,
    /// Why this app can't install (or update to) the entry: it needs a newer
    /// Dogubako (`min_dogubako`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported: Option<String>,
}

/// Store tools installed here, by id, with their installed version. An
/// extension replaced by hand (its file no longer has the store's SHA-256)
/// is not a store tool any more.
fn installed_versions(engine: &Engine, ext_dir: &Path) -> BTreeMap<String, String> {
    let records = load_installed(ext_dir);
    let statuses = engine.extensions();
    let registry = engine.catalog();
    records
        .tools
        .into_iter()
        .filter(|(id, rec)| {
            statuses
                .iter()
                .any(|s| s.id.as_deref() == Some(id) && s.sha256 == rec.sha256)
        })
        .map(|(id, rec)| {
            let v = registry
                .get(&id)
                .map(|t| t.spec().version.clone())
                .unwrap_or(rec.version);
            (id, v)
        })
        .collect()
}

/// `min` is a newer release than `app`, by semver precedence (build metadata
/// such as `+build.1` doesn't count).
fn needs_newer(min: &str, app: &str) -> bool {
    match (semver::Version::parse(min), semver::Version::parse(app)) {
        (Ok(min), Ok(app)) => min.cmp_precedence(&app).is_gt(),
        _ => is_newer(min, app),
    }
}

/// `a` is a newer version than `b` (semver, else numeric dotted parts).
pub fn is_newer(a: &str, b: &str) -> bool {
    match (semver::Version::parse(a), semver::Version::parse(b)) {
        (Ok(a), Ok(b)) => a > b,
        _ => {
            let parts = |s: &str| -> Vec<u64> {
                s.split(|c: char| !c.is_ascii_digit())
                    .filter(|p| !p.is_empty())
                    .map(|p| p.parse().unwrap_or(0))
                    .collect()
            };
            parts(a) > parts(b)
        }
    }
}

/// Every tool in the index with its install state.
pub fn annotate(index: &StoreIndex, engine: &Engine, ext_dir: &Path) -> Vec<StoreTool> {
    annotate_for(index, engine, ext_dir, APP_VERSION)
}

fn annotate_for(index: &StoreIndex, engine: &Engine, ext_dir: &Path, app: &str) -> Vec<StoreTool> {
    let installed = installed_versions(engine, ext_dir);
    index
        .tools
        .iter()
        .map(|e| {
            let v = installed.get(&e.id).cloned();
            let newer = v.as_deref().is_some_and(|v| is_newer(&e.version, v));
            // An installed tool at the index's version is fine as it is.
            let unsupported = (v.is_none() || newer)
                .then(|| e.unsupported_by(app))
                .flatten();
            StoreTool {
                update_available: newer && unsupported.is_none(),
                installed: v.is_some(),
                installed_version: v,
                unsupported,
                entry: e.clone(),
            }
        })
        .collect()
}

/// Tools matching every word of `query` (id, name, description, keywords,
/// category); an empty query matches all. Name matches come first.
pub fn search(tools: Vec<StoreTool>, query: &str) -> Vec<StoreTool> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    let mut hits: Vec<(usize, StoreTool)> = tools
        .into_iter()
        .filter_map(|t| {
            let e = &t.entry;
            let name = format!("{} {}", e.id, e.name).to_lowercase();
            let rest =
                format!("{} {} {}", e.description, e.keywords.join(" "), e.category).to_lowercase();
            let all = words.iter().all(|w| name.contains(w) || rest.contains(w));
            let score = words.iter().filter(|w| name.contains(w.as_str())).count();
            all.then_some((score, t))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.entry.id.cmp(&b.1.entry.id)));
    hits.into_iter().map(|(_, t)| t).collect()
}

fn entry<'a>(index: &'a LoadedIndex, id: &str) -> ToolResult<&'a StoreEntry> {
    index
        .index
        .tools
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| ToolError::NotFound {
            id: format!("{id} (not in the tool store)"),
        })
}

/// Download an entry's manifest and check it against the index's SHA-256.
pub async fn fetch_entry(
    index: &LoadedIndex,
    e: &StoreEntry,
    net: &NetPolicy,
) -> ToolResult<String> {
    if index.offline {
        return Err(offline_error(
            &index.url,
            index.error.as_deref().unwrap_or("no connection"),
        ));
    }
    let base = if index.base.is_empty() {
        &index.url
    } else {
        &index.base
    };
    let url = url::Url::parse(base)
        .and_then(|base| base.join(&e.url))
        .map_err(|err| ToolError::failed(format!("store entry '{}' has a bad url: {err}", e.id)))?;
    if url.scheme() == "file" && !index.url.starts_with("file://") {
        return Err(ToolError::invalid(
            "url",
            format!("store entry '{}' points at a local file", e.id),
        ));
    }
    require_https(&url).map_err(|why| ToolError::invalid("url", why))?;
    let (bytes, _) = if url.scheme() == "file" {
        read_file_url(url.as_str()).map_err(ToolError::failed)?
    } else {
        fetch_bytes(url.as_str(), net, extensions::MAX_MANIFEST_BYTES)
            .await
            .map_err(|why| offline_error(url.as_str(), &why))?
    };
    let sha = sha256_hex(&bytes);
    if sha != e.sha256 {
        return Err(ToolError::invalid(
            "sha256",
            format!(
                "the store's manifest for '{}' does not match its index entry (SHA-256 {}… vs {}…); \
                 refusing to install it",
                e.id,
                &sha[..12],
                e.sha256.get(..12).unwrap_or(&e.sha256)
            ),
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| ToolError::invalid("manifest", "manifest is not UTF-8 text"))?;
    let tool =
        parse_manifest(&text, url.as_str()).map_err(|err| ToolError::invalid("manifest", err))?;
    if tool.spec().id != e.id {
        return Err(ToolError::invalid(
            "manifest",
            format!(
                "the store's manifest for '{}' declares id '{}'; refusing to install it",
                e.id,
                tool.spec().id
            ),
        ));
    }
    Ok(text)
}

/// Install a store tool through the one-step install (validate, copy, trust,
/// reload, test) after checking its SHA-256 against the index.
pub async fn install(
    engine: &Engine,
    ext_dir: &Path,
    index: &LoadedIndex,
    id: &str,
    replace: bool,
) -> ToolResult<ExtensionReport> {
    install_for(
        engine,
        ext_dir,
        index,
        id,
        replace,
        APP_VERSION,
        &|_| Ok(()),
    )
    .await
}

/// [`install`], refused by `allow` after the manifest is fetched and
/// verified, so callers check the tool's real id and capabilities rather
/// than the index's summary of them.
pub async fn install_if(
    engine: &Engine,
    ext_dir: &Path,
    index: &LoadedIndex,
    id: &str,
    allow: &(dyn Fn(&crate::spec::ToolSpec) -> ToolResult<()> + Sync),
) -> ToolResult<ExtensionReport> {
    install_for(engine, ext_dir, index, id, false, APP_VERSION, allow).await
}

async fn install_for(
    engine: &Engine,
    ext_dir: &Path,
    index: &LoadedIndex,
    id: &str,
    replace: bool,
    app: &str,
    allow: &(dyn Fn(&crate::spec::ToolSpec) -> ToolResult<()> + Sync),
) -> ToolResult<ExtensionReport> {
    let e = entry(index, id)?;
    if let Some(message) = e.unsupported_by(app) {
        return Err(ToolError::Unsupported { message });
    }
    let text = fetch_entry(index, e, &engine.config.net).await?;
    let tool = parse_manifest(&text, "store").map_err(|err| ToolError::invalid("manifest", err))?;
    allow(tool.spec())?;
    let report = extensions::install(engine, ext_dir, &text, InstallOptions { replace }).await?;
    with_installed(ext_dir, |i| {
        i.tools.insert(
            e.id.clone(),
            InstalledTool {
                version: e.version.clone(),
                sha256: e.sha256.clone(),
            },
        );
        true
    })?;
    Ok(report)
}

/// Update installed store tools that have a newer version in the index:
/// `id`, or all of them. A tool that fails is reported with its error and
/// the rest still update. Newer versions that need a newer Dogubako are
/// skipped (see [`StoreTool::unsupported`]); naming one is an error.
pub async fn update(
    engine: &Engine,
    ext_dir: &Path,
    index: &LoadedIndex,
    id: Option<&str>,
) -> ToolResult<Vec<ExtensionReport>> {
    update_for(engine, ext_dir, index, id, APP_VERSION).await
}

async fn update_for(
    engine: &Engine,
    ext_dir: &Path,
    index: &LoadedIndex,
    id: Option<&str>,
    app: &str,
) -> ToolResult<Vec<ExtensionReport>> {
    let tools = annotate_for(&index.index, engine, ext_dir, app);
    if let Some(id) = id {
        let t = tools
            .iter()
            .find(|t| t.entry.id == id)
            .ok_or_else(|| ToolError::NotFound {
                id: format!("{id} (not in the tool store)"),
            })?;
        if !t.installed {
            return Err(ToolError::invalid(
                "id",
                format!("'{id}' is not installed from the tool store; install it first"),
            ));
        }
        if let Some(message) = &t.unsupported {
            return Err(ToolError::Unsupported {
                message: message.clone(),
            });
        }
    }
    let targets: Vec<String> = tools
        .into_iter()
        .filter(|t| t.update_available && id.is_none_or(|id| t.entry.id == id))
        .map(|t| t.entry.id)
        .collect();
    let mut out = vec![];
    for t in targets {
        out.push(
            match install_for(engine, ext_dir, index, &t, true, app, &|_| Ok(())).await {
                Ok(r) => r,
                Err(e) => ExtensionReport {
                    id: t,
                    error: Some(e.to_string()),
                    ..Default::default()
                },
            },
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------- not installed

/// Whether the cached index for `url` is missing or older than `max_age_secs`.
pub fn cache_is_stale(url: &str, ext_dir: &Path, max_age_secs: u64) -> bool {
    read_cache(ext_dir, url).is_none_or(|c| now().saturating_sub(c.fetched_at) > max_age_secs)
}

/// The cached index for `url`, without touching the network.
pub fn cached_index(url: &str, ext_dir: &Path) -> Option<StoreIndex> {
    read_cache(ext_dir, url).map(|c| c.index)
}

/// A tool this installation could add: a built-in of a turned-off pack
/// (`source: "pack"`) or a store tool not installed yet (`source: "store"`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AvailableTool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub category: String,
    pub source: &'static str,
    /// The pack that turns it on (`source: "pack"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    pub version: String,
    pub permissions: Vec<String>,
    pub requires: Vec<String>,
    pub platforms: Vec<String>,
}

/// Tools not installed here: built-ins of turned-off packs that `keep`
/// accepts, then entries of `index` that are not in the catalog at all and
/// that this Dogubako can load.
pub fn available(
    engine: &Engine,
    index: Option<&StoreIndex>,
    keep: impl Fn(&crate::spec::ToolSpec) -> bool,
) -> Vec<AvailableTool> {
    let tools = engine.snapshot();
    let mut out: Vec<AvailableTool> = tools
        .catalog
        .specs()
        .filter(|s| tools.installed.get(&s.id).is_none() && keep(s))
        .map(|s| AvailableTool {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            keywords: s.keywords.clone(),
            category: s.category.id().to_string(),
            source: "pack",
            pack: Some(s.pack.clone()),
            version: s.version.clone(),
            permissions: s.permissions.iter().map(|p| p.id().to_string()).collect(),
            requires: s.requires.clone(),
            platforms: s
                .platforms
                .iter()
                .filter_map(|p| serde_json::to_value(p).ok()?.as_str().map(String::from))
                .collect(),
        })
        .collect();
    if let Some(index) = index {
        out.extend(
            index
                .tools
                .iter()
                .filter(|e| tools.catalog.get(&e.id).is_none() && e.unsupported().is_none())
                .map(|e| AvailableTool {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    description: e.description.clone(),
                    keywords: e.keywords.clone(),
                    category: e.category.clone(),
                    source: "store",
                    pack: None,
                    version: e.version.clone(),
                    permissions: e.permissions.clone(),
                    requires: e.requires.clone(),
                    platforms: e.platforms.clone(),
                }),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineConfig;
    use crate::manifest::load_extensions;
    use crate::registry::Registry;
    use crate::tool::Environment;
    use std::sync::Arc;

    fn manifest(version: &str) -> String {
        format!(
            r#"[tool]
id = "text.shout"
name = "Shout"
category = "text"
description = "Upper-case some words"
version = "{version}"
keywords = ["upper", "caps"]

[http]
url = "https://example.com/shout?q={{text}}"

[[params]]
name = "text"
type = "text"
required = true

[[tests]]
params = {{ text = "hi" }}
http = {{ body = "HI" }}
expect = {{ text = "HI" }}
"#
        )
    }

    fn engine(dir: &Path) -> Engine {
        let d = dir.to_path_buf();
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.temp_root = dir.join(".tmp");
        Engine::from_loader(
            cfg,
            crate::deps::DepRegistry::new(vec![]),
            Arc::new(move || {
                let mut reg = Registry::new();
                let (tools, st) = load_extensions(&d, false);
                for t in tools {
                    let _ = reg.register(t);
                }
                (reg, st)
            }),
        )
    }

    /// `manifest(version)` that declares `min_dogubako = min`.
    fn manifest_min(version: &str, min: &str) -> String {
        manifest(version).replacen(
            "keywords =",
            &format!("min_dogubako = \"{min}\"\nkeywords ="),
            1,
        )
    }

    /// A store folder for `text.shout` at `version`, published to `out`;
    /// returns the index URL.
    fn publish(root: &Path, version: &str) -> String {
        publish_text(root, &manifest(version))
    }

    fn publish_text(root: &Path, text: &str) -> String {
        let src = root.join("store");
        std::fs::create_dir_all(src.join("text.shout")).unwrap();
        std::fs::write(src.join("text.shout/manifest.toml"), text).unwrap();
        let out = root.join("public");
        build_index(&src).unwrap().write(&out).unwrap();
        format!("file://{}", out.join("index.json").display())
    }

    #[tokio::test]
    async fn cached_store_tools_are_available_until_installed() {
        let root = tempfile::tempdir().unwrap();
        let url = publish(root.path(), "1.0.0");
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let e = engine(&ext);
        assert!(cached_index(&url, &ext).is_none());
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let index = cached_index(&url, &ext).unwrap();
        let avail = available(&e, Some(&index), |_| true);
        assert_eq!(avail.len(), 1);
        assert_eq!(
            (avail[0].id.as_str(), avail[0].source),
            ("text.shout", "store")
        );
        assert_eq!(avail[0].permissions, index.tools[0].permissions);
        install(&e, &ext, &idx, "text.shout", false).await.unwrap();
        assert!(available(&e, Some(&index), |_| true).is_empty());
    }

    #[test]
    fn store_url_defaults_to_official_and_can_be_turned_off() {
        assert_eq!(resolve_url(None).as_deref(), Some(OFFICIAL_INDEX_URL));
        assert_eq!(resolve_url(Some("")), None);
        assert_eq!(resolve_url(Some("off")), None);
        assert_eq!(
            resolve_url(Some(" https://tools.example/index.json ")).as_deref(),
            Some("https://tools.example/index.json")
        );
    }

    #[test]
    fn build_lists_entries_and_detects_a_stale_index() {
        let root = tempfile::tempdir().unwrap();
        publish(root.path(), "1.0.0");
        let built = build_index(&root.path().join("store")).unwrap();
        let e = &built.index.tools[0];
        assert_eq!(e.id, "text.shout");
        assert_eq!(e.version, "1.0.0");
        assert_eq!(e.permissions, vec!["network"]);
        assert_eq!(e.url, "text.shout/manifest.toml");
        assert_eq!(e.sha256, sha256_hex(manifest("1.0.0").as_bytes()));
        let out = root.path().join("public");
        assert!(built.stale(&out).is_empty());
        std::fs::write(
            root.path().join("store/text.shout/manifest.toml"),
            manifest("1.0.1"),
        )
        .unwrap();
        let rebuilt = build_index(&root.path().join("store")).unwrap();
        assert_eq!(
            rebuilt.stale(&out),
            vec!["index.json", "text.shout/manifest.toml"]
        );
    }

    #[test]
    fn build_rejects_a_folder_not_named_after_its_id() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("wrong")).unwrap();
        std::fs::write(root.path().join("wrong/manifest.toml"), manifest("1.0.0")).unwrap();
        let err = build_index(root.path()).unwrap_err();
        assert!(err.contains("must match its folder name"), "{err}");
    }

    #[test]
    fn versions_compare_by_semver() {
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("2", "1.9"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0-beta", "1.0.0"));
    }

    #[tokio::test]
    async fn install_update_and_offline_search() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let eng = engine(&ext);
        let net = NetPolicy::default();
        let url = publish(root.path(), "1.0.0");

        let idx = load_index(&url, &ext, &net).await.unwrap();
        assert!(!idx.offline);
        let r = install(&eng, &ext, &idx, "text.shout", false)
            .await
            .unwrap();
        assert!(r.passed, "{r:?}");
        let tools = annotate(&idx.index, &eng, &ext);
        assert_eq!(tools[0].installed_version.as_deref(), Some("1.0.0"));
        assert!(!tools[0].update_available);
        assert!(update(&eng, &ext, &idx, None).await.unwrap().is_empty());

        publish(root.path(), "1.1.0");
        let idx = load_index(&url, &ext, &net).await.unwrap();
        let tools = annotate(&idx.index, &eng, &ext);
        assert_eq!(tools[0].installed_version.as_deref(), Some("1.0.0"));
        assert!(tools[0].update_available);
        let updated = update(&eng, &ext, &idx, Some("text.shout")).await.unwrap();
        assert_eq!(updated[0].version, "1.1.0");
        let tools = annotate(&idx.index, &eng, &ext);
        assert_eq!(tools[0].installed_version.as_deref(), Some("1.1.0"));
        assert!(!tools[0].update_available);

        // A manifest that fails its SHA check is reported, not raised.
        publish(root.path(), "1.2.0");
        let idx = load_index(&url, &ext, &net).await.unwrap();
        std::fs::write(
            root.path().join("public/text.shout/manifest.toml"),
            manifest("9.9.9"),
        )
        .unwrap();
        let reports = update(&eng, &ext, &idx, None).await.unwrap();
        assert_eq!(reports.len(), 1);
        assert!(!reports[0].passed);
        assert!(reports[0].error.as_deref().unwrap().contains("refusing"));
        assert_eq!(
            annotate(&idx.index, &eng, &ext)[0]
                .installed_version
                .as_deref(),
            Some("1.1.0")
        );

        // The index can't be fetched: search uses the cache, install refuses.
        std::fs::remove_file(root.path().join("public/index.json")).unwrap();
        let idx = load_index(&url, &ext, &net).await.unwrap();
        assert!(idx.offline);
        assert_eq!(search(annotate(&idx.index, &eng, &ext), "CAPS").len(), 1);
        assert!(search(annotate(&idx.index, &eng, &ext), "weather").is_empty());
        let err = install(&eng, &ext, &idx, "text.shout", true)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("the tool store is offline"), "{err}");

        // Uninstalling forgets the store record.
        extensions::uninstall(&eng, &ext, "text.shout", false).unwrap();
        assert!(!annotate(&idx.index, &eng, &ext)[0].installed);
    }

    #[tokio::test]
    async fn no_cache_and_no_network_is_a_clear_error() {
        let root = tempfile::tempdir().unwrap();
        let url = format!("file://{}/missing.json", root.path().display());
        let err = load_index(&url, root.path(), &NetPolicy::default())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("offline") && err.contains("no cached index"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn install_refuses_a_manifest_that_does_not_match_the_index() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let eng = engine(&ext);
        let url = publish(root.path(), "1.0.0");
        // The published manifest changes after the index was built.
        std::fs::write(
            root.path().join("public/text.shout/manifest.toml"),
            manifest("6.6.6"),
        )
        .unwrap();
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let err = install(&eng, &ext, &idx, "text.shout", false)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("refusing to install"), "{err}");
        assert!(!ext.join("text.shout.toml").exists());
        assert!(
            !ext.join("trusted.toml").exists()
                || !std::fs::read_to_string(ext.join("trusted.toml"))
                    .unwrap()
                    .contains("text.shout")
        );
        assert!(!annotate(&idx.index, &eng, &ext)[0].installed);
    }

    #[test]
    fn remote_stores_must_use_https() {
        let ok = |u: &str| require_https(&url::Url::parse(u).unwrap()).is_ok();
        assert!(ok("https://tools.example/index.json"));
        assert!(ok("http://localhost:8080/index.json"));
        assert!(ok("http://127.0.0.1/index.json"));
        assert!(!ok("http://tools.example/index.json"));
    }

    #[tokio::test]
    async fn file_urls_with_escapes_load_and_builds_keep_unrelated_folders() {
        let root = tempfile::tempdir().unwrap();
        let spaced = root.path().join("tool store");
        std::fs::create_dir_all(spaced.join("public/assets")).unwrap();
        publish(&spaced, "1.0.0");
        assert!(spaced.join("public/assets").is_dir());
        let built = build_index(&spaced.join("store")).unwrap();
        assert!(built.stale(&spaced.join("public")).is_empty());
        let url = url::Url::from_file_path(spaced.join("public/index.json"))
            .unwrap()
            .to_string();
        assert!(url.contains("%20"), "{url}");
        let ext = root.path().join("ext");
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        assert_eq!(idx.index.tools[0].id, "text.shout");
        let eng = engine(&ext);
        install(&eng, &ext, &idx, "text.shout", false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn install_refuses_an_entry_whose_manifest_has_another_id() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let url = publish(root.path(), "1.0.0");
        let index_path = root.path().join("public/index.json");
        let text = std::fs::read_to_string(&index_path).unwrap();
        std::fs::write(
            &index_path,
            text.replace("\"id\": \"text.shout\"", "\"id\": \"text.other\""),
        )
        .unwrap();
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let err = install(&engine(&ext), &ext, &idx, "text.other", false)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("declares id 'text.shout'"), "{err}");
        assert!(!ext.join("text.shout.toml").exists());
    }

    #[tokio::test]
    async fn a_hand_replaced_extension_is_no_longer_a_store_tool() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let eng = engine(&ext);
        let url = publish(root.path(), "1.0.0");
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        install(&eng, &ext, &idx, "text.shout", false)
            .await
            .unwrap();
        extensions::install(
            &eng,
            &ext,
            &manifest("0.9.0-custom"),
            InstallOptions { replace: true },
        )
        .await
        .unwrap();
        publish(root.path(), "2.0.0");
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let t = &annotate(&idx.index, &eng, &ext)[0];
        assert!(!t.installed && !t.update_available);
        assert!(update(&eng, &ext, &idx, None).await.unwrap().is_empty());
    }

    #[test]
    fn build_carries_min_dogubako_and_rejects_a_bad_one() {
        let root = tempfile::tempdir().unwrap();
        publish(root.path(), "1.0.0");
        let built = build_index(&root.path().join("store")).unwrap();
        assert_eq!(built.index.tools[0].min_dogubako, None);
        let json = serde_json::to_string(&built.index).unwrap();
        assert!(!json.contains("min_dogubako"), "{json}");

        publish_text(root.path(), &manifest_min("1.1.0", "0.2.0-alpha.1"));
        let built = build_index(&root.path().join("store")).unwrap();
        assert_eq!(
            built.index.tools[0].min_dogubako.as_deref(),
            Some("0.2.0-alpha.1")
        );

        let src = root.path().join("store/text.shout/manifest.toml");
        std::fs::write(&src, manifest_min("1.2.0", "next")).unwrap();
        let err = build_index(&root.path().join("store")).unwrap_err();
        assert!(err.contains("min_dogubako 'next' is not a semver"), "{err}");
    }

    #[test]
    fn min_dogubako_compares_releases_and_prereleases() {
        let e = |min: Option<&str>| StoreEntry {
            id: "text.shout".into(),
            name: "Shout".into(),
            description: String::new(),
            keywords: vec![],
            category: "text".into(),
            version: "1.1.0".into(),
            sha256: String::new(),
            permissions: vec![],
            requires: vec![],
            platforms: vec![],
            url: String::new(),
            min_dogubako: min.map(String::from),
        };
        assert_eq!(e(None).unsupported_by("0.1.0-alpha.2"), None);
        assert_eq!(
            e(Some("0.1.0-alpha.2")).unsupported_by("0.1.0-alpha.2"),
            None
        );
        assert_eq!(e(Some("0.1.0-alpha.3")).unsupported_by("0.1.0"), None);
        let why = e(Some("0.1.0-alpha.3"))
            .unsupported_by("0.1.0-alpha.2")
            .unwrap();
        assert_eq!(
            why,
            "text.shout 1.1.0 needs Dogubako 0.1.0-alpha.3 or newer (this is 0.1.0-alpha.2); \
             update Dogubako to install it"
        );
        assert!(e(Some("0.2.0")).unsupported_by("0.1.9").is_some());
        assert_eq!(
            e(Some("0.1.0-alpha.2+build.1")).unsupported_by("0.1.0-alpha.2"),
            None
        );
    }

    #[tokio::test]
    async fn install_if_judges_the_fetched_manifest() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let eng = engine(&ext);
        let text = manifest_min("1.0.0", "0.0.1").replacen(
            "keywords =",
            "permissions = [\"network\"]\nkeywords =",
            1,
        );
        let url = publish_text(root.path(), &text);
        let mut idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        // An index that understates what the manifest needs.
        assert_eq!(idx.index.tools[0].permissions, ["network"]);
        idx.index.tools[0].permissions.clear();
        let err = install_if(&eng, &ext, &idx, "text.shout", &|spec| {
            assert_eq!(spec.id, "text.shout");
            if spec.permissions.contains(&crate::spec::Permission::Network) {
                Err(ToolError::denied("no network"))
            } else {
                Ok(())
            }
        })
        .await
        .unwrap_err();
        assert_eq!(err.code(), "permission_denied");
        assert!(!ext.join("text.shout.toml").exists());
    }

    #[tokio::test]
    async fn older_apps_neither_install_nor_update_to_an_entry_that_needs_a_newer_one() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let eng = engine(&ext);
        let net = NetPolicy::default();
        let url = publish_text(root.path(), &manifest_min("1.0.0", "0.2.0"));
        let idx = load_index(&url, &ext, &net).await.unwrap();

        // Not installed: an older app says why and refuses to install.
        let t = &annotate_for(&idx.index, &eng, &ext, "0.1.0")[0];
        assert!(!t.installed && !t.update_available);
        assert!(t
            .unsupported
            .as_deref()
            .unwrap()
            .contains("needs Dogubako 0.2.0"));
        let err = install_for(&eng, &ext, &idx, "text.shout", false, "0.1.0", &|_| Ok(()))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "unsupported");
        assert!(err.to_string().contains("update Dogubako"), "{err}");
        assert!(!ext.join("text.shout.toml").exists());

        // A new enough app installs it; the installed version stays fine.
        let r = install_for(&eng, &ext, &idx, "text.shout", false, "0.2.0", &|_| Ok(()))
            .await
            .unwrap();
        assert!(r.passed, "{r:?}");
        let t = &annotate_for(&idx.index, &eng, &ext, "0.1.0")[0];
        assert!(t.installed && t.unsupported.is_none());

        // A newer version that needs a newer app is not offered as an update.
        publish_text(root.path(), &manifest_min("1.1.0", "0.3.0"));
        let idx = load_index(&url, &ext, &net).await.unwrap();
        let t = &annotate_for(&idx.index, &eng, &ext, "0.2.0")[0];
        assert!(!t.update_available);
        assert!(t
            .unsupported
            .as_deref()
            .unwrap()
            .contains("1.1.0 needs Dogubako 0.3.0"));
        assert!(update_for(&eng, &ext, &idx, None, "0.2.0")
            .await
            .unwrap()
            .is_empty());
        let err = update_for(&eng, &ext, &idx, Some("text.shout"), "0.2.0")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("needs Dogubako 0.3.0"), "{err}");
        let updated = update_for(&eng, &ext, &idx, None, "0.3.0").await.unwrap();
        assert_eq!(updated[0].version, "1.1.0");
    }

    #[tokio::test]
    async fn entries_this_app_cannot_load_are_not_available() {
        let root = tempfile::tempdir().unwrap();
        let ext = root.path().join("ext");
        std::fs::create_dir_all(&ext).unwrap();
        let eng = engine(&ext);
        let url = publish_text(root.path(), &manifest_min("1.0.0", APP_VERSION));
        load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let index = cached_index(&url, &ext).unwrap();
        assert_eq!(available(&eng, Some(&index), |_| true).len(), 1);

        publish_text(root.path(), &manifest_min("1.0.0", "999.0.0"));
        load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let index = cached_index(&url, &ext).unwrap();
        assert!(available(&eng, Some(&index), |_| true).is_empty());
        let idx = load_index(&url, &ext, &NetPolicy::default()).await.unwrap();
        let err = install(&eng, &ext, &idx, "text.shout", false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("needs Dogubako 999.0.0"), "{err}");
    }
}
