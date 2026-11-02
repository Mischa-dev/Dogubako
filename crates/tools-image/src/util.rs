//! Shared helpers: limit-aware decoding, encoding, colors, text rendering,
//! compositing and ImageMagick fallbacks.

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use dogubako_core::prelude::*;
use dogubako_core::process::Exec;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, Rgba, RgbaImage};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// File types accepted by image inputs (the image crate natively, plus
/// formats handled via resvg or the optional ImageMagick fallback).
pub const IMAGE_ACCEPT: &[&str] = &[
    "image/*", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".tif", ".tiff", ".ico", ".tga",
    ".qoi", ".ppm", ".pgm", ".pbm", ".pnm", ".pam", ".avif", ".svg", ".svgz", ".jfif", ".apng",
    ".heic", ".heif", ".psd", ".jxl", ".jp2", ".pcx", ".xcf", ".dds", ".exr", ".hdr", ".dng",
    ".cr2", ".nef", ".arw", ".orf", ".rw2",
];

/// Run an async external command from synchronous tool code (we are on
/// the blocking pool, where blocking on the runtime is allowed).
pub fn run_exec(exec: Exec) -> ToolResult<dogubako_core::process::ExecOutput> {
    tokio::runtime::Handle::current().block_on(exec.run())
}

pub fn check_pixels(ctx: &ToolContext, w: u64, h: u64) -> ToolResult<()> {
    let max = ctx.limits().max_image_pixels;
    if w.saturating_mul(h) > max {
        return Err(ToolError::limit(format!(
            "image of {w}x{h} pixels exceeds the limit of {max} pixels"
        )));
    }
    if w == 0 || h == 0 {
        return Err(ToolError::input("image has zero width or height"));
    }
    Ok(())
}

fn limits_for(ctx: &ToolContext) -> Limits {
    let mut l = Limits::default();
    // 16 bytes per pixel covers RGBA32F intermediate buffers.
    let px = ctx.limits().max_image_pixels;
    l.max_alloc = Some(px.saturating_mul(16).max(512 * 1024 * 1024));
    l
}

pub fn looks_like_svg(path: &Path) -> bool {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ext == "svg" || ext == "svgz" {
        return true;
    }
    let mut head = [0u8; 512];
    use std::io::Read;
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .unwrap_or(0);
    let s = String::from_utf8_lossy(&head[..n]);
    let t = s.trim_start_matches('\u{feff}').trim_start();
    (t.starts_with("<?xml") || t.starts_with("<svg") || t.starts_with("<!--")) && s.contains("<svg")
}

/// Decode an image file, honoring the pixel limit and EXIF orientation.
/// SVG is rasterized with resvg; formats unknown to the image crate fall
/// back to ImageMagick when it is installed.
pub fn load_image(ctx: &ToolContext, file: &InputFile) -> ToolResult<DynamicImage> {
    load_path(ctx, &file.path, &file.name)
}

pub fn load_path(ctx: &ToolContext, path: &Path, name: &str) -> ToolResult<DynamicImage> {
    if looks_like_svg(path) {
        let data = std::fs::read(path)?;
        let tree =
            crate::svg::parse_svg(&data).map_err(|e| ToolError::input(format!("{name}: {e}")))?;
        let size = tree.size().to_int_size();
        return crate::svg::render_tree(ctx, &tree, size.width(), size.height(), None)
            .map(DynamicImage::ImageRgba8);
    }
    match decode_native(ctx, path) {
        Ok(img) => Ok(img),
        Err(NativeErr::Limit(e)) => Err(e),
        Err(NativeErr::Unsupported(msg)) => {
            if ctx.deps().path("imagemagick").is_some() {
                let png = magick_to_png(ctx, path)?;
                decode_native(ctx, &png).map_err(|e| e.into_tool(name))
            } else {
                Err(ToolError::input(format!(
                    "{name}: unsupported or corrupt image ({msg}); installing ImageMagick adds support for more formats"
                )))
            }
        }
        Err(e) => Err(e.into_tool(name)),
    }
}

pub enum NativeErr {
    Limit(ToolError),
    Unsupported(String),
    Corrupt(String),
}

impl NativeErr {
    fn into_tool(self, name: &str) -> ToolError {
        match self {
            NativeErr::Limit(e) => e,
            NativeErr::Unsupported(m) => {
                ToolError::input(format!("{name}: unsupported image format ({m})"))
            }
            NativeErr::Corrupt(m) => {
                ToolError::input(format!("{name}: could not decode image: {m}"))
            }
        }
    }
}

fn decode_native(ctx: &ToolContext, path: &Path) -> Result<DynamicImage, NativeErr> {
    let open = || -> Result<ImageReader<std::io::BufReader<std::fs::File>>, NativeErr> {
        let r = ImageReader::open(path).map_err(|e| NativeErr::Corrupt(e.to_string()))?;
        let r = r
            .with_guessed_format()
            .map_err(|e| NativeErr::Corrupt(e.to_string()))?;
        if r.format().is_none() {
            return Err(NativeErr::Unsupported("unknown format".into()));
        }
        Ok(r)
    };
    let (w, h) = open()?.into_dimensions().map_err(classify)?;
    check_pixels(ctx, w as u64, h as u64).map_err(NativeErr::Limit)?;
    let mut r = open()?;
    r.limits(limits_for(ctx));
    let mut dec = r.into_decoder().map_err(classify)?;
    let orientation = dec.orientation().ok();
    let mut img = DynamicImage::from_decoder(dec).map_err(classify)?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

fn classify(e: image::ImageError) -> NativeErr {
    match e {
        image::ImageError::Unsupported(u) => NativeErr::Unsupported(u.to_string()),
        image::ImageError::Limits(l) => NativeErr::Limit(ToolError::limit(l.to_string())),
        other => NativeErr::Corrupt(other.to_string()),
    }
}

/// Copy/link `path` into the scratch dir under a safe name so ImageMagick
/// never interprets user-controlled names (`fmt:`, `[0]`, `@list`...).
pub fn safe_input_copy(ctx: &ToolContext, path: &Path, stem: &str) -> ToolResult<PathBuf> {
    let ext: String = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(5)
        .collect();
    let name = if ext.is_empty() {
        format!("{stem}-{}", dogubako_core::util::random_id(4))
    } else {
        format!("{stem}-{}.{ext}", dogubako_core::util::random_id(4))
    };
    let dst = ctx.temp_path(&name);
    if std::fs::hard_link(path, &dst).is_err() {
        std::fs::copy(path, &dst)?;
    }
    Ok(dst)
}

/// Convert any ImageMagick-readable file (first frame) to a temporary PNG.
pub fn magick_to_png(ctx: &ToolContext, path: &Path) -> ToolResult<PathBuf> {
    let src = safe_input_copy(ctx, path, "magick-in")?;
    let out = ctx.temp_path(&format!("magick-{}.png", dogubako_core::util::random_id(4)));
    let mut input = src.as_os_str().to_os_string();
    input.push("[0]");
    let mut o = std::ffi::OsString::from("png32:");
    o.push(out.as_os_str());
    run_exec(
        ctx.command("imagemagick")?
            .arg(input)
            .arg("-auto-orient")
            .arg(o),
    )?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Encoding

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFmt {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Tiff,
    Ico,
    Avif,
    Tga,
    Qoi,
    Pnm,
    /// Written by ImageMagick.
    Magick(&'static str),
}

pub const NATIVE_FORMATS: &[(&str, &str)] = &[
    ("png", "PNG"),
    ("jpg", "JPEG"),
    ("webp", "WebP"),
    ("avif", "AVIF"),
    ("gif", "GIF"),
    ("bmp", "BMP"),
    ("tiff", "TIFF"),
    ("ico", "ICO (max 256px)"),
    ("tga", "TGA"),
    ("qoi", "QOI"),
    ("ppm", "PPM/PNM"),
];

pub const MAGICK_FORMATS: &[(&str, &str)] = &[
    ("heic", "HEIC (ImageMagick)"),
    ("jxl", "JPEG XL (ImageMagick)"),
    ("jp2", "JPEG 2000 (ImageMagick)"),
    ("psd", "PSD (ImageMagick)"),
];

impl OutFmt {
    pub fn parse(s: &str) -> Option<OutFmt> {
        Some(match s.to_ascii_lowercase().as_str() {
            "png" => OutFmt::Png,
            "jpg" | "jpeg" => OutFmt::Jpeg,
            "webp" => OutFmt::Webp,
            "gif" => OutFmt::Gif,
            "bmp" => OutFmt::Bmp,
            "tif" | "tiff" => OutFmt::Tiff,
            "ico" => OutFmt::Ico,
            "avif" => OutFmt::Avif,
            "tga" => OutFmt::Tga,
            "qoi" => OutFmt::Qoi,
            "ppm" | "pnm" => OutFmt::Pnm,
            "heic" => OutFmt::Magick("heic"),
            "jxl" => OutFmt::Magick("jxl"),
            "jp2" => OutFmt::Magick("jp2"),
            "psd" => OutFmt::Magick("psd"),
            _ => return None,
        })
    }
    /// Format matching an input's extension, when we can write it.
    pub fn from_name(name: &str) -> Option<OutFmt> {
        let ext = Path::new(name)
            .extension()?
            .to_string_lossy()
            .to_lowercase();
        match OutFmt::parse(&ext)? {
            OutFmt::Magick(_) => None,
            f => Some(f),
        }
    }
    pub fn ext(&self) -> &'static str {
        match self {
            OutFmt::Png => "png",
            OutFmt::Jpeg => "jpg",
            OutFmt::Webp => "webp",
            OutFmt::Gif => "gif",
            OutFmt::Bmp => "bmp",
            OutFmt::Tiff => "tiff",
            OutFmt::Ico => "ico",
            OutFmt::Avif => "avif",
            OutFmt::Tga => "tga",
            OutFmt::Qoi => "qoi",
            OutFmt::Pnm => "ppm",
            OutFmt::Magick(e) => e,
        }
    }
    pub fn supports_alpha(&self) -> bool {
        !matches!(self, OutFmt::Jpeg | OutFmt::Pnm)
    }
}

/// Resolve the output format: explicit choice, or "auto"/"same" → input's format (PNG fallback).
pub fn pick_format(choice: &str, input_name: &str) -> ToolResult<OutFmt> {
    match choice {
        "" | "auto" | "same" | "original" => {
            Ok(OutFmt::from_name(input_name).unwrap_or(OutFmt::Png))
        }
        c => OutFmt::parse(c).ok_or_else(|| {
            ToolError::invalid("format", format!("unsupported output format '{c}'"))
        }),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SaveOpts {
    /// 1..=100, used by lossy encoders.
    pub quality: u8,
    /// Used to flatten transparency for formats without alpha.
    pub background: Rgba<u8>,
    /// Lossless WebP instead of lossy.
    pub lossless: bool,
}

impl Default for SaveOpts {
    fn default() -> Self {
        SaveOpts {
            quality: 85,
            background: Rgba([255, 255, 255, 255]),
            lossless: false,
        }
    }
}

/// Composite onto an opaque background.
pub fn flatten(img: &DynamicImage, bg: Rgba<u8>) -> image::RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = image::RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p[3] as f32 / 255.0;
        for c in 0..3 {
            o[c] = (p[c] as f32 * a + bg[c] as f32 * (1.0 - a)).round() as u8;
        }
    }
    out
}

pub fn encode(
    ctx: &ToolContext,
    img: &DynamicImage,
    fmt: OutFmt,
    opts: &SaveOpts,
) -> ToolResult<Vec<u8>> {
    let q = opts.quality.clamp(1, 100);
    let mut buf = Cursor::new(Vec::new());
    let enc_err =
        |e: image::ImageError| ToolError::failed(format!("encoding {} failed: {e}", fmt.ext()));
    match fmt {
        OutFmt::Png => {
            let img = normalize_depth(img);
            img.write_to(&mut buf, ImageFormat::Png).map_err(enc_err)?;
        }
        OutFmt::Jpeg => {
            let rgb = flatten(img, opts.background);
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q);
            rgb.write_with_encoder(enc).map_err(enc_err)?;
        }
        OutFmt::Webp => {
            let (w, h) = (img.width(), img.height());
            let mem = if img.color().has_alpha() {
                let rgba = img.to_rgba8();
                let e = webp::Encoder::from_rgba(rgba.as_raw(), w, h);
                if opts.lossless {
                    e.encode_lossless()
                } else {
                    e.encode(q as f32)
                }
            } else {
                let rgb = img.to_rgb8();
                let e = webp::Encoder::from_rgb(rgb.as_raw(), w, h);
                if opts.lossless {
                    e.encode_lossless()
                } else {
                    e.encode(q as f32)
                }
            };
            return Ok(mem.to_vec());
        }
        OutFmt::Gif => {
            let rgba = img.to_rgba8();
            let mut enc = image::codecs::gif::GifEncoder::new_with_speed(&mut buf, 10);
            enc.encode(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(enc_err)?;
            drop(enc);
        }
        OutFmt::Bmp => {
            let img = if img.color().has_alpha() {
                DynamicImage::ImageRgba8(img.to_rgba8())
            } else {
                DynamicImage::ImageRgb8(img.to_rgb8())
            };
            img.write_to(&mut buf, ImageFormat::Bmp).map_err(enc_err)?;
        }
        OutFmt::Tiff => {
            let img = normalize_depth(img);
            img.write_to(&mut buf, ImageFormat::Tiff).map_err(enc_err)?;
        }
        OutFmt::Ico => {
            let img = if img.width() > 256 || img.height() > 256 {
                img.resize(256, 256, image::imageops::FilterType::Lanczos3)
            } else {
                img.clone()
            };
            DynamicImage::ImageRgba8(img.to_rgba8())
                .write_to(&mut buf, ImageFormat::Ico)
                .map_err(enc_err)?;
        }
        OutFmt::Avif => {
            let enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut buf, 8, q);
            if img.color().has_alpha() {
                img.to_rgba8().write_with_encoder(enc).map_err(enc_err)?;
            } else {
                img.to_rgb8().write_with_encoder(enc).map_err(enc_err)?;
            }
        }
        OutFmt::Tga => {
            let img = if img.color().has_alpha() {
                DynamicImage::ImageRgba8(img.to_rgba8())
            } else {
                DynamicImage::ImageRgb8(img.to_rgb8())
            };
            img.write_to(&mut buf, ImageFormat::Tga).map_err(enc_err)?;
        }
        OutFmt::Qoi => {
            let img = if img.color().has_alpha() {
                DynamicImage::ImageRgba8(img.to_rgba8())
            } else {
                DynamicImage::ImageRgb8(img.to_rgb8())
            };
            img.write_to(&mut buf, ImageFormat::Qoi).map_err(enc_err)?;
        }
        OutFmt::Pnm => {
            DynamicImage::ImageRgb8(flatten(img, opts.background))
                .write_to(&mut buf, ImageFormat::Pnm)
                .map_err(enc_err)?;
        }
        OutFmt::Magick(ext) => {
            let tmp = ctx.temp_path(&format!(
                "magick-src-{}.png",
                dogubako_core::util::random_id(4)
            ));
            std::fs::write(&tmp, encode(ctx, img, OutFmt::Png, opts)?)?;
            let out = ctx.temp_path(&format!(
                "magick-out-{}.{ext}",
                dogubako_core::util::random_id(4)
            ));
            let mut target = std::ffi::OsString::from(format!("{ext}:"));
            target.push(out.as_os_str());
            run_exec(
                ctx.command("imagemagick")?
                    .arg(&tmp)
                    .arg("-quality")
                    .arg(q.to_string())
                    .arg(target),
            )?;
            return Ok(std::fs::read(&out)?);
        }
    }
    Ok(buf.into_inner())
}

/// PNG/TIFF support 8/16-bit; convert float images to 16-bit.
fn normalize_depth(img: &DynamicImage) -> DynamicImage {
    match img {
        DynamicImage::ImageRgb32F(_) => DynamicImage::ImageRgb16(img.to_rgb16()),
        DynamicImage::ImageRgba32F(_) => DynamicImage::ImageRgba16(img.to_rgba16()),
        other => other.clone(),
    }
}

/// Encode and write to a fresh output file, returning the artifact.
pub fn save_output(
    ctx: &ToolContext,
    img: &DynamicImage,
    name: &str,
    fmt: OutFmt,
    opts: &SaveOpts,
) -> ToolResult<Artifact> {
    let bytes = encode(ctx, img, fmt, opts)?;
    ctx.write_output(name, &bytes)
}

pub fn save_png(ctx: &ToolContext, img: &RgbaImage, name: &str) -> ToolResult<Artifact> {
    save_output(
        ctx,
        &DynamicImage::ImageRgba8(img.clone()),
        name,
        OutFmt::Png,
        &SaveOpts::default(),
    )
}

// ---------------------------------------------------------------------------
// Colors

pub fn parse_color(param: &str, s: &str) -> ToolResult<Rgba<u8>> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("transparent") || t.eq_ignore_ascii_case("none") {
        return Ok(Rgba([0, 0, 0, 0]));
    }
    let n = dogubako_core::params::normalize_hex_color(t).ok_or_else(|| {
        ToolError::invalid(param, format!("'{s}' is not a hex color like #3366ff"))
    })?;
    let h = &n[1..];
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    Ok(Rgba([
        b(0),
        b(2),
        b(4),
        if h.len() == 8 { b(6) } else { 255 },
    ]))
}

pub fn color_param(p: &Params, name: &str, default: &str) -> ToolResult<Rgba<u8>> {
    parse_color(name, p.opt_str(name).unwrap_or(default))
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn hex_rgba(c: Rgba<u8>) -> String {
    if c[3] == 255 {
        hex([c[0], c[1], c[2]])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
    }
}

// ---------------------------------------------------------------------------
// Text rendering with the bundled DejaVu Sans font.

pub fn font() -> &'static FontArc {
    static FONT: OnceLock<FontArc> = OnceLock::new();
    FONT.get_or_init(|| {
        FontArc::try_from_slice(include_bytes!("../assets/DejaVuSans.ttf"))
            .expect("bundled font is valid")
    })
}

pub const FONT_BYTES: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");

/// Render (multi-line) text into a tightly sized transparent RGBA image.
pub fn render_text(text: &str, px: f32, color: Rgba<u8>) -> RgbaImage {
    let font = font();
    let scale = PxScale::from(px.max(1.0));
    let sf = font.as_scaled(scale);
    let lines: Vec<&str> = if text.is_empty() {
        vec![""]
    } else {
        text.lines().collect()
    };
    let line_h = sf.height() + sf.line_gap();
    let mut glyphs = Vec::new();
    let mut max_w = 0f32;
    for (li, line) in lines.iter().enumerate() {
        let mut x = 0f32;
        let y = li as f32 * line_h + sf.ascent();
        let mut prev = None;
        for ch in line.chars() {
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            glyphs.push(id.with_scale_and_position(scale, point(x, y)));
            x += sf.h_advance(id);
            prev = Some(id);
        }
        max_w = max_w.max(x);
    }
    let w = (max_w.ceil() as u32 + 2).max(1);
    let h = ((lines.len() as f32 * line_h).ceil() as u32 + 2).max(1);
    let mut img = RgbaImage::from_pixel(w, h, Rgba([color[0], color[1], color[2], 0]));
    for g in glyphs {
        if let Some(o) = font.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|gx, gy, c| {
                let x = b.min.x as i64 + gx as i64;
                let y = b.min.y as i64 + gy as i64;
                if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                    let a = (c.clamp(0.0, 1.0) * color[3] as f32).round() as u8;
                    let p = img.get_pixel_mut(x as u32, y as u32);
                    p[3] = p[3].max(a);
                }
            });
        }
    }
    img
}

// ---------------------------------------------------------------------------
// Compositing

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Add,
}

impl Blend {
    pub fn parse(s: &str) -> Blend {
        match s {
            "multiply" => Blend::Multiply,
            "screen" => Blend::Screen,
            "overlay" => Blend::Overlay,
            "darken" => Blend::Darken,
            "lighten" => Blend::Lighten,
            "difference" => Blend::Difference,
            "add" => Blend::Add,
            _ => Blend::Normal,
        }
    }
    fn apply(self, b: f32, s: f32) -> f32 {
        match self {
            Blend::Normal => s,
            Blend::Multiply => b * s,
            Blend::Screen => 1.0 - (1.0 - b) * (1.0 - s),
            Blend::Overlay => {
                if b < 0.5 {
                    2.0 * b * s
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - s)
                }
            }
            Blend::Darken => b.min(s),
            Blend::Lighten => b.max(s),
            Blend::Difference => (b - s).abs(),
            Blend::Add => (b + s).min(1.0),
        }
    }
}

/// Source-over composite of `top` onto `base` at (x, y) with opacity and blend mode.
pub fn composite(
    base: &mut RgbaImage,
    top: &RgbaImage,
    x: i64,
    y: i64,
    opacity: f32,
    blend: Blend,
) {
    let (bw, bh) = (base.width() as i64, base.height() as i64);
    let opacity = opacity.clamp(0.0, 1.0);
    for ty in 0..top.height() as i64 {
        let by = y + ty;
        if by < 0 || by >= bh {
            continue;
        }
        for tx in 0..top.width() as i64 {
            let bx = x + tx;
            if bx < 0 || bx >= bw {
                continue;
            }
            let s = top.get_pixel(tx as u32, ty as u32);
            let sa = s[3] as f32 / 255.0 * opacity;
            if sa <= 0.0 {
                continue;
            }
            let d = base.get_pixel_mut(bx as u32, by as u32);
            let da = d[3] as f32 / 255.0;
            let oa = sa + da * (1.0 - sa);
            for c in 0..3 {
                let sc = s[c] as f32 / 255.0;
                let dc = d[c] as f32 / 255.0;
                // Blend result only applies where the backdrop exists.
                let mixed = (1.0 - da) * sc + da * blend.apply(dc, sc);
                let oc = (mixed * sa + dc * da * (1.0 - sa)) / oa.max(1e-6);
                d[c] = (oc * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            d[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// 9-way anchor positions.
pub const POSITIONS: &[(&str, &str)] = &[
    ("center", "Center"),
    ("top-left", "Top left"),
    ("top", "Top"),
    ("top-right", "Top right"),
    ("left", "Left"),
    ("right", "Right"),
    ("bottom-left", "Bottom left"),
    ("bottom", "Bottom"),
    ("bottom-right", "Bottom right"),
];

/// Top-left coordinate to place an item of size (w,h) inside (cw,ch) with margin.
pub fn anchor(pos: &str, cw: u32, ch: u32, w: u32, h: u32, margin: i64) -> (i64, i64) {
    let (cw, ch, w, h) = (cw as i64, ch as i64, w as i64, h as i64);
    let cx = (cw - w) / 2;
    let cy = (ch - h) / 2;
    let l = margin;
    let r = cw - w - margin;
    let t = margin;
    let b = ch - h - margin;
    match pos {
        "top-left" => (l, t),
        "top" => (cx, t),
        "top-right" => (r, t),
        "left" => (l, cy),
        "right" => (r, cy),
        "bottom-left" => (l, b),
        "bottom" => (cx, b),
        "bottom-right" => (r, b),
        _ => (cx, cy),
    }
}

pub fn filter_type(s: &str) -> image::imageops::FilterType {
    use image::imageops::FilterType::*;
    match s {
        "nearest" => Nearest,
        "bilinear" | "triangle" => Triangle,
        "bicubic" | "catmullrom" => CatmullRom,
        "gaussian" => Gaussian,
        _ => Lanczos3,
    }
}

pub const FILTERS: &[(&str, &str)] = &[
    ("lanczos", "Lanczos (sharpest)"),
    ("bicubic", "Bicubic"),
    ("bilinear", "Bilinear"),
    ("gaussian", "Gaussian"),
    ("nearest", "Nearest neighbor (pixel art)"),
];

pub fn quality_param() -> ParamSpec {
    ParamSpec::integer("quality")
        .label("Quality")
        .describe("Quality for lossy formats (JPEG, WebP, AVIF), 1–100")
        .range(1.0, 100.0)
        .default_int(85)
}

pub fn images_param() -> ParamSpec {
    ParamSpec::files("input")
        .label("Images")
        .required()
        .primary()
        .accept(IMAGE_ACCEPT)
}

pub fn image_param() -> ParamSpec {
    ParamSpec::file("input")
        .label("Image")
        .required()
        .primary()
        .accept(IMAGE_ACCEPT)
}

/// Run `f` over every input file with progress, collecting artifacts.
pub fn for_each_file<F>(
    ctx: &ToolContext,
    files: &[InputFile],
    verb: &str,
    mut f: F,
) -> ToolResult<Vec<Artifact>>
where
    F: FnMut(&InputFile) -> ToolResult<Artifact>,
{
    if files.is_empty() {
        return Err(ToolError::invalid(
            "input",
            "at least one image is required",
        ));
    }
    let n = files.len();
    let mut out = Vec::with_capacity(n);
    for (i, file) in files.iter().enumerate() {
        ctx.check_cancelled()?;
        ctx.progress(
            i as f32 / n as f32,
            format!("{verb} {} ({}/{n})", file.name, i + 1),
        );
        out.push(f(file)?);
    }
    ctx.progress(1.0, "done");
    Ok(out)
}

/// Summarize per-file sizes for batch tools.
pub fn size_table(files: &[InputFile], arts: &[Artifact]) -> Value {
    let rows: Vec<Value> = files
        .iter()
        .zip(arts)
        .map(|(f, a)| {
            let before = f.size();
            let change = if before > 0 {
                format!(
                    "{:+.1}%",
                    (a.size as f64 - before as f64) / before as f64 * 100.0
                )
            } else {
                "".into()
            };
            json!([
                f.name,
                human_size(before),
                a.name,
                human_size(a.size),
                change
            ])
        })
        .collect();
    json!({"columns": ["Input", "Input size", "Output", "Output size", "Change"], "rows": rows})
}

/// Parse a JSON list of rectangles `[{"x","y","width","height"}]` (also accepts w/h).
pub fn parse_regions(v: Option<&Value>, param: &str) -> ToolResult<Vec<(i64, i64, i64, i64)>> {
    let Some(v) = v else { return Ok(vec![]) };
    let arr = match v {
        Value::Array(a) => a.clone(),
        Value::Object(o) if o.contains_key("regions") => {
            o["regions"].as_array().cloned().unwrap_or_default()
        }
        Value::Object(_) => vec![v.clone()],
        _ => {
            return Err(ToolError::invalid(
                param,
                "expected a JSON list of {x, y, width, height} objects",
            ))
        }
    };
    let num = |o: &Value, keys: &[&str]| -> Option<f64> {
        keys.iter().find_map(|k| o.get(*k).and_then(|x| x.as_f64()))
    };
    arr.iter()
        .map(|o| {
            let x = num(o, &["x", "left"]);
            let y = num(o, &["y", "top"]);
            let w = num(o, &["width", "w"]);
            let h = num(o, &["height", "h"]);
            match (x, y, w, h) {
                (Some(x), Some(y), Some(w), Some(h)) if w > 0.0 && h > 0.0 => Ok((
                    x.round() as i64,
                    y.round() as i64,
                    w.round() as i64,
                    h.round() as i64,
                )),
                _ => Err(ToolError::invalid(
                    param,
                    format!("invalid region {o}: needs numeric x, y, width > 0, height > 0"),
                )),
            }
        })
        .collect()
}

/// Clip a rectangle to image bounds; None if empty.
pub fn clip_rect(r: (i64, i64, i64, i64), w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = r.0.max(0);
    let y0 = r.1.max(0);
    let x1 = (r.0 + r.2).min(w as i64);
    let y1 = (r.1 + r.3).min(h as i64);
    if x1 <= x0 || y1 <= y0 {
        None
    } else {
        Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
    }
}

pub fn ratio_string(w: u64, h: u64) -> String {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    let g = gcd(w, h).max(1);
    format!("{}:{}", w / g, h / g)
}

pub fn opt_dim(p: &Params, name: &str) -> Option<u32> {
    p.opt_int(name).filter(|v| *v > 0).map(|v| v as u32)
}
