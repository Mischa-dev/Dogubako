//! Shared helpers: directory walking, hashing, formatting.

use dogubako_core::prelude::*;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Directory walking

pub struct WalkOpts {
    pub max_entries: u64,
    pub follow_symlinks: bool,
    pub skip_hidden: bool,
    /// When the entry cap is hit: `true` stops quietly (sets `truncated`),
    /// `false` fails with a limit error.
    pub truncate_ok: bool,
}

impl WalkOpts {
    pub fn from_params(p: &Params) -> Self {
        WalkOpts {
            max_entries: p.int_or("max_entries", 1_000_000).max(1) as u64,
            follow_symlinks: p.bool("follow_symlinks"),
            skip_hidden: p.bool("skip_hidden"),
            truncate_ok: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct WalkStats {
    pub entries: u64,
    pub errors: u64,
    pub skipped_links: u64,
    pub truncated: bool,
}

impl WalkStats {
    pub fn warnings(&self) -> Vec<String> {
        let mut w = vec![];
        if self.errors > 0 {
            w.push(format!(
                "{} entries could not be read (permissions or I/O errors) and were skipped",
                self.errors
            ));
        }
        if self.truncated {
            w.push(format!(
                "stopped after {} entries (raise 'max_entries' to scan more)",
                self.entries
            ));
        }
        w
    }
}

/// Standard walker parameters for host-directory tools.
pub fn walk_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::boolean("skip_hidden")
            .label("Skip hidden files and folders")
            .advanced(),
        ParamSpec::boolean("follow_symlinks")
            .label("Follow symbolic links")
            .describe("Off by default; links leaving the allowed folders are never followed")
            .advanced(),
        ParamSpec::integer("max_entries")
            .label("Maximum entries to scan")
            .range(1.0, 100_000_000.0)
            .default_int(1_000_000)
            .advanced(),
    ]
}

fn is_hidden(e: &walkdir::DirEntry) -> bool {
    e.depth() > 0 && e.file_name().to_string_lossy().starts_with('.')
}

/// Walk `root` without following symlinks (unless asked, and then only to
/// targets the filesystem policy allows), honoring cancellation, the entry
/// cap, and reporting progress. The callback sees every entry except the
/// root itself; symlinks are reported only when not followed.
pub fn walk(
    ctx: &ToolContext,
    root: &Path,
    opts: &WalkOpts,
    mut f: impl FnMut(&walkdir::DirEntry) -> ToolResult<()>,
) -> ToolResult<WalkStats> {
    let mut stats = WalkStats::default();
    let mut it = walkdir::WalkDir::new(root)
        .follow_links(opts.follow_symlinks)
        .into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(_)) => {
                stats.errors += 1;
                continue;
            }
            Some(Ok(e)) => e,
        };
        if entry.depth() == 0 {
            continue;
        }
        if opts.skip_hidden && is_hidden(&entry) {
            if entry.file_type().is_dir() {
                it.skip_current_dir();
            }
            continue;
        }
        if opts.follow_symlinks
            && entry.path_is_symlink()
            && ctx.fs_policy().check("path", entry.path()).is_err()
        {
            stats.skipped_links += 1;
            if entry.file_type().is_dir() {
                it.skip_current_dir();
            }
            continue;
        }
        stats.entries += 1;
        if stats.entries > opts.max_entries {
            if opts.truncate_ok {
                stats.entries -= 1;
                stats.truncated = true;
                break;
            }
            return Err(ToolError::limit(format!(
                "more than {} entries; raise 'max_entries' or choose a smaller folder",
                opts.max_entries
            )));
        }
        if stats.entries % 2048 == 0 {
            ctx.check_cancelled()?;
            ctx.message(format!("Scanned {} entries…", stats.entries));
        }
        f(&entry)?;
    }
    Ok(stats)
}

/// Path relative to `root`, with `/` separators.
pub fn rel(root: &Path, p: &Path) -> String {
    let r = p.strip_prefix(root).unwrap_or(p);
    r.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn fmt_time(t: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn fmt_time_utc(t: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = t.into();
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn table(columns: &[&str], rows: Vec<Value>) -> Value {
    json!({"columns": columns, "rows": rows})
}

// ---------------------------------------------------------------------------
// Hashing

pub const ALGOS: &[(&str, &str)] = &[
    ("md5", "MD5"),
    ("sha1", "SHA-1"),
    ("sha256", "SHA-256"),
    ("sha512", "SHA-512"),
    ("sha3-256", "SHA3-256"),
    ("blake3", "BLAKE3"),
    ("crc32", "CRC32"),
    ("xxh3", "XXH3-64"),
];

pub enum Hasher {
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha512(Box<sha2::Sha512>),
    Sha3(Box<sha3::Sha3_256>),
    Blake3(Box<blake3::Hasher>),
    Crc32(crc32fast::Hasher),
    Xxh3(Box<xxhash_rust::xxh3::Xxh3>),
}

impl Hasher {
    pub fn new(algo: &str) -> Option<Hasher> {
        use sha2::Digest;
        Some(match algo {
            "md5" => Hasher::Md5(md5::Md5::new()),
            "sha1" => Hasher::Sha1(sha1::Sha1::new()),
            "sha256" => Hasher::Sha256(sha2::Sha256::new()),
            "sha512" => Hasher::Sha512(Box::new(sha2::Sha512::new())),
            "sha3-256" => Hasher::Sha3(Box::new(sha3::Sha3_256::new())),
            "blake3" => Hasher::Blake3(Box::new(blake3::Hasher::new())),
            "crc32" => Hasher::Crc32(crc32fast::Hasher::new()),
            "xxh3" => Hasher::Xxh3(Box::new(xxhash_rust::xxh3::Xxh3::new())),
            _ => return None,
        })
    }

    pub fn update(&mut self, b: &[u8]) {
        use sha2::Digest;
        match self {
            Hasher::Md5(h) => h.update(b),
            Hasher::Sha1(h) => h.update(b),
            Hasher::Sha256(h) => h.update(b),
            Hasher::Sha512(h) => h.update(b),
            Hasher::Sha3(h) => h.update(b),
            Hasher::Blake3(h) => {
                h.update(b);
            }
            Hasher::Crc32(h) => h.update(b),
            Hasher::Xxh3(h) => h.update(b),
        }
    }

    pub fn hex(self) -> String {
        use sha2::Digest;
        match self {
            Hasher::Md5(h) => hex::encode(h.finalize()),
            Hasher::Sha1(h) => hex::encode(h.finalize()),
            Hasher::Sha256(h) => hex::encode(h.finalize()),
            Hasher::Sha512(h) => hex::encode(h.finalize()),
            Hasher::Sha3(h) => hex::encode(h.finalize()),
            Hasher::Blake3(h) => h.finalize().to_hex().to_string(),
            Hasher::Crc32(h) => format!("{:08x}", h.finalize()),
            Hasher::Xxh3(h) => format!("{:016x}", h.digest()),
        }
    }
}

/// Hash a reader with several algorithms in one pass.
pub fn hash_reader(
    ctx: &ToolContext,
    mut r: impl Read,
    algos: &[String],
    mut on_bytes: impl FnMut(u64),
) -> ToolResult<Vec<String>> {
    let mut hs: Vec<Hasher> = algos
        .iter()
        .map(|a| {
            Hasher::new(a)
                .ok_or_else(|| ToolError::invalid("algorithms", format!("unknown algorithm '{a}'")))
        })
        .collect::<ToolResult<_>>()?;
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for h in hs.iter_mut() {
            h.update(&buf[..n]);
        }
        total += n as u64;
        on_bytes(total);
        ctx.check_cancelled()?;
    }
    Ok(hs.into_iter().map(Hasher::hex).collect())
}

pub fn hash_file(ctx: &ToolContext, path: &Path, algos: &[String]) -> ToolResult<Vec<String>> {
    let f = std::fs::File::open(path)?;
    hash_reader(ctx, f, algos, |_| {})
}

pub fn blake3_file(ctx: &ToolContext, path: &Path) -> ToolResult<String> {
    Ok(hash_file(ctx, path, &["blake3".to_string()])?.remove(0))
}

/// Candidate algorithms for a hex digest of the given length.
pub fn algos_for_len(len: usize) -> &'static [&'static str] {
    match len {
        8 => &["crc32"],
        16 => &["xxh3"],
        32 => &["md5"],
        40 => &["sha1"],
        64 => &["sha256", "blake3", "sha3-256"],
        128 => &["sha512"],
        _ => &[],
    }
}

/// Sniff whether a byte prefix looks like text (no NULs, mostly printable).
pub fn looks_binary(b: &[u8]) -> bool {
    if b.contains(&0) {
        return true;
    }
    if std::str::from_utf8(b).is_ok() {
        return false;
    }
    let ctrl = b
        .iter()
        .filter(|&&c| c < 0x09 || (0x0E..0x20).contains(&c))
        .count();
    ctrl * 10 > b.len()
}

pub fn read_prefix(path: &Path, n: usize) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = f.read(&mut buf[got..])?;
        if k == 0 {
            break;
        }
        got += k;
    }
    buf.truncate(got);
    Ok(buf)
}

/// Host path param that must be a directory.
pub fn dir_param(name: &str, label: &str) -> ParamSpec {
    ParamSpec::path(name, PathKind::Dir)
        .label(label)
        .required()
        .primary()
}
