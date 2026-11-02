//! Format conversion, resizing, compression, info, icons and favicons.

use crate::util::*;
use dogubako_core::prelude::*;
use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::{AnimationDecoder, DynamicImage, GenericImageView, ImageReader, Rgba, RgbaImage};
use std::io::BufReader;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        convert(),
        resize(),
        thumbnail(),
        compress(),
        info(),
        colorspace(),
        gif_frames(),
        icons(),
        favicon(),
    ]
}

fn format_choices(with_auto: bool) -> Vec<(&'static str, &'static str)> {
    let mut v = vec![];
    if with_auto {
        v.push(("same", "Same as input"));
    }
    v.extend_from_slice(NATIVE_FORMATS);
    v.extend_from_slice(MAGICK_FORMATS);
    v
}

fn save_opts(p: &Params) -> ToolResult<SaveOpts> {
    Ok(SaveOpts {
        quality: p.int_or("quality", 85).clamp(1, 100) as u8,
        background: color_param(p, "background", "#ffffff")?,
        lossless: p.bool("lossless"),
    })
}

fn convert() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.convert", "Convert Images", Category::Image)
        .describe("Convert images between PNG, JPEG, WebP, AVIF, GIF, BMP, TIFF, ICO, TGA, QOI and more, in bulk")
        .keywords(&["convert", "format", "png to jpg", "jpg to png", "webp", "avif", "heic", "batch", "transcode"])
        .param(images_param())
        .param(ParamSpec::choice("format", &format_choices(false)).label("Output format").default_str("png"))
        .param(quality_param().show_if("format", &["jpg", "webp", "avif", "heic", "jxl", "jp2"]))
        .param(ParamSpec::boolean("lossless").describe("Lossless WebP encoding").show_if("format", &["webp"]).default_bool(false))
        .param(ParamSpec::color("background").describe("Background used when flattening transparency (e.g. PNG → JPEG)").default_str("#ffffff").advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let fmt = OutFmt::parse(p.str("format")?)
            .ok_or_else(|| ToolError::invalid("format", "unsupported format"))?;
        if matches!(fmt, OutFmt::Magick(_)) && ctx.deps().path("imagemagick").is_none() {
            return Err(ToolError::MissingDependency {
                dependency: "imagemagick".into(),
                hint: format!("writing {} requires ImageMagick", fmt.ext()),
            });
        }
        let opts = save_opts(p)?;
        let files = p.files("input");
        let mut warnings = vec![];
        let arts = for_each_file(ctx, files, "Converting", |f| {
            let img = load_image(ctx, f)?;
            if fmt == OutFmt::Ico && (img.width() > 256 || img.height() > 256) {
                warnings.push(format!("{}: downscaled to fit 256×256 (ICO limit)", f.name));
            }
            if !fmt.supports_alpha() && img.color().has_alpha() {
                warnings.push(format!(
                    "{}: transparency flattened onto {}",
                    f.name,
                    hex_rgba(opts.background)
                ));
            }
            save_output(ctx, &img, &with_extension(&f.name, fmt.ext()), fmt, &opts)
        })?;
        let mut out =
            ToolOutput::files(arts.clone()).with_data(json!({"table": size_table(files, &arts)}));
        for w in warnings {
            out = out.warn(w);
        }
        Ok(out)
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Contain,
    Cover,
    Fill,
    Exact,
}

impl Fit {
    pub fn parse(s: &str) -> Fit {
        match s {
            "cover" => Fit::Cover,
            "fill" => Fit::Fill,
            "exact" | "stretch" => Fit::Exact,
            _ => Fit::Contain,
        }
    }
}

pub const FITS: &[(&str, &str)] = &[
    ("contain", "Contain (fit inside, keep aspect)"),
    ("cover", "Cover (fill box, crop overflow)"),
    ("fill", "Fill (fit inside, pad to exact size)"),
    ("exact", "Exact (stretch to size)"),
];

pub struct ResizeSpec {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub percent: Option<f64>,
    pub fit: Fit,
    pub filter: image::imageops::FilterType,
    pub no_upscale: bool,
    pub background: Rgba<u8>,
}

/// Compute and apply a resize. Returns the new image.
pub fn resize_image(
    ctx: &ToolContext,
    img: &DynamicImage,
    s: &ResizeSpec,
) -> ToolResult<DynamicImage> {
    let (w0, h0) = (img.width() as f64, img.height() as f64);
    let (mut tw, mut th) = match (s.percent, s.width, s.height) {
        (Some(pc), _, _) => (
            (w0 * pc / 100.0).round().max(1.0),
            (h0 * pc / 100.0).round().max(1.0),
        ),
        (None, Some(w), Some(h)) => (w as f64, h as f64),
        (None, Some(w), None) => (w as f64, (h0 * w as f64 / w0).round().max(1.0)),
        (None, None, Some(h)) => ((w0 * h as f64 / h0).round().max(1.0), h as f64),
        (None, None, None) => {
            return Err(ToolError::input(
                "specify a width, a height or a percentage",
            ))
        }
    };
    let fit = if s.percent.is_some() || s.width.is_none() || s.height.is_none() {
        Fit::Exact
    } else {
        s.fit
    };
    if s.no_upscale && fit == Fit::Exact {
        tw = tw.min(w0);
        th = th.min(h0);
    }
    check_pixels(ctx, tw as u64, th as u64)?;
    let (tw, th) = (tw as u32, th as u32);
    Ok(match fit {
        Fit::Exact => img.resize_exact(tw, th, s.filter),
        Fit::Contain | Fit::Fill => {
            let mut k = (tw as f64 / w0).min(th as f64 / h0);
            if s.no_upscale {
                k = k.min(1.0);
            }
            let (nw, nh) = (
                (w0 * k).round().max(1.0) as u32,
                (h0 * k).round().max(1.0) as u32,
            );
            let r = img.resize_exact(nw, nh, s.filter);
            if fit == Fit::Fill {
                let mut canvas = RgbaImage::from_pixel(tw, th, s.background);
                let (x, y) = anchor("center", tw, th, nw, nh, 0);
                image::imageops::overlay(&mut canvas, &r.to_rgba8(), x, y);
                DynamicImage::ImageRgba8(canvas)
            } else {
                r
            }
        }
        Fit::Cover => {
            let mut k = (tw as f64 / w0).max(th as f64 / h0);
            if s.no_upscale {
                k = k.min(1.0);
            }
            // Crop the source to the visible region first, then scale only
            // that: scaling the whole image can need an intermediate far
            // larger than the output (10000x1 covering 1x10000 would be
            // 100000000x10000 pixels).
            let cw = (tw as f64).min((w0 * k).round().max(1.0));
            let ch = (th as f64).min((h0 * k).round().max(1.0));
            let sw = (cw / k).round().clamp(1.0, w0);
            let sh = (ch / k).round().clamp(1.0, h0);
            let (sx, sy) = (((w0 - sw) / 2.0) as u32, ((h0 - sh) / 2.0) as u32);
            img.crop_imm(sx, sy, sw as u32, sh as u32)
                .resize_exact(cw as u32, ch as u32, s.filter)
        }
    })
}

fn resize() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.resize", "Resize Images", Category::Image)
        .describe("Resize images in bulk by width, height or percentage with contain/cover/fill/exact fit modes")
        .keywords(&["resize", "scale", "shrink", "enlarge", "dimensions", "batch", "downscale"])
        .param(images_param())
        .param(ParamSpec::integer("width").describe("Target width in pixels").min(1.0).max(100000.0))
        .param(ParamSpec::integer("height").describe("Target height in pixels").min(1.0).max(100000.0))
        .param(ParamSpec::number("percent").describe("Scale by percentage instead of pixel size").range(0.1, 10000.0))
        .param(ParamSpec::choice("fit", FITS).describe("How to fit when both width and height are given").default_str("contain"))
        .param(ParamSpec::choice("filter", FILTERS).default_str("lanczos").advanced())
        .param(ParamSpec::boolean("no_upscale").label("Don't enlarge").describe("Never make images larger than the original").default_bool(false))
        .param(ParamSpec::choice("format", &format_choices(true)).label("Output format").default_str("same").advanced())
        .param(quality_param().advanced())
        .param(ParamSpec::color("background").describe("Padding color for 'fill' and flattening for JPEG").default_str("#ffffff").advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let rs = ResizeSpec {
            width: opt_dim(p, "width"),
            height: opt_dim(p, "height"),
            percent: p.opt_num("percent"),
            fit: Fit::parse(p.str_or("fit", "contain")),
            filter: filter_type(p.str_or("filter", "lanczos")),
            no_upscale: p.bool("no_upscale"),
            background: color_param(p, "background", "#ffffff")?,
        };
        if rs.width.is_none() && rs.height.is_none() && rs.percent.is_none() {
            return Err(ToolError::invalid(
                "width",
                "specify a width, a height or a percentage",
            ));
        }
        let opts = save_opts(p)?;
        let files = p.files("input");
        let mut rows = vec![];
        let arts = for_each_file(ctx, files, "Resizing", |f| {
            let fmt = pick_format(p.str_or("format", "same"), &f.name)?;
            let img = load_image(ctx, f)?;
            let out = resize_image(ctx, &img, &rs)?;
            rows.push(json!([
                f.name,
                format!("{}×{}", img.width(), img.height()),
                format!("{}×{}", out.width(), out.height())
            ]));
            save_output(
                ctx,
                &out,
                &suffixed(
                    &f.name,
                    &format!("{}x{}", out.width(), out.height()),
                    Some(fmt.ext()),
                ),
                fmt,
                &opts,
            )
        })?;
        Ok(ToolOutput::files(arts).with_data(
            json!({"table": {"columns": ["Input", "Original", "Resized"], "rows": rows}}),
        ))
    })
}

fn thumbnail() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.thumbnail", "Create Thumbnails", Category::Image)
        .describe(
            "Generate small thumbnails for many images at once (square crop or fit inside a box)",
        )
        .keywords(&["thumbnail", "thumb", "preview", "small", "batch"])
        .param(images_param())
        .param(
            ParamSpec::integer("size")
                .describe("Maximum edge length in pixels")
                .range(8.0, 4096.0)
                .default_int(256),
        )
        .param(
            ParamSpec::boolean("square")
                .describe("Center-crop to a square")
                .default_bool(false),
        )
        .param(
            ParamSpec::choice(
                "format",
                &[("jpg", "JPEG"), ("png", "PNG"), ("webp", "WebP")],
            )
            .default_str("jpg"),
        )
        .param(quality_param().default_int(80).advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let size = p.int_or("size", 256) as u32;
        let fmt = OutFmt::parse(p.str_or("format", "jpg")).unwrap_or(OutFmt::Jpeg);
        let opts = SaveOpts {
            quality: p.int_or("quality", 80) as u8,
            ..Default::default()
        };
        let square = p.bool("square");
        let arts = for_each_file(ctx, p.files("input"), "Thumbnailing", |f| {
            let img = load_image(ctx, f)?;
            let t = if square {
                resize_image(
                    ctx,
                    &img,
                    &ResizeSpec {
                        width: Some(size),
                        height: Some(size),
                        percent: None,
                        fit: Fit::Cover,
                        filter: image::imageops::FilterType::Triangle,
                        no_upscale: true,
                        background: opts.background,
                    },
                )?
            } else if img.width() > size || img.height() > size {
                img.thumbnail(size, size)
            } else {
                img
            };
            save_output(
                ctx,
                &t,
                &suffixed(&f.name, "thumb", Some(fmt.ext())),
                fmt,
                &opts,
            )
        })?;
        Ok(ToolOutput::files(arts))
    })
}

fn compress() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.compress", "Compress Images", Category::Image)
        .describe("Reduce image file size: lossy JPEG/WebP/AVIF re-encoding and lossless PNG optimization (oxipng), in bulk")
        .keywords(&["compress", "optimize", "reduce size", "smaller", "oxipng", "tinypng", "shrink", "batch"])
        .param(images_param())
        .param(quality_param().default_int(75))
        .param(ParamSpec::choice("format", &[("same", "Same as input"), ("jpg", "JPEG"), ("webp", "WebP"), ("avif", "AVIF"), ("png", "PNG (lossless)")]).default_str("same"))
        .param(ParamSpec::integer("max_dimension").describe("Also downscale so the longest edge is at most this many pixels").min(1.0).max(100000.0))
        .param(ParamSpec::integer("png_level").describe("Lossless PNG optimization effort (0–6)").range(0.0, 6.0).default_int(2).advanced())
        .param(ParamSpec::boolean("keep_if_larger").label("Keep original if smaller").describe("Return the original file when re-encoding would make it larger").default_bool(true))
        .output(OutputKind::Files)
        .progress()
        .timeout(900);
    sync_tool(spec, |ctx, p| {
        let q = p.int_or("quality", 75) as u8;
        let level = p.int_or("png_level", 2).clamp(0, 6) as u8;
        let max_dim = opt_dim(p, "max_dimension");
        let files = p.files("input");
        let mut notes = vec![];
        let arts = for_each_file(ctx, files, "Compressing", |f| {
            let fmt = match p.str_or("format", "same") {
                "same" => match OutFmt::from_name(&f.name) {
                    Some(f @ (OutFmt::Jpeg | OutFmt::Webp | OutFmt::Avif | OutFmt::Png)) => f,
                    _ => OutFmt::Png,
                },
                other => OutFmt::parse(other).unwrap_or(OutFmt::Png),
            };
            let mut img = load_image(ctx, f)?;
            let mut resized = false;
            if let Some(m) = max_dim {
                if img.width() > m || img.height() > m {
                    img = img.resize(m, m, image::imageops::FilterType::Lanczos3);
                    resized = true;
                }
            }
            let mut bytes = encode(
                ctx,
                &img,
                fmt,
                &SaveOpts {
                    quality: q,
                    ..Default::default()
                },
            )?;
            if fmt == OutFmt::Png {
                let mut o = oxipng::Options::from_preset(level);
                o.strip = oxipng::StripChunks::Safe;
                bytes = oxipng::optimize_from_memory(&bytes, &o)
                    .map_err(|e| ToolError::failed(format!("PNG optimization failed: {e}")))?;
                // Losslessly optimizing the original PNG directly can beat re-encoding.
                if !resized && f.extension() == "png" {
                    if let Ok(orig) = std::fs::read(&f.path) {
                        if let Ok(b2) = oxipng::optimize_from_memory(&orig, &o) {
                            if b2.len() < bytes.len() {
                                bytes = b2;
                            }
                        }
                    }
                }
            }
            let name = suffixed(&f.name, "compressed", Some(fmt.ext()));
            let same_fmt = OutFmt::from_name(&f.name) == Some(fmt);
            if p.bool("keep_if_larger") && same_fmt && !resized && bytes.len() as u64 >= f.size() {
                notes.push(format!("{}: already optimal, original kept", f.name));
                let orig = std::fs::read(&f.path)?;
                return ctx.write_output(&name, &orig);
            }
            ctx.write_output(&name, &bytes)
        })?;
        let total_in: u64 = files.iter().map(|f| f.size()).sum();
        let total_out: u64 = arts.iter().map(|a| a.size).sum();
        let mut out = ToolOutput::files(arts.clone()).with_data(json!({
            "table": size_table(files, &arts),
            "total_before": total_in,
            "total_after": total_out,
            "saved_percent": if total_in > 0 { ((total_in as f64 - total_out as f64) / total_in as f64 * 1000.0).round() / 10.0 } else { 0.0 },
        }));
        for n in notes {
            out = out.warn(n);
        }
        Ok(out)
    })
}

pub fn describe_file(ctx: &ToolContext, f: &InputFile) -> ToolResult<Value> {
    let fmt = ImageReader::open(&f.path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.format());
    let fmt_name = match fmt {
        Some(x) => format!("{x:?}").to_uppercase(),
        None if looks_like_svg(&f.path) => "SVG".into(),
        None => f.extension().to_uppercase(),
    };
    let img = load_image(ctx, f)?;
    let c = img.color();
    let (w, h) = img.dimensions();
    let frames = count_frames(f);
    let dpi = read_dpi(f);
    Ok(json!({
        "file": f.name,
        "format": fmt_name,
        "width": w,
        "height": h,
        "megapixels": ((w as f64 * h as f64) / 1e4).round() / 100.0,
        "aspect_ratio": ratio_string(w as u64, h as u64),
        "aspect_decimal": ((w as f64 / h as f64) * 1000.0).round() / 1000.0,
        "color_type": format!("{c:?}"),
        "channels": c.channel_count(),
        "bit_depth": c.bits_per_pixel() / c.channel_count() as u16,
        "has_alpha": c.has_alpha(),
        "frames": frames,
        "dpi": dpi,
        "file_size": f.size(),
        "file_size_human": human_size(f.size()),
    }))
}

fn count_frames(f: &InputFile) -> Option<usize> {
    let r = BufReader::new(std::fs::File::open(&f.path).ok()?);
    match f.extension().as_str() {
        "gif" => image::codecs::gif::GifDecoder::new(r)
            .ok()
            .map(|d| d.into_frames().take(10000).count()),
        _ => None,
    }
}

/// DPI from EXIF (JPEG/TIFF/WebP/PNG eXIf) or the PNG pHYs chunk.
pub fn read_dpi(f: &InputFile) -> Option<(f64, f64)> {
    if let Ok(file) = std::fs::File::open(&f.path) {
        let mut br = BufReader::new(file);
        if let Ok(ex) = exif::Reader::new().read_from_container(&mut br) {
            let get = |t| {
                ex.get_field(t, exif::In::PRIMARY)
                    .and_then(|fl| match &fl.value {
                        exif::Value::Rational(v) if !v.is_empty() => Some(v[0].to_f64()),
                        _ => None,
                    })
            };
            let unit = ex
                .get_field(exif::Tag::ResolutionUnit, exif::In::PRIMARY)
                .and_then(|fl| fl.value.get_uint(0))
                .unwrap_or(2);
            if let (Some(x), Some(y)) = (get(exif::Tag::XResolution), get(exif::Tag::YResolution)) {
                let k = if unit == 3 { 2.54 } else { 1.0 };
                return Some((x * k, y * k));
            }
        }
    }
    if f.extension() == "png" {
        let data = std::fs::read(&f.path).ok()?;
        let png = img_parts::png::Png::from_bytes(data.into()).ok()?;
        let c = png.chunk_by_type(*b"pHYs")?;
        let d = c.contents();
        if d.len() >= 9 && d[8] == 1 {
            let x = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) as f64 * 0.0254;
            let y = u32::from_be_bytes([d[4], d[5], d[6], d[7]]) as f64 * 0.0254;
            return Some(((x * 10.0).round() / 10.0, (y * 10.0).round() / 10.0));
        }
    }
    None
}

fn info() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.info", "Image Info", Category::Image)
        .describe("Show image dimensions, format, color type, bit depth, transparency, DPI, frame count, file size and aspect ratio")
        .keywords(&["info", "dimensions", "size", "resolution", "inspect", "properties", "bit depth"])
        .param(images_param())
        .output(OutputKind::Data);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let infos: Vec<Value> = files
            .iter()
            .map(|f| describe_file(ctx, f))
            .collect::<ToolResult<_>>()?;
        if infos.len() == 1 {
            return Ok(ToolOutput::data(infos[0].clone()));
        }
        let rows: Vec<Value> = infos
            .iter()
            .map(|i| {
                json!([
                    i["file"],
                    i["format"],
                    i["width"],
                    i["height"],
                    i["aspect_ratio"],
                    i["color_type"],
                    i["has_alpha"],
                    i["file_size_human"]
                ])
            })
            .collect();
        Ok(ToolOutput::data(
            json!({"table": {"columns": ["File", "Format", "Width", "Height", "Aspect", "Color", "Alpha", "Size"], "rows": rows}, "images": infos}),
        ))
    })
}

fn colorspace() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.colorspace", "Convert Color Space", Category::Image)
        .describe("Convert images between sRGB, CMYK, Lab, grayscale and linear RGB using ImageMagick, optionally with an ICC profile")
        .keywords(&["cmyk", "rgb", "lab", "colorspace", "color space", "print", "grayscale", "icc"])
        .param(images_param())
        .param(ParamSpec::choice("colorspace", &[("sRGB", "sRGB"), ("CMYK", "CMYK (print)"), ("Lab", "CIE Lab"), ("Gray", "Grayscale"), ("RGB", "Linear RGB"), ("HSL", "HSL"), ("YCbCr", "YCbCr")]).default_str("CMYK"))
        .param(ParamSpec::choice("format", &[("auto", "Auto (TIFF for CMYK/Lab)"), ("tiff", "TIFF"), ("jpg", "JPEG"), ("png", "PNG"), ("same", "Same as input")]).default_str("auto"))
        .param(ParamSpec::file("profile").label("ICC profile").describe("Optional target ICC profile (.icc/.icm) for accurate conversion").accept(&[".icc", ".icm"]).advanced())
        .param(quality_param().advanced())
        .requires(&["imagemagick"])
        .output(OutputKind::Files)
        .progress()
        .timeout(600);
    sync_tool(spec, |ctx, p| {
        let cs = p.str("colorspace")?.to_string();
        let profile = p
            .opt_file("profile")
            .map(|f| safe_input_copy(ctx, &f.path, "profile"))
            .transpose()?;
        let arts = for_each_file(ctx, p.files("input"), "Converting", |f| {
            let ext = match p.str_or("format", "auto") {
                "auto" => match cs.as_str() {
                    "CMYK" if matches!(f.extension().as_str(), "jpg" | "jpeg") => "jpg".to_string(),
                    "CMYK" | "Lab" | "HSL" | "YCbCr" => "tiff".into(),
                    _ => match f.extension().as_str() {
                        e @ ("png" | "jpg" | "jpeg" | "tif" | "tiff" | "webp") => e.into(),
                        _ => "png".into(),
                    },
                },
                "same" => match f.extension().as_str() {
                    e @ ("png" | "jpg" | "jpeg" | "tif" | "tiff" | "webp" | "gif" | "bmp") => {
                        e.into()
                    }
                    _ => "png".into(),
                },
                o => o.into(),
            };
            if ext == "png" && cs == "CMYK" {
                return Err(ToolError::invalid(
                    "format",
                    "PNG cannot store CMYK; choose TIFF or JPEG",
                ));
            }
            let src = safe_input_copy(ctx, &f.path, "cs-in")?;
            let tmp_out = ctx.temp_path(&format!(
                "cs-out-{}.{ext}",
                dogubako_core::util::random_id(4)
            ));
            let mut target = std::ffi::OsString::from(format!("{ext}:"));
            target.push(tmp_out.as_os_str());
            let mut cmd = ctx.command("imagemagick")?.arg(&src).arg("-auto-orient");
            if let Some(pr) = &profile {
                cmd = cmd.arg("-profile").arg(pr);
            } else {
                cmd = cmd.arg("-colorspace").arg(&cs);
            }
            cmd = cmd
                .arg("-quality")
                .arg(p.int_or("quality", 92).to_string())
                .arg(target);
            run_exec(cmd)?;
            let out = ctx.output_path(&suffixed(&f.name, &cs.to_lowercase(), Some(&ext)));
            std::fs::rename(&tmp_out, &out)
                .or_else(|_| std::fs::copy(&tmp_out, &out).map(|_| ()))?;
            ctx.artifact(&out)
        })?;
        Ok(ToolOutput::files(arts))
    })
}

fn gif_frames() -> Arc<dyn Tool> {
    let spec = ToolSpec::new(
        "image.gif_frames",
        "Extract Animation Frames",
        Category::Image,
    )
    .describe(
        "Extract the frames of an animated GIF, WebP or APNG as PNG images, with their delays",
    )
    .keywords(&[
        "gif",
        "frames",
        "extract",
        "animation",
        "split gif",
        "apng",
        "webp",
    ])
    .param(
        ParamSpec::file("input")
            .label("Animation")
            .required()
            .primary()
            .accept(&[".gif", ".webp", ".png", ".apng"]),
    )
    .param(
        ParamSpec::integer("every")
            .label("Every Nth frame")
            .describe("Keep every Nth frame")
            .range(1.0, 1000.0)
            .default_int(1),
    )
    .param(
        ParamSpec::integer("max_frames")
            .describe("Stop after this many extracted frames")
            .range(1.0, 2000.0)
            .default_int(300),
    )
    .output(OutputKind::Files)
    .progress()
    .timeout(600);
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let every = p.int_or("every", 1).max(1) as usize;
        let max = p.int_or("max_frames", 300).max(1) as usize;
        let (w, h) = ImageReader::open(&f.path)?
            .with_guessed_format()?
            .into_dimensions()
            .map_err(|e| ToolError::invalid("input", e.to_string()))?;
        check_pixels(ctx, w as u64, h as u64)?;
        let reader = || -> ToolResult<BufReader<std::fs::File>> {
            Ok(BufReader::new(std::fs::File::open(&f.path)?))
        };
        let fmt = ImageReader::open(&f.path)?.with_guessed_format()?.format();
        let frames: image::Frames = match fmt {
            Some(image::ImageFormat::Gif) => image::codecs::gif::GifDecoder::new(reader()?)
                .map_err(|e| ToolError::invalid("input", e.to_string()))?
                .into_frames(),
            Some(image::ImageFormat::WebP) => image::codecs::webp::WebPDecoder::new(reader()?)
                .map_err(|e| ToolError::invalid("input", e.to_string()))?
                .into_frames(),
            Some(image::ImageFormat::Png) => {
                let d = image::codecs::png::PngDecoder::new(reader()?)
                    .map_err(|e| ToolError::invalid("input", e.to_string()))?;
                if !d.is_apng().unwrap_or(false) {
                    return Err(ToolError::invalid("input", "this PNG is not animated"));
                }
                d.apng()
                    .map_err(|e| ToolError::invalid("input", e.to_string()))?
                    .into_frames()
            }
            _ => {
                return Err(ToolError::invalid(
                    "input",
                    "expected an animated GIF, WebP or PNG",
                ))
            }
        };
        let stem = f.stem();
        let mut arts = vec![];
        let mut rows = vec![];
        for (i, fr) in frames.enumerate() {
            ctx.check_cancelled()?;
            let fr =
                fr.map_err(|e| ToolError::invalid("input", format!("frame {}: {e}", i + 1)))?;
            if i % every != 0 {
                continue;
            }
            let (n, d) = fr.delay().numer_denom_ms();
            let ms = n.checked_div(d).unwrap_or(0);
            let buf = fr.into_buffer();
            let a = save_png(ctx, &buf, &format!("{stem}-frame-{:04}.png", i + 1))?;
            rows.push(json!([i + 1, ms, a.name]));
            arts.push(a);
            ctx.progress(
                (arts.len() as f32 / max as f32).min(0.99),
                format!("frame {}", i + 1),
            );
            if arts.len() >= max {
                break;
            }
        }
        if arts.is_empty() {
            return Err(ToolError::invalid("input", "no frames found"));
        }
        let count = arts.len();
        Ok(ToolOutput::files(arts).with_data(json!({"frames": count, "width": w, "height": h, "table": {"columns": ["Frame", "Delay (ms)", "File"], "rows": rows}})))
    })
}

/// Source that can be rendered as a square icon of any size.
enum IconSource {
    Raster(DynamicImage),
    Svg(Box<resvg::usvg::Tree>),
}

impl IconSource {
    fn load(ctx: &ToolContext, f: &InputFile) -> ToolResult<Self> {
        if looks_like_svg(&f.path) {
            let data = std::fs::read(&f.path)?;
            return crate::svg::parse_svg(&data)
                .map(|t| IconSource::Svg(Box::new(t)))
                .map_err(|e| ToolError::invalid("input", e));
        }
        Ok(IconSource::Raster(load_image(ctx, f)?))
    }

    /// Square icon of `size` px; content fits inside with `padding` fraction and optional background.
    fn render(
        &self,
        ctx: &ToolContext,
        size: u32,
        mode: &str,
        padding: f32,
        bg: Rgba<u8>,
    ) -> ToolResult<RgbaImage> {
        let inner = ((size as f32) * (1.0 - 2.0 * padding)).round().max(1.0) as u32;
        let mut canvas = RgbaImage::from_pixel(size, size, bg);
        let content: RgbaImage = match self {
            IconSource::Svg(tree) => {
                let s = tree.size();
                let k = (inner as f32 / s.width()).min(inner as f32 / s.height());
                let (w, h) = (
                    (s.width() * k).round().max(1.0) as u32,
                    (s.height() * k).round().max(1.0) as u32,
                );
                crate::svg::render_tree(ctx, tree, w, h, None)?
            }
            IconSource::Raster(img) => {
                let fit = match mode {
                    "crop" => Fit::Cover,
                    "stretch" => Fit::Exact,
                    _ => Fit::Contain,
                };
                resize_image(
                    ctx,
                    img,
                    &ResizeSpec {
                        width: Some(inner),
                        height: Some(inner),
                        percent: None,
                        fit,
                        filter: image::imageops::FilterType::Lanczos3,
                        no_upscale: false,
                        background: bg,
                    },
                )?
                .to_rgba8()
            }
        };
        let (x, y) = anchor("center", size, size, content.width(), content.height(), 0);
        composite(&mut canvas, &content, x, y, 1.0, Blend::Normal);
        Ok(canvas)
    }
}

pub fn ico_bytes(images: &[RgbaImage]) -> ToolResult<Vec<u8>> {
    let mut pngs = vec![];
    for img in images {
        let mut b = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img.clone())
            .write_to(&mut b, image::ImageFormat::Png)
            .map_err(|e| ToolError::failed(e.to_string()))?;
        pngs.push((b.into_inner(), img.width(), img.height()));
    }
    let frames: Vec<IcoFrame> = pngs
        .iter()
        .map(|(b, w, h)| {
            IcoFrame::with_encoded(b.as_slice(), *w, *h, image::ExtendedColorType::Rgba8)
        })
        .collect::<Result<_, _>>()
        .map_err(|e| ToolError::failed(format!("ICO frame: {e}")))?;
    let mut out = Vec::new();
    IcoEncoder::new(&mut out)
        .encode_images(&frames)
        .map_err(|e| ToolError::failed(format!("ICO encoding: {e}")))?;
    Ok(out)
}

const ICON_SIZES: &[(&str, &str)] = &[
    ("16", "16"),
    ("24", "24"),
    ("32", "32"),
    ("48", "48"),
    ("64", "64"),
    ("96", "96"),
    ("128", "128"),
    ("180", "180"),
    ("192", "192"),
    ("256", "256"),
    ("512", "512"),
    ("1024", "1024"),
];

fn icons() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.icons", "Generate Icons", Category::Image)
        .describe("Generate square PNG icons in many sizes plus a multi-resolution .ico from one image or SVG")
        .keywords(&["icon", "ico", "app icon", "sizes", "png icons", "windows icon"])
        .param(image_param())
        .param(ParamSpec::multi_choice("sizes", ICON_SIZES).default_value(json!(["16", "32", "48", "64", "128", "256", "512"])))
        .param(ParamSpec::boolean("ico").label("Also create .ico").describe("Multi-size Windows icon (sizes up to 256)").default_bool(true))
        .param(ParamSpec::choice("mode", &[("pad", "Fit and pad to square"), ("crop", "Center-crop to square"), ("stretch", "Stretch")]).default_str("pad"))
        .param(ParamSpec::number("padding").describe("Inner padding as a fraction of the size (0–0.4)").range(0.0, 0.4).default_num(0.0).advanced())
        .param(ParamSpec::color("background").describe("Background (default transparent)").default_str("#00000000").advanced())
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let src = IconSource::load(ctx, f)?;
        let mut sizes: Vec<u32> = p
            .list("sizes")
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        sizes.sort_unstable();
        sizes.dedup();
        if sizes.is_empty() {
            return Err(ToolError::invalid("sizes", "choose at least one size"));
        }
        let bg = color_param(p, "background", "#00000000")?;
        let pad = p.num_or("padding", 0.0) as f32;
        let mode = p.str_or("mode", "pad");
        let stem = f.stem();
        let mut arts = vec![];
        let mut ico_imgs = vec![];
        for (i, s) in sizes.iter().enumerate() {
            ctx.progress(i as f32 / sizes.len() as f32, format!("{s}×{s}"));
            let img = src.render(ctx, *s, mode, pad, bg)?;
            arts.push(save_png(ctx, &img, &format!("{stem}-{s}x{s}.png"))?);
            if *s <= 256 {
                ico_imgs.push(img);
            }
        }
        if p.bool("ico") {
            if ico_imgs.is_empty() {
                for s in [16u32, 32, 48, 256] {
                    ico_imgs.push(src.render(ctx, s, mode, pad, bg)?);
                }
            }
            arts.push(ctx.write_output(&format!("{stem}.ico"), &ico_bytes(&ico_imgs)?)?);
        }
        Ok(ToolOutput::files(arts))
    })
}

fn favicon() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.favicon", "Favicon Generator", Category::Image)
        .describe("Create a complete favicon package: favicon.ico, PNG favicons, Apple touch icon, Android icons, site.webmanifest and HTML tags")
        .keywords(&["favicon", "ico", "apple-touch-icon", "webmanifest", "website icon", "pwa"])
        .param(image_param())
        .param(ParamSpec::text("app_name").describe("Name used in site.webmanifest").default_str("My Site"))
        .param(ParamSpec::text("short_name").describe("Short name for home screens").advanced())
        .param(ParamSpec::color("theme_color").default_str("#ffffff"))
        .param(ParamSpec::color("background_color").describe("Background of the Apple touch icon and PWA splash").default_str("#ffffff"))
        .param(ParamSpec::number("apple_padding").describe("Padding around the Apple touch icon content (fraction)").range(0.0, 0.4).default_num(0.1).advanced())
        .param(ParamSpec::text("path_prefix").describe("URL prefix for the icon links, e.g. /static/").default_str("/").advanced())
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let src = IconSource::load(ctx, f)?;
        let clear = Rgba([0, 0, 0, 0]);
        let bgc = color_param(p, "background_color", "#ffffff")?;
        let mut arts = vec![];
        ctx.progress(0.1, "favicon.ico");
        let ico: Vec<RgbaImage> = [16u32, 32, 48]
            .iter()
            .map(|s| src.render(ctx, *s, "pad", 0.0, clear))
            .collect::<ToolResult<_>>()?;
        arts.push(ctx.write_output("favicon.ico", &ico_bytes(&ico)?)?);
        arts.push(save_png(ctx, &ico[0], "favicon-16x16.png")?);
        arts.push(save_png(ctx, &ico[1], "favicon-32x32.png")?);
        ctx.progress(0.4, "apple-touch-icon");
        let apple = src.render(
            ctx,
            180,
            "pad",
            p.num_or("apple_padding", 0.1) as f32,
            Rgba([bgc[0], bgc[1], bgc[2], 255]),
        )?;
        arts.push(save_png(ctx, &apple, "apple-touch-icon.png")?);
        ctx.progress(0.6, "android icons");
        arts.push(save_png(
            ctx,
            &src.render(ctx, 192, "pad", 0.0, clear)?,
            "android-chrome-192x192.png",
        )?);
        arts.push(save_png(
            ctx,
            &src.render(ctx, 512, "pad", 0.0, clear)?,
            "android-chrome-512x512.png",
        )?);
        let prefix = {
            let s = p.str_or("path_prefix", "/");
            if s.ends_with('/') {
                s.to_string()
            } else {
                format!("{s}/")
            }
        };
        let name = p.str_or("app_name", "My Site");
        let theme = p.str_or("theme_color", "#ffffff");
        let manifest = json!({
            "name": name,
            "short_name": p.opt_str("short_name").unwrap_or(name),
            "icons": [
                {"src": format!("{prefix}android-chrome-192x192.png"), "sizes": "192x192", "type": "image/png"},
                {"src": format!("{prefix}android-chrome-512x512.png"), "sizes": "512x512", "type": "image/png"}
            ],
            "theme_color": theme,
            "background_color": p.str_or("background_color", "#ffffff"),
            "display": "standalone"
        });
        arts.push(ctx.write_output(
            "site.webmanifest",
            serde_json::to_string_pretty(&manifest)?.as_bytes(),
        )?);
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;")
        };
        let pre = esc(&prefix);
        let html = format!(
            "<link rel=\"icon\" href=\"{pre}favicon.ico\" sizes=\"48x48\">\n<link rel=\"icon\" type=\"image/png\" sizes=\"32x32\" href=\"{pre}favicon-32x32.png\">\n<link rel=\"icon\" type=\"image/png\" sizes=\"16x16\" href=\"{pre}favicon-16x16.png\">\n<link rel=\"apple-touch-icon\" sizes=\"180x180\" href=\"{pre}apple-touch-icon.png\">\n<link rel=\"manifest\" href=\"{pre}site.webmanifest\">\n<meta name=\"theme-color\" content=\"{}\">\n",
            esc(theme)
        );
        arts.push(ctx.write_output("favicon-snippet.html", html.as_bytes())?);
        ctx.progress(1.0, "done");
        Ok(ToolOutput::files(arts).with_text(html, "text/plain"))
    })
}
