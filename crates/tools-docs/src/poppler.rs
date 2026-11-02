//! Poppler-based tools: text, images, rendering and document info.

use crate::common::*;
use crate::structure::{pdf_input, pdf_inputs};
use dogubako_core::prelude::*;
use std::path::{Path, PathBuf};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        extract_text(),
        extract_images(),
        to_images(),
        info(),
        compare_text(),
    ]
}

/// Validate optional first/last page params against the page count.
fn first_last(p: &Params, n: u32) -> ToolResult<(u32, u32)> {
    let first = p.opt_int("first_page").unwrap_or(1).max(1) as u32;
    let last = p.opt_int("last_page").map(|v| v as u32).unwrap_or(n).min(n);
    if first > n {
        return Err(ToolError::invalid(
            "first_page",
            format!("the document has only {n} page(s)"),
        ));
    }
    if last < first {
        return Err(ToolError::invalid(
            "last_page",
            "must not be before the first page",
        ));
    }
    Ok((first, last))
}

fn page_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::integer("first_page")
            .label("First page")
            .min(1.0)
            .advanced(),
        ParamSpec::integer("last_page")
            .label("Last page")
            .min(1.0)
            .advanced(),
    ]
}

async fn pdftotext(
    ctx: &ToolContext,
    file: &Path,
    first: u32,
    last: u32,
    layout: bool,
) -> ToolResult<String> {
    let out = ctx
        .command_sibling("poppler", "pdftotext")?
        .args([
            "-enc",
            "UTF-8",
            "-f",
            &first.to_string(),
            "-l",
            &last.to_string(),
        ])
        .arg_if(layout, "-layout")
        .arg(arg_path(file))
        .arg("-")
        .run()
        .await?;
    Ok(out.stdout_text())
}

fn extract_text() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.extract_text", "Extract Text", Category::Pdf)
        .describe("Extract the text layer of PDFs as plain text, optionally preserving the physical layout")
        .keywords(&["text", "pdftotext", "copy", "content", "txt", "pdf"])
        .param(pdf_inputs())
        .param(ParamSpec::boolean("layout").label("Preserve layout").describe("Keep the original column layout using spacing"))
        .params(page_params())
        .requires(&["qpdf", "poppler"])
        .text_output("text/plain")
        .progress()
        .timeout(900);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input");
        let mut all = String::new();
        let mut arts = vec![];
        let mut stats = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, files.len(), &f.name);
            let n = page_count(&ctx, &f.path).await?;
            let (first, last) = first_last(&p, n)?;
            let text = pdftotext(&ctx, &f.path, first, last, p.bool("layout")).await?;
            if files.len() > 1 {
                all.push_str(&format!("===== {} =====\n", f.name));
            }
            all.push_str(&text);
            stats.push(json!({"file": f.name, "pages": last - first + 1, "characters": text.chars().count(), "words": text.split_whitespace().count()}));
            arts.push(ctx.write_output(&with_extension(&f.name, "txt"), text.as_bytes())?);
        }
        let mut o = ToolOutput::text(all.clone())
            .with_artifacts(arts)
            .with_data(json!({"files": stats}));
        if all.trim().is_empty() {
            o = o.warn("No text layer found. If this is a scanned document, run pdf.ocr first.");
        }
        Ok(o)
    })
}

fn extract_images() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.extract_images", "Extract Images", Category::Pdf)
        .describe("Save the images embedded in a PDF in their original formats (JPEG, PNG, JPEG 2000, ...)")
        .keywords(&["images", "pictures", "photos", "pdfimages", "pdf"])
        .param(pdf_input())
        .params(page_params())
        .param(ParamSpec::integer("min_size").label("Minimum size (px)").describe("Skip images smaller than this in both dimensions").default_int(0).min(0.0).advanced())
        .requires(&["qpdf", "poppler"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        const MAX_IMAGES: usize = 2000;
        let f = p.file("input")?;
        let n = page_count(&ctx, &f.path).await?;
        let (first, last) = first_last(&p, n)?;
        let pages = [
            "-f".to_string(),
            first.to_string(),
            "-l".to_string(),
            last.to_string(),
        ];
        // Table of images first (also tells us the sizes).
        let list = ctx
            .command_sibling("poppler", "pdfimages")?
            .arg("-list")
            .args(&pages)
            .arg(arg_path(&f.path))
            .run()
            .await?;
        let mut rows = vec![];
        for line in list.stdout_text().lines().skip(2) {
            let c: Vec<&str> = line.split_whitespace().collect();
            if c.len() >= 16 {
                rows.push(json!([
                    c[0], c[1], c[2], c[3], c[4], c[5], c[8], c[9], c[14]
                ]));
            }
        }
        let dir = ctx.temp_dir("images")?;
        ctx.progress(0.2, "Extracting images");
        ctx.command_sibling("poppler", "pdfimages")?
            .arg("-all")
            .arg("-p")
            .args(&pages)
            .arg(arg_path(&f.path))
            .arg(dir.join("img"))
            .run()
            .await?;
        let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        found.sort();
        let min = p.int_or("min_size", 0).max(0) as u32;
        let mut arts = vec![];
        let mut skipped = 0;
        let stem = f.stem();
        for (i, img) in found.iter().enumerate() {
            ctx.check_cancelled()?;
            if min > 0 {
                if let Ok((w, h)) = image::image_dimensions(img) {
                    if w < min && h < min {
                        skipped += 1;
                        continue;
                    }
                }
            }
            if arts.len() >= MAX_IMAGES {
                return Err(ToolError::limit(format!(
                    "more than {MAX_IMAGES} images; narrow the page range"
                )));
            }
            ctx.progress(0.3 + 0.7 * i as f32 / found.len().max(1) as f32, "");
            let name = img
                .file_name()
                .map(|n| n.to_string_lossy().replacen("img", &stem, 1))
                .unwrap_or_default();
            let out = ctx.output_path(&name);
            std::fs::rename(img, &out).or_else(|_| std::fs::copy(img, &out).map(|_| ()))?;
            arts.push(ctx.artifact(&out)?);
        }
        let count = arts.len();
        let mut o = ToolOutput::files(arts).with_data(json!({
            "count": count,
            "skipped_small": skipped,
            "table": {"columns": ["Page", "Num", "Type", "Width", "Height", "Color", "Encoding", "Interp", "Size"], "rows": rows},
        }));
        if count == 0 {
            o = o.warn("No embedded images found in the selected pages");
        }
        Ok(o)
    })
}

/// Page sizes in points from pdfinfo (first..=last).
async fn page_sizes(
    ctx: &ToolContext,
    file: &Path,
    first: u32,
    last: u32,
) -> ToolResult<Vec<(u32, f64, f64, i64)>> {
    let out = ctx
        .command_sibling("poppler", "pdfinfo")?
        .args(["-f", &first.to_string(), "-l", &last.to_string()])
        .arg(arg_path(file))
        .run()
        .await?;
    let re = regex::Regex::new(r"^Page\s+(\d+)\s+size:\s+([\d.]+)\s+x\s+([\d.]+)").unwrap();
    let rot = regex::Regex::new(r"^Page\s+(\d+)\s+rot:\s+(-?\d+)").unwrap();
    let mut v: Vec<(u32, f64, f64, i64)> = vec![];
    for line in out.stdout_text().lines() {
        if let Some(c) = re.captures(line) {
            v.push((
                c[1].parse().unwrap_or(0),
                c[2].parse().unwrap_or(0.0),
                c[3].parse().unwrap_or(0.0),
                0,
            ));
        } else if let Some(c) = rot.captures(line) {
            let pg: u32 = c[1].parse().unwrap_or(0);
            if let Some(e) = v.iter_mut().find(|e| e.0 == pg) {
                e.3 = c[2].parse().unwrap_or(0);
            }
        }
    }
    Ok(v)
}

fn to_images() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.to_images", "PDF to Images", Category::Pdf)
        .describe("Render PDF pages to PNG, JPEG or TIFF images at a chosen resolution")
        .keywords(&[
            "render",
            "convert",
            "png",
            "jpg",
            "jpeg",
            "tiff",
            "rasterize",
            "pdftoppm",
            "pdf",
        ])
        .param(pdf_inputs())
        .param(
            ParamSpec::choice(
                "format",
                &[("png", "PNG"), ("jpeg", "JPEG"), ("tiff", "TIFF")],
            )
            .default_str("png"),
        )
        .param(
            ParamSpec::integer("dpi")
                .label("Resolution (DPI)")
                .range(18.0, 1200.0)
                .default_int(150),
        )
        .param(
            ParamSpec::text("pages")
                .label("Pages")
                .describe("Optional page range (default: all)")
                .placeholder("1-3,5"),
        )
        .param(
            ParamSpec::integer("quality")
                .label("JPEG quality")
                .range(1.0, 100.0)
                .default_int(90)
                .show_if("format", &["jpeg"])
                .advanced(),
        )
        .param(
            ParamSpec::boolean("grayscale")
                .label("Grayscale")
                .advanced(),
        )
        .param(
            ParamSpec::boolean("transparent")
                .label("Transparent background (PNG)")
                .show_if("format", &["png"])
                .advanced(),
        )
        .requires(&["qpdf", "poppler"])
        .output(OutputKind::Files)
        .progress()
        .timeout(3600);
    async_tool(spec, |ctx, p| async move {
        const MAX_PAGES: usize = 2000;
        let files = p.files("input");
        let fmt = p.str_or("format", "png").to_string();
        let dpi = p.int_or("dpi", 150);
        let ext = match fmt.as_str() {
            "jpeg" => "jpg",
            "tiff" => "tif",
            _ => "png",
        };
        let mut arts = vec![];
        let max_px = ctx.limits().max_image_pixels as f64;
        let mut jobs: Vec<(usize, u32)> = vec![];
        let mut counts = vec![];
        for (fi, f) in files.iter().enumerate() {
            let n = page_count(&ctx, &f.path).await?;
            let pages = page_filter(p.opt_str("pages"), n, "pages")?;
            let sel: Vec<u32> = (1..=n).filter(|i| pages[(*i - 1) as usize]).collect();
            if let (Some(a), Some(b)) = (sel.first(), sel.last()) {
                for (pg, w, h, _) in page_sizes(&ctx, &f.path, *a, *b).await? {
                    let px = (w / 72.0 * dpi as f64) * (h / 72.0 * dpi as f64);
                    if sel.contains(&pg) && px > max_px {
                        return Err(ToolError::limit(format!(
                            "{} page {pg} would be {:.0} megapixels at {dpi} DPI; lower the DPI",
                            f.name,
                            px / 1e6
                        )));
                    }
                }
            }
            counts.push(sel.len());
            jobs.extend(sel.into_iter().map(|pg| (fi, pg)));
        }
        if jobs.len() > MAX_PAGES {
            return Err(ToolError::limit(format!(
                "{} pages selected; at most {MAX_PAGES} per run",
                jobs.len()
            )));
        }
        let total = jobs.len();
        for (k, (fi, pg)) in jobs.into_iter().enumerate() {
            ctx.check_cancelled()?;
            let f = &files[fi];
            ctx.progress(k as f32 / total as f32, format!("{} page {pg}", f.name));
            let dir = ctx.temp_dir(&format!("r{k}"))?;
            let mut cmd = ctx
                .command("poppler")?
                .arg(format!("-{fmt}"))
                .args([
                    "-r",
                    &dpi.to_string(),
                    "-f",
                    &pg.to_string(),
                    "-l",
                    &pg.to_string(),
                    "-singlefile",
                ])
                .arg_if(p.bool("grayscale"), "-gray")
                .arg_if(fmt == "png" && p.bool("transparent"), "-transp");
            if fmt == "jpeg" {
                cmd = cmd.args(["-jpegopt", &format!("quality={}", p.int_or("quality", 90))]);
            }
            if fmt == "tiff" {
                cmd = cmd.args(["-tiffcompression", "lzw"]);
            }
            cmd.arg(arg_path(&f.path))
                .arg(dir.join("page"))
                .run()
                .await?;
            let produced = std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| p.is_file())
                .ok_or_else(|| ToolError::failed("pdftoppm produced no image"))?;
            let width = counts[fi].to_string().len().max(2);
            let out = ctx.output_path(&format!(
                "{}-p{:0width$}.{ext}",
                f.stem(),
                pg,
                width = width
            ));
            std::fs::rename(&produced, &out)
                .or_else(|_| std::fs::copy(&produced, &out).map(|_| ()))?;
            arts.push(ctx.artifact_labeled(&out, &format!("page {pg}"))?);
        }
        Ok(ToolOutput::files(arts).with_data(json!({"images": total, "dpi": dpi, "format": fmt})))
    })
}

fn info() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.info", "PDF Info", Category::Pdf)
        .describe("Inspect a PDF: page count and sizes, PDF version, metadata, encryption, fonts, forms and more")
        .keywords(&["properties", "metadata", "pages", "fonts", "encryption", "version", "inspect", "pdfinfo"])
        .param(pdf_input())
        .requires(&["qpdf", "poppler"])
        .output(OutputKind::Data)
        .timeout(300);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?;
        let enc_out = ctx
            .command("qpdf")?
            .arg("--show-encryption")
            .arg(arg_path(&f.path))
            .ok_codes(&[2, 3])
            .run()
            .await?;
        let enc_text = format!("{}{}", enc_out.stdout_text(), enc_out.stderr);
        if enc_text.to_ascii_lowercase().contains("invalid password") {
            return Err(ToolError::invalid(
                "input",
                "the PDF requires a password to open; it cannot be inspected without it",
            ));
        }
        if !enc_out.success() && enc_out.exit_code != Some(3) {
            return Err(ToolError::invalid(
                "input",
                format!("not a readable PDF: {}", enc_out.stderr.trim()),
            ));
        }
        let pdfinfo = ctx
            .command_sibling("poppler", "pdfinfo")?
            .arg("-isodates")
            .arg(arg_path(&f.path))
            .run()
            .await?;
        let mut info = serde_json::Map::new();
        for line in pdfinfo.stdout_text().lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_lowercase().replace([' ', '-'], "_");
                if !key.is_empty() && !key.starts_with("page_") {
                    info.insert(key, Value::String(v.trim().to_string()));
                }
            }
        }
        let pages: u32 = info
            .get("pages")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut size_counts: Vec<(String, usize)> = vec![];
        if pages > 0 {
            for (_, w, h, rot) in page_sizes(&ctx, &f.path, 1, pages.min(5000)).await? {
                let label = format!(
                    "{:.0} × {:.0} pt ({:.0} × {:.0} mm){}",
                    w,
                    h,
                    w / 72.0 * 25.4,
                    h / 72.0 * 25.4,
                    if rot != 0 {
                        format!(", rotated {rot}°")
                    } else {
                        String::new()
                    }
                );
                match size_counts.iter_mut().find(|(l, _)| *l == label) {
                    Some(e) => e.1 += 1,
                    None => size_counts.push((label, 1)),
                }
            }
        }
        let mut fonts = vec![];
        if let Ok(fo) = ctx.command_sibling("poppler", "pdffonts") {
            if let Ok(out) = fo.arg(arg_path(&f.path)).run().await {
                fonts = parse_pdffonts(&out.stdout_text());
            }
        }
        let encryption: Vec<String> = enc_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let data = json!({
            "file": f.name,
            "file_size": human_size(f.size()),
            "pages": pages,
            "pdf_version": info.get("pdf_version"),
            "title": info.get("title"),
            "author": info.get("author"),
            "subject": info.get("subject"),
            "keywords": info.get("keywords"),
            "creator": info.get("creator"),
            "producer": info.get("producer"),
            "created": info.get("creationdate"),
            "modified": info.get("moddate"),
            "encrypted": info.get("encrypted"),
            "encryption": encryption,
            "linearized": info.get("optimized"),
            "tagged": info.get("tagged"),
            "forms": info.get("form"),
            "javascript": info.get("javascript"),
            "page_sizes": size_counts.iter().map(|(l, c)| json!({"size": l, "pages": c})).collect::<Vec<_>>(),
            "table": {"columns": ["Font", "Type", "Encoding", "Embedded", "Subset", "Unicode"], "rows": fonts},
            "pdfinfo": info,
        });
        Ok(ToolOutput::data(data))
    })
}

/// Parse `pdffonts` output using the dashed separator line for column widths.
fn parse_pdffonts(text: &str) -> Vec<Value> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(sep_idx) = lines.iter().position(|l| l.starts_with("---")) else {
        return vec![];
    };
    let sep = lines[sep_idx];
    let mut cols = vec![];
    let mut start = None;
    for (i, ch) in sep.char_indices() {
        match (ch, start) {
            ('-', None) => start = Some(i),
            (' ', Some(s)) => {
                cols.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        cols.push((s, usize::MAX));
    }
    let cut = |l: &str, (a, b): (usize, usize)| -> String {
        let end = b.min(l.len());
        if a >= l.len() {
            return String::new();
        }
        l.get(a..end).unwrap_or("").trim().to_string()
    };
    lines[sep_idx + 1..]
        .iter()
        .filter(|l| !l.trim().is_empty())
        .take(500)
        .map(|l| {
            let v: Vec<String> = cols.iter().take(6).map(|c| cut(l, *c)).collect();
            json!(v)
        })
        .collect()
}

fn compare_text() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.compare_text", "Compare PDF Text", Category::Pdf)
        .describe("Show the textual differences between two PDFs (line by line)")
        .keywords(&["diff", "compare", "changes", "versions", "pdf"])
        .param(
            ParamSpec::file("original")
                .label("Original PDF")
                .accept(PDF_ACCEPT)
                .required()
                .primary(),
        )
        .param(
            ParamSpec::file("changed")
                .label("Changed PDF")
                .accept(PDF_ACCEPT)
                .required()
                .primary(),
        )
        .requires(&["qpdf", "poppler"])
        .text_output("text/x-diff")
        .ui("diff")
        .timeout(600);
    async_tool(spec, |ctx, p| async move {
        let a = p.file("original")?;
        let b = p.file("changed")?;
        let na = page_count(&ctx, &a.path).await?;
        let nb = page_count(&ctx, &b.path).await?;
        let ta = pdftotext(&ctx, &a.path, 1, na, false).await?;
        let tb = pdftotext(&ctx, &b.path, 1, nb, false).await?;
        let norm = |t: &str| {
            t.replace('\u{c}', "\n")
                .lines()
                .map(|l| l.trim_end().to_string())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        };
        let (ta, tb) = (norm(&ta), norm(&tb));
        let diff = similar::TextDiff::from_lines(&ta, &tb);
        let mut chunks: Vec<Value> = vec![];
        let (mut ins, mut del) = (0, 0);
        for change in diff.iter_all_changes() {
            let tag = match change.tag() {
                similar::ChangeTag::Equal => "equal",
                similar::ChangeTag::Insert => {
                    ins += 1;
                    "insert"
                }
                similar::ChangeTag::Delete => {
                    del += 1;
                    "delete"
                }
            };
            match chunks.last_mut() {
                Some(last) if last["tag"] == tag => {
                    let s = last["text"].as_str().unwrap_or("").to_string() + change.value();
                    last["text"] = Value::String(s);
                }
                _ => chunks.push(json!({"tag": tag, "text": change.value()})),
            }
        }
        let unified = diff
            .unified_diff()
            .context_radius(3)
            .header(&a.name, &b.name)
            .to_string();
        Ok(ToolOutput::text_as(if unified.is_empty() { "No text differences.".to_string() } else { unified }, "text/x-diff")
            .with_data(json!({"identical": ins == 0 && del == 0, "lines_added": ins, "lines_removed": del, "diff": chunks})))
    })
}
