//! Safe, limited extraction into a private directory.

use dogubako_core::prelude::*;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Validate an archive entry name and turn it into a relative path.
/// Rejects absolute paths, drive letters, `..` components and NULs.
pub fn safe_rel_path(name: &str) -> Result<PathBuf, &'static str> {
    if name.contains('\0') {
        return Err("name contains a NUL byte");
    }
    let n = name.replace('\\', "/");
    if n.starts_with('/') {
        return Err("absolute path");
    }
    let b = n.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Err("absolute path (drive letter)");
    }
    let mut out = PathBuf::new();
    for comp in n.split('/') {
        match comp {
            "" | "." => continue,
            ".." => return Err("path traversal ('..')"),
            c => {
                if c.chars().any(|ch| ch.is_control()) {
                    return Err("name contains control characters");
                }
                out.push(c);
            }
        }
    }
    if out.as_os_str().is_empty() {
        return Err("empty name");
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub struct Extracted {
    pub rel: String,
    pub size: u64,
}

/// Writes entries below `root`, never following or creating links, never
/// overwriting, and counting the bytes actually written against limits.
pub struct Extractor<'a> {
    ctx: &'a ToolContext,
    pub root: PathBuf,
    pub max_bytes: u64,
    pub max_entries: u64,
    pub total_bytes: u64,
    pub entries: u64,
    pub files: Vec<Extracted>,
    pub dirs: u64,
    pub skipped: Vec<(String, String)>,
    seen: HashSet<PathBuf>,
}

impl<'a> Extractor<'a> {
    pub fn new(ctx: &'a ToolContext, root: PathBuf, max_bytes: u64, max_entries: u64) -> Self {
        Extractor {
            ctx,
            root,
            max_bytes,
            max_entries,
            total_bytes: 0,
            entries: 0,
            files: vec![],
            dirs: 0,
            skipped: vec![],
            seen: HashSet::new(),
        }
    }

    pub fn skip(&mut self, name: &str, reason: impl Into<String>) {
        if self.skipped.len() < 10_000 {
            self.skipped.push((name.to_string(), reason.into()));
        }
    }

    /// Count an entry against the entry limit (call for every entry seen).
    pub fn count_entry(&mut self) -> ToolResult<()> {
        self.entries += 1;
        if self.entries > self.max_entries {
            return Err(ToolError::limit(format!(
                "archive has more than {} entries",
                self.max_entries
            )));
        }
        if self.entries.is_multiple_of(256) {
            self.ctx.check_cancelled()?;
        }
        Ok(())
    }

    /// Ensure every ancestor of `rel` below root is a real directory we made.
    fn prepare_parent(&mut self, rel: &Path) -> Result<PathBuf, String> {
        let mut cur = self.root.clone();
        if let Some(parent) = rel.parent() {
            for c in parent.components() {
                cur.push(c);
                match std::fs::symlink_metadata(&cur) {
                    Ok(m) if m.is_dir() => {}
                    Ok(_) => return Err("a parent path is not a directory".into()),
                    Err(_) => std::fs::create_dir(&cur)
                        .map_err(|e| format!("cannot create directory: {e}"))?,
                }
            }
        }
        Ok(cur.join(rel.file_name().ok_or("no file name")?))
    }

    pub fn add_dir(&mut self, name: &str) {
        let rel = match safe_rel_path(name) {
            Ok(r) => r,
            Err(why) => return self.skip(name, why),
        };
        let target = self.root.join(&rel);
        if let Err(e) =
            self.prepare_parent(&rel)
                .and_then(|_| match std::fs::symlink_metadata(&target) {
                    Ok(m) if m.is_dir() => Ok(()),
                    Ok(_) => Err("exists and is not a directory".to_string()),
                    Err(_) => std::fs::create_dir(&target).map_err(|e| e.to_string()),
                })
        {
            return self.skip(name, e);
        }
        self.dirs += 1;
    }

    /// Stream a regular file entry to disk. Errors only on limit/cancel/IO
    /// failures that must abort the whole extraction; per-entry problems are
    /// recorded as skips.
    pub fn add_file(&mut self, name: &str, reader: &mut dyn Read) -> ToolResult<()> {
        let rel = match safe_rel_path(name) {
            Ok(r) => r,
            Err(why) => {
                self.skip(name, why);
                return Ok(());
            }
        };
        let key = PathBuf::from(rel.to_string_lossy().to_lowercase());
        if !self.seen.insert(key) {
            self.skip(name, "duplicate entry name");
            return Ok(());
        }
        let target = match self.prepare_parent(&rel) {
            Ok(t) => t,
            Err(e) => {
                self.skip(name, e);
                return Ok(());
            }
        };
        let mut f = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(f) => f,
            Err(e) => {
                self.skip(name, format!("cannot create file: {e}"));
                return Ok(());
            }
        };
        let mut buf = vec![0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let n = match reader.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    drop(f);
                    let _ = std::fs::remove_file(&target);
                    return Err(ToolError::invalid(
                        "input",
                        format!("corrupt archive data in '{name}': {e}"),
                    ));
                }
            };
            if n == 0 {
                break;
            }
            size += n as u64;
            self.total_bytes += n as u64;
            if self.total_bytes > self.max_bytes {
                drop(f);
                let _ = std::fs::remove_file(&target);
                return Err(ToolError::limit(format!(
                    "extracted data exceeds the limit of {} (possible decompression bomb)",
                    human_size(self.max_bytes)
                )));
            }
            f.write_all(&buf[..n])?;
            if size % (16 << 20) < n as u64 {
                self.ctx.check_cancelled()?;
            }
        }
        self.files.push(Extracted {
            rel: rel.to_string_lossy().replace('\\', "/"),
            size,
        });
        Ok(())
    }
}

/// Reader that fails once more than `limit` bytes were produced (used for
/// single-stream decompression).
pub struct LimitedReader<R> {
    pub inner: R,
    pub limit: u64,
    pub read: u64,
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if self.read > self.limit {
            return Err(std::io::Error::other("size limit exceeded"));
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths() {
        assert_eq!(
            safe_rel_path("a/b/c.txt").unwrap(),
            PathBuf::from("a/b/c.txt")
        );
        assert_eq!(safe_rel_path("./a//b").unwrap(), PathBuf::from("a/b"));
        assert_eq!(
            safe_rel_path("dir\\file").unwrap(),
            PathBuf::from("dir/file")
        );
        for bad in [
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a/..",
            "C:\\Windows\\x",
            "c:x",
            "\\\\server\\share",
            "",
            ".",
            "a\0b",
            "a/\nb",
        ] {
            assert!(safe_rel_path(bad).is_err(), "{bad:?}");
        }
    }
}
