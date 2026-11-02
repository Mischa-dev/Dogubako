//! Face detection (SeetaFace via rustface) and one-step face blurring.

use crate::edit::{redact_mode_params, redact_regions};
use crate::util::*;
use dogubako_core::prelude::*;
use image::{DynamicImage, Rgba, RgbaImage};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![face_detect(), face_blur()]
}

#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub score: f64,
}

fn model_path(ctx: &ToolContext) -> ToolResult<std::path::PathBuf> {
    if let Some(p) = ctx.models().path("seeta-face") {
        return Ok(p);
    }
    ctx.models()
        .installed_in_group("face-detection")
        .first()
        .and_then(|m| ctx.models().path(m.id))
        .ok_or_else(|| ToolError::MissingModel {
            message: "install the 'seeta-face' face detection model (models → face-detection)"
                .into(),
        })
}

/// Detect faces; boxes are in the coordinates of `img`.
pub fn detect_faces(
    ctx: &ToolContext,
    img: &DynamicImage,
    min_face: u32,
    threshold: f64,
) -> ToolResult<Vec<Face>> {
    let path = model_path(ctx)?;
    let mut det = rustface::create_detector(&path.to_string_lossy())
        .map_err(|e| ToolError::failed(format!("could not load face model: {e}")))?;
    // Work on at most ~1600px for speed; scale boxes back.
    let max_edge = 1600u32;
    let k = if img.width().max(img.height()) > max_edge {
        max_edge as f64 / img.width().max(img.height()) as f64
    } else {
        1.0
    };
    let gray = if k < 1.0 {
        img.resize(
            ((img.width() as f64) * k) as u32,
            ((img.height() as f64) * k) as u32,
            image::imageops::FilterType::Triangle,
        )
        .to_luma8()
    } else {
        img.to_luma8()
    };
    let min_face = ((min_face as f64) * k).round().max(20.0) as u32;
    det.set_min_face_size(min_face);
    det.set_score_thresh(threshold);
    det.set_pyramid_scale_factor(0.8);
    det.set_slide_window_step(4, 4);
    ctx.check_cancelled()?;
    let data = rustface::ImageData::new(gray.as_raw(), gray.width(), gray.height());
    let faces = det.detect(&data);
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    Ok(faces
        .iter()
        .map(|f| {
            let b = f.bbox();
            let x = ((b.x() as f64) / k).round() as i64;
            let y = ((b.y() as f64) / k).round() as i64;
            let w = ((b.width() as f64) / k).round() as i64;
            let h = ((b.height() as f64) / k).round() as i64;
            let x0 = x.clamp(0, iw);
            let y0 = y.clamp(0, ih);
            Face {
                x: x0,
                y: y0,
                w: (x + w).min(iw) - x0,
                h: (y + h).min(ih) - y0,
                score: (f.score() * 100.0).round() / 100.0,
            }
        })
        .filter(|f| f.w > 0 && f.h > 0)
        .collect())
}

fn detection_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::integer("min_face_size")
            .describe("Smallest face to look for, in pixels")
            .range(20.0, 2000.0)
            .default_int(40)
            .advanced(),
        ParamSpec::number("threshold")
            .describe("Detection score threshold (higher = fewer false positives)")
            .range(0.0, 20.0)
            .default_num(2.0)
            .advanced(),
    ]
}

fn draw_box(img: &mut RgbaImage, f: &Face, color: Rgba<u8>) {
    let t = ((img.width().max(img.height()) as f32) / 400.0)
        .ceil()
        .max(2.0) as i64;
    let (w, h) = (img.width() as i64, img.height() as i64);
    for d in 0..t {
        for x in f.x..f.x + f.w {
            for y in [f.y + d, f.y + f.h - 1 - d] {
                if x >= 0 && x < w && y >= 0 && y < h {
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
        for y in f.y..f.y + f.h {
            for x in [f.x + d, f.x + f.w - 1 - d] {
                if x >= 0 && x < w && y >= 0 && y < h {
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
    }
}

fn regions_json(faces: &[Face]) -> Vec<Value> {
    faces.iter().enumerate().map(|(i, f)| json!({"x": f.x, "y": f.y, "width": f.w, "height": f.h, "score": f.score, "label": format!("face {}", i + 1)})).collect()
}

fn face_detect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.face_detect", "Detect Faces", Category::Image)
        .describe("Find faces in a photo locally (SeetaFace model) and return their bounding boxes with an annotated preview")
        .keywords(&["face", "detect", "faces", "find faces", "privacy", "people"])
        .param(image_param())
        .params(detection_params())
        .requires_model("face-detection")
        .output(OutputKind::Data)
        .timeout(300);
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let img = load_image(ctx, f)?;
        let faces = detect_faces(
            ctx,
            &img,
            p.int_or("min_face_size", 40) as u32,
            p.num_or("threshold", 2.0),
        )?;
        let mut preview = img.to_rgba8();
        for face in &faces {
            draw_box(&mut preview, face, Rgba([255, 40, 80, 255]));
        }
        let a = ctx.artifact_labeled(
            save_png(ctx, &preview, &suffixed(&f.name, "faces", Some("png")))?.path,
            "preview",
        )?;
        Ok(ToolOutput::files(vec![a]).with_data(json!({"faces": faces.len(), "width": img.width(), "height": img.height(), "regions": regions_json(&faces)})))
    })
}

fn face_blur() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.face_blur", "Blur Faces", Category::Image)
        .describe("Automatically detect and blur, pixelate or black out faces in photos; returns the regions for review")
        .keywords(&["blur faces", "anonymize", "privacy", "pixelate faces", "censor", "gdpr", "redact"])
        .param(images_param())
        .params(redact_mode_params())
        .param(ParamSpec::number("padding").describe("Grow each face box by this percentage").range(0.0, 100.0).default_num(20.0))
        .param(ParamSpec::json("extra_regions").label("Extra regions").describe("Additional {x, y, width, height} rectangles to redact (first image)").advanced())
        .params(detection_params())
        .param(quality_param().default_int(92).advanced())
        .requires_model("face-detection")
        .output(OutputKind::Files)
        .ui("face-redact")
        .progress()
        .timeout(900);
    sync_tool(spec, |ctx, p| {
        let pad = p.num_or("padding", 20.0) / 100.0;
        let extra = parse_regions(p.json("extra_regions"), "extra_regions")?;
        let mode = p.str_or("mode", "blur").to_string();
        let strength = p.num_or("strength", 25.0) as f32;
        let color = color_param(p, "color", "#000000")?;
        let mut first_regions = vec![];
        let mut table = vec![];
        let files = p.files("input");
        let arts = for_each_file(ctx, files, "Blurring faces in", |f| {
            let img = load_image(ctx, f)?;
            let faces = detect_faces(
                ctx,
                &img,
                p.int_or("min_face_size", 40) as u32,
                p.num_or("threshold", 2.0),
            )?;
            let (w, h) = (img.width(), img.height());
            let mut rects: Vec<(u32, u32, u32, u32)> = faces
                .iter()
                .filter_map(|fc| {
                    let dx = (fc.w as f64 * pad / 2.0).round() as i64;
                    let dy = (fc.h as f64 * pad / 2.0).round() as i64;
                    clip_rect((fc.x - dx, fc.y - dy, fc.w + 2 * dx, fc.h + 2 * dy), w, h)
                })
                .collect();
            if first_regions.is_empty() && table.is_empty() {
                rects.extend(extra.iter().filter_map(|r| clip_rect(*r, w, h)));
                first_regions = rects.iter().map(|(x, y, rw, rh)| json!({"x": x, "y": y, "width": rw, "height": rh, "label": "face"})).collect();
            }
            table.push(json!([f.name, faces.len()]));
            let had_alpha = img.color().has_alpha();
            let mut rgba = img.to_rgba8();
            redact_regions(&mut rgba, &rects, &mode, strength, color);
            let out = if had_alpha {
                DynamicImage::ImageRgba8(rgba)
            } else {
                DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
            };
            let fmt = OutFmt::from_name(&f.name).unwrap_or(OutFmt::Png);
            save_output(
                ctx,
                &out,
                &suffixed(&f.name, "faces-blurred", Some(fmt.ext())),
                fmt,
                &SaveOpts {
                    quality: p.int_or("quality", 92) as u8,
                    ..Default::default()
                },
            )
        })?;
        let total: u64 = table.iter().filter_map(|r| r[1].as_u64()).sum();
        let mut out = ToolOutput::files(arts).with_data(json!({"faces": total, "regions": first_regions, "table": {"columns": ["File", "Faces"], "rows": table}}));
        if total == 0 {
            out = out.warn("no faces were detected; check the result or add regions manually");
        }
        Ok(out)
    })
}
