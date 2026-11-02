//! Archive tools: list, extract (safely), create, and single-file compression.

pub mod formats;
pub mod safe;
pub mod sevenzip;

use crate::util::*;
use dogubako_core::prelude::*;
use formats::Format;
use safe::{Extractor, LimitedReader};
use std::io::{Read, Write};
use std::path::Path;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        archive_list(),
        archive_extract(),
        archive_create(),
        compress(),
        decompress(),
    ]
}

const ARCHIVE_ACCEPT: &[&str] = &[
    ".zip", ".tar", ".tgz", ".gz", ".tbz", ".tbz2", ".bz2", ".txz", ".xz", ".zst", ".tzst", ".7z",
    ".rar", ".jar", ".apk", ".epub", ".docx", ".xlsx", ".pptx", ".odt", ".cbz", ".whl", ".nupkg",
    ".lzma",
];

fn archive_param() -> ParamSpec {
    ParamSpec::file("input")
        .label("Archive")
        .accept(ARCHIVE_ACCEPT)
        .required()
        .primary()
}

fn archive_list() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.archive_list", "List Archive Contents", Category::Files)
        .describe("Show what is inside a zip, tar, 7z or rar archive (sizes, compression ratios, dates) and flag suspicious entries")
        .keywords(&["zip", "tar", "7z", "rar", "contents", "list", "inspect", "archive", "unzip -l"])
        .param(archive_param())
        .param(ParamSpec::secret("password").label("Password").describe("For encrypted 7z/rar archives (zip listings do not need it)").advanced())
        .output(OutputKind::Data)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let path = f.path.clone();
        let fmt = formats::detect(&path)?;
        let max = ctx
            .limits()
            .max_extract_entries
            .saturating_mul(10)
            .max(1000);
        let entries = match fmt {
            Format::Zip => {
                let c = ctx.clone();
                ctx.blocking(move || formats::list_zip(&c, &path, max))
                    .await?
            }
            Format::Tar(comp) => {
                let c = ctx.clone();
                ctx.blocking(move || formats::list_tar(&c, &path, comp, max))
                    .await?
            }
            Format::Single(comp) => vec![formats::Entry {
                name: formats::single_name(&f.name, comp),
                size: 0,
                compressed: Some(f.size()),
                modified: None,
                kind: "file",
                link: None,
                encrypted: false,
            }],
            Format::External(_) => sevenzip::list(&ctx, &path, p.opt_str("password")).await?,
        };
        let limit_bytes = ctx.limits().max_extract_bytes;
        let mut names = std::collections::HashSet::new();
        let (mut total, mut packed, mut suspicious) = (0u64, 0u64, 0usize);
        let mut rows = vec![];
        for e in &entries {
            let mut flags = vec![];
            if let Err(why) = safe::safe_rel_path(&e.name) {
                flags.push(why.to_string());
            }
            match e.kind {
                "symlink" => {
                    flags.push(format!("symlink → {}", e.link.clone().unwrap_or_default()))
                }
                "hardlink" => flags.push(format!(
                    "hard link → {}",
                    e.link.clone().unwrap_or_default()
                )),
                "device" | "special" => flags.push(format!("{} file", e.kind)),
                _ => {}
            }
            if let Some(c) = e.compressed {
                if e.size > 1 << 20 && c > 0 && e.size / c > 100 {
                    flags.push(format!("extreme compression ratio ({}:1)", e.size / c));
                }
            }
            if e.size > limit_bytes {
                flags.push("larger than the extraction limit".into());
            }
            if e.encrypted {
                flags.push("encrypted".into());
            }
            if e.kind != "dir" && !names.insert(e.name.to_lowercase()) {
                flags.push("duplicate name".into());
            }
            let risky = flags.iter().any(|f| f != "encrypted");
            if risky {
                suspicious += 1;
            }
            total = total.saturating_add(e.size);
            packed = packed.saturating_add(e.compressed.unwrap_or(0));
            if rows.len() < 20_000 {
                let ratio = match e.compressed {
                    Some(c) if e.size > 0 && e.kind == "file" => {
                        format!("{:.0}%", 100.0 - c as f64 * 100.0 / e.size as f64)
                    }
                    _ => String::new(),
                };
                rows.push(json!([
                    e.name,
                    e.size,
                    e.compressed,
                    ratio,
                    e.modified,
                    e.kind,
                    flags.join("; ")
                ]));
            }
        }
        if let Format::Single(_) = fmt {
            total = 0;
        }
        let mut o = ToolOutput::data(json!({
            "format": fmt.label(),
            "entries": entries.len(),
            "files": entries.iter().filter(|e| e.kind == "file").count(),
            "total_size": total,
            "total_size_human": human_size(total),
            "compressed_size": if packed > 0 { Some(packed) } else { None },
            "suspicious": suspicious,
            "table": table(&["Path", "Size", "Compressed", "Saved", "Modified", "Type", "Flags"], rows),
        }));
        if suspicious > 0 {
            o = o.warn(format!("{suspicious} suspicious entr{} (see Flags); they would be skipped or rejected on extraction", if suspicious == 1 { "y" } else { "ies" }));
        }
        if total > limit_bytes {
            o = o.warn(format!(
                "total size {} exceeds this instance's extraction limit of {}",
                human_size(total),
                human_size(limit_bytes)
            ));
        }
        Ok(o)
    })
}

fn archive_extract() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.archive_extract", "Extract Archive", Category::Files)
        .describe("Safely unpack zip, tar(.gz/.bz2/.xz/.zst), gz/bz2/xz/zst, 7z and rar archives; path traversal, links and decompression bombs are blocked")
        .keywords(&["unzip", "extract", "untar", "decompress", "unpack", "7z", "rar", "tar.gz", "tgz", "archive"])
        .param(archive_param())
        .param(
            ParamSpec::choice("output", &[("auto", "Automatic"), ("files", "Individual files"), ("zip", "One zip (keeps folders)")])
                .describe("Automatic returns individual files for up to 50 files, otherwise a single zip")
                .default_str("auto"),
        )
        .param(ParamSpec::secret("password").label("Password").describe("For encrypted zip / 7z / rar archives").advanced())
        .param(ParamSpec::integer("max_size_mb").label("Maximum extracted size (MB)").describe("Stop if the archive expands beyond this (capped by the instance limit)").min(1.0).advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    async_tool(spec, |ctx, p| async move {
        const AUTO_FILES: usize = 50;
        const MAX_FILES: usize = 1000;
        let f = p.file("input")?.clone();
        let fmt = formats::detect(&f.path)?;
        let mut max_bytes = ctx.limits().max_extract_bytes;
        if let Some(mb) = p.opt_int("max_size_mb") {
            max_bytes = max_bytes.min((mb.max(1) as u64).saturating_mul(1 << 20));
        }
        let max_entries = ctx.limits().max_extract_entries;
        let root = ctx.temp_dir("extract")?;
        let password = p.opt_str("password").map(|s| s.to_string());
        ctx.progress(0.05, format!("Extracting {} archive", fmt.label()));
        let ex = match fmt {
            Format::External(_) => {
                let mut ex = Extractor::new(&ctx, root.clone(), max_bytes, max_entries);
                sevenzip::extract(&ctx, &mut ex, &f.path, password.as_deref()).await?;
                (ex.files.clone(), ex.skipped.clone(), ex.total_bytes)
            }
            _ => {
                let c = ctx.clone();
                let r = root.clone();
                let path = f.path.clone();
                let name = f.name.clone();
                ctx.blocking(move || {
                    let mut ex = Extractor::new(&c, r, max_bytes, max_entries);
                    match fmt {
                        Format::Zip => formats::extract_zip(&mut ex, &path, password.as_deref())?,
                        Format::Tar(comp) => formats::extract_tar(&mut ex, &path, comp)?,
                        Format::Single(comp) => {
                            formats::extract_single(&mut ex, &path, comp, &name)?
                        }
                        Format::External(_) => unreachable!(),
                    }
                    Ok((ex.files.clone(), ex.skipped.clone(), ex.total_bytes))
                })
                .await?
            }
        };
        let (files, skipped, total) = ex;
        if files.is_empty() {
            return Err(ToolError::invalid(
                "input",
                if skipped.is_empty() {
                    "the archive contains no files".to_string()
                } else {
                    format!(
                        "nothing could be extracted safely ({} entries skipped)",
                        skipped.len()
                    )
                },
            ));
        }
        let mode = match p.str_or("output", "auto") {
            "auto" if files.len() <= AUTO_FILES => "files",
            "auto" => "zip",
            m => m,
        };
        if mode == "files" && files.len() > MAX_FILES {
            return Err(ToolError::invalid(
                "output",
                format!(
                    "{} files is too many to return individually; choose 'One zip'",
                    files.len()
                ),
            ));
        }
        ctx.progress(0.8, "Preparing output");
        let mut arts = vec![];
        if mode == "files" {
            for e in &files {
                ctx.check_cancelled()?;
                let src = root.join(&e.rel);
                let base = Path::new(&e.rel)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "file".into());
                let out = ctx.output_path(&base);
                std::fs::rename(&src, &out).or_else(|_| std::fs::copy(&src, &out).map(|_| ()))?;
                arts.push(ctx.artifact_labeled(&out, &e.rel)?);
            }
        } else {
            let stem = Path::new(&f.name)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let stem = stem
                .split('.')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("extracted")
                .to_string();
            let out = ctx.output_path(&format!("{stem}-extracted.zip"));
            let c = ctx.clone();
            let (o2, r2, fl) = (out.clone(), root.clone(), files.clone());
            ctx.blocking(move || {
                let mut w =
                    zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&o2)?));
                for e in &fl {
                    c.check_cancelled()?;
                    let opts = zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated)
                        .large_file(e.size >= u32::MAX as u64);
                    w.start_file(e.rel.clone(), opts).ctx("writing zip")?;
                    std::io::copy(&mut std::fs::File::open(r2.join(&e.rel))?, &mut w)?;
                }
                w.finish().ctx("writing zip")?.flush()?;
                Ok(())
            })
            .await?;
            arts.push(ctx.artifact(&out)?);
        }
        let skipped_rows: Vec<Value> = skipped
            .iter()
            .take(1000)
            .map(|(n, r)| json!([n, r]))
            .collect();
        let mut o = ToolOutput::files(arts).with_data(json!({
            "format": fmt.label(),
            "files": files.len(),
            "total_size": total,
            "total_size_human": human_size(total),
            "skipped": skipped.len(),
            "table": table(&["Path", "Size"], files.iter().take(5000).map(|e| json!([e.rel, e.size])).collect()),
            "skipped_entries": table(&["Entry", "Reason"], skipped_rows),
        }));
        if !skipped.is_empty() {
            o = o.warn(format!(
                "{} entr{} skipped for safety (links, unsafe paths, special files or duplicates)",
                skipped.len(),
                if skipped.len() == 1 {
                    "y was"
                } else {
                    "ies were"
                }
            ));
        }
        Ok(o)
    })
}

/// Unique entry names derived from the uploaded files' names.
fn entry_names(files: &[InputFile]) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    files
        .iter()
        .map(|f| {
            let clean = dogubako_core::policy::sanitize_filename(&f.name);
            let mut cand = clean.clone();
            let mut i = 2;
            while !used.insert(cand.to_lowercase()) {
                let p = Path::new(&clean);
                let stem = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                cand = match p.extension() {
                    Some(e) => format!("{stem}-{i}.{}", e.to_string_lossy()),
                    None => format!("{stem}-{i}"),
                };
                i += 1;
            }
            cand
        })
        .collect()
}

fn mtime(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::UNIX_EPOCH)
}

fn archive_create() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.archive_create", "Create Archive", Category::Files)
        .describe("Pack files into a zip (optionally AES-encrypted), tar, tar.gz, tar.bz2, tar.xz, tar.zst or 7z archive")
        .keywords(&["zip", "compress", "tar", "tar.gz", "tgz", "7z", "archive", "pack", "bundle"])
        .param(ParamSpec::files("input").label("Files").required().primary())
        .param(
            ParamSpec::choice("format", &[("zip", "ZIP"), ("tar.gz", "tar.gz"), ("tar.xz", "tar.xz"), ("tar.zst", "tar.zst"), ("tar.bz2", "tar.bz2"), ("tar", "tar (uncompressed)"), ("7z", "7z")])
                .default_str("zip"),
        )
        .param(ParamSpec::choice("zip_method", &[("deflate", "Deflate (compatible)"), ("zstd", "Zstandard"), ("bzip2", "bzip2"), ("store", "Store (no compression)")]).default_str("deflate").show_if("format", &["zip"]).advanced())
        .param(ParamSpec::integer("level").label("Compression level").describe("Higher is smaller but slower (format-specific range; empty = default)").range(0.0, 22.0).advanced())
        .param(ParamSpec::secret("password").label("Password").describe("Encrypt the zip (AES-256) or 7z archive").advanced())
        .param(ParamSpec::text("name").label("Archive name").default_str("archive"))
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    async_tool(spec, |ctx, p| async move {
        let files: Vec<InputFile> = p.files("input").to_vec();
        let format = p.str_or("format", "zip").to_string();
        let level = p.opt_int("level");
        let password = p.opt_str("password").map(|s| s.to_string());
        if password.is_some() && !matches!(format.as_str(), "zip" | "7z") {
            return Err(ToolError::invalid(
                "password",
                "only zip and 7z archives can be encrypted",
            ));
        }
        let base = p
            .str_or("name", "archive")
            .trim()
            .trim_end_matches(&format!(".{format}"))
            .to_string();
        let out = ctx.output_path(&format!(
            "{}.{format}",
            if base.is_empty() { "archive" } else { &base }
        ));
        let names = entry_names(&files);
        let count = names.len();
        let total: u64 = files.iter().map(|f| f.size()).sum::<u64>().max(1);
        if format == "7z" {
            let stage = ctx.temp_dir("7z-stage")?;
            for (f, n) in files.iter().zip(&names) {
                let dst = stage.join(n);
                if std::fs::hard_link(&f.path, &dst).is_err() {
                    std::fs::copy(&f.path, &dst)?;
                }
            }
            let mx = level.unwrap_or(5).clamp(0, 9);
            let mut cmd = ctx
                .command("7zip")?
                .args(["a", "-t7z", "-bd", "-y", &format!("-mx={mx}")])
                .current_dir(&stage);
            if let Some(pw) = &password {
                if pw.contains(['\n', '\r', '\0']) {
                    return Err(ToolError::invalid("password", "invalid password"));
                }
                cmd = cmd.arg(format!("-p{pw}")).arg("-mhe=on");
            }
            ctx.progress(0.1, "Compressing with 7-Zip");
            cmd.arg("--").arg(&out).args(&names).run().await?;
        } else {
            let c = ctx.clone();
            let o2 = out.clone();
            let method = p.str_or("zip_method", "deflate").to_string();
            ctx.blocking(move || {
                let mut done = 0u64;
                let progress =
                    |done: u64, n: &str| c.progress(done as f32 / total as f32, n.to_string());
                if format == "zip" {
                    let mut w =
                        zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&o2)?));
                    let m = match method.as_str() {
                        "zstd" => zip::CompressionMethod::Zstd,
                        "bzip2" => zip::CompressionMethod::Bzip2,
                        "store" => zip::CompressionMethod::Stored,
                        _ => zip::CompressionMethod::Deflated,
                    };
                    for (f, n) in files.iter().zip(&names) {
                        c.check_cancelled()?;
                        progress(done, n);
                        let dt: chrono::DateTime<chrono::Local> = mtime(&f.path).into();
                        use chrono::{Datelike, Timelike};
                        let zdt = zip::DateTime::from_date_and_time(
                            dt.year().clamp(1980, 2107) as u16,
                            dt.month() as u8,
                            dt.day() as u8,
                            dt.hour() as u8,
                            dt.minute() as u8,
                            dt.second() as u8,
                        )
                        .unwrap_or_default();
                        let mut opts = zip::write::SimpleFileOptions::default()
                            .compression_method(m)
                            .compression_level(if m == zip::CompressionMethod::Stored {
                                None
                            } else {
                                level
                            })
                            .last_modified_time(zdt)
                            .unix_permissions(0o644)
                            .large_file(f.size() >= u32::MAX as u64);
                        if let Some(pw) = &password {
                            opts = opts.with_aes_encryption(zip::AesMode::Aes256, pw);
                        }
                        w.start_file(n.clone(), opts).map_err(|e| {
                            ToolError::invalid("level", format!("cannot add '{n}': {e}"))
                        })?;
                        std::io::copy(&mut std::fs::File::open(&f.path)?, &mut w)?;
                        done += f.size();
                    }
                    w.finish().ctx("writing zip")?.flush()?;
                } else {
                    let file = std::io::BufWriter::new(std::fs::File::create(&o2)?);
                    let lv = level.map(|l| l as u32);
                    let writer: Box<dyn Write> = match format.as_str() {
                        "tar.gz" => Box::new(flate2::write::GzEncoder::new(
                            file,
                            flate2::Compression::new(lv.unwrap_or(6).min(9)),
                        )),
                        "tar.bz2" => Box::new(bzip2::write::BzEncoder::new(
                            file,
                            bzip2::Compression::new(lv.unwrap_or(9).clamp(1, 9)),
                        )),
                        "tar.xz" => {
                            Box::new(liblzma::write::XzEncoder::new(file, lv.unwrap_or(6).min(9)))
                        }
                        "tar.zst" => Box::new(
                            zstd::stream::write::Encoder::new(
                                file,
                                lv.unwrap_or(3).clamp(1, 22) as i32,
                            )?
                            .auto_finish(),
                        ),
                        _ => Box::new(file),
                    };
                    let mut b = tar::Builder::new(writer);
                    for (f, n) in files.iter().zip(&names) {
                        c.check_cancelled()?;
                        progress(done, n);
                        let mut h = tar::Header::new_gnu();
                        h.set_size(f.size());
                        h.set_mode(0o644);
                        h.set_mtime(
                            mtime(&f.path)
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0),
                        );
                        h.set_entry_type(tar::EntryType::Regular);
                        b.append_data(&mut h, n, std::fs::File::open(&f.path)?)
                            .ctx("writing tar")?;
                        done += f.size();
                    }
                    let mut w = b.into_inner().ctx("writing tar")?;
                    w.flush()?;
                    drop(w);
                }
                Ok(())
            })
            .await?;
        }
        let size = std::fs::metadata(&out)?.len();
        Ok(
            ToolOutput::files(vec![ctx.artifact(&out)?]).with_data(json!({
                "files": count,
                "input_size": human_size(total),
                "archive_size": human_size(size),
                "ratio": format!("{:.1}%", size as f64 * 100.0 / total as f64),
            })),
        )
    })
}

const STREAM_FORMATS: &[(&str, &str)] = &[
    ("gzip", "gzip (.gz)"),
    ("zstd", "Zstandard (.zst)"),
    ("xz", "xz (.xz)"),
    ("bzip2", "bzip2 (.bz2)"),
    ("brotli", "Brotli (.br)"),
];

fn compress() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.compress", "Compress File", Category::Files)
        .describe("Compress individual files with gzip, Zstandard, xz, bzip2 or Brotli")
        .keywords(&[
            "gzip", "gz", "zstd", "xz", "bzip2", "brotli", "compress", "shrink",
        ])
        .param(
            ParamSpec::files("input")
                .label("Files")
                .required()
                .primary(),
        )
        .param(ParamSpec::choice("format", STREAM_FORMATS).default_str("gzip"))
        .param(
            ParamSpec::integer("level")
                .label("Level")
                .describe("gzip/bzip2/xz 1-9, zstd 1-22, brotli 0-11 (empty = default)")
                .range(0.0, 22.0)
                .advanced(),
        )
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let fmt = p.str_or("format", "gzip");
        let level = p.opt_int("level").map(|l| l as u32);
        let ext = match fmt {
            "zstd" => "zst",
            "xz" => "xz",
            "bzip2" => "bz2",
            "brotli" => "br",
            _ => "gz",
        };
        let mut arts = vec![];
        let mut rows = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(i as f32 / files.len() as f32, f.name.clone());
            let out = ctx.output_path(&format!("{}.{ext}", f.name));
            let file = std::io::BufWriter::new(std::fs::File::create(&out)?);
            let mut input = std::fs::File::open(&f.path)?;
            match fmt {
                "zstd" => {
                    let mut e = zstd::stream::write::Encoder::new(
                        file,
                        level.unwrap_or(3).clamp(1, 22) as i32,
                    )?;
                    std::io::copy(&mut input, &mut e)?;
                    e.finish()?.flush()?;
                }
                "xz" => {
                    let mut e = liblzma::write::XzEncoder::new(file, level.unwrap_or(6).min(9));
                    std::io::copy(&mut input, &mut e)?;
                    e.finish()?.flush()?;
                }
                "bzip2" => {
                    let mut e = bzip2::write::BzEncoder::new(
                        file,
                        bzip2::Compression::new(level.unwrap_or(9).clamp(1, 9)),
                    );
                    std::io::copy(&mut input, &mut e)?;
                    e.finish()?.flush()?;
                }
                "brotli" => {
                    let mut e = brotli::CompressorWriter::new(
                        file,
                        64 * 1024,
                        level.unwrap_or(9).min(11),
                        22,
                    );
                    std::io::copy(&mut input, &mut e)?;
                    e.flush()?;
                    let mut inner = e.into_inner();
                    inner.flush()?;
                }
                _ => {
                    let mut e = flate2::write::GzEncoder::new(
                        file,
                        flate2::Compression::new(level.unwrap_or(6).min(9)),
                    );
                    std::io::copy(&mut input, &mut e)?;
                    e.finish()?.flush()?;
                }
            }
            let (a, b) = (f.size(), std::fs::metadata(&out)?.len());
            rows.push(json!([
                f.name,
                human_size(a),
                human_size(b),
                format!("{:.1}%", b as f64 * 100.0 / a.max(1) as f64)
            ]));
            arts.push(ctx.artifact(&out)?);
        }
        Ok(ToolOutput::files(arts)
            .with_data(json!({"table": table(&["File", "Original", "Compressed", "Ratio"], rows)})))
    })
}

fn decompress() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.decompress", "Decompress File", Category::Files)
        .describe("Decompress .gz, .zst, .xz, .bz2 and .br files (detected by content), with decompression-bomb protection")
        .keywords(&["gunzip", "unzstd", "unxz", "bunzip2", "brotli", "decompress", "gz", "zst"])
        .param(ParamSpec::files("input").label("Compressed files").accept(&[".gz", ".zst", ".xz", ".bz2", ".br", ".tgz", ".zstd", ".lzma", ".gzip"]).required().primary())
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let limit = ctx.limits().max_extract_bytes;
        let mut arts = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(i as f32 / files.len() as f32, f.name.clone());
            let head = read_prefix(&f.path, 16)?;
            let file = std::fs::File::open(&f.path)?;
            let (reader, name): (Box<dyn Read>, String) = match formats::comp_from_magic(&head) {
                Some(c) => (formats::decoder(c, file)?, formats::single_name(&f.name, c)),
                None if f.extension() == "br" => (
                    Box::new(brotli::Decompressor::new(file, 64 * 1024)),
                    f.name[..f.name.len() - 3].to_string(),
                ),
                None => {
                    return Err(ToolError::invalid(
                        "input",
                        format!("{}: not a gzip, zstd, xz, bzip2 or brotli file", f.name),
                    ))
                }
            };
            let out = ctx.output_path(&name);
            let mut w = std::io::BufWriter::new(std::fs::File::create(&out)?);
            let mut r = LimitedReader {
                inner: reader,
                limit,
                read: 0,
            };
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                let n = match r.read(&mut buf) {
                    Ok(n) => n,
                    Err(e) => {
                        drop(w);
                        let _ = std::fs::remove_file(&out);
                        return Err(if r.read > limit {
                            ToolError::limit(format!("{}: decompressed data exceeds the limit of {} (possible decompression bomb)", f.name, human_size(limit)))
                        } else {
                            ToolError::invalid("input", format!("{}: corrupt data: {e}", f.name))
                        });
                    }
                };
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                if r.read % (64 << 20) < n as u64 {
                    ctx.check_cancelled()?;
                }
            }
            w.flush()?;
            drop(w);
            arts.push(ctx.artifact(&out)?);
        }
        Ok(ToolOutput::files(arts))
    })
}
