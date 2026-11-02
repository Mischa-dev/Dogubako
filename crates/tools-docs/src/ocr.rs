//! OCR scanned PDFs with Tesseract.

use crate::common::*;
use crate::structure::pdf_inputs;
use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![ocr()]
}

/// Validate a Tesseract language list like `eng+deu`.
pub fn parse_langs(s: &str) -> ToolResult<String> {
    let parts: Vec<&str> = s
        .split(['+', ',', ' '])
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(ToolError::invalid(
            "languages",
            "at least one language is required",
        ));
    }
    for l in &parts {
        if l.len() > 32 || !l.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(ToolError::invalid(
                "languages",
                format!("'{l}' is not a valid Tesseract language code"),
            ));
        }
    }
    Ok(parts.join("+"))
}

fn ocr() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.ocr", "OCR PDF", Category::Pdf)
        .describe("Make scanned PDFs searchable by adding an invisible OCR text layer (Tesseract), or extract the recognized text")
        .keywords(&["ocr", "scan", "searchable", "recognize", "text recognition", "tesseract", "pdf"])
        .param(pdf_inputs())
        .param(ParamSpec::choice("output", &[("pdf", "Searchable PDF"), ("text", "Text only")]).default_str("pdf"))
        .param(ParamSpec::text("languages").label("Languages").describe("Tesseract language codes joined with '+', e.g. 'eng+deu'").default_str("eng"))
        .param(ParamSpec::integer("dpi").label("Scan resolution (DPI)").range(100.0, 600.0).default_int(300).advanced())
        .param(ParamSpec::text("pages").label("Pages").describe("Optional page range (default: all)").advanced())
        .param(ParamSpec::choice("image_format", &[("jpeg", "JPEG (smaller)"), ("png", "PNG (lossless)")]).default_str("jpeg").show_if("output", &["pdf"]).advanced())
        .requires(&["qpdf", "poppler", "tesseract"])
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input");
        let langs = parse_langs(p.str_or("languages", "eng"))?;
        let installed = ctx.command("tesseract")?.arg("--list-langs").run().await?;
        let have: Vec<String> = format!("{}\n{}", installed.stdout_text(), installed.stderr)
            .lines()
            .skip(1)
            .map(|l| l.trim().to_string())
            .collect();
        for l in langs.split('+') {
            if !have.iter().any(|h| h == l) {
                return Err(ToolError::invalid(
                    "languages",
                    format!(
                        "Tesseract language '{l}' is not installed (installed: {})",
                        have.join(", ")
                    ),
                ));
            }
        }
        let text_mode = p.str_or("output", "pdf") == "text";
        let dpi = p.int_or("dpi", 300).to_string();
        let jpeg = p.str_or("image_format", "jpeg") == "jpeg" && !text_mode;
        // Count the work up front for overall progress.
        let mut plan = vec![];
        for f in files {
            let n = page_count(&ctx, &f.path).await?;
            let sel = page_filter(p.opt_str("pages"), n, "pages")?;
            plan.push(
                (1..=n)
                    .filter(|i| sel[(*i - 1) as usize])
                    .collect::<Vec<u32>>(),
            );
        }
        let total: usize = plan.iter().map(|v| v.len()).sum();
        let mut done = 0usize;
        let mut arts = vec![];
        let mut all_text = String::new();
        for (fi, f) in files.iter().enumerate() {
            let work = ctx.temp_dir(&format!("ocr{fi}"))?;
            let mut page_pdfs = vec![];
            let mut text = String::new();
            for pg in &plan[fi] {
                ctx.check_cancelled()?;
                ctx.progress(
                    done as f32 / total.max(1) as f32,
                    format!("{}: OCR page {pg}", f.name),
                );
                let stem = work.join(format!("p{pg:05}"));
                ctx.command("poppler")?
                    .arg(if jpeg { "-jpeg" } else { "-png" })
                    .args([
                        "-r",
                        &dpi,
                        "-f",
                        &pg.to_string(),
                        "-l",
                        &pg.to_string(),
                        "-singlefile",
                    ])
                    .arg_if(jpeg, "-jpegopt")
                    .arg_if(jpeg, "quality=85")
                    .arg(arg_path(&f.path))
                    .arg(&stem)
                    .run()
                    .await?;
                let img = stem.with_extension(if jpeg { "jpg" } else { "png" });
                if !img.is_file() {
                    return Err(ToolError::failed(format!(
                        "page {pg} could not be rendered"
                    )));
                }
                let base = work.join(format!("o{pg:05}"));
                ctx.command("tesseract")?
                    .arg(&img)
                    .arg(&base)
                    .args(["-l", &langs, "--dpi", &dpi])
                    .arg(if text_mode { "txt" } else { "pdf" })
                    .run()
                    .await?;
                if text_mode {
                    let t = std::fs::read_to_string(base.with_extension("txt")).unwrap_or_default();
                    text.push_str(&t);
                    text.push('\u{c}');
                } else {
                    page_pdfs.push(base.with_extension("pdf"));
                }
                let _ = std::fs::remove_file(&img);
                done += 1;
            }
            if text_mode {
                if files.len() > 1 {
                    all_text.push_str(&format!("===== {} =====\n", f.name));
                }
                all_text.push_str(&text);
                arts.push(
                    ctx.write_output(&suffixed(&f.name, "ocr", Some("txt")), text.as_bytes())?,
                );
            } else {
                ctx.progress(
                    done as f32 / total.max(1) as f32,
                    format!("{}: assembling", f.name),
                );
                let out = ctx.output_path(&suffixed(&f.name, "ocr", Some("pdf")));
                let mut cmd = qpdf(&ctx)?.arg("--empty").arg("--pages");
                for pp in &page_pdfs {
                    cmd = cmd.arg(pp).arg("1-z");
                }
                cmd.arg("--").arg(&out).run().await?;
                arts.push(ctx.artifact(&out)?);
            }
        }
        let mut o = ToolOutput::files(arts).with_data(json!({"pages": total, "languages": langs}));
        if text_mode {
            o = o.with_text(all_text, "text/plain");
        }
        Ok(o)
    })
}
