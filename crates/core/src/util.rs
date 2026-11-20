//! Small shared helpers.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// MIME type for a path, by extension, falling back to content sniffing.
pub fn mime_for_path(path: &Path) -> String {
    if let Some(m) = mime_override(path) {
        return m.to_string();
    }
    if let Some(m) = mime_guess::from_path(path).first_raw() {
        return m.to_string();
    }
    sniff_mime(path).unwrap_or_else(|| "application/octet-stream".to_string())
}

/// Types `mime_guess` lacks or gets wrong for files the tools write.
fn mime_override(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "qoi" => "image/qoi",
        "ass" | "ssa" => "text/x-ssa",
        "m4a" => "audio/mp4",
        _ => return None,
    })
}

/// Detect common formats from magic bytes.
pub fn sniff_mime(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 16];
    let n = f.read(&mut buf).ok()?;
    sniff_bytes(&buf[..n]).map(|s| s.to_string())
}

pub fn sniff_bytes(b: &[u8]) -> Option<&'static str> {
    let starts = |p: &[u8]| b.starts_with(p);
    Some(if starts(b"\x89PNG") {
        "image/png"
    } else if starts(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if starts(b"GIF8") {
        "image/gif"
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WAVE" {
        "audio/wav"
    } else if starts(b"%PDF") {
        "application/pdf"
    } else if starts(b"PK\x03\x04") {
        "application/zip"
    } else if starts(b"\x1F\x8B") {
        "application/gzip"
    } else if starts(b"BM") {
        "image/bmp"
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        "image/tiff"
    } else if starts(b"ID3") || starts(b"\xFF\xFB") {
        "audio/mpeg"
    } else if starts(b"fLaC") {
        "audio/flac"
    } else if starts(b"OggS") {
        "audio/ogg"
    } else if b.len() >= 12 && &b[4..8] == b"ftyp" {
        match &b[8..12] {
            b"heic" | b"heix" | b"mif1" | b"msf1" => "image/heic",
            b"avif" | b"avis" => "image/avif",
            b"M4A " => "audio/mp4",
            b"qt  " => "video/quicktime",
            _ => "video/mp4",
        }
    } else if starts(b"qoif") {
        "image/qoi"
    } else if starts(b"\x1A\x45\xDF\xA3") {
        "video/webm"
    } else {
        return None;
    })
}

/// Human readable byte size.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Random URL-safe identifier with `bytes` bytes of entropy (hex encoded).
pub fn random_id(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::fill(&mut buf[..]);
    hex::encode(buf)
}

/// Replace the extension of a file name.
pub fn with_extension(name: &str, ext: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{}", ext.trim_start_matches('.'))
    }
}

/// `photo.jpg` + `small` → `photo-small.jpg` (optionally changing extension).
pub fn suffixed(name: &str, suffix: &str, ext: Option<&str>) -> String {
    let p = Path::new(name);
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let ext = ext
        .map(|e| e.trim_start_matches('.').to_string())
        .or_else(|| p.extension().map(|e| e.to_string_lossy().to_string()))
        .unwrap_or_default();
    if ext.is_empty() {
        format!("{stem}-{suffix}")
    } else {
        format!("{stem}-{suffix}.{ext}")
    }
}

/// A relative path made only of plain components (no root, drive, `..`),
/// with `/` or `\` separators; `None` when it would leave its base folder.
pub fn plain_rel_path(rel: &str) -> Option<PathBuf> {
    if rel.starts_with(['/', '\\']) {
        return None;
    }
    let mut out = PathBuf::new();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part.contains(':') || part.chars().any(|c| c.is_control()) {
            return None;
        }
        let mut comps = Path::new(part).components();
        match (comps.next(), comps.next()) {
            (Some(Component::Normal(c)), None) => out.push(c),
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Copy `(source, relative target)` pairs into `dest`, creating folders from
/// the relative paths. Unsafe targets fall back to the source's file name;
/// existing files are never overwritten (`a.txt` becomes `a-2.txt`).
/// Returns where each file was written.
pub fn copy_files_into(items: &[(PathBuf, String)], dest: &Path) -> std::io::Result<Vec<PathBuf>> {
    let root = std::fs::canonicalize(dest)?;
    let mut written = Vec::with_capacity(items.len());
    for (src, rel) in items {
        let rel = plain_rel_path(rel)
            .or_else(|| src.file_name().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("output"));
        let target = dest.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
            if !std::fs::canonicalize(parent)?.starts_with(&root) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{} leads outside the chosen folder", rel.display()),
                ));
            }
        }
        let name = rel
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut candidate = target.clone();
        let mut i = 2;
        let mut out = loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(f) => break f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    candidate = target.with_file_name(suffixed(&name, &i.to_string(), None));
                    i += 1;
                }
                Err(e) => return Err(e),
            }
        };
        std::io::copy(&mut std::fs::File::open(src)?, &mut out)?;
        written.push(candidate);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_for_formats_mime_guess_lacks() {
        for (name, want) in [
            ("a.qoi", "image/qoi"),
            ("subs.ass", "text/x-ssa"),
            ("subs.SSA", "text/x-ssa"),
            ("song.m4a", "audio/mp4"),
            ("a.tga", "image/x-tga"),
            ("a.ppm", "image/x-portable-pixmap"),
            ("a.tiff", "image/tiff"),
        ] {
            assert_eq!(mime_for_path(Path::new(name)), want, "{name}");
        }
        assert_eq!(
            sniff_bytes(b"qoif\0\0\0\x01\0\0\0\x01\x04\0"),
            Some("image/qoi")
        );
    }

    #[test]
    fn plain_rel_path_rejects_escapes() {
        assert_eq!(
            plain_rel_path("docs/a.txt"),
            Some(PathBuf::from("docs").join("a.txt"))
        );
        assert_eq!(
            plain_rel_path("docs\\sub\\a.txt"),
            Some(PathBuf::from("docs/sub/a.txt"))
        );
        assert_eq!(plain_rel_path("./a.txt"), Some(PathBuf::from("a.txt")));
        for bad in [
            "",
            "/etc/passwd",
            "../a",
            "a/../../b",
            "C:/x",
            "a/C:x",
            "a\u{0}b",
        ] {
            assert_eq!(plain_rel_path(bad), None, "{bad:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn copy_files_into_refuses_symlinked_folders() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let a = src.path().join("a.txt");
        std::fs::write(&a, "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dest.path().join("docs")).unwrap();
        let err = copy_files_into(&[(a, "docs/a.txt".into())], dest.path()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(!outside.path().join("a.txt").exists());
    }

    #[test]
    fn copy_files_into_keeps_folders_and_never_overwrites() {
        let src = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        let a = src.path().join("a.txt");
        let a2 = src.path().join("a-2.txt");
        std::fs::write(&a, "top").unwrap();
        std::fs::write(&a2, "nested").unwrap();
        std::fs::write(dest.path().join("a.txt"), "already here").unwrap();
        let items = vec![
            (a.clone(), "a.txt".to_string()),
            (a2.clone(), "docs/a.txt".to_string()),
            (a.clone(), "../escape.txt".to_string()),
        ];
        let out = copy_files_into(&items, dest.path()).unwrap();
        assert_eq!(out[0], dest.path().join("a-2.txt"));
        assert_eq!(out[1], dest.path().join("docs").join("a.txt"));
        assert_eq!(out[2], dest.path().join("a-3.txt"));
        assert_eq!(
            std::fs::read_to_string(dest.path().join("a.txt")).unwrap(),
            "already here"
        );
        assert_eq!(std::fs::read_to_string(&out[0]).unwrap(), "top");
        assert_eq!(std::fs::read_to_string(&out[1]).unwrap(), "nested");
        assert!(!dest.path().parent().unwrap().join("escape.txt").exists());
    }
}
