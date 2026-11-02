//! Small shared helpers.

use std::io::Read;
use std::path::Path;

/// MIME type for a path, by extension, falling back to content sniffing.
pub fn mime_for_path(path: &Path) -> String {
    if let Some(m) = mime_guess::from_path(path).first_raw() {
        return m.to_string();
    }
    sniff_mime(path).unwrap_or_else(|| "application/octet-stream".to_string())
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
