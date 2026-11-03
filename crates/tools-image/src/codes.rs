//! QR codes and barcodes: generation (qrcode, rxing) and reading (rqrr, rxing).

use crate::util::*;
use dogubako_core::prelude::*;
use image::{DynamicImage, Rgba, RgbaImage};
use rxing::{BarcodeFormat, EncodeHints, Writer};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        qr_generate(),
        qr_style(),
        qr_read(),
        barcode_generate(),
        barcode_read(),
    ]
}

/// A square boolean module matrix (true = dark).
pub struct Matrix {
    pub w: usize,
    pub h: usize,
    pub bits: Vec<bool>,
}

impl Matrix {
    pub fn get(&self, x: i64, y: i64) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.w
            && (y as usize) < self.h
            && self.bits[y as usize * self.w + x as usize]
    }
}

pub fn qr_matrix(text: &str, ec: &str) -> ToolResult<Matrix> {
    let level = match ec {
        "L" => qrcode::EcLevel::L,
        "Q" => qrcode::EcLevel::Q,
        "H" => qrcode::EcLevel::H,
        _ => qrcode::EcLevel::M,
    };
    let code = qrcode::QrCode::with_error_correction_level(text.as_bytes(), level)
        .map_err(|e| ToolError::invalid("text", format!("cannot encode as QR code: {e}")))?;
    let w = code.width();
    let bits = code
        .to_colors()
        .into_iter()
        .map(|c| c == qrcode::Color::Dark)
        .collect();
    Ok(Matrix { w, h: w, bits })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModuleStyle {
    Square,
    Rounded,
    Dots,
}

impl ModuleStyle {
    fn parse(s: &str) -> Self {
        match s {
            "rounded" => ModuleStyle::Rounded,
            "dots" => ModuleStyle::Dots,
            _ => ModuleStyle::Square,
        }
    }
}

/// Whether module (x,y) belongs to one of the three 7×7 finder patterns.
fn in_finder(m: &Matrix, x: usize, y: usize) -> bool {
    let n = m.w;
    (x < 7 && y < 7) || (x >= n - 7 && y < 7) || (x < 7 && y >= n - 7)
}

fn finder_origins(m: &Matrix) -> [(usize, usize); 3] {
    [(0, 0), (m.w - 7, 0), (0, m.w - 7)]
}

/// Coverage test in module space for a point (fx, fy) relative to a module's top-left.
fn module_hit(style: ModuleStyle, fx: f64, fy: f64) -> bool {
    match style {
        ModuleStyle::Square => true,
        ModuleStyle::Dots => {
            let (dx, dy) = (fx - 0.5, fy - 0.5);
            dx * dx + dy * dy <= 0.45 * 0.45
        }
        ModuleStyle::Rounded => rounded_rect_hit(fx, fy, 0.0, 0.0, 1.0, 1.0, 0.35),
    }
}

fn rounded_rect_hit(px: f64, py: f64, x: f64, y: f64, w: f64, h: f64, r: f64) -> bool {
    if px < x || py < y || px > x + w || py > y + h {
        return false;
    }
    let cx = px.clamp(x + r, x + w - r);
    let cy = py.clamp(y + r, y + h - r);
    (px - cx).powi(2) + (py - cy).powi(2) <= r * r
}

/// Finder pattern coverage at module-space point relative to the pattern origin.
fn finder_hit(eye: ModuleStyle, fx: f64, fy: f64) -> bool {
    let r_out = if eye == ModuleStyle::Square { 0.0 } else { 1.6 };
    let r_in = if eye == ModuleStyle::Square { 0.0 } else { 1.0 };
    let outer = rounded_rect_hit(fx, fy, 0.0, 0.0, 7.0, 7.0, r_out)
        && !rounded_rect_hit(fx, fy, 1.0, 1.0, 5.0, 5.0, (r_out - 1.0).max(0.0));
    let inner = if eye == ModuleStyle::Dots {
        (fx - 3.5).powi(2) + (fy - 3.5).powi(2) <= 1.5 * 1.5
    } else {
        rounded_rect_hit(fx, fy, 2.0, 2.0, 3.0, 3.0, r_in * 0.8)
    };
    outer || inner
}

pub struct QrRender {
    pub size: u32,
    pub margin: u32,
    pub fg: Rgba<u8>,
    pub bg: Rgba<u8>,
    pub style: ModuleStyle,
    pub eye: ModuleStyle,
}

/// Rasterize with 4× supersampling for smooth round shapes.
pub fn render_matrix(m: &Matrix, r: &QrRender) -> RgbaImage {
    let total = (m.w as u32 + 2 * r.margin) as f64;
    let scale = r.size as f64 / total;
    let styled = r.style != ModuleStyle::Square || r.eye != ModuleStyle::Square;
    let ss: &[(f64, f64)] = if styled {
        &[(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
    } else {
        &[(0.5, 0.5)]
    };
    let origins = finder_origins(m);
    RgbaImage::from_fn(r.size, r.size, |x, y| {
        let mut hits = 0;
        for (ox, oy) in ss {
            let mx = (x as f64 + ox) / scale - r.margin as f64;
            let my = (y as f64 + oy) / scale - r.margin as f64;
            if mx < 0.0 || my < 0.0 || mx >= m.w as f64 || my >= m.h as f64 {
                continue;
            }
            let (ix, iy) = (mx.floor() as usize, my.floor() as usize);
            let hit = if styled && in_finder(m, ix, iy) {
                let (fx0, fy0) = origins
                    .iter()
                    .find(|(a, b)| ix >= *a && ix < a + 7 && iy >= *b && iy < b + 7)
                    .copied()
                    .unwrap_or((0, 0));
                finder_hit(r.eye, mx - fx0 as f64, my - fy0 as f64)
            } else {
                m.get(ix as i64, iy as i64) && module_hit(r.style, mx - ix as f64, my - iy as f64)
            };
            if hit {
                hits += 1;
            }
        }
        let k = hits as f32 / ss.len() as f32;
        let mix = |a: u8, b: u8| (a as f32 * k + b as f32 * (1.0 - k)).round() as u8;
        Rgba([
            mix(r.fg[0], r.bg[0]),
            mix(r.fg[1], r.bg[1]),
            mix(r.fg[2], r.bg[2]),
            mix(r.fg[3], r.bg[3]),
        ])
    })
}

fn svg_color(c: Rgba<u8>) -> (String, String) {
    (
        hex([c[0], c[1], c[2]]),
        format!("{:.3}", c[3] as f64 / 255.0),
    )
}

/// SVG in module units, scaled to `size` px.
pub fn matrix_svg(m: &Matrix, r: &QrRender, logo: Option<(&[u8], f64)>) -> String {
    let total = m.w as u32 + 2 * r.margin;
    let (fg, fo) = svg_color(r.fg);
    let (bg, bo) = svg_color(r.bg);
    let mg = r.margin as i64;
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {total} {total}\" width=\"{}\" height=\"{}\" shape-rendering=\"{}\">",
        r.size,
        r.size,
        if r.style == ModuleStyle::Square && r.eye == ModuleStyle::Square { "crispEdges" } else { "geometricPrecision" }
    );
    if r.bg[3] > 0 {
        s.push_str(&format!(
            "<rect width=\"{total}\" height=\"{total}\" fill=\"{bg}\" fill-opacity=\"{bo}\"/>"
        ));
    }
    let styled = r.style != ModuleStyle::Square || r.eye != ModuleStyle::Square;
    s.push_str(&format!("<g fill=\"{fg}\" fill-opacity=\"{fo}\">"));
    let mut path = String::new();
    for y in 0..m.h {
        for x in 0..m.w {
            if !m.bits[y * m.w + x] || (styled && in_finder(m, x, y)) {
                continue;
            }
            let (px, py) = (x as i64 + mg, y as i64 + mg);
            match r.style {
                ModuleStyle::Square => path.push_str(&format!("M{px} {py}h1v1h-1z")),
                ModuleStyle::Rounded => s.push_str(&format!(
                    "<rect x=\"{px}\" y=\"{py}\" width=\"1\" height=\"1\" rx=\"0.35\"/>"
                )),
                ModuleStyle::Dots => s.push_str(&format!(
                    "<circle cx=\"{}.5\" cy=\"{}.5\" r=\"0.45\"/>",
                    px, py
                )),
            }
        }
    }
    if !path.is_empty() {
        s.push_str(&format!("<path d=\"{path}\"/>"));
    }
    if styled {
        for (ox, oy) in finder_origins(m) {
            let (x, y) = (ox as i64 + mg, oy as i64 + mg);
            let (ro, ri) = if r.eye == ModuleStyle::Square {
                (0.0, 0.0)
            } else {
                (1.6, 0.8)
            };
            s.push_str(&format!(
                "<path fill-rule=\"evenodd\" d=\"{}{}\"/>",
                rounded_rect_path(x as f64, y as f64, 7.0, 7.0, ro),
                rounded_rect_path(
                    x as f64 + 1.0,
                    y as f64 + 1.0,
                    5.0,
                    5.0,
                    (ro - 1.0f64).max(0.0)
                )
            ));
            if r.eye == ModuleStyle::Dots {
                s.push_str(&format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"1.5\"/>",
                    x as f64 + 3.5,
                    y as f64 + 3.5
                ));
            } else {
                s.push_str(&format!(
                    "<path d=\"{}\"/>",
                    rounded_rect_path(x as f64 + 2.0, y as f64 + 2.0, 3.0, 3.0, ri)
                ));
            }
        }
    }
    s.push_str("</g>");
    if let Some((png, frac)) = logo {
        let lw = total as f64 * frac;
        let pad = lw * 0.08;
        let x = (total as f64 - lw) / 2.0;
        s.push_str(&format!("<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" rx=\"{:.3}\" fill=\"{bg}\"/>", x - pad, x - pad, lw + 2.0 * pad, lw + 2.0 * pad, pad));
        s.push_str(&format!("<image x=\"{x:.3}\" y=\"{x:.3}\" width=\"{lw:.3}\" height=\"{lw:.3}\" xlink:href=\"data:image/png;base64,{}\"/>", base64(png)));
    }
    s.push_str("</svg>");
    s
}

fn rounded_rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    if r <= 0.0 {
        return format!("M{x} {y}h{w}v{h}h-{w}z");
    }
    format!(
        "M{:.2} {y}h{:.2}a{r} {r} 0 0 1 {r} {r}v{:.2}a{r} {r} 0 0 1 -{r} {r}h-{:.2}a{r} {r} 0 0 1 -{r} -{r}v-{:.2}a{r} {r} 0 0 1 {r} -{r}z",
        x + r,
        w - 2.0 * r,
        h - 2.0 * r,
        w - 2.0 * r,
        h - 2.0 * r
    )
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn qr_common_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::textarea("text")
            .label("Content")
            .required()
            .primary()
            .placeholder("https://example.com")
            .max_len(4296),
        ParamSpec::integer("size")
            .describe("Image size in pixels")
            .range(64.0, 4096.0)
            .default_int(512),
        ParamSpec::choice(
            "error_correction",
            &[
                ("L", "Low (7%)"),
                ("M", "Medium (15%)"),
                ("Q", "Quartile (25%)"),
                ("H", "High (30%)"),
            ],
        )
        .default_str("M"),
        ParamSpec::integer("margin")
            .describe("Quiet zone in modules (4 recommended)")
            .range(0.0, 20.0)
            .default_int(4),
        ParamSpec::color("foreground").default_str("#000000"),
        ParamSpec::color("background").default_str("#ffffff"),
    ]
}

fn render_qr_outputs(
    ctx: &ToolContext,
    p: &Params,
    style: ModuleStyle,
    eye: ModuleStyle,
    logo: Option<&InputFile>,
) -> ToolResult<ToolOutput> {
    let text = p.text("text");
    if text.is_empty() {
        return Err(ToolError::invalid("text", "content is empty"));
    }
    let ec = if logo.is_some() {
        "H"
    } else {
        p.str_or("error_correction", "M")
    };
    let m = qr_matrix(text, ec)?;
    let r = QrRender {
        size: p.int_or("size", 512) as u32,
        margin: p.int_or("margin", 4) as u32,
        fg: color_param(p, "foreground", "#000000")?,
        bg: color_param(p, "background", "#ffffff")?,
        style,
        eye,
    };
    let fl = crate::design::luminance([r.fg[0], r.fg[1], r.fg[2]]);
    let bl = crate::design::luminance([r.bg[0], r.bg[1], r.bg[2]]);
    let mut img = render_matrix(&m, &r);
    let mut logo_png = None;
    let frac = p.num_or("logo_size", 20.0).clamp(5.0, 30.0) / 100.0;
    if let Some(lf) = logo {
        let l = load_image(ctx, lf)?;
        let total = m.w as u32 + 2 * r.margin;
        let lw = ((r.size as f64) * frac).round().max(1.0) as u32;
        let scaled = l
            .resize(lw, lw, image::imageops::FilterType::Lanczos3)
            .to_rgba8();
        let pad = (lw as f64 * 0.08).round() as u32;
        let bx = (r.size - lw) / 2;
        let back = RgbaImage::from_pixel(
            lw + 2 * pad,
            lw + 2 * pad,
            Rgba([r.bg[0], r.bg[1], r.bg[2], 255]),
        );
        composite(
            &mut img,
            &back,
            bx as i64 - pad as i64,
            bx as i64 - pad as i64,
            1.0,
            Blend::Normal,
        );
        composite(
            &mut img,
            &scaled,
            (bx + (lw - scaled.width()) / 2) as i64,
            (bx + (lw - scaled.height()) / 2) as i64,
            1.0,
            Blend::Normal,
        );
        let mut b = std::io::Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(scaled)
            .write_to(&mut b, image::ImageFormat::Png)
            .map_err(|e| ToolError::failed(e.to_string()))?;
        logo_png = Some(b.into_inner());
        let _ = total;
    }
    let svg = matrix_svg(&m, &r, logo_png.as_deref().map(|b| (b, frac)));
    let base = p.str_or("file_name", "qr-code").to_string();
    let mut arts = vec![];
    let fmt = p.str_or("format", "both");
    if fmt != "svg" {
        arts.push(save_png(ctx, &img, &format!("{base}.png"))?);
    }
    if fmt != "png" {
        arts.push(ctx.write_output(&format!("{base}.svg"), svg.as_bytes())?);
    }
    // Verify scannability by decoding our own raster output.
    let decoded = decode_qr(&DynamicImage::ImageRgba8(img.clone()));
    let verified = decoded.iter().any(|c| c.text == text);
    let mut out = ToolOutput::files(arts)
        .with_text(svg, "image/svg+xml")
        .with_data(json!({
            "version": (m.w - 17) / 4,
            "modules": m.w,
            "error_correction": ec,
            "verified_scan": verified,
            "contrast_ratio": ((fl.max(bl) + 0.05) / (fl.min(bl) + 0.05) * 100.0).round() / 100.0,
        }));
    if fl > bl {
        out = out.warn(
            "light modules on a dark background (inverted QR) are not supported by every scanner",
        );
    }
    if !verified {
        out = out.warn("the generated image could not be decoded by the built-in reader; increase size, contrast or error correction");
    }
    Ok(out)
}

fn qr_generate() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.qr_generate", "QR Code Generator", Category::Dev)
        .describe("Generate QR codes from text or URLs as PNG and SVG with size, error correction, margin and colors")
        .keywords(&["qr", "qr code", "generate", "barcode", "url", "wifi", "2d code"])
        .params(qr_common_params())
        .param(ParamSpec::choice("format", &[("both", "PNG + SVG"), ("png", "PNG"), ("svg", "SVG")]).default_str("both").advanced())
        .param(ParamSpec::text("file_name").default_str("qr-code").advanced())
        .output(OutputKind::Files)
        .ui("qr");
    sync_tool(spec, |ctx, p| {
        render_qr_outputs(ctx, p, ModuleStyle::Square, ModuleStyle::Square, None)
    })
}

fn qr_style() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.qr_style", "Styled QR Code", Category::Design)
        .describe("Design branded QR codes: colors, rounded or dot modules, styled eyes and a center logo (uses high error correction)")
        .keywords(&["qr", "qr code", "logo", "branded", "custom", "styled", "dots", "rounded"])
        .params(qr_common_params())
        .param(ParamSpec::choice("module_style", &[("square", "Square"), ("rounded", "Rounded"), ("dots", "Dots")]).default_str("rounded"))
        .param(ParamSpec::choice("eye_style", &[("square", "Square"), ("rounded", "Rounded"), ("dots", "Rounded with dot")]).default_str("rounded"))
        .param(ParamSpec::file("logo").label("Logo (optional)").accept(IMAGE_ACCEPT))
        .param(ParamSpec::number("logo_size").label("Logo size (% of width)").range(5.0, 30.0).default_num(20.0))
        .param(ParamSpec::choice("format", &[("both", "PNG + SVG"), ("png", "PNG"), ("svg", "SVG")]).default_str("both").advanced())
        .param(ParamSpec::text("file_name").default_str("qr-code").advanced())
        .output(OutputKind::Files)
        .ui("qr");
    sync_tool(spec, |ctx, p| {
        let style = ModuleStyle::parse(p.str_or("module_style", "rounded"));
        let eye = ModuleStyle::parse(p.str_or("eye_style", "rounded"));
        render_qr_outputs(ctx, p, style, eye, p.opt_file("logo"))
    })
}

pub struct Decoded {
    pub text: String,
    pub format: String,
    pub points: Vec<(f64, f64)>,
}

fn bbox_region(points: &[(f64, f64)], label: &str) -> Value {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in points {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    if points.is_empty() {
        return json!({"x": 0, "y": 0, "width": 0, "height": 0, "label": label});
    }
    json!({"x": x0.round(), "y": y0.round(), "width": (x1 - x0).round().max(1.0), "height": (y1 - y0).round().max(1.0), "label": label})
}

/// Decode QR codes with rqrr, falling back to rxing.
pub fn decode_qr(img: &DynamicImage) -> Vec<Decoded> {
    let gray = img.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(gray.clone());
    let mut out: Vec<Decoded> = prepared
        .detect_grids()
        .into_iter()
        .filter_map(|g| {
            let pts = g.bounds.iter().map(|p| (p.x as f64, p.y as f64)).collect();
            g.decode().ok().map(|(_, text)| Decoded {
                text,
                format: "QR_CODE".into(),
                points: pts,
            })
        })
        .collect();
    if out.is_empty() {
        let mut hints = rxing::DecodeHints {
            PossibleFormats: Some(
                [BarcodeFormat::QR_CODE, BarcodeFormat::MICRO_QR_CODE]
                    .into_iter()
                    .collect(),
            ),
            ..Default::default()
        };
        if let Ok(rs) = rxing::helpers::detect_multiple_in_luma_with_hints(
            gray.as_raw().clone(),
            gray.width(),
            gray.height(),
            &mut hints,
        ) {
            out = rs.iter().map(rx_decoded).collect();
        }
    }
    out
}

fn rx_decoded(r: &rxing::RXingResult) -> Decoded {
    Decoded {
        text: r.getText().to_string(),
        format: format!("{:?}", r.getBarcodeFormat()),
        points: r
            .getPoints()
            .iter()
            .map(|p| (p.x as f64, p.y as f64))
            .collect(),
    }
}

fn decoded_output(codes: &[Decoded], what: &str) -> ToolResult<ToolOutput> {
    if codes.is_empty() {
        return Ok(ToolOutput::text("")
            .with_data(json!({"found": 0, "codes": [], "regions": []}))
            .warn(format!("no {what} found in the image")));
    }
    let regions: Vec<Value> = codes
        .iter()
        .map(|c| bbox_region(&c.points, &c.format))
        .collect();
    let list: Vec<Value> = codes
        .iter()
        .map(|c| json!({"text": c.text, "format": c.format}))
        .collect();
    let text = codes
        .iter()
        .map(|c| c.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(ToolOutput::text(text)
        .with_data(json!({"found": codes.len(), "codes": list, "regions": regions})))
}

fn qr_read() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.qr_read", "QR Code Reader", Category::Dev)
        .describe("Decode one or more QR codes from an image or screenshot")
        .keywords(&["qr", "scan", "read", "decode", "qr code reader", "scanner"])
        .param(image_param())
        .text_output("text/plain");
    sync_tool(spec, |ctx, p| {
        let img = load_image(ctx, p.file("input")?)?;
        let mut codes = decode_qr(&img);
        if codes.is_empty() && (img.width() > 1600 || img.height() > 1600) {
            let small = img.resize(1600, 1600, image::imageops::FilterType::Triangle);
            let k = img.width() as f64 / small.width() as f64;
            codes = decode_qr(&small);
            for c in &mut codes {
                c.points.iter_mut().for_each(|p| *p = (p.0 * k, p.1 * k));
            }
        }
        decoded_output(&codes, "QR code")
    })
}

const BARCODE_FORMATS: &[(&str, &str)] = &[
    ("code128", "Code 128"),
    ("ean13", "EAN-13"),
    ("ean8", "EAN-8"),
    ("upca", "UPC-A"),
    ("upce", "UPC-E"),
    ("code39", "Code 39"),
    ("code93", "Code 93"),
    ("itf", "ITF (Interleaved 2 of 5)"),
    ("codabar", "Codabar"),
    ("datamatrix", "Data Matrix"),
    ("pdf417", "PDF417"),
    ("aztec", "Aztec"),
    ("qr", "QR Code"),
];

fn rx_format(s: &str) -> BarcodeFormat {
    match s {
        "ean13" => BarcodeFormat::EAN_13,
        "ean8" => BarcodeFormat::EAN_8,
        "upca" => BarcodeFormat::UPC_A,
        "upce" => BarcodeFormat::UPC_E,
        "code39" => BarcodeFormat::CODE_39,
        "code93" => BarcodeFormat::CODE_93,
        "itf" => BarcodeFormat::ITF,
        "codabar" => BarcodeFormat::CODABAR,
        "datamatrix" => BarcodeFormat::DATA_MATRIX,
        "pdf417" => BarcodeFormat::PDF_417,
        "aztec" => BarcodeFormat::AZTEC,
        "qr" => BarcodeFormat::QR_CODE,
        _ => BarcodeFormat::CODE_128,
    }
}

fn is_1d(s: &str) -> bool {
    !matches!(s, "datamatrix" | "pdf417" | "aztec" | "qr")
}

pub fn barcode_matrix(fmt: &str, text: &str) -> ToolResult<Matrix> {
    let hints = EncodeHints {
        Margin: Some("0".into()),
        ..Default::default()
    };
    let bm = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rxing::MultiFormatWriter.encode_with_hints(text, &rx_format(fmt), 0, 0, &hints)
    }))
    .map_err(|_| ToolError::invalid("text", "content cannot be encoded in this barcode format"))?
    .map_err(|e| ToolError::invalid("text", format!("cannot encode as {fmt}: {e}")))?;
    let (w, h) = (bm.width() as usize, bm.height() as usize);
    if is_1d(fmt) {
        let bits = (0..w).map(|x| bm.get(x as u32, 0)).collect();
        return Ok(Matrix { w, h: 1, bits });
    }
    let mut bits = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            bits.push(bm.get(x as u32, y as u32));
        }
    }
    Ok(Matrix { w, h, bits })
}

fn barcode_generate() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.barcode_generate", "Barcode Generator", Category::Dev)
        .describe("Generate barcodes (Code 128, EAN-13, EAN-8, UPC-A/E, Code 39/93, ITF, Codabar, Data Matrix, PDF417, Aztec) as PNG and SVG")
        .keywords(&["barcode", "ean", "upc", "code128", "code39", "datamatrix", "pdf417", "aztec", "generate", "isbn"])
        .param(ParamSpec::text("text").label("Content").required().primary().placeholder("5901234123457"))
        .param(ParamSpec::choice("format", BARCODE_FORMATS).default_str("code128"))
        .param(ParamSpec::integer("module_width").label("Bar / module size (px)").range(1.0, 50.0).default_int(3))
        .param(ParamSpec::integer("height").label("Bar height (px, 1D)").range(10.0, 2000.0).default_int(120))
        .param(ParamSpec::integer("margin").label("Quiet zone (modules)").range(0.0, 50.0).default_int(10))
        .param(ParamSpec::boolean("show_text").describe("Print the content under 1D barcodes").default_bool(true))
        .param(ParamSpec::color("foreground").default_str("#000000").advanced())
        .param(ParamSpec::color("background").default_str("#ffffff").advanced())
        .output(OutputKind::Files);
    sync_tool(spec, |ctx, p| {
        let fmt = p.str_or("format", "code128");
        let text = p.str("text")?;
        let m = barcode_matrix(fmt, text)?;
        let mw = p.int_or("module_width", 3) as u32;
        let fg = color_param(p, "foreground", "#000000")?;
        let bg = color_param(p, "background", "#ffffff")?;
        let margin = p.int_or("margin", 10) as u32;
        let one_d = is_1d(fmt);
        let qm = if one_d {
            margin
        } else {
            margin.min(4).max(if margin == 0 { 0 } else { 1 })
        };
        let bar_h = if one_d {
            p.int_or("height", 120) as u32
        } else {
            m.h as u32 * mw
        };
        let label = (one_d && p.bool("show_text"))
            .then(|| render_text(ctx, text, (mw as f32 * 6.0).clamp(12.0, 64.0), fg))
            .transpose()?;
        let w = (m.w as u32 + 2 * qm) * mw;
        let vpad = if one_d { mw * 4 } else { qm * mw };
        let label_h = label.as_ref().map(|l| l.height() + mw * 2).unwrap_or(0);
        let h = bar_h + 2 * vpad + label_h;
        check_pixels(ctx, w as u64, h as u64)?;
        let mut img = RgbaImage::from_pixel(w, h, bg);
        let mut svg_rects = String::new();
        for my in 0..m.h {
            for mx in 0..m.w {
                if !m.bits[my * m.w + mx] {
                    continue;
                }
                let x0 = (mx as u32 + qm) * mw;
                let (y0, hh) = if one_d {
                    (vpad, bar_h)
                } else {
                    (vpad + my as u32 * mw, mw)
                };
                for y in y0..y0 + hh {
                    for x in x0..x0 + mw {
                        img.put_pixel(x, y, fg);
                    }
                }
                svg_rects.push_str(&format!("M{x0} {y0}h{mw}v{hh}h-{mw}z"));
            }
        }
        let (fgh, _) = svg_color(fg);
        let (bgh, _) = svg_color(bg);
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" shape-rendering=\"crispEdges\"><rect width=\"{w}\" height=\"{h}\" fill=\"{bgh}\"/><path fill=\"{fgh}\" d=\"{svg_rects}\"/>");
        if let Some(l) = &label {
            let lx = (w.saturating_sub(l.width())) / 2;
            composite(
                &mut img,
                l,
                lx as i64,
                (vpad + bar_h + mw) as i64,
                1.0,
                Blend::Normal,
            );
            let esc = text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            svg.push_str(&format!("<text x=\"{}\" y=\"{}\" font-family=\"DejaVu Sans, Arial, sans-serif\" font-size=\"{:.0}\" text-anchor=\"middle\" fill=\"{fgh}\">{esc}</text>", w / 2, vpad + bar_h + mw + (l.height() as f32 * 0.8) as u32, (mw as f32 * 6.0).clamp(12.0, 64.0)));
        }
        svg.push_str("</svg>");
        let arts = vec![
            save_png(ctx, &img, &format!("barcode-{fmt}.png"))?,
            ctx.write_output(&format!("barcode-{fmt}.svg"), svg.as_bytes())?,
        ];
        Ok(ToolOutput::files(arts)
            .with_text(svg, "image/svg+xml")
            .with_data(json!({"format": fmt, "modules": m.w, "width": w, "height": h})))
    })
}

fn barcode_read() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.barcode_read", "Barcode Reader", Category::Dev)
        .describe("Read 1D and 2D barcodes (EAN, UPC, Code 128/39/93, ITF, Codabar, QR, Data Matrix, PDF417, Aztec) from an image")
        .keywords(&["barcode", "scan", "read", "decode", "ean", "upc", "datamatrix", "pdf417", "scanner"])
        .param(image_param())
        .text_output("text/plain");
    sync_tool(spec, |ctx, p| {
        let img = load_image(ctx, p.file("input")?)?;
        let gray = img.to_luma8();
        let (w, h) = gray.dimensions();
        let mut codes: Vec<Decoded> =
            match rxing::helpers::detect_multiple_in_luma(gray.as_raw().clone(), w, h) {
                Ok(rs) => rs.iter().map(rx_decoded).collect(),
                Err(_) => vec![],
            };
        if codes.is_empty() {
            if let Ok(r) = rxing::helpers::detect_in_luma(gray.as_raw().clone(), w, h, None) {
                codes.push(rx_decoded(&r));
            }
        }
        if codes.is_empty() {
            codes = decode_qr(&img);
        }
        // De-duplicate identical results.
        let mut seen = std::collections::HashSet::new();
        codes.retain(|c| seen.insert((c.text.clone(), c.format.clone())));
        decoded_output(&codes, "barcode")
    })
}
