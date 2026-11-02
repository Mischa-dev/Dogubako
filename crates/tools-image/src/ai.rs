//! Tools backed by external engines: OCR (Tesseract), background removal
//! (rembg) and AI upscaling (Real-ESRGAN ncnn).

use crate::meta::arg_path;
use crate::util::*;
use dogubako_core::prelude::*;
use image::DynamicImage;
use std::sync::atomic::{AtomicU32, Ordering};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![ocr(), remove_background(), upscale()]
}

/// Decode with our limits and write a temporary PNG that external tools can read.
async fn temp_png(ctx: &ToolContext, f: &InputFile) -> ToolResult<(std::path::PathBuf, u32, u32)> {
    let c = ctx.clone();
    let f = f.clone();
    ctx.blocking(move || {
        let img = load_image(&c, &f)?;
        let p = c.temp_path(&format!("in-{}.png", dogubako_core::util::random_id(4)));
        let bytes = encode(&c, &img, OutFmt::Png, &SaveOpts::default())?;
        std::fs::write(&p, bytes)?;
        Ok((p, img.width(), img.height()))
    })
    .await
}

fn valid_langs(s: &str) -> ToolResult<String> {
    let s = s.trim();
    if s.is_empty()
        || s.len() > 100
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '+')
        || s.starts_with('+')
    {
        return Err(ToolError::invalid(
            "languages",
            "use Tesseract language codes joined with '+', e.g. eng or eng+deu",
        ));
    }
    Ok(s.to_string())
}

/// Parse Tesseract TSV into (plain text, word regions, mean confidence).
pub fn parse_tsv(tsv: &str, min_conf: f64) -> (String, Vec<Value>, Option<f64>) {
    let mut text = String::new();
    let mut regions = vec![];
    let mut last_key: Option<(i64, i64, i64)> = None;
    let mut last_par: Option<(i64, i64)> = None;
    let (mut csum, mut cn) = (0.0, 0usize);
    for line in tsv.lines().skip(1) {
        let cols: Vec<&str> = line.splitn(12, '\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let n = |i: usize| cols[i].trim().parse::<i64>().unwrap_or(0);
        let conf: f64 = cols[10].trim().parse().unwrap_or(-1.0);
        let word = cols[11].trim();
        if word.is_empty() {
            continue;
        }
        let key = (n(2), n(3), n(4));
        let par = (n(2), n(3));
        if let Some(lk) = last_key {
            if lk != key {
                text.push('\n');
                if last_par != Some(par) {
                    text.push('\n');
                }
            } else {
                text.push(' ');
            }
        }
        text.push_str(word);
        last_key = Some(key);
        last_par = Some(par);
        if conf >= 0.0 {
            csum += conf;
            cn += 1;
        }
        if conf >= min_conf {
            regions.push(json!({"x": n(6), "y": n(7), "width": n(8), "height": n(9), "label": word, "score": (conf * 10.0).round() / 10.0}));
        }
    }
    (
        text,
        regions,
        (cn > 0).then(|| (csum / cn as f64 * 10.0).round() / 10.0),
    )
}

fn ocr() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.ocr", "Extract Text (OCR)", Category::Image)
        .describe("Recognize text in images with Tesseract OCR: plain text, word boxes with confidence, TSV or hOCR")
        .keywords(&["ocr", "text recognition", "extract text", "scan", "tesseract", "image to text", "read text"])
        .param(image_param())
        .param(ParamSpec::text("languages").describe("Tesseract language codes joined with '+', e.g. eng+deu").default_str("eng"))
        .param(ParamSpec::choice("psm", &[
            ("3", "Automatic page layout"),
            ("4", "Single column of text"),
            ("6", "Single uniform block"),
            ("7", "Single line"),
            ("8", "Single word"),
            ("11", "Sparse text"),
            ("12", "Sparse text with OSD"),
        ]).label("Page segmentation").default_str("3"))
        .param(ParamSpec::choice("output", &[("txt", "Plain text"), ("tsv", "TSV with word boxes"), ("hocr", "hOCR (HTML)")]).default_str("txt"))
        .param(ParamSpec::number("min_confidence").describe("Hide word boxes below this confidence (0–100)").range(0.0, 100.0).default_num(30.0).advanced())
        .requires(&["tesseract"])
        .text_output("text/plain")
        .timeout(600);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let langs = valid_langs(p.str_or("languages", "eng"))?;
        let psm = p.str_or("psm", "3").to_string();
        if !psm.chars().all(|c| c.is_ascii_digit()) {
            return Err(ToolError::invalid("psm", "invalid"));
        }
        ctx.progress(0.05, "preparing image");
        let (png, w, h) = temp_png(&ctx, &f).await?;
        ctx.progress(0.2, "recognizing text");
        let tsv_out = ctx
            .command("tesseract")?
            .arg(arg_path(&png))
            .arg("stdout")
            .arg("-l")
            .arg(&langs)
            .arg("--psm")
            .arg(&psm)
            .arg("tsv")
            .run()
            .await?;
        let tsv = tsv_out.stdout_text();
        let (text, regions, conf) = parse_tsv(&tsv, p.num_or("min_confidence", 30.0));
        let stem = f.stem();
        let mut out = match p.str_or("output", "txt") {
            "tsv" => {
                let a = ctx.write_output(&format!("{stem}.tsv"), tsv.as_bytes())?;
                ToolOutput::text(tsv.clone()).with_artifact(a)
            }
            "hocr" => {
                ctx.progress(0.6, "building hOCR");
                let h = ctx
                    .command("tesseract")?
                    .arg(arg_path(&png))
                    .arg("stdout")
                    .arg("-l")
                    .arg(&langs)
                    .arg("--psm")
                    .arg(&psm)
                    .arg("hocr")
                    .run()
                    .await?;
                let html = h.stdout_text();
                let a = ctx.write_output(&format!("{stem}.hocr.html"), html.as_bytes())?;
                ToolOutput::text(html).with_artifact(a)
            }
            _ => {
                let a = ctx.write_output(&format!("{stem}.txt"), text.as_bytes())?;
                ToolOutput::text(text.clone()).with_artifact(a)
            }
        };
        out = out.with_data(json!({
            "words": regions.len(),
            "mean_confidence": conf,
            "width": w,
            "height": h,
            "languages": langs,
            "regions": regions,
        }));
        if text.trim().is_empty() {
            out = out.warn("no text was recognized");
        }
        ctx.progress(1.0, "done");
        Ok(out)
    })
}

fn remove_background() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.remove_background", "Remove Background", Category::Image)
        .describe("Cut out the subject and make the background transparent using local rembg (U²-Net / IS-Net models)")
        .keywords(&["remove background", "background eraser", "cutout", "transparent", "rembg", "remove bg", "segmentation"])
        .param(images_param())
        .param(ParamSpec::choice("model", &[
            ("u2net", "U²-Net (general)"),
            ("u2netp", "U²-Net lite (fast)"),
            ("u2net_human_seg", "Human segmentation"),
            ("isnet-general-use", "IS-Net (general, sharper)"),
            ("isnet-anime", "IS-Net anime"),
            ("silueta", "Silueta (small)"),
        ]).default_str("u2net"))
        .param(ParamSpec::boolean("alpha_matting").describe("Refine soft edges such as hair (slower)").default_bool(false).advanced())
        .param(ParamSpec::boolean("mask_only").describe("Output the black/white mask instead of the cutout").default_bool(false).advanced())
        .requires(&["rembg"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        if files.is_empty() {
            return Err(ToolError::invalid(
                "input",
                "at least one image is required",
            ));
        }
        let model = p.str_or("model", "u2net").to_string();
        let mut arts = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(
                i as f32 / files.len() as f32,
                format!("Processing {}", f.name),
            );
            let (png, _, _) = temp_png(&ctx, f).await?;
            let out = ctx.output_path(&suffixed(
                &f.name,
                if p.bool("mask_only") { "mask" } else { "nobg" },
                Some("png"),
            ));
            // rembg downloads its model on first use.
            let mut cmd = ctx
                .command("rembg")?
                .arg("i")
                .arg("-m")
                .arg(&model)
                .allow_network();
            if p.bool("alpha_matting") {
                cmd = cmd.arg("-a");
            }
            if p.bool("mask_only") {
                cmd = cmd.arg("-om");
            }
            cmd.arg(arg_path(&png)).arg(arg_path(&out)).run().await?;
            arts.push(ctx.artifact(&out)?);
        }
        ctx.progress(1.0, "done");
        Ok(ToolOutput::files(arts))
    })
}

fn upscale() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.upscale", "AI Upscale", Category::Image)
        .describe("Enlarge images 2×, 3× or 4× with Real-ESRGAN, restoring detail locally on the GPU (Vulkan)")
        .keywords(&["upscale", "super resolution", "enlarge", "esrgan", "enhance", "ai", "increase resolution"])
        .param(images_param())
        .param(ParamSpec::choice("scale", &[("2", "2×"), ("3", "3×"), ("4", "4×")]).default_str("4"))
        .param(ParamSpec::choice("model", &[
            ("realesrgan-x4plus", "Photo (Real-ESRGAN x4plus)"),
            ("realesrgan-x4plus-anime", "Illustration / anime"),
            ("realesr-animevideov3", "Anime video frames (fast)"),
        ]).default_str("realesrgan-x4plus"))
        .param(ParamSpec::choice("format", &[("png", "PNG"), ("jpg", "JPEG"), ("webp", "WebP")]).default_str("png"))
        .requires(&["realesrgan"])
        .output(OutputKind::Files)
        .progress()
        .timeout(3600);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        if files.is_empty() {
            return Err(ToolError::invalid(
                "input",
                "at least one image is required",
            ));
        }
        let scale: u32 = p.str_or("scale", "4").parse().unwrap_or(4);
        let model = p.str_or("model", "realesrgan-x4plus").to_string();
        // The x4plus models only run at 4×; other factors are reached by resampling the 4× result.
        let native_scale = if model == "realesr-animevideov3" {
            scale
        } else {
            4
        };
        let fmt = OutFmt::parse(p.str_or("format", "png")).unwrap_or(OutFmt::Png);
        let n = files.len();
        let mut arts = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            let (png, w, h) = temp_png(&ctx, f).await?;
            check_pixels(
                &ctx,
                w as u64 * native_scale as u64,
                h as u64 * native_scale as u64,
            )?;
            let tmp_out = ctx.temp_path(&format!("up-{}.png", dogubako_core::util::random_id(4)));
            let pct = Arc::new(AtomicU32::new(0));
            let (c2, pct2, base) = (ctx.clone(), pct.clone(), i as f32 / n as f32);
            // Models shipped next to the binary (the app-managed engine pack
            // and the upstream release both have bin/models).
            let model_dir: Vec<std::ffi::OsString> = ctx
                .deps()
                .path("realesrgan")
                .and_then(|p| p.parent().map(|d| d.join("models")))
                .filter(|d| d.is_dir())
                .map(|d| vec!["-m".into(), arg_path(&d)])
                .unwrap_or_default();
            ctx.command("realesrgan")?
                .args(model_dir)
                .arg("-i")
                .arg(arg_path(&png))
                .arg("-o")
                .arg(arg_path(&tmp_out))
                .arg("-s")
                .arg(native_scale.to_string())
                .arg("-n")
                .arg(&model)
                .arg("-f")
                .arg("png")
                .on_stderr_line(move |l| {
                    if let Some(v) = l
                        .trim()
                        .strip_suffix('%')
                        .and_then(|s| s.trim().parse::<f32>().ok())
                    {
                        pct2.store(v as u32, Ordering::Relaxed);
                        c2.progress(base + v / 100.0 / n as f32, format!("{v:.0}%"));
                    }
                })
                .run()
                .await?;
            let (c3, fname) = (ctx.clone(), f.name.clone());
            let art = ctx
                .blocking(move || {
                    let up = crate::util::load_path(&c3, &tmp_out, "upscaled.png")?;
                    let up = if native_scale != scale {
                        DynamicImage::ImageRgba8(image::imageops::resize(
                            &up.to_rgba8(),
                            w * scale,
                            h * scale,
                            image::imageops::FilterType::Lanczos3,
                        ))
                    } else {
                        up
                    };
                    save_output(
                        &c3,
                        &up,
                        &suffixed(&fname, &format!("x{scale}"), Some(fmt.ext())),
                        fmt,
                        &SaveOpts {
                            quality: 92,
                            ..Default::default()
                        },
                    )
                })
                .await?;
            arts.push(art);
        }
        ctx.progress(1.0, "done");
        Ok(ToolOutput::files(arts))
    })
}
