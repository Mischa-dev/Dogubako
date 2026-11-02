//! Archive format detection, listing and entry iteration.

use super::safe::Extractor;
use crate::util::read_prefix;
use dogubako_core::prelude::*;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Comp {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

impl Comp {
    pub fn label(&self) -> &'static str {
        match self {
            Comp::None => "",
            Comp::Gzip => "gzip",
            Comp::Bzip2 => "bzip2",
            Comp::Xz => "xz",
            Comp::Zstd => "zstd",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Zip,
    Tar(Comp),
    /// A single compressed file (not a tar).
    Single(Comp),
    /// Handled by 7-Zip (7z, rar, ...).
    External(&'static str),
}

impl Format {
    pub fn label(&self) -> String {
        match self {
            Format::Zip => "zip".into(),
            Format::Tar(Comp::None) => "tar".into(),
            Format::Tar(c) => format!("tar.{}", c.label()),
            Format::Single(c) => c.label().into(),
            Format::External(n) => (*n).into(),
        }
    }
}

pub fn comp_from_magic(b: &[u8]) -> Option<Comp> {
    if b.starts_with(&[0x1f, 0x8b]) {
        Some(Comp::Gzip)
    } else if b.starts_with(b"BZh") {
        Some(Comp::Bzip2)
    } else if b.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        Some(Comp::Xz)
    } else if b.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Some(Comp::Zstd)
    } else {
        None
    }
}

/// True if `b` starts with a plausible tar header (ustar magic or valid checksum).
pub fn is_tar(b: &[u8]) -> bool {
    if b.len() < 512 || b[..512].iter().all(|&x| x == 0) {
        return false;
    }
    if &b[257..262] == b"ustar" {
        return true;
    }
    let field = String::from_utf8_lossy(&b[148..156]);
    let Ok(expected) = u32::from_str_radix(field.trim_matches(|c: char| c == '\0' || c == ' '), 8)
    else {
        return false;
    };
    let sum: u32 = b[..512]
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            if (148..156).contains(&i) {
                32
            } else {
                x as u32
            }
        })
        .sum();
    sum == expected
}

pub fn decoder<'a, R: Read + 'a>(c: Comp, r: R) -> ToolResult<Box<dyn Read + 'a>> {
    Ok(match c {
        Comp::None => Box::new(r),
        Comp::Gzip => Box::new(flate2::read::MultiGzDecoder::new(r)),
        Comp::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(r)),
        Comp::Xz => Box::new(liblzma::read::XzDecoder::new_multi_decoder(r)),
        Comp::Zstd => Box::new(
            zstd::stream::read::Decoder::new(r)
                .map_err(|e| ToolError::invalid("input", format!("bad zstd stream: {e}")))?,
        ),
    })
}

pub fn detect(path: &Path) -> ToolResult<Format> {
    let head = read_prefix(path, 1024)?;
    if head.starts_with(b"PK\x03\x04")
        || head.starts_with(b"PK\x05\x06")
        || head.starts_with(b"PK\x07\x08")
    {
        return Ok(Format::Zip);
    }
    if head.starts_with(&[b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C]) {
        return Ok(Format::External("7z"));
    }
    if head.starts_with(b"Rar!\x1a\x07") {
        return Ok(Format::External("rar"));
    }
    if let Some(c) = comp_from_magic(&head) {
        let mut dec = decoder(c, std::fs::File::open(path)?)?;
        let mut block = vec![0u8; 512];
        let mut got = 0;
        while got < 512 {
            match dec.read(&mut block[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) => {
                    return Err(ToolError::invalid(
                        "input",
                        format!("corrupt {} data: {e}", c.label()),
                    ))
                }
            }
        }
        return Ok(if is_tar(&block[..got]) {
            Format::Tar(c)
        } else {
            Format::Single(c)
        });
    }
    if is_tar(&head) {
        return Ok(Format::Tar(Comp::None));
    }
    Err(ToolError::invalid(
        "input",
        "not a supported archive (zip, tar, tar.gz/bz2/xz/zst, gz, bz2, xz, zst, 7z, rar)",
    ))
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub size: u64,
    pub compressed: Option<u64>,
    pub modified: Option<String>,
    pub kind: &'static str,
    pub link: Option<String>,
    pub encrypted: bool,
}

fn zip_kind(f: &zip::read::ZipFile<'_, std::fs::File>) -> &'static str {
    if f.is_symlink() {
        return "symlink";
    }
    if f.is_dir() {
        return "dir";
    }
    match f.unix_mode().map(|m| m & 0o170000) {
        Some(0o020000) | Some(0o060000) => "device",
        Some(0o010000) | Some(0o140000) => "special",
        _ => "file",
    }
}

fn zip_time(f: &zip::read::ZipFile<'_, std::fs::File>) -> Option<String> {
    f.last_modified().map(|d| {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            d.year(),
            d.month(),
            d.day(),
            d.hour(),
            d.minute(),
            d.second()
        )
    })
}

fn open_zip(path: &Path) -> ToolResult<zip::ZipArchive<std::fs::File>> {
    zip::ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|e| ToolError::invalid("input", format!("cannot read zip: {e}")))
}

pub fn list_zip(ctx: &ToolContext, path: &Path, max: u64) -> ToolResult<Vec<Entry>> {
    let mut z = open_zip(path)?;
    if z.len() as u64 > max {
        return Err(ToolError::limit(format!(
            "archive lists {} entries (limit {max})",
            z.len()
        )));
    }
    let mut v = Vec::with_capacity(z.len());
    for i in 0..z.len() {
        if i % 512 == 0 {
            ctx.check_cancelled()?;
        }
        let f = z
            .by_index_raw(i)
            .map_err(|e| ToolError::invalid("input", format!("bad zip entry {i}: {e}")))?;
        let kind = zip_kind(&f);
        v.push(Entry {
            name: f
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| String::from_utf8_lossy(f.name_raw()).to_string()),
            size: f.size(),
            compressed: Some(f.compressed_size()),
            modified: zip_time(&f),
            kind,
            link: None,
            encrypted: f.encrypted(),
        });
    }
    Ok(v)
}

pub fn list_tar(ctx: &ToolContext, path: &Path, c: Comp, max: u64) -> ToolResult<Vec<Entry>> {
    let mut ar = tar::Archive::new(decoder(c, std::fs::File::open(path)?)?);
    let mut v = vec![];
    for (i, e) in ar
        .entries()
        .map_err(|e| ToolError::invalid("input", format!("cannot read tar: {e}")))?
        .enumerate()
    {
        if i % 256 == 0 {
            ctx.check_cancelled()?;
        }
        if i as u64 >= max {
            return Err(ToolError::limit(format!(
                "archive has more than {max} entries"
            )));
        }
        let e = e.map_err(|e| ToolError::invalid("input", format!("corrupt tar entry: {e}")))?;
        let h = e.header();
        let kind = tar_kind(h.entry_type());
        let link = e
            .link_name_bytes()
            .map(|l| String::from_utf8_lossy(&l).to_string());
        let mtime = h.mtime().ok().map(|t| {
            crate::util::fmt_time_utc(std::time::UNIX_EPOCH + std::time::Duration::from_secs(t))
        });
        v.push(Entry {
            name: String::from_utf8_lossy(&e.path_bytes()).to_string(),
            size: e.size(),
            compressed: None,
            modified: mtime,
            kind,
            link,
            encrypted: false,
        });
    }
    Ok(v)
}

fn tar_kind(t: tar::EntryType) -> &'static str {
    use tar::EntryType::*;
    match t {
        Regular | Continuous => "file",
        Directory => "dir",
        Symlink => "symlink",
        Link => "hardlink",
        Char | Block => "device",
        Fifo => "special",
        _ => "other",
    }
}

// ---------------------------------------------------------------------------
// Extraction (native formats)

pub fn extract_zip(ex: &mut Extractor, path: &Path, password: Option<&str>) -> ToolResult<()> {
    let mut z = open_zip(path)?;
    if z.len() as u64 > ex.max_entries {
        return Err(ToolError::limit(format!(
            "archive lists {} entries (limit {})",
            z.len(),
            ex.max_entries
        )));
    }
    for i in 0..z.len() {
        ex.count_entry()?;
        let (name, kind, encrypted) = {
            let f = z
                .by_index_raw(i)
                .map_err(|e| ToolError::invalid("input", format!("bad zip entry {i}: {e}")))?;
            (
                f.name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| String::from_utf8_lossy(f.name_raw()).to_string()),
                zip_kind(&f),
                f.encrypted(),
            )
        };
        match kind {
            "dir" => ex.add_dir(&name),
            "file" => {
                let mut f = if encrypted {
                    let pw = password.ok_or_else(|| {
                        ToolError::invalid(
                            "password",
                            "the archive is encrypted; enter its password",
                        )
                    })?;
                    z.by_index_decrypt(i, pw.as_bytes()).map_err(|e| {
                        ToolError::invalid("password", format!("cannot decrypt '{name}': {e}"))
                    })?
                } else {
                    z.by_index(i).map_err(|e| {
                        ToolError::invalid("input", format!("cannot read '{name}': {e}"))
                    })?
                };
                ex.add_file(&name, &mut f)?;
            }
            "symlink" => ex.skip(&name, "symbolic link (not extracted)"),
            other => ex.skip(&name, format!("{other} entry (not extracted)")),
        }
    }
    Ok(())
}

pub fn extract_tar(ex: &mut Extractor, path: &Path, c: Comp) -> ToolResult<()> {
    let mut ar = tar::Archive::new(decoder(c, std::fs::File::open(path)?)?);
    for e in ar
        .entries()
        .map_err(|e| ToolError::invalid("input", format!("cannot read tar: {e}")))?
    {
        ex.count_entry()?;
        let mut e =
            e.map_err(|e| ToolError::invalid("input", format!("corrupt tar entry: {e}")))?;
        let name = String::from_utf8_lossy(&e.path_bytes()).to_string();
        match tar_kind(e.header().entry_type()) {
            "file" => ex.add_file(&name, &mut e)?,
            "dir" => ex.add_dir(&name),
            "symlink" => ex.skip(&name, "symbolic link (not extracted)"),
            "hardlink" => ex.skip(&name, "hard link (not extracted)"),
            other => ex.skip(&name, format!("{other} entry (not extracted)")),
        }
    }
    Ok(())
}

/// Output name for a single compressed file: `data.csv.gz` → `data.csv`.
pub fn single_name(archive_name: &str, c: Comp) -> String {
    let lower = archive_name.to_ascii_lowercase();
    let exts: &[&str] = match c {
        Comp::Gzip => &[".gz", ".gzip", ".z"],
        Comp::Bzip2 => &[".bz2", ".bzip2", ".bz"],
        Comp::Xz => &[".xz", ".lzma"],
        Comp::Zstd => &[".zst", ".zstd"],
        Comp::None => &[],
    };
    for e in exts {
        if lower.ends_with(e) && lower.len() > e.len() {
            return archive_name[..archive_name.len() - e.len()].to_string();
        }
    }
    if lower.ends_with(".tgz") {
        return format!("{}.tar", &archive_name[..archive_name.len() - 4]);
    }
    format!("{archive_name}.out")
}

pub fn extract_single(
    ex: &mut Extractor,
    path: &Path,
    c: Comp,
    archive_name: &str,
) -> ToolResult<()> {
    ex.count_entry()?;
    let mut dec = decoder(c, std::fs::File::open(path)?)?;
    let name = dogubako_core::policy::sanitize_filename(&single_name(archive_name, c));
    ex.add_file(&name, &mut dec)
}
