//! Editing tools: crop, rotate/flip, adjust, watermark, combine, split,
//! compare, contact/sprite sheets, redaction and denoising.

use crate::util::*;
use dogubako_core::prelude::*;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use imageproc::geometric_transformations::{rotate_about_center_no_crop, Border, Interpolation};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        crop(),
        rotate_flip(),
        adjust(),
        watermark(),
        combine(),
        split(),
        compare(),
        contact_sheet(),
        sprite_sheet(),
        redact(),
        denoise(),
    ]
}

fn same_format_opts(p: &Params) -> SaveOpts {
    SaveOpts {
        quality: p.int_or("quality", 92).clamp(1, 100) as u8,
        ..Default::default()
    }
}

fn out_format(p: &Params, f: &InputFile) -> ToolResult<OutFmt> {
    pick_format(p.str_or("format", "same"), &f.name)
}

fn format_param() -> ParamSpec {
    let mut opts = vec![("same", "Same as input")];
    opts.extend_from_slice(NATIVE_FORMATS);
    ParamSpec::choice("format", &opts)
        .label("Output format")
        .default_str("same")
        .advanced()
}

/// Parse "16:9", "16/9", "1.777" into a ratio.
pub fn parse_ratio(s: &str) -> Option<f64> {
    let s = s.trim();
    let r = if let Some((a, b)) = s.split_once([':', '/', 'x']) {
        let a: f64 = a.trim().parse().ok()?;
        let b: f64 = b.trim().parse().ok()?;
        if b == 0.0 {
            return None;
        }
        a / b
    } else {
        s.parse().ok()?
    };
    (r.is_finite() && r > 0.0).then_some(r)
}

fn crop() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.crop", "Crop Image", Category::Image)
        .describe("Crop images to a rectangle or to an aspect ratio (centered or anchored)")
        .keywords(&["crop", "trim", "cut", "aspect ratio", "square"])
        .param(images_param())
        .param(
            ParamSpec::choice("mode", &[("rect", "Rectangle"), ("aspect", "Aspect ratio")])
                .default_str("rect"),
        )
        .param(
            ParamSpec::integer("x")
                .min(0.0)
                .default_int(0)
                .show_if("mode", &["rect"]),
        )
        .param(
            ParamSpec::integer("y")
                .min(0.0)
                .default_int(0)
                .show_if("mode", &["rect"]),
        )
        .param(
            ParamSpec::integer("width")
                .min(1.0)
                .show_if("mode", &["rect"]),
        )
        .param(
            ParamSpec::integer("height")
                .min(1.0)
                .show_if("mode", &["rect"]),
        )
        .param(
            ParamSpec::text("aspect")
                .label("Aspect ratio")
                .placeholder("16:9")
                .describe("Ratio like 1:1, 4:3, 16:9 or a decimal")
                .default_str("1:1")
                .show_if("mode", &["aspect"]),
        )
        .param(
            ParamSpec::choice("anchor", POSITIONS)
                .default_str("center")
                .show_if("mode", &["aspect"]),
        )
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .ui("crop")
        .progress();
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let mut regions = vec![];
        let arts = for_each_file(ctx, files, "Cropping", |f| {
            let img = load_image(ctx, f)?;
            let (w, h) = img.dimensions();
            let (x, y, cw, ch) = if p.str_or("mode", "rect") == "aspect" {
                let r = parse_ratio(p.str_or("aspect", "1:1"))
                    .ok_or_else(|| ToolError::invalid("aspect", "expected a ratio like 16:9"))?;
                let (cw, ch) = if w as f64 / h as f64 > r {
                    (((h as f64) * r).round().max(1.0) as u32, h)
                } else {
                    (w, ((w as f64) / r).round().max(1.0) as u32)
                };
                let (x, y) = anchor(p.str_or("anchor", "center"), w, h, cw.min(w), ch.min(h), 0);
                (x.max(0) as u32, y.max(0) as u32, cw.min(w), ch.min(h))
            } else {
                let x = p.int_or("x", 0);
                let y = p.int_or("y", 0);
                let cw = p
                    .opt_int("width")
                    .ok_or_else(|| ToolError::invalid("width", "is required in rectangle mode"))?;
                let ch = p
                    .opt_int("height")
                    .ok_or_else(|| ToolError::invalid("height", "is required in rectangle mode"))?;
                clip_rect((x, y, cw, ch), w, h).ok_or_else(|| {
                    ToolError::invalid(
                        "x",
                        format!("crop rectangle lies outside the {w}×{h} image"),
                    )
                })?
            };
            regions.push(json!({"x": x, "y": y, "width": cw, "height": ch, "label": f.name}));
            let out = img.crop_imm(x, y, cw, ch);
            let fmt = out_format(p, f)?;
            save_output(
                ctx,
                &out,
                &suffixed(&f.name, "cropped", Some(fmt.ext())),
                fmt,
                &same_format_opts(p),
            )
        })?;
        Ok(ToolOutput::files(arts).with_data(json!({"regions": regions})))
    })
}

pub fn rotate_any(
    ctx: &ToolContext,
    img: &DynamicImage,
    degrees: f64,
    bg: Rgba<u8>,
) -> ToolResult<DynamicImage> {
    let d = degrees.rem_euclid(360.0);
    if d.abs() < 1e-9 {
        return Ok(img.clone());
    }
    if (d - 90.0).abs() < 1e-9 {
        return Ok(img.rotate90());
    }
    if (d - 180.0).abs() < 1e-9 {
        return Ok(img.rotate180());
    }
    if (d - 270.0).abs() < 1e-9 {
        return Ok(img.rotate270());
    }
    let theta = (d as f32).to_radians();
    check_rotated(ctx, img.width(), img.height(), theta)?;
    let rgba = img.to_rgba8();
    Ok(DynamicImage::ImageRgba8(rotate_about_center_no_crop(
        &rgba,
        theta,
        Interpolation::Bicubic,
        Border::Constant(bg),
    )))
}

/// Check the canvas `rotate_about_center_no_crop` will allocate (same formula)
/// against the pixel limit: a 100000x1 strip at 45 degrees needs ~70711x70711.
fn check_rotated(ctx: &ToolContext, w: u32, h: u32, theta: f32) -> ToolResult<()> {
    let (s, c) = (theta.sin().abs(), theta.cos().abs());
    let nw = (h as f32 * s + w as f32 * c).ceil() as u64;
    let nh = (h as f32 * c + w as f32 * s).ceil() as u64;
    check_pixels(ctx, nw, nh)
}

fn rotate_flip() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.rotate_flip", "Rotate & Flip", Category::Image)
        .describe("Rotate images by 90/180/270 degrees or any angle, and flip horizontally or vertically, in bulk")
        .keywords(&["rotate", "flip", "mirror", "turn", "straighten", "orientation"])
        .param(images_param())
        .param(ParamSpec::number("angle").describe("Clockwise rotation in degrees (90, 180, 270 are lossless in pixels; others expand the canvas)").range(-360.0, 360.0).default_num(90.0))
        .param(ParamSpec::boolean("flip_horizontal").default_bool(false))
        .param(ParamSpec::boolean("flip_vertical").default_bool(false))
        .param(ParamSpec::color("background").describe("Fill for corners exposed by arbitrary angles").default_str("#00000000").advanced())
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let bg = color_param(p, "background", "#00000000")?;
        let angle = p.num_or("angle", 0.0);
        let arts = for_each_file(ctx, p.files("input"), "Rotating", |f| {
            let mut img = load_image(ctx, f)?;
            if p.bool("flip_horizontal") {
                img = img.fliph();
            }
            if p.bool("flip_vertical") {
                img = img.flipv();
            }
            let img = rotate_any(ctx, &img, angle, bg)?;
            let fmt = out_format(p, f)?;
            let mut opts = same_format_opts(p);
            if bg[3] == 0 {
                opts.background = Rgba([255, 255, 255, 255]);
            } else {
                opts.background = bg;
            }
            save_output(
                ctx,
                &img,
                &suffixed(&f.name, "rotated", Some(fmt.ext())),
                fmt,
                &opts,
            )
        })?;
        Ok(ToolOutput::files(arts))
    })
}

#[derive(Default, Clone, Copy)]
pub struct Adjust {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub hue: f32,
    pub gamma: f32,
    pub grayscale: bool,
    pub invert: bool,
    pub sepia: bool,
}

pub fn apply_adjust(img: &mut RgbaImage, a: &Adjust) {
    let cf = ((100.0 + a.contrast) / 100.0).powi(2);
    let bo = a.brightness * 2.55;
    let sat = 1.0 + a.saturation / 100.0;
    let inv_gamma = if a.gamma > 0.0 { 1.0 / a.gamma } else { 1.0 };
    // CSS hue-rotate matrix.
    let th = a.hue.to_radians();
    let (c, s) = (th.cos(), th.sin());
    let hm = [
        [
            0.213 + c * 0.787 - s * 0.213,
            0.715 - c * 0.715 - s * 0.715,
            0.072 - c * 0.072 + s * 0.928,
        ],
        [
            0.213 - c * 0.213 + s * 0.143,
            0.715 + c * 0.285 + s * 0.140,
            0.072 - c * 0.072 - s * 0.283,
        ],
        [
            0.213 - c * 0.213 - s * 0.787,
            0.715 - c * 0.715 + s * 0.715,
            0.072 + c * 0.928 + s * 0.072,
        ],
    ];
    let use_hue = a.hue.abs() > 1e-3;
    let use_gamma = (inv_gamma - 1.0).abs() > 1e-4;
    for px in img.pixels_mut() {
        let mut v = [px[0] as f32, px[1] as f32, px[2] as f32];
        if bo != 0.0 {
            v.iter_mut().for_each(|x| *x += bo);
        }
        if cf != 1.0 {
            v.iter_mut().for_each(|x| *x = (*x - 128.0) * cf + 128.0);
        }
        if sat != 1.0 {
            let g = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
            v.iter_mut().for_each(|x| *x = g + (*x - g) * sat);
        }
        if use_hue {
            let o = v;
            for (i, row) in hm.iter().enumerate() {
                v[i] = row[0] * o[0] + row[1] * o[1] + row[2] * o[2];
            }
        }
        if use_gamma {
            v.iter_mut()
                .for_each(|x| *x = 255.0 * (x.clamp(0.0, 255.0) / 255.0).powf(inv_gamma));
        }
        if a.sepia {
            let o = v;
            v[0] = 0.393 * o[0] + 0.769 * o[1] + 0.189 * o[2];
            v[1] = 0.349 * o[0] + 0.686 * o[1] + 0.168 * o[2];
            v[2] = 0.272 * o[0] + 0.534 * o[1] + 0.131 * o[2];
        }
        if a.grayscale {
            let g = 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
            v = [g, g, g];
        }
        if a.invert {
            v.iter_mut().for_each(|x| *x = 255.0 - x.clamp(0.0, 255.0));
        }
        for i in 0..3 {
            px[i] = v[i].round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Unsharp mask: out = src + amount * (src - blur(src)).
pub fn sharpen(img: &RgbaImage, amount: f32, sigma: f32) -> RgbaImage {
    let blurred = DynamicImage::ImageRgba8(img.clone())
        .blur(sigma.max(0.3))
        .to_rgba8();
    let mut out = img.clone();
    for (o, b) in out.pixels_mut().zip(blurred.pixels()) {
        for c in 0..3 {
            let v = o[c] as f32 + amount * (o[c] as f32 - b[c] as f32);
            o[c] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

fn adjust() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.adjust", "Adjust & Filter", Category::Image)
        .describe("Adjust brightness, contrast, saturation, hue and gamma; apply grayscale, sepia, invert, blur or sharpen, in bulk")
        .keywords(&["brightness", "contrast", "saturation", "hue", "gamma", "grayscale", "black and white", "sepia", "invert", "blur", "sharpen", "filter"])
        .param(images_param())
        .param(ParamSpec::number("brightness").range(-100.0, 100.0).step(1.0).default_num(0.0))
        .param(ParamSpec::number("contrast").range(-100.0, 100.0).step(1.0).default_num(0.0))
        .param(ParamSpec::number("saturation").range(-100.0, 100.0).step(1.0).default_num(0.0))
        .param(ParamSpec::number("hue").label("Hue shift (°)").range(-180.0, 180.0).step(1.0).default_num(0.0))
        .param(ParamSpec::number("gamma").range(0.1, 5.0).step(0.05).default_num(1.0))
        .param(ParamSpec::boolean("grayscale").default_bool(false))
        .param(ParamSpec::boolean("sepia").default_bool(false))
        .param(ParamSpec::boolean("invert").default_bool(false))
        .param(ParamSpec::number("blur").label("Blur radius (σ)").range(0.0, 100.0).step(0.5).default_num(0.0))
        .param(ParamSpec::number("sharpen").label("Sharpen amount").range(0.0, 10.0).step(0.1).default_num(0.0))
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let a = Adjust {
            brightness: p.num_or("brightness", 0.0) as f32,
            contrast: p.num_or("contrast", 0.0) as f32,
            saturation: p.num_or("saturation", 0.0) as f32,
            hue: p.num_or("hue", 0.0) as f32,
            gamma: p.num_or("gamma", 1.0) as f32,
            grayscale: p.bool("grayscale"),
            invert: p.bool("invert"),
            sepia: p.bool("sepia"),
        };
        let blur = p.num_or("blur", 0.0) as f32;
        let sharp = p.num_or("sharpen", 0.0) as f32;
        let arts = for_each_file(ctx, p.files("input"), "Adjusting", |f| {
            let img = load_image(ctx, f)?;
            let had_alpha = img.color().has_alpha();
            let mut rgba = img.to_rgba8();
            apply_adjust(&mut rgba, &a);
            if blur > 0.0 {
                rgba = DynamicImage::ImageRgba8(rgba).blur(blur).to_rgba8();
            }
            if sharp > 0.0 {
                rgba = sharpen(&rgba, sharp, 1.0);
            }
            let out = if had_alpha {
                DynamicImage::ImageRgba8(rgba)
            } else {
                DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
            };
            let fmt = out_format(p, f)?;
            save_output(
                ctx,
                &out,
                &suffixed(&f.name, "adjusted", Some(fmt.ext())),
                fmt,
                &same_format_opts(p),
            )
        })?;
        Ok(ToolOutput::files(arts))
    })
}

/// Stamp `layer` over `base` at an anchor or tiled across the whole image.
fn stamp(
    base: &mut RgbaImage,
    layer: &RgbaImage,
    pos: &str,
    margin: i64,
    tile: bool,
    spacing: f32,
    opacity: f32,
) {
    if tile {
        let sx = (layer.width() as f32 * (1.0 + spacing)).max(1.0) as i64;
        let sy = (layer.height() as f32 * (1.0 + spacing * 2.0)).max(1.0) as i64;
        let mut row = 0i64;
        let mut y = -(layer.height() as i64) / 2;
        while y < base.height() as i64 {
            let off = if row % 2 == 1 { sx / 2 } else { 0 };
            let mut x = -off;
            while x < base.width() as i64 {
                composite(base, layer, x, y, opacity, Blend::Normal);
                x += sx;
            }
            y += sy;
            row += 1;
        }
    } else {
        let (x, y) = anchor(
            pos,
            base.width(),
            base.height(),
            layer.width(),
            layer.height(),
            margin,
        );
        composite(base, layer, x, y, opacity, Blend::Normal);
    }
}

fn watermark() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.watermark", "Watermark Images", Category::Image)
        .describe("Add a text or image watermark to many images, with position, opacity, rotation and tiling")
        .keywords(&["watermark", "logo", "copyright", "stamp", "overlay", "brand", "protect"])
        .param(images_param())
        .param(ParamSpec::choice("kind", &[("text", "Text"), ("image", "Image / logo")]).default_str("text"))
        .param(ParamSpec::text("text").default_str("© Dogubako").show_if("kind", &["text"]))
        .param(ParamSpec::number("font_size").label("Text size (% of image width)").range(0.5, 50.0).default_num(5.0).show_if("kind", &["text"]))
        .param(ParamSpec::color("color").default_str("#ffffff").show_if("kind", &["text"]))
        .param(ParamSpec::file("watermark").label("Watermark image").accept(IMAGE_ACCEPT).show_if("kind", &["image"]))
        .param(ParamSpec::number("scale").label("Logo width (% of image width)").range(1.0, 100.0).default_num(20.0).show_if("kind", &["image"]))
        .param(ParamSpec::number("opacity").range(0.0, 1.0).step(0.05).default_num(0.5))
        .param(ParamSpec::choice("position", POSITIONS).default_str("bottom-right"))
        .param(ParamSpec::number("margin").label("Margin (% of width)").range(0.0, 25.0).default_num(2.0))
        .param(ParamSpec::boolean("tile").describe("Repeat the watermark across the whole image").default_bool(false))
        .param(ParamSpec::number("rotation").label("Rotation (°)").range(-180.0, 180.0).default_num(0.0))
        .param(ParamSpec::number("spacing").describe("Gap between tiles relative to the watermark size").range(0.0, 5.0).default_num(0.6).show_if_bool("tile", true).advanced())
        .param(ParamSpec::boolean("shadow").describe("Add a subtle dark shadow behind text for legibility").default_bool(true).show_if("kind", &["text"]).advanced())
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let kind = p.str_or("kind", "text");
        let logo = if kind == "image" {
            let wf = p
                .opt_file("watermark")
                .ok_or_else(|| ToolError::invalid("watermark", "a watermark image is required"))?;
            Some(load_image(ctx, wf)?.to_rgba8())
        } else {
            if p.text("text").trim().is_empty() {
                return Err(ToolError::invalid("text", "watermark text is empty"));
            }
            None
        };
        let color = color_param(p, "color", "#ffffff")?;
        let opacity = p.num_or("opacity", 0.5) as f32;
        let rotation = p.num_or("rotation", 0.0);
        let arts = for_each_file(ctx, p.files("input"), "Watermarking", |f| {
            let img = load_image(ctx, f)?;
            let had_alpha = img.color().has_alpha();
            let mut base = img.to_rgba8();
            let bw = base.width() as f32;
            let mut layer = match &logo {
                Some(l) => {
                    let tw = (f64::from(bw) * p.num_or("scale", 20.0) / 100.0)
                        .round()
                        .max(1.0);
                    let th = (f64::from(l.height()) * tw / f64::from(l.width()))
                        .round()
                        .max(1.0);
                    // f64 -> u64 casts saturate, so a huge size is rejected, not wrapped.
                    check_pixels(ctx, tw as u64, th as u64)?;
                    let (tw, th) = (tw as u32, th as u32);
                    image::imageops::resize(l, tw, th, image::imageops::FilterType::Lanczos3)
                }
                None => {
                    let px = (bw * p.num_or("font_size", 5.0) as f32 / 100.0).max(6.0);
                    let txt = render_text(ctx, p.text("text"), px, color)?;
                    if p.bool("shadow") {
                        let off = (px / 18.0).ceil().max(1.0) as i64;
                        let sh = render_text(ctx, p.text("text"), px, Rgba([0, 0, 0, 140]))?;
                        check_pixels(
                            ctx,
                            txt.width() as u64 + off as u64,
                            txt.height() as u64 + off as u64,
                        )?;
                        let mut l =
                            RgbaImage::new(txt.width() + off as u32, txt.height() + off as u32);
                        composite(&mut l, &sh, off, off, 1.0, Blend::Normal);
                        composite(&mut l, &txt, 0, 0, 1.0, Blend::Normal);
                        l
                    } else {
                        txt
                    }
                }
            };
            if rotation.abs() > 0.01 {
                check_rotated(
                    ctx,
                    layer.width(),
                    layer.height(),
                    (rotation as f32).to_radians(),
                )?;
                layer = rotate_about_center_no_crop(
                    &layer,
                    (rotation as f32).to_radians(),
                    Interpolation::Bilinear,
                    Border::Constant(Rgba([0, 0, 0, 0])),
                );
            }
            let margin = (bw * p.num_or("margin", 2.0) as f32 / 100.0).round() as i64;
            stamp(
                &mut base,
                &layer,
                p.str_or("position", "bottom-right"),
                margin,
                p.bool("tile"),
                p.num_or("spacing", 0.6) as f32,
                opacity,
            );
            let out = if had_alpha {
                DynamicImage::ImageRgba8(base)
            } else {
                DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(base).to_rgb8())
            };
            let fmt = out_format(p, f)?;
            save_output(
                ctx,
                &out,
                &suffixed(&f.name, "watermarked", Some(fmt.ext())),
                fmt,
                &same_format_opts(p),
            )
        })?;
        Ok(ToolOutput::files(arts))
    })
}

/// Check a combined canvas size against the pixel limit and u32 range.
fn canvas_dims(ctx: &ToolContext, w: u64, h: u64) -> ToolResult<(u32, u32)> {
    check_pixels(ctx, w, h)?;
    match (u32::try_from(w), u32::try_from(h)) {
        (Ok(w), Ok(h)) => Ok((w, h)),
        _ => Err(ToolError::limit(format!(
            "combined image of {w}x{h} pixels is too large"
        ))),
    }
}

fn load_all(ctx: &ToolContext, files: &[InputFile]) -> ToolResult<Vec<DynamicImage>> {
    let mut v = vec![];
    for (i, f) in files.iter().enumerate() {
        ctx.check_cancelled()?;
        ctx.progress(
            i as f32 / files.len() as f32 * 0.5,
            format!("loading {}", f.name),
        );
        v.push(load_image(ctx, f)?);
    }
    Ok(v)
}

fn combine() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.combine", "Combine Images", Category::Image)
        .describe("Stitch images side by side, top to bottom or into a grid")
        .keywords(&[
            "combine",
            "merge",
            "stitch",
            "collage",
            "join",
            "side by side",
            "grid",
            "montage",
        ])
        .param(
            ParamSpec::files("input")
                .label("Images (in order)")
                .required()
                .primary()
                .accept(IMAGE_ACCEPT),
        )
        .param(
            ParamSpec::choice(
                "direction",
                &[
                    ("horizontal", "Horizontal"),
                    ("vertical", "Vertical"),
                    ("grid", "Grid"),
                ],
            )
            .default_str("horizontal"),
        )
        .param(
            ParamSpec::integer("columns")
                .range(1.0, 100.0)
                .default_int(2)
                .show_if("direction", &["grid"]),
        )
        .param(
            ParamSpec::boolean("match_size")
                .describe(
                    "Scale images to the same height (horizontal), width (vertical) or cell (grid)",
                )
                .default_bool(true),
        )
        .param(
            ParamSpec::integer("spacing")
                .describe("Gap between images in pixels")
                .range(0.0, 1000.0)
                .default_int(0),
        )
        .param(
            ParamSpec::choice(
                "align",
                &[
                    ("center", "Center"),
                    ("start", "Start (top/left)"),
                    ("end", "End (bottom/right)"),
                ],
            )
            .default_str("center")
            .advanced(),
        )
        .param(ParamSpec::color("background").default_str("#ffffff"))
        .param(
            ParamSpec::choice(
                "format",
                &[("png", "PNG"), ("jpg", "JPEG"), ("webp", "WebP")],
            )
            .default_str("png"),
        )
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        if files.len() < 2 {
            return Err(ToolError::invalid("input", "select at least two images"));
        }
        let mut imgs = load_all(ctx, files)?;
        let dir = p.str_or("direction", "horizontal");
        let gap = p.int_or("spacing", 0) as u32;
        let bg = color_param(p, "background", "#ffffff")?;
        let align = p.str_or("align", "center");
        let off = |space: u32, size: u32| -> i64 {
            match align {
                "start" => 0,
                "end" => space as i64 - size as i64,
                _ => (space as i64 - size as i64) / 2,
            }
        };
        let canvas = match dir {
            "vertical" => {
                // Work out every size (and the canvas) in u64 and check the
                // limit before resizing anything.
                let tw = imgs.iter().map(|i| i.width()).max().unwrap_or(1);
                let sizes: Vec<(u64, u64)> = imgs
                    .iter()
                    .map(|i| {
                        if p.bool("match_size") {
                            let h = (i.height() as f64 * tw as f64 / i.width() as f64)
                                .round()
                                .max(1.0);
                            (tw as u64, h.min(u64::MAX as f64) as u64)
                        } else {
                            (i.width() as u64, i.height() as u64)
                        }
                    })
                    .collect();
                let w = sizes.iter().map(|s| s.0).max().unwrap_or(1);
                let h = sizes
                    .iter()
                    .fold(0u64, |a, s| a.saturating_add(s.1))
                    .saturating_add(gap as u64 * (imgs.len() as u64 - 1));
                let (w, h) = canvas_dims(ctx, w, h)?;
                if p.bool("match_size") {
                    imgs = imgs
                        .iter()
                        .zip(&sizes)
                        .map(|(i, s)| {
                            i.resize_exact(
                                s.0 as u32,
                                s.1 as u32,
                                image::imageops::FilterType::Lanczos3,
                            )
                        })
                        .collect();
                }
                let mut c = RgbaImage::from_pixel(w, h, bg);
                let mut y = 0i64;
                for i in &imgs {
                    composite(
                        &mut c,
                        &i.to_rgba8(),
                        off(w, i.width()),
                        y,
                        1.0,
                        Blend::Normal,
                    );
                    y += (i.height() + gap) as i64;
                }
                c
            }
            "grid" => {
                let cols = p.int_or("columns", 2).max(1) as usize;
                let rows = imgs.len().div_ceil(cols);
                let cw = imgs.iter().map(|i| i.width()).max().unwrap_or(1);
                let chh = imgs.iter().map(|i| i.height()).max().unwrap_or(1);
                let span = |cell: u32, n: usize| {
                    (cell as u64)
                        .saturating_mul(n as u64)
                        .saturating_add((gap as u64).saturating_mul(n as u64 - 1))
                };
                let (w, h) = canvas_dims(ctx, span(cw, cols), span(chh, rows))?;
                let mut c = RgbaImage::from_pixel(w, h, bg);
                for (k, i) in imgs.iter().enumerate() {
                    let i = if p.bool("match_size") {
                        i.resize(cw, chh, image::imageops::FilterType::Lanczos3)
                    } else {
                        i.clone()
                    };
                    let (cx, cy) = (
                        (k % cols) as u32 * (cw + gap),
                        (k / cols) as u32 * (chh + gap),
                    );
                    composite(
                        &mut c,
                        &i.to_rgba8(),
                        cx as i64 + off(cw, i.width()),
                        cy as i64 + off(chh, i.height()),
                        1.0,
                        Blend::Normal,
                    );
                }
                c
            }
            _ => {
                let th = imgs.iter().map(|i| i.height()).max().unwrap_or(1);
                let sizes: Vec<(u64, u64)> = imgs
                    .iter()
                    .map(|i| {
                        if p.bool("match_size") {
                            let w = (i.width() as f64 * th as f64 / i.height() as f64)
                                .round()
                                .max(1.0);
                            (w.min(u64::MAX as f64) as u64, th as u64)
                        } else {
                            (i.width() as u64, i.height() as u64)
                        }
                    })
                    .collect();
                let h = sizes.iter().map(|s| s.1).max().unwrap_or(1);
                let w = sizes
                    .iter()
                    .fold(0u64, |a, s| a.saturating_add(s.0))
                    .saturating_add(gap as u64 * (imgs.len() as u64 - 1));
                let (w, h) = canvas_dims(ctx, w, h)?;
                if p.bool("match_size") {
                    imgs = imgs
                        .iter()
                        .zip(&sizes)
                        .map(|(i, s)| {
                            i.resize_exact(
                                s.0 as u32,
                                s.1 as u32,
                                image::imageops::FilterType::Lanczos3,
                            )
                        })
                        .collect();
                }
                let mut c = RgbaImage::from_pixel(w, h, bg);
                let mut x = 0i64;
                for i in &imgs {
                    composite(
                        &mut c,
                        &i.to_rgba8(),
                        x,
                        off(h, i.height()),
                        1.0,
                        Blend::Normal,
                    );
                    x += (i.width() + gap) as i64;
                }
                c
            }
        };
        ctx.progress(0.9, "encoding");
        let fmt = OutFmt::parse(p.str_or("format", "png")).unwrap_or(OutFmt::Png);
        let (w, h) = canvas.dimensions();
        let a = save_output(
            ctx,
            &DynamicImage::ImageRgba8(canvas),
            &format!("combined.{}", fmt.ext()),
            fmt,
            &SaveOpts {
                background: bg,
                ..Default::default()
            },
        )?;
        Ok(ToolOutput::files(vec![a])
            .with_data(json!({"width": w, "height": h, "images": files.len()})))
    })
}

fn split() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.split", "Split Image into Tiles", Category::Image)
        .describe("Cut an image into a grid of tiles by rows × columns or by tile size (e.g. for Instagram grids)")
        .keywords(&["split", "slice", "tiles", "grid", "cut", "instagram grid", "chop"])
        .param(image_param())
        .param(ParamSpec::choice("mode", &[("grid", "Rows × columns"), ("size", "Tile size")]).default_str("grid"))
        .param(ParamSpec::integer("rows").range(1.0, 100.0).default_int(2).show_if("mode", &["grid"]))
        .param(ParamSpec::integer("columns").range(1.0, 100.0).default_int(2).show_if("mode", &["grid"]))
        .param(ParamSpec::integer("tile_width").range(1.0, 100000.0).default_int(512).show_if("mode", &["size"]))
        .param(ParamSpec::integer("tile_height").range(1.0, 100000.0).default_int(512).show_if("mode", &["size"]))
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let img = load_image(ctx, f)?;
        let (w, h) = img.dimensions();
        // Tile edges along each axis. Grid mode keeps the requested counts
        // and spreads remainder pixels (100px / 30 -> tiles of 3 and 4 px);
        // size mode uses fixed tiles with a smaller last one.
        let (xs, ys): (Vec<u32>, Vec<u32>) = if p.str_or("mode", "grid") == "size" {
            let tw = p.int_or("tile_width", 512).max(1) as u32;
            let th = p.int_or("tile_height", 512).max(1) as u32;
            let (cols, rows) = (w.div_ceil(tw), h.div_ceil(th));
            if (cols as u64) * (rows as u64) > 2500 {
                return Err(ToolError::invalid(
                    "tile_width",
                    format!(
                        "would create {} tiles (max 2500)",
                        cols as u64 * rows as u64
                    ),
                ));
            }
            (
                (0..=cols).map(|i| (i * tw).min(w)).collect(),
                (0..=rows).map(|i| (i * th).min(h)).collect(),
            )
        } else {
            let r = p.int_or("rows", 2).max(1) as u32;
            let c = p.int_or("columns", 2).max(1) as u32;
            if r > h || c > w {
                return Err(ToolError::invalid("rows", "more tiles than pixels"));
            }
            let edges = |n: u32, len: u32| -> Vec<u32> {
                (0..=n)
                    .map(|i| (u64::from(i) * u64::from(len) / u64::from(n)) as u32)
                    .collect()
            };
            (edges(c, w), edges(r, h))
        };
        let (cols, rows) = (xs.len() as u32 - 1, ys.len() as u32 - 1);
        let fmt = out_format(p, f)?;
        let stem = f.stem();
        let mut arts = vec![];
        let mut regions = vec![];
        for r in 0..rows {
            for c in 0..cols {
                ctx.check_cancelled()?;
                let (x, y) = (xs[c as usize], ys[r as usize]);
                let (cw, ch) = (xs[c as usize + 1] - x, ys[r as usize + 1] - y);
                let t = img.crop_imm(x, y, cw, ch);
                arts.push(save_output(
                    ctx,
                    &t,
                    &format!("{stem}-r{}-c{}.{}", r + 1, c + 1, fmt.ext()),
                    fmt,
                    &same_format_opts(p),
                )?);
                regions.push(json!({"x": x, "y": y, "width": cw, "height": ch, "label": format!("r{} c{}", r + 1, c + 1)}));
                ctx.progress(arts.len() as f32 / (rows * cols) as f32, "");
            }
        }
        Ok(ToolOutput::files(arts)
            .with_data(json!({"rows": rows, "columns": cols, "regions": regions})))
    })
}

/// Comparison metrics between two equally sized RGBA images.
pub struct CompareResult {
    pub differing: u64,
    pub mse: f64,
    pub psnr: f64,
    pub ssim: f64,
    pub bbox: Option<(u32, u32, u32, u32)>,
    pub diff: RgbaImage,
}

pub fn compare_images(a: &RgbaImage, b: &RgbaImage, tolerance: u8) -> CompareResult {
    let (w, h) = a.dimensions();
    let mut diff = RgbaImage::new(w, h);
    let mut differing = 0u64;
    let mut se = 0f64;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let pa = a.get_pixel(x, y);
            let pb = b.get_pixel(x, y);
            let mut maxd = 0u8;
            for c in 0..4 {
                let d = (pa[c] as i32 - pb[c] as i32).unsigned_abs() as u8;
                maxd = maxd.max(d);
                if c < 3 {
                    se += (d as f64) * (d as f64);
                }
            }
            let g = (0.299 * pa[0] as f32 + 0.587 * pa[1] as f32 + 0.114 * pa[2] as f32) as u8;
            let faded = 255 - ((255 - g) / 3);
            if maxd > tolerance {
                differing += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                diff.put_pixel(x, y, Rgba([255, 0, (255 - maxd.max(64)) / 2, 255]));
            } else {
                diff.put_pixel(x, y, Rgba([faded, faded, faded, 255]));
            }
        }
    }
    let n = (w as f64) * (h as f64) * 3.0;
    let mse = se / n;
    let psnr = if mse == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    };
    let bbox = (differing > 0).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1));
    CompareResult {
        differing,
        mse,
        psnr,
        ssim: ssim(a, b),
        bbox,
        diff,
    }
}

/// Mean SSIM over 8×8 windows (stride 4) on luma.
pub fn ssim(a: &RgbaImage, b: &RgbaImage) -> f64 {
    let luma = |i: &RgbaImage| -> Vec<f64> {
        i.pixels()
            .map(|p| 0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64)
            .collect()
    };
    let (la, lb) = (luma(a), luma(b));
    let (w, h) = (a.width() as usize, a.height() as usize);
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let win = 8.min(w).min(h).max(1);
    let stride = (win / 2).max(1);
    let mut total = 0.0;
    let mut count = 0usize;
    let mut y = 0;
    while y + win <= h {
        let mut x = 0;
        while x + win <= w {
            let (mut ma, mut mb) = (0.0, 0.0);
            for j in 0..win {
                for i in 0..win {
                    ma += la[(y + j) * w + x + i];
                    mb += lb[(y + j) * w + x + i];
                }
            }
            let nn = (win * win) as f64;
            ma /= nn;
            mb /= nn;
            let (mut va, mut vb, mut cov) = (0.0, 0.0, 0.0);
            for j in 0..win {
                for i in 0..win {
                    let da = la[(y + j) * w + x + i] - ma;
                    let db = lb[(y + j) * w + x + i] - mb;
                    va += da * da;
                    vb += db * db;
                    cov += da * db;
                }
            }
            va /= nn - 1.0_f64.min(nn - 1.0).max(0.0);
            vb /= nn - 1.0_f64.min(nn - 1.0).max(0.0);
            cov /= nn - 1.0_f64.min(nn - 1.0).max(0.0);
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            count += 1;
            x += stride;
        }
        y += stride;
    }
    if count == 0 {
        1.0
    } else {
        total / count as f64
    }
}

fn compare() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.compare", "Compare Images", Category::Image)
        .describe("Compare two images: highlighted difference image, % of differing pixels, MSE, PSNR and SSIM")
        .keywords(&["compare", "diff", "difference", "psnr", "ssim", "visual regression", "similarity"])
        .param(ParamSpec::file("input").label("Image A").required().primary().accept(IMAGE_ACCEPT))
        .param(ParamSpec::file("other").label("Image B").required().primary().accept(IMAGE_ACCEPT))
        .param(ParamSpec::integer("tolerance").describe("Per-channel difference (0–255) ignored as noise").range(0.0, 255.0).default_int(0))
        .param(ParamSpec::boolean("resize_to_match").describe("Resize B to A's size if dimensions differ").default_bool(true))
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let fa = p.file("input")?;
        let fb = p.file("other")?;
        let a = load_image(ctx, fa)?.to_rgba8();
        let b0 = load_image(ctx, fb)?;
        let same_dims = a.dimensions() == b0.dimensions();
        if !same_dims && !p.bool("resize_to_match") {
            return Ok(ToolOutput::data(json!({
                "identical": false,
                "same_dimensions": false,
                "a": {"width": a.width(), "height": a.height()},
                "b": {"width": b0.width(), "height": b0.height()},
            }))
            .warn("dimensions differ; enable resize_to_match to compare pixels"));
        }
        let b = if same_dims {
            b0.to_rgba8()
        } else {
            b0.resize_exact(a.width(), a.height(), image::imageops::FilterType::Lanczos3)
                .to_rgba8()
        };
        ctx.progress(0.3, "comparing");
        let r = compare_images(&a, &b, p.int_or("tolerance", 0).clamp(0, 255) as u8);
        let total = a.width() as u64 * a.height() as u64;
        let art = save_png(ctx, &r.diff, &suffixed(&fa.name, "diff", Some("png")))?;
        let regions: Vec<Value> = r.bbox.iter().map(|(x, y, w, h)| json!({"x": x, "y": y, "width": w, "height": h, "label": "differences"})).collect();
        Ok(ToolOutput::files(vec![art]).with_data(json!({
            "identical": r.differing == 0,
            "same_dimensions": same_dims,
            "a": {"width": a.width(), "height": a.height()},
            "b": {"width": b0.width(), "height": b0.height()},
            "differing_pixels": r.differing,
            "differing_percent": (r.differing as f64 / total as f64 * 10000.0).round() / 100.0,
            "mse": (r.mse * 1000.0).round() / 1000.0,
            "psnr_db": if r.psnr.is_finite() { json!((r.psnr * 100.0).round() / 100.0) } else { json!("infinite") },
            "ssim": (r.ssim * 10000.0).round() / 10000.0,
            "regions": regions,
        })))
    })
}

fn contact_sheet() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.contact_sheet", "Contact Sheet", Category::Image)
        .describe("Lay out many images as labeled thumbnails on a single sheet")
        .keywords(&[
            "contact sheet",
            "overview",
            "thumbnails",
            "proof sheet",
            "index print",
            "gallery",
        ])
        .param(
            ParamSpec::files("input")
                .label("Images")
                .required()
                .primary()
                .accept(IMAGE_ACCEPT),
        )
        .param(
            ParamSpec::integer("columns")
                .range(1.0, 50.0)
                .default_int(5),
        )
        .param(
            ParamSpec::integer("thumb_size")
                .describe("Thumbnail cell size in pixels")
                .range(16.0, 2048.0)
                .default_int(240),
        )
        .param(
            ParamSpec::boolean("labels")
                .describe("Print file names under the thumbnails")
                .default_bool(true),
        )
        .param(ParamSpec::text("title").describe("Optional heading at the top"))
        .param(
            ParamSpec::integer("spacing")
                .range(0.0, 200.0)
                .default_int(12)
                .advanced(),
        )
        .param(
            ParamSpec::color("background")
                .default_str("#ffffff")
                .advanced(),
        )
        .param(
            ParamSpec::color("text_color")
                .default_str("#222222")
                .advanced(),
        )
        .param(ParamSpec::choice("format", &[("jpg", "JPEG"), ("png", "PNG")]).default_str("jpg"))
        .output(OutputKind::Files)
        .progress()
        .timeout(900);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        if files.is_empty() {
            return Err(ToolError::invalid("input", "select images"));
        }
        let cols = (p.int_or("columns", 5).max(1) as usize).min(files.len());
        let ts = p.int_or("thumb_size", 240) as u32;
        let gap = p.int_or("spacing", 12) as u32;
        let bg = color_param(p, "background", "#ffffff")?;
        let tc = color_param(p, "text_color", "#222222")?;
        let labels = p.bool("labels");
        let label_px = (ts as f32 / 12.0).clamp(10.0, 28.0);
        let label_h = if labels { (label_px * 1.6) as u32 } else { 0 };
        let title = p
            .opt_str("title")
            .map(|t| render_text(ctx, t, label_px * 1.8, tc))
            .transpose()?;
        let title_h = title.as_ref().map(|t| t.height() + gap).unwrap_or(0);
        let rows = files.len().div_ceil(cols) as u32;
        let w = cols as u32 * ts + (cols as u32 + 1) * gap;
        let h = title_h + rows * (ts + label_h) + (rows + 1) * gap;
        check_pixels(ctx, w as u64, h as u64)?;
        let mut sheet = RgbaImage::from_pixel(w, h, bg);
        if let Some(t) = &title {
            composite(
                &mut sheet,
                t,
                ((w - t.width().min(w)) / 2) as i64,
                gap as i64,
                1.0,
                Blend::Normal,
            );
        }
        for (k, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(k as f32 / files.len() as f32, f.name.clone());
            let cx = gap + (k % cols) as u32 * (ts + gap);
            let cy = title_h + gap + (k / cols) as u32 * (ts + label_h + gap);
            match load_image(ctx, f) {
                Ok(img) => {
                    let t = img.thumbnail(ts, ts).to_rgba8();
                    composite(
                        &mut sheet,
                        &t,
                        (cx + (ts - t.width()) / 2) as i64,
                        (cy + (ts - t.height()) / 2) as i64,
                        1.0,
                        Blend::Normal,
                    );
                }
                Err(_) => {
                    let x = render_text(ctx, "unreadable", label_px, Rgba([200, 0, 0, 255]))?;
                    composite(
                        &mut sheet,
                        &x,
                        cx as i64,
                        (cy + ts / 2) as i64,
                        1.0,
                        Blend::Normal,
                    );
                }
            }
            if labels {
                let mut name = f.name.clone();
                let mut lbl = render_text(ctx, &name, label_px, tc)?;
                while lbl.width() > ts && name.chars().count() > 4 {
                    let keep: String = name
                        .chars()
                        .take(name.chars().count().saturating_sub(4))
                        .collect();
                    name = format!("{keep}…");
                    lbl = render_text(ctx, &name, label_px, tc)?;
                    name = name.trim_end_matches('…').to_string();
                }
                composite(
                    &mut sheet,
                    &lbl,
                    (cx + ts.saturating_sub(lbl.width()) / 2) as i64,
                    (cy + ts + 2) as i64,
                    1.0,
                    Blend::Normal,
                );
            }
        }
        let fmt = OutFmt::parse(p.str_or("format", "jpg")).unwrap_or(OutFmt::Jpeg);
        let a = save_output(
            ctx,
            &DynamicImage::ImageRgba8(sheet),
            &format!("contact-sheet.{}", fmt.ext()),
            fmt,
            &SaveOpts {
                quality: 88,
                background: bg,
                lossless: false,
            },
        )?;
        Ok(ToolOutput::files(vec![a])
            .with_data(json!({"width": w, "height": h, "images": files.len()})))
    })
}

fn css_ident(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) || out.is_empty() {
        out.insert(0, 'i');
    }
    out
}

fn sprite_sheet() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.sprite_sheet", "Sprite Sheet", Category::Image)
        .describe(
            "Pack images into a CSS sprite sheet with generated CSS classes and JSON coordinates",
        )
        .keywords(&[
            "sprite",
            "spritesheet",
            "css sprite",
            "atlas",
            "texture atlas",
            "game",
        ])
        .param(
            ParamSpec::files("input")
                .label("Images")
                .required()
                .primary()
                .accept(IMAGE_ACCEPT),
        )
        .param(
            ParamSpec::choice(
                "layout",
                &[
                    ("grid", "Grid"),
                    ("horizontal", "Horizontal strip"),
                    ("vertical", "Vertical strip"),
                ],
            )
            .default_str("grid"),
        )
        .param(
            ParamSpec::integer("columns")
                .describe("Grid columns (empty = square-ish)")
                .range(1.0, 1000.0)
                .show_if("layout", &["grid"]),
        )
        .param(
            ParamSpec::integer("padding")
                .describe("Pixels between sprites")
                .range(0.0, 256.0)
                .default_int(2),
        )
        .param(
            ParamSpec::integer("cell_size")
                .describe("Resize every sprite to fit this square cell (empty = keep sizes)")
                .range(1.0, 4096.0)
                .advanced(),
        )
        .param(
            ParamSpec::text("class_prefix")
                .describe("CSS class prefix")
                .default_str("sprite")
                .advanced(),
        )
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        if files.is_empty() {
            return Err(ToolError::invalid("input", "select images"));
        }
        let mut imgs = load_all(ctx, files)?;
        if let Some(cs) = opt_dim(p, "cell_size") {
            imgs = imgs
                .iter()
                .map(|i| i.resize(cs, cs, image::imageops::FilterType::Lanczos3))
                .collect();
        }
        let pad = p.int_or("padding", 2) as u32;
        let n = imgs.len();
        let cell_w = imgs.iter().map(|i| i.width()).max().unwrap_or(1);
        let cell_h = imgs.iter().map(|i| i.height()).max().unwrap_or(1);
        let mut pos = vec![];
        let (w, h) = match p.str_or("layout", "grid") {
            "horizontal" => {
                let mut x = 0;
                for i in &imgs {
                    pos.push((x, 0));
                    x += i.width() + pad;
                }
                (x - pad, cell_h)
            }
            "vertical" => {
                let mut y = 0;
                for i in &imgs {
                    pos.push((0, y));
                    y += i.height() + pad;
                }
                (cell_w, y - pad)
            }
            _ => {
                let cols = opt_dim(p, "columns")
                    .map(|c| c as usize)
                    .unwrap_or_else(|| (n as f64).sqrt().ceil() as usize)
                    .clamp(1, n);
                let rows = n.div_ceil(cols);
                for k in 0..n {
                    pos.push((
                        (k % cols) as u32 * (cell_w + pad),
                        (k / cols) as u32 * (cell_h + pad),
                    ));
                }
                (
                    cols as u32 * (cell_w + pad) - pad,
                    rows as u32 * (cell_h + pad) - pad,
                )
            }
        };
        check_pixels(ctx, w as u64, h as u64)?;
        let mut sheet = RgbaImage::new(w, h);
        let prefix = css_ident(p.str_or("class_prefix", "sprite"));
        let mut css = format!(".{prefix} {{\n  display: inline-block;\n  background-image: url(\"sprite.png\");\n  background-repeat: no-repeat;\n}}\n");
        let mut frames = serde_json::Map::new();
        let mut used = std::collections::HashSet::new();
        for (k, (img, f)) in imgs.iter().zip(files).enumerate() {
            let (x, y) = pos[k];
            image::imageops::overlay(&mut sheet, &img.to_rgba8(), x as i64, y as i64);
            let mut id = css_ident(&f.stem());
            let base = id.clone();
            let mut j = 2;
            while !used.insert(id.clone()) {
                id = format!("{base}-{j}");
                j += 1;
            }
            let off = |v: u32| {
                if v == 0 {
                    "0".to_string()
                } else {
                    format!("-{v}px")
                }
            };
            css.push_str(&format!(
                ".{prefix}-{id} {{ width: {}px; height: {}px; background-position: {} {}; }}\n",
                img.width(),
                img.height(),
                off(x),
                off(y)
            ));
            frames.insert(id, json!({"file": f.name, "x": x, "y": y, "width": img.width(), "height": img.height()}));
        }
        let json_doc = json!({"image": "sprite.png", "width": w, "height": h, "frames": frames});
        let arts = vec![
            save_png(ctx, &sheet, "sprite.png")?,
            ctx.write_output("sprite.css", css.as_bytes())?,
            ctx.write_output(
                "sprite.json",
                serde_json::to_string_pretty(&json_doc)?.as_bytes(),
            )?,
        ];
        Ok(ToolOutput::files(arts)
            .with_data(json_doc)
            .with_text(css, "text/plain"))
    })
}

/// Apply blur / pixelate / solid fill to rectangular regions in place.
pub fn redact_regions(
    img: &mut RgbaImage,
    regions: &[(u32, u32, u32, u32)],
    mode: &str,
    strength: f32,
    color: Rgba<u8>,
) {
    for &(x, y, w, h) in regions {
        match mode {
            "fill" => {
                for yy in y..y + h {
                    for xx in x..x + w {
                        img.put_pixel(xx, yy, color);
                    }
                }
            }
            "pixelate" => {
                let block = ((w.max(h) as f32) * strength / 100.0).round().max(2.0) as u32;
                let mut by = y;
                while by < y + h {
                    let mut bx = x;
                    let bh = block.min(y + h - by);
                    while bx < x + w {
                        let bw = block.min(x + w - bx);
                        let mut acc = [0u64; 4];
                        for yy in by..by + bh {
                            for xx in bx..bx + bw {
                                let p = img.get_pixel(xx, yy);
                                for c in 0..4 {
                                    acc[c] += p[c] as u64;
                                }
                            }
                        }
                        let n = (bw * bh) as u64;
                        let avg = Rgba([
                            (acc[0] / n) as u8,
                            (acc[1] / n) as u8,
                            (acc[2] / n) as u8,
                            (acc[3] / n) as u8,
                        ]);
                        for yy in by..by + bh {
                            for xx in bx..bx + bw {
                                img.put_pixel(xx, yy, avg);
                            }
                        }
                        bx += bw;
                    }
                    by += bh;
                }
            }
            _ => {
                // Heavy blur: downscale + gaussian so faces/text are irrecoverable.
                let sub = image::imageops::crop_imm(img, x, y, w, h).to_image();
                let sigma = (w.max(h) as f32 * strength / 100.0).max(2.0);
                let small_w = (w / 8).max(1);
                let small_h = (h / 8).max(1);
                let small = image::imageops::resize(
                    &sub,
                    small_w,
                    small_h,
                    image::imageops::FilterType::Triangle,
                );
                let up =
                    image::imageops::resize(&small, w, h, image::imageops::FilterType::Triangle);
                let blurred = image::imageops::blur(&up, sigma / 4.0);
                image::imageops::replace(img, &blurred, x as i64, y as i64);
            }
        }
    }
}

pub fn redact_mode_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::choice(
            "mode",
            &[
                ("blur", "Blur"),
                ("pixelate", "Pixelate"),
                ("fill", "Solid box"),
            ],
        )
        .default_str("blur"),
        ParamSpec::number("strength")
            .describe("Blur radius / pixel block size relative to the region (%)")
            .range(1.0, 100.0)
            .default_num(25.0)
            .show_if("mode", &["blur", "pixelate"]),
        ParamSpec::color("color")
            .label("Box color")
            .default_str("#000000")
            .show_if("mode", &["fill"]),
    ]
}

fn redact() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.redact", "Redact Regions", Category::Image)
        .describe("Blur, pixelate or black out rectangular regions of an image (faces, license plates, personal data)")
        .keywords(&["redact", "blur", "pixelate", "censor", "anonymize", "hide", "privacy", "black box"])
        .param(image_param())
        .param(ParamSpec::json("regions").label("Regions").required().describe("JSON list of {x, y, width, height} rectangles in pixels").schema(json!({
            "type": "array",
            "items": {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "width": {"type": "number"}, "height": {"type": "number"}}, "required": ["x", "y", "width", "height"]}
        })))
        .params(redact_mode_params())
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .ui("face-redact");
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let img = load_image(ctx, f)?;
        let had_alpha = img.color().has_alpha();
        let mut rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let rects: Vec<_> = parse_regions(p.json("regions"), "regions")?
            .into_iter()
            .filter_map(|r| clip_rect(r, w, h))
            .collect();
        if rects.is_empty() {
            return Err(ToolError::invalid(
                "regions",
                "no region overlaps the image",
            ));
        }
        redact_regions(
            &mut rgba,
            &rects,
            p.str_or("mode", "blur"),
            p.num_or("strength", 25.0) as f32,
            color_param(p, "color", "#000000")?,
        );
        let out = if had_alpha {
            DynamicImage::ImageRgba8(rgba)
        } else {
            DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
        };
        let fmt = out_format(p, f)?;
        let a = save_output(
            ctx,
            &out,
            &suffixed(&f.name, "redacted", Some(fmt.ext())),
            fmt,
            &same_format_opts(p),
        )?;
        let regions: Vec<Value> = rects
            .iter()
            .map(|(x, y, w, h)| json!({"x": x, "y": y, "width": w, "height": h}))
            .collect();
        Ok(ToolOutput::files(vec![a]).with_data(json!({"regions": regions})))
    })
}

fn denoise() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.denoise", "Reduce Noise", Category::Image)
        .describe("Remove image noise with an edge-preserving bilateral filter or a median filter (great for salt-and-pepper noise)")
        .keywords(&["denoise", "noise reduction", "grain", "median", "bilateral", "smooth", "clean"])
        .param(images_param())
        .param(ParamSpec::choice("method", &[("bilateral", "Bilateral (edge preserving)"), ("median", "Median (salt & pepper)")]).default_str("bilateral"))
        .param(ParamSpec::integer("radius").range(1.0, 10.0).default_int(2))
        .param(ParamSpec::number("strength").describe("Color similarity range for the bilateral filter").range(1.0, 100.0).default_num(25.0).show_if("method", &["bilateral"]))
        .param(format_param())
        .param(quality_param().default_int(92).advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(900);
    sync_tool(spec, |ctx, p| {
        let radius = p.int_or("radius", 2).clamp(1, 10) as u32;
        let strength = p.num_or("strength", 25.0) as f32;
        let method = p.str_or("method", "bilateral").to_string();
        let arts = for_each_file(ctx, p.files("input"), "Denoising", |f| {
            let img = load_image(ctx, f)?;
            let alpha = img.color().has_alpha();
            let out = if method == "median" {
                if alpha {
                    DynamicImage::ImageRgba8(imageproc::filter::median_filter(
                        &img.to_rgba8(),
                        radius,
                        radius,
                    ))
                } else {
                    DynamicImage::ImageRgb8(imageproc::filter::median_filter(
                        &img.to_rgb8(),
                        radius,
                        radius,
                    ))
                }
            } else {
                use imageproc::filter::bilateral::{
                    bilateral_filter, GaussianEuclideanColorDistance,
                };
                let rgb = img.to_rgb8();
                let filtered: image::RgbImage = bilateral_filter(
                    &rgb,
                    radius as u8,
                    radius as f32,
                    GaussianEuclideanColorDistance::new(strength),
                );
                if alpha {
                    let mut rgba = img.to_rgba8();
                    for (o, s) in rgba.pixels_mut().zip(filtered.pixels()) {
                        o[0] = s[0];
                        o[1] = s[1];
                        o[2] = s[2];
                    }
                    DynamicImage::ImageRgba8(rgba)
                } else {
                    DynamicImage::ImageRgb8(filtered)
                }
            };
            let fmt = out_format(p, f)?;
            save_output(
                ctx,
                &out,
                &suffixed(&f.name, "denoised", Some(fmt.ext())),
                fmt,
                &same_format_opts(p),
            )
        })?;
        Ok(ToolOutput::files(arts))
    })
}
