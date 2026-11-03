//! PDF compression with Ghostscript + qpdf.

use crate::common::*;
use crate::structure::pdf_inputs;
use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![compress()]
}

fn compress() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.compress", "Compress PDF", Category::Pdf)
        .describe("Shrink PDFs by downsampling images (Ghostscript) and packing objects into compressed streams (qpdf); keeps the smallest result")
        .keywords(&["compress", "reduce", "shrink", "smaller", "optimize", "size", "pdf"])
        .param(pdf_inputs())
        .param(
            ParamSpec::choice(
                "preset",
                &[
                    ("screen", "Smallest (72 dpi images)"),
                    ("ebook", "Balanced (150 dpi images)"),
                    ("printer", "High quality (300 dpi images)"),
                    ("prepress", "Prepress (300 dpi, color preserving)"),
                    ("lossless", "Lossless only (no image changes)"),
                ],
            )
            .default_str("ebook"),
        )
        .param(ParamSpec::boolean("linearize").label("Optimize for web (linearize)").advanced())
        .param(ParamSpec::boolean("grayscale").label("Convert to grayscale").advanced())
        .requires(&["qpdf", "ghostscript"])
        .output(OutputKind::Files)
        .progress()
        .timeout(3600);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input");
        let preset = p.str_or("preset", "ebook").to_string();
        let lin = p.bool("linearize");
        let gray = p.bool("grayscale");
        let mut arts = vec![];
        let mut rows = vec![];
        let mut warnings = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let base = i as f32 / files.len() as f32;
            let span = 1.0 / files.len() as f32;
            ctx.progress(base, format!("{}: checking", f.name));
            // Validates that the file is a readable, unencrypted PDF.
            page_count(&ctx, &f.path).await?;
            let before = f.size();
            let work = ctx.temp_dir(&format!("c{i}"))?;
            let mut candidates: Vec<(String, std::path::PathBuf)> = vec![];

            if preset != "lossless" || gray {
                ctx.progress(
                    base + span * 0.1,
                    format!("{}: Ghostscript ({preset})", f.name),
                );
                let gs_out = work.join("gs.pdf");
                let settings = if preset == "lossless" {
                    "default"
                } else {
                    preset.as_str()
                };
                let mut gs = ctx
                    .command("ghostscript")?
                    .args([
                        "-sDEVICE=pdfwrite",
                        "-dCompatibilityLevel=1.6",
                        "-dNOPAUSE",
                        "-dBATCH",
                        "-dSAFER",
                        "-dQUIET",
                    ])
                    .arg(format!("-dPDFSETTINGS=/{settings}"))
                    .arg("-dDetectDuplicateImages=true")
                    .arg("-dAutoRotatePages=/None");
                if gray {
                    gs = gs.args([
                        "-sColorConversionStrategy=Gray",
                        "-dProcessColorModel=/DeviceGray",
                    ]);
                }
                let mut ofile = std::ffi::OsString::from("-sOutputFile=");
                ofile.push(&gs_out);
                let r = gs.arg(ofile).arg("--").arg(arg_path(&f.path)).run().await;
                match r {
                    Ok(_) if gs_out.is_file() => {
                        let q = work.join("gs-q.pdf");
                        if optimize(&ctx, &gs_out, &q, lin).await.is_ok() {
                            candidates.push(("ghostscript+qpdf".into(), q));
                        } else {
                            candidates.push(("ghostscript".into(), gs_out));
                        }
                    }
                    Ok(_) => warnings.push(format!("{}: Ghostscript produced no output", f.name)),
                    Err(e) => warnings.push(format!(
                        "{}: Ghostscript failed ({e}); trying lossless only",
                        f.name
                    )),
                }
            }
            if gray {
                // Only Ghostscript output is grayscale; a smaller colour file
                // would silently ignore the request.
                if candidates.is_empty() {
                    return Err(ToolError::failed(format!(
                        "{}: grayscale conversion needs Ghostscript, which did not produce output",
                        f.name
                    )));
                }
            } else {
                ctx.progress(
                    base + span * 0.7,
                    format!("{}: qpdf object streams", f.name),
                );
                let q = work.join("q.pdf");
                optimize(&ctx, &f.path, &q, lin).await?;
                candidates.push(("qpdf".into(), q));
            }

            let (method, best) = candidates
                .iter()
                .filter_map(|(m, path)| {
                    std::fs::metadata(path)
                        .ok()
                        .map(|md| (m.clone(), path.clone(), md.len()))
                })
                .filter(|(_, _, len)| *len > 0)
                .min_by_key(|(_, _, len)| *len)
                .map(|(m, p, _)| (m, p))
                .ok_or_else(|| ToolError::failed("compression produced no output"))?;
            let best_size = std::fs::metadata(&best)?.len();
            let out = ctx.output_path(&suffixed(&f.name, "compressed", Some("pdf")));
            let (used, after) = if gray || best_size < before {
                std::fs::copy(&best, &out)?;
                (method, best_size)
            } else {
                warnings.push(format!(
                    "{}: already well compressed; the original is returned unchanged",
                    f.name
                ));
                std::fs::copy(&f.path, &out)?;
                ("original".to_string(), before)
            };
            let saved = before.saturating_sub(after);
            rows.push(json!([
                f.name,
                human_size(before),
                human_size(after),
                format!("{:.1}%", saved as f64 * 100.0 / before.max(1) as f64),
                used
            ]));
            arts.push(ctx.artifact(&out)?);
        }
        let mut o = ToolOutput::files(arts).with_data(json!({
            "table": {"columns": ["File", "Before", "After", "Saved", "Method"], "rows": rows}
        }));
        o.warnings = warnings;
        Ok(o)
    })
}

async fn optimize(
    ctx: &ToolContext,
    input: &std::path::Path,
    out: &std::path::Path,
    linearize: bool,
) -> ToolResult<()> {
    qpdf(ctx)?
        .args([
            "--object-streams=generate",
            "--compress-streams=y",
            "--recompress-flate",
            "--compression-level=9",
            "--remove-unreferenced-resources=yes",
        ])
        .arg_if(linearize, "--linearize")
        .arg(arg_path(input))
        .arg(out)
        .run()
        .await?;
    Ok(())
}
