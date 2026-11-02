//! Design tools: palettes, color extraction, WCAG contrast, gradients,
//! aspect ratios, canvas resizing, composition and bulk dimensions.

use crate::util::*;
use dogubako_core::prelude::*;
use image::{DynamicImage, ImageReader, Rgba, RgbaImage};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        palette(),
        palette_extract(),
        dominant_colors(),
        contrast(),
        gradient(),
        aspect_ratio(),
        canvas_resize(),
        compose(),
        image_dimensions(),
    ]
}

// ---------------------------------------------------------------------------
// Color math

pub fn rgb_to_hsl(c: [u8; 3]) -> (f64, f64, f64) {
    let r = c[0] as f64 / 255.0;
    let g = c[1] as f64 / 255.0;
    let b = c[2] as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-12 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return [v, v, v];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

/// WCAG relative luminance.
pub fn luminance(c: [u8; 3]) -> f64 {
    let ch = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

const NAMED: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("gray", [128, 128, 128]),
    ("silver", [192, 192, 192]),
    ("dim gray", [105, 105, 105]),
    ("red", [255, 0, 0]),
    ("dark red", [139, 0, 0]),
    ("crimson", [220, 20, 60]),
    ("salmon", [250, 128, 114]),
    ("pink", [255, 192, 203]),
    ("hot pink", [255, 105, 180]),
    ("orange", [255, 165, 0]),
    ("dark orange", [255, 140, 0]),
    ("coral", [255, 127, 80]),
    ("gold", [255, 215, 0]),
    ("yellow", [255, 255, 0]),
    ("khaki", [240, 230, 140]),
    ("beige", [245, 245, 220]),
    ("tan", [210, 180, 140]),
    ("brown", [165, 42, 42]),
    ("chocolate", [210, 105, 30]),
    ("maroon", [128, 0, 0]),
    ("olive", [128, 128, 0]),
    ("lime", [0, 255, 0]),
    ("green", [0, 128, 0]),
    ("dark green", [0, 100, 0]),
    ("sea green", [46, 139, 87]),
    ("mint", [152, 255, 152]),
    ("teal", [0, 128, 128]),
    ("turquoise", [64, 224, 208]),
    ("cyan", [0, 255, 255]),
    ("sky blue", [135, 206, 235]),
    ("steel blue", [70, 130, 180]),
    ("blue", [0, 0, 255]),
    ("royal blue", [65, 105, 225]),
    ("navy", [0, 0, 128]),
    ("indigo", [75, 0, 130]),
    ("purple", [128, 0, 128]),
    ("violet", [238, 130, 238]),
    ("magenta", [255, 0, 255]),
    ("lavender", [230, 230, 250]),
    ("slate gray", [112, 128, 144]),
];

pub fn color_name(c: [u8; 3]) -> &'static str {
    NAMED
        .iter()
        .min_by_key(|(_, n)| {
            let dr = c[0] as i32 - n[0] as i32;
            let dg = c[1] as i32 - n[1] as i32;
            let db = c[2] as i32 - n[2] as i32;
            2 * dr * dr + 4 * dg * dg + 3 * db * db
        })
        .map(|(n, _)| *n)
        .unwrap_or("")
}

fn swatch(c: [u8; 3], label: &str) -> Value {
    let (h, s, l) = rgb_to_hsl(c);
    json!({
        "hex": hex(c),
        "name": label,
        "rgb": format!("rgb({}, {}, {})", c[0], c[1], c[2]),
        "hsl": format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s * 100.0, l * 100.0),
    })
}

pub fn rgb3(c: Rgba<u8>) -> [u8; 3] {
    [c[0], c[1], c[2]]
}

// ---------------------------------------------------------------------------

pub fn generate_palette(base: [u8; 3], scheme: &str, count: usize) -> Vec<([u8; 3], String)> {
    let (h, s, l) = rgb_to_hsl(base);
    let rot = |deg: f64, label: &str| (hsl_to_rgb(h + deg, s, l), label.to_string());
    let n = count.clamp(2, 20);
    match scheme {
        "complementary" => vec![rot(0.0, "base"), rot(180.0, "complement")],
        "split_complementary" => vec![
            rot(0.0, "base"),
            rot(150.0, "split 1"),
            rot(210.0, "split 2"),
        ],
        "analogous" => vec![
            rot(-60.0, "-60°"),
            rot(-30.0, "-30°"),
            rot(0.0, "base"),
            rot(30.0, "+30°"),
            rot(60.0, "+60°"),
        ],
        "triadic" => vec![rot(0.0, "base"), rot(120.0, "+120°"), rot(240.0, "+240°")],
        "tetradic" => vec![
            rot(0.0, "base"),
            rot(60.0, "+60°"),
            rot(180.0, "+180°"),
            rot(240.0, "+240°"),
        ],
        "square" => vec![
            rot(0.0, "base"),
            rot(90.0, "+90°"),
            rot(180.0, "+180°"),
            rot(270.0, "+270°"),
        ],
        "shades" => (0..n)
            .map(|i| {
                let k = i as f64 / n as f64;
                let c = [
                    (base[0] as f64 * (1.0 - k)).round() as u8,
                    (base[1] as f64 * (1.0 - k)).round() as u8,
                    (base[2] as f64 * (1.0 - k)).round() as u8,
                ];
                (c, format!("shade {}%", (k * 100.0).round()))
            })
            .collect(),
        "tints" => (0..n)
            .map(|i| {
                let k = i as f64 / n as f64;
                let t = |v: u8| (v as f64 + (255.0 - v as f64) * k).round() as u8;
                (
                    [t(base[0]), t(base[1]), t(base[2])],
                    format!("tint {}%", (k * 100.0).round()),
                )
            })
            .collect(),
        "tones" => (0..n)
            .map(|i| {
                let k = i as f64 / n as f64;
                let t = |v: u8| (v as f64 + (128.0 - v as f64) * k).round() as u8;
                (
                    [t(base[0]), t(base[1]), t(base[2])],
                    format!("tone {}%", (k * 100.0).round()),
                )
            })
            .collect(),
        // Tailwind-like 50..900 scale around the base hue.
        "scale" => {
            let steps = [
                (50, 0.97),
                (100, 0.93),
                (200, 0.86),
                (300, 0.76),
                (400, 0.64),
                (500, 0.52),
                (600, 0.43),
                (700, 0.35),
                (800, 0.27),
                (900, 0.19),
                (950, 0.12),
            ];
            steps
                .iter()
                .map(|(name, lt)| (hsl_to_rgb(h, s.max(0.05), *lt), name.to_string()))
                .collect()
        }
        _ => {
            // monochromatic: vary lightness
            (0..n)
                .map(|i| {
                    let lt = 0.15 + 0.7 * i as f64 / (n - 1) as f64;
                    (hsl_to_rgb(h, s, lt), format!("L {}%", (lt * 100.0).round()))
                })
                .collect()
        }
    }
}

fn palette() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.palette", "Color Palette Generator", Category::Design)
        .describe("Generate harmonious color palettes from a base color: complementary, analogous, triadic, tetradic, monochromatic, shades, tints and a 50–950 scale")
        .keywords(&["palette", "color scheme", "complementary", "analogous", "triadic", "monochromatic", "shades", "tints", "harmony"])
        .param(ParamSpec::color("color").label("Base color").required().primary().default_str("#3366ff"))
        .param(ParamSpec::choice("scheme", &[
            ("complementary", "Complementary"),
            ("split_complementary", "Split complementary"),
            ("analogous", "Analogous"),
            ("triadic", "Triadic"),
            ("tetradic", "Tetradic (rectangle)"),
            ("square", "Square"),
            ("monochromatic", "Monochromatic"),
            ("shades", "Shades (darker)"),
            ("tints", "Tints (lighter)"),
            ("tones", "Tones (grayer)"),
            ("scale", "50–950 scale"),
        ]).default_str("complementary"))
        .param(ParamSpec::integer("count").describe("Number of colors for monochromatic/shades/tints/tones").range(2.0, 20.0).default_int(5))
        .output(OutputKind::Data)
        .ui("color");
    sync_tool(spec, |_ctx, p| {
        let base = rgb3(color_param(p, "color", "#3366ff")?);
        let scheme = p.str_or("scheme", "complementary");
        let pal = generate_palette(base, scheme, p.int_or("count", 5) as usize);
        let colors: Vec<Value> = pal.iter().map(|(c, l)| swatch(*c, l)).collect();
        let css: String = pal
            .iter()
            .enumerate()
            .map(|(i, (c, _))| format!("  --color-{}: {};\n", i + 1, hex(*c)))
            .collect();
        Ok(ToolOutput::data(
            json!({"colors": colors, "scheme": scheme, "css": format!(":root {{\n{css}}}")}),
        ))
    })
}

// ---------------------------------------------------------------------------
// Dominant colors (median-cut seeded k-means)

pub fn sample_pixels(img: &DynamicImage) -> Vec<[f32; 3]> {
    let t = if img.width() > 160 || img.height() > 160 {
        img.thumbnail(160, 160)
    } else {
        img.clone()
    };
    t.to_rgba8()
        .pixels()
        .filter(|p| p[3] >= 128)
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect()
}

fn median_cut(px: &[[f32; 3]], k: usize) -> Vec<[f32; 3]> {
    let mut boxes: Vec<Vec<[f32; 3]>> = vec![px.to_vec()];
    while boxes.len() < k {
        // Split the box with the largest range.
        let (bi, ch, range) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let mut best = (0, 0f32);
                for c in 0..3 {
                    let (mn, mx) = b
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(a, z), p| (a.min(p[c]), z.max(p[c])));
                    if mx - mn > best.1 {
                        best = (c, mx - mn);
                    }
                }
                (i, best.0, best.1)
            })
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap())
            .unwrap_or((0, 0, 0.0));
        if range <= 0.0 {
            break;
        }
        let mut b = boxes.swap_remove(bi);
        b.sort_by(|x, y| x[ch].partial_cmp(&y[ch]).unwrap());
        let hi = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(hi);
    }
    boxes
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let n = b.len() as f32;
            let s = b
                .iter()
                .fold([0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
            [s[0] / n, s[1] / n, s[2] / n]
        })
        .collect()
}

/// Returns (color, fraction) sorted by fraction desc.
pub fn dominant(img: &DynamicImage, k: usize) -> Vec<([u8; 3], f64)> {
    let px = sample_pixels(img);
    if px.is_empty() {
        return vec![];
    }
    let mut centers = median_cut(&px, k.max(1));
    let mut assign = vec![0usize; px.len()];
    for _ in 0..12 {
        let mut changed = false;
        for (i, p) in px.iter().enumerate() {
            let best = centers
                .iter()
                .enumerate()
                .map(|(j, c)| {
                    (
                        j,
                        (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2),
                    )
                })
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|x| x.0)
                .unwrap_or(0);
            if assign[i] != best {
                assign[i] = best;
                changed = true;
            }
        }
        let mut sums = vec![[0f64; 4]; centers.len()];
        for (p, &a) in px.iter().zip(&assign) {
            sums[a][0] += p[0] as f64;
            sums[a][1] += p[1] as f64;
            sums[a][2] += p[2] as f64;
            sums[a][3] += 1.0;
        }
        for (c, s) in centers.iter_mut().zip(&sums) {
            if s[3] > 0.0 {
                *c = [
                    (s[0] / s[3]) as f32,
                    (s[1] / s[3]) as f32,
                    (s[2] / s[3]) as f32,
                ];
            }
        }
        if !changed {
            break;
        }
    }
    let mut counts = vec![0usize; centers.len()];
    for &a in &assign {
        counts[a] += 1;
    }
    let total = px.len() as f64;
    let mut out: Vec<([u8; 3], f64)> = centers
        .iter()
        .zip(&counts)
        .filter(|(_, &n)| n > 0)
        .map(|(c, &n)| {
            (
                [c[0].round() as u8, c[1].round() as u8, c[2].round() as u8],
                n as f64 / total,
            )
        })
        .collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    out
}

fn colors_data(cols: &[([u8; 3], f64)]) -> Vec<Value> {
    cols.iter()
        .map(|(c, f)| {
            let mut s = swatch(*c, &format!("{} · {:.1}%", color_name(*c), f * 100.0));
            s["percent"] = json!((f * 1000.0).round() / 10.0);
            s
        })
        .collect()
}

fn dominant_colors() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.dominant_colors", "Dominant Colors", Category::Image)
        .describe("Find the dominant colors of an image with their share of the picture (median cut + k-means)")
        .keywords(&["dominant color", "main colors", "average color", "color analysis", "kmeans"])
        .param(image_param())
        .param(ParamSpec::integer("count").range(1.0, 16.0).default_int(5))
        .output(OutputKind::Data)
        .ui("color");
    sync_tool(spec, |ctx, p| {
        let img = load_image(ctx, p.file("input")?)?;
        let cols = dominant(&img, p.int_or("count", 5) as usize);
        if cols.is_empty() {
            return Err(ToolError::invalid("input", "image is fully transparent"));
        }
        Ok(ToolOutput::data(json!({"colors": colors_data(&cols)})))
    })
}

fn palette_extract() -> Arc<dyn Tool> {
    let spec = ToolSpec::new(
        "design.palette_extract",
        "Extract Palette from Image",
        Category::Design,
    )
    .describe(
        "Extract a color palette from an image, with percentages, CSS variables and a swatch image",
    )
    .keywords(&[
        "palette",
        "extract colors",
        "color picker",
        "image colors",
        "swatches",
        "theme from image",
    ])
    .param(image_param())
    .param(ParamSpec::integer("count").range(2.0, 16.0).default_int(6))
    .param(
        ParamSpec::choice(
            "sort",
            &[
                ("share", "By share of image"),
                ("lightness", "By lightness"),
                ("hue", "By hue"),
            ],
        )
        .default_str("share"),
    )
    .output(OutputKind::Data)
    .ui("color");
    sync_tool(spec, |ctx, p| {
        let f = p.file("input")?;
        let img = load_image(ctx, f)?;
        let mut cols = dominant(&img, p.int_or("count", 6) as usize);
        if cols.is_empty() {
            return Err(ToolError::invalid("input", "image is fully transparent"));
        }
        match p.str_or("sort", "share") {
            "lightness" => {
                cols.sort_by(|a, b| rgb_to_hsl(a.0).2.partial_cmp(&rgb_to_hsl(b.0).2).unwrap())
            }
            "hue" => {
                cols.sort_by(|a, b| rgb_to_hsl(a.0).0.partial_cmp(&rgb_to_hsl(b.0).0).unwrap())
            }
            _ => {}
        }
        // Swatch strip proportional to share.
        let (w, h) = (600u32, 120u32);
        let mut strip = RgbaImage::new(w, h);
        let mut x0 = 0u32;
        for (i, (c, frac)) in cols.iter().enumerate() {
            let x1 = if i + 1 == cols.len() {
                w
            } else {
                (x0 as f64 + frac * w as f64).round() as u32
            };
            for x in x0..x1.min(w) {
                for y in 0..h {
                    strip.put_pixel(x, y, Rgba([c[0], c[1], c[2], 255]));
                }
            }
            x0 = x1;
        }
        let a = save_png(ctx, &strip, &suffixed(&f.name, "palette", Some("png")))?;
        let css: String = cols
            .iter()
            .enumerate()
            .map(|(i, (c, _))| format!("  --palette-{}: {};\n", i + 1, hex(*c)))
            .collect();
        Ok(ToolOutput::files(vec![a])
            .with_data(json!({"colors": colors_data(&cols), "css": format!(":root {{\n{css}}}")})))
    })
}

// ---------------------------------------------------------------------------
// Contrast

/// Blend a possibly translucent foreground over an opaque background.
fn over(fg: Rgba<u8>, bg: [u8; 3]) -> [u8; 3] {
    let a = fg[3] as f64 / 255.0;
    [0, 1, 2].map(|i| (fg[i] as f64 * a + bg[i] as f64 * (1.0 - a)).round() as u8)
}

/// Closest color (by lightness change) to `fg` reaching `target` contrast on `bg`.
pub fn suggest(fg: [u8; 3], bg: [u8; 3], target: f64) -> Option<[u8; 3]> {
    let (h, s, l) = rgb_to_hsl(fg);
    let mut best: Option<([u8; 3], f64)> = None;
    for dir in [-1.0, 1.0] {
        let mut step = 0.0;
        while step <= 1.0 {
            let c = hsl_to_rgb(h, s, l + dir * step);
            if contrast_ratio(c, bg) >= target {
                if best.map(|b| step < b.1).unwrap_or(true) {
                    best = Some((c, step));
                }
                break;
            }
            step += 0.005;
        }
    }
    best.map(|b| b.0)
}

fn contrast() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.contrast", "Contrast Checker", Category::Design)
        .describe("Check WCAG 2.x contrast between text and background colors (AA/AAA, normal and large text) and get passing suggestions")
        .keywords(&["contrast", "wcag", "accessibility", "a11y", "aa", "aaa", "color contrast", "readability"])
        .param(ParamSpec::color("foreground").label("Text color").required().primary().default_str("#777777"))
        .param(ParamSpec::color("background").label("Background color").required().default_str("#ffffff"))
        .output(OutputKind::Data)
        .ui("color");
    sync_tool(spec, |_ctx, p| {
        let bg_c = color_param(p, "background", "#ffffff")?;
        let bg = over(bg_c, [255, 255, 255]);
        let fg_c = color_param(p, "foreground", "#777777")?;
        let fg = over(fg_c, bg);
        let r = contrast_ratio(fg, bg);
        let rr = (r * 100.0).floor() / 100.0;
        let pass = |t: f64| r >= t;
        let mut suggestions = vec![];
        for (label, t) in [
            ("AA normal text (4.5:1)", 4.5),
            ("AAA normal text (7:1)", 7.0),
        ] {
            if !pass(t) {
                if let Some(c) = suggest(fg, bg, t) {
                    suggestions.push(json!({"hex": hex(c), "name": format!("text for {label}"), "ratio": (contrast_ratio(c, bg) * 100.0).floor() / 100.0}));
                }
                if let Some(c) = suggest(bg, fg, t) {
                    suggestions.push(json!({"hex": hex(c), "name": format!("background for {label}"), "ratio": (contrast_ratio(fg, c) * 100.0).floor() / 100.0}));
                }
            }
        }
        let rows = vec![
            json!(["AA", "Normal text", "4.5:1", pass(4.5)]),
            json!(["AA", "Large text (18pt / 14pt bold)", "3:1", pass(3.0)]),
            json!(["AA", "UI components & graphics", "3:1", pass(3.0)]),
            json!(["AAA", "Normal text", "7:1", pass(7.0)]),
            json!(["AAA", "Large text", "4.5:1", pass(4.5)]),
        ];
        let mut colors = vec![
            json!({"hex": hex(fg), "name": "text"}),
            json!({"hex": hex(bg), "name": "background"}),
        ];
        colors.extend(suggestions.iter().cloned());
        Ok(ToolOutput::data(json!({
            "ratio": rr,
            "ratio_text": format!("{rr:.2}:1"),
            "aa_normal": pass(4.5),
            "aa_large": pass(3.0),
            "aaa_normal": pass(7.0),
            "aaa_large": pass(4.5),
            "table": {"columns": ["Level", "Use", "Required", "Pass"], "rows": rows},
            "colors": colors,
            "suggestions": suggestions,
        })))
    })
}

// ---------------------------------------------------------------------------
// Gradient

/// Parse "#f00, #00f 70%, rgba-ish..." into (color, position 0..1) stops.
pub fn parse_stops(s: &str) -> ToolResult<Vec<(Rgba<u8>, f64)>> {
    let parts: Vec<&str> = s
        .split([',', '\n', ';'])
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .collect();
    if parts.len() < 2 {
        return Err(ToolError::invalid(
            "stops",
            "need at least two colors, e.g. \"#ff7e5f, #feb47b\"",
        ));
    }
    let mut stops: Vec<(Rgba<u8>, Option<f64>)> = vec![];
    for part in parts {
        let mut it = part.split_whitespace();
        let c = parse_color("stops", it.next().unwrap_or(""))?;
        let pos = match it.next() {
            Some(t) => Some(
                t.trim_end_matches('%')
                    .parse::<f64>()
                    .map_err(|_| ToolError::invalid("stops", format!("bad position '{t}'")))?
                    / 100.0,
            ),
            None => None,
        };
        stops.push((c, pos));
    }
    let n = stops.len();
    if stops[0].1.is_none() {
        stops[0].1 = Some(0.0);
    }
    if stops[n - 1].1.is_none() {
        stops[n - 1].1 = Some(1.0);
    }
    // Fill missing positions evenly between known ones (CSS behavior).
    let mut i = 0;
    while i < n {
        if stops[i].1.is_none() {
            let start = i - 1;
            let mut end = i;
            while stops[end].1.is_none() {
                end += 1;
            }
            let (a, b) = (stops[start].1.unwrap(), stops[end].1.unwrap());
            for (k, stop) in stops.iter_mut().enumerate().take(end).skip(i) {
                stop.1 = Some(a + (b - a) * (k - start) as f64 / (end - start) as f64);
            }
            i = end;
        }
        i += 1;
    }
    let mut last = 0f64;
    Ok(stops
        .into_iter()
        .map(|(c, p)| {
            let p = p.unwrap().clamp(0.0, 1.0).max(last);
            last = p;
            (c, p)
        })
        .collect())
}

fn sample_stops(stops: &[(Rgba<u8>, f64)], t: f64) -> Rgba<u8> {
    let t = t.clamp(0.0, 1.0);
    if t <= stops[0].1 {
        return stops[0].0;
    }
    for w in stops.windows(2) {
        let ((ca, pa), (cb, pb)) = (w[0], w[1]);
        if t <= pb {
            let k = if pb > pa { (t - pa) / (pb - pa) } else { 1.0 };
            // Premultiplied interpolation, like CSS.
            let (aa, ab) = (ca[3] as f64 / 255.0, cb[3] as f64 / 255.0);
            let a = aa + (ab - aa) * k;
            let ch = |i: usize| {
                let v = (ca[i] as f64 * aa) + ((cb[i] as f64 * ab) - (ca[i] as f64 * aa)) * k;
                if a > 0.0 {
                    (v / a).round().clamp(0.0, 255.0) as u8
                } else {
                    0
                }
            };
            return Rgba([ch(0), ch(1), ch(2), (a * 255.0).round() as u8]);
        }
    }
    stops[stops.len() - 1].0
}

pub fn render_gradient(
    w: u32,
    h: u32,
    kind: &str,
    angle_deg: f64,
    stops: &[(Rgba<u8>, f64)],
) -> RgbaImage {
    let (wf, hf) = (w as f64, h as f64);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let a = angle_deg.to_radians();
    let (sa, ca) = (a.sin(), a.cos());
    let len = (wf * sa).abs() + (hf * ca).abs();
    let maxd = (cx * cx + cy * cy).sqrt();
    RgbaImage::from_fn(w, h, |x, y| {
        let (px, py) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
        let t = match kind {
            "radial" => (px * px + py * py).sqrt() / maxd.max(1e-9),
            "conic" => ((px.atan2(-py).to_degrees() - angle_deg).rem_euclid(360.0)) / 360.0,
            _ => (px * sa - py * ca) / len.max(1e-9) + 0.5,
        };
        sample_stops(stops, t)
    })
}

fn gradient() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.gradient", "Gradient Generator", Category::Design)
        .describe(
            "Create CSS linear, radial or conic gradients from color stops and render them as PNG",
        )
        .keywords(&[
            "gradient",
            "css gradient",
            "linear-gradient",
            "radial",
            "background",
            "wallpaper",
        ])
        .param(
            ParamSpec::textarea("stops")
                .label("Color stops")
                .required()
                .primary()
                .placeholder("#ff7e5f, #feb47b 60%, #86a8e7")
                .default_str("#ff7e5f, #feb47b"),
        )
        .param(
            ParamSpec::choice(
                "kind",
                &[
                    ("linear", "Linear"),
                    ("radial", "Radial"),
                    ("conic", "Conic"),
                ],
            )
            .default_str("linear"),
        )
        .param(
            ParamSpec::number("angle")
                .label("Angle (°)")
                .range(-360.0, 360.0)
                .default_num(90.0)
                .show_if("kind", &["linear", "conic"]),
        )
        .param(
            ParamSpec::integer("width")
                .range(1.0, 8000.0)
                .default_int(1200),
        )
        .param(
            ParamSpec::integer("height")
                .range(1.0, 8000.0)
                .default_int(630),
        )
        .text_output("text/plain")
        .output(OutputKind::Files);
    sync_tool(spec, |ctx, p| {
        let stops = parse_stops(p.str("stops")?)?;
        let kind = p.str_or("kind", "linear");
        let angle = p.num_or("angle", 90.0);
        let (w, h) = (
            p.int_or("width", 1200) as u32,
            p.int_or("height", 630) as u32,
        );
        check_pixels(ctx, w as u64, h as u64)?;
        let list: Vec<String> = stops
            .iter()
            .map(|(c, pos)| format!("{} {}%", css_color(*c), (pos * 1000.0).round() / 10.0))
            .collect();
        let css = match kind {
            "radial" => format!(
                "radial-gradient(circle farthest-corner at center, {})",
                list.join(", ")
            ),
            "conic" => format!(
                "conic-gradient(from {angle}deg at center, {})",
                list.join(", ")
            ),
            _ => format!("linear-gradient({angle}deg, {})", list.join(", ")),
        };
        let img = render_gradient(w, h, kind, angle, &stops);
        let a = save_png(ctx, &img, &format!("gradient-{w}x{h}.png"))?;
        let colors: Vec<Value> = stops.iter().map(|(c, pos)| json!({"hex": hex_rgba(*c), "name": format!("{}%", (pos * 1000.0).round() / 10.0)})).collect();
        Ok(ToolOutput::files(vec![a])
            .with_text(format!("background: {css};"), "text/plain")
            .with_data(json!({"css": css, "colors": colors})))
    })
}

fn css_color(c: Rgba<u8>) -> String {
    if c[3] == 255 {
        hex([c[0], c[1], c[2]])
    } else {
        format!(
            "rgba({}, {}, {}, {:.3})",
            c[0],
            c[1],
            c[2],
            c[3] as f64 / 255.0
        )
    }
}

// ---------------------------------------------------------------------------

const COMMON_RATIOS: &[(&str, f64)] = &[
    ("1:1", 1.0),
    ("5:4", 1.25),
    ("4:3", 4.0 / 3.0),
    ("3:2", 1.5),
    ("16:10", 1.6),
    ("16:9", 16.0 / 9.0),
    ("1.85:1", 1.85),
    ("2:1", 2.0),
    ("21:9", 21.0 / 9.0),
    ("2.39:1", 2.39),
    ("3:1", 3.0),
    ("4:5", 0.8),
    ("3:4", 0.75),
    ("2:3", 2.0 / 3.0),
    ("9:16", 9.0 / 16.0),
];

fn aspect_ratio() -> Arc<dyn Tool> {
    let spec = ToolSpec::new(
        "design.aspect_ratio",
        "Aspect Ratio Calculator",
        Category::Design,
    )
    .describe("Calculate the aspect ratio of a size, or the missing width/height for a given ratio")
    .keywords(&[
        "aspect ratio",
        "ratio",
        "16:9",
        "resolution",
        "dimensions",
        "calculator",
        "scale",
    ])
    .param(
        ParamSpec::integer("width")
            .min(1.0)
            .describe("Original width"),
    )
    .param(
        ParamSpec::integer("height")
            .min(1.0)
            .describe("Original height"),
    )
    .param(
        ParamSpec::text("ratio")
            .placeholder("16:9")
            .describe("Use this ratio instead of width × height"),
    )
    .param(
        ParamSpec::integer("target_width")
            .min(1.0)
            .describe("New width → compute height"),
    )
    .param(
        ParamSpec::integer("target_height")
            .min(1.0)
            .describe("New height → compute width"),
    )
    .output(OutputKind::Data)
    .ui("live");
    sync_tool(spec, |_ctx, p| {
        let r = match (p.opt_str("ratio"), p.opt_int("width"), p.opt_int("height")) {
            (Some(s), _, _) => crate::edit::parse_ratio(s)
                .ok_or_else(|| ToolError::invalid("ratio", "expected a ratio like 16:9 or 1.85"))?,
            (None, Some(w), Some(h)) if w > 0 && h > 0 => w as f64 / h as f64,
            _ => {
                return Err(ToolError::invalid(
                    "width",
                    "enter width and height, or a ratio",
                ))
            }
        };
        let mut data = json!({
            "decimal": (r * 10000.0).round() / 10000.0,
            "orientation": if (r - 1.0).abs() < 1e-9 { "square" } else if r > 1.0 { "landscape" } else { "portrait" },
        });
        if let (None, Some(w), Some(h)) =
            (p.opt_str("ratio"), p.opt_int("width"), p.opt_int("height"))
        {
            data["ratio"] = json!(ratio_string(w as u64, h as u64));
            data["megapixels"] = json!(((w * h) as f64 / 1e4).round() / 100.0);
        }
        let nearest = COMMON_RATIOS
            .iter()
            .min_by(|a, b| (a.1 - r).abs().partial_cmp(&(b.1 - r).abs()).unwrap())
            .unwrap();
        data["nearest_common"] = json!(nearest.0);
        data["exact_common"] = json!((nearest.1 - r).abs() < 0.005);
        if let Some(tw) = p.opt_int("target_width") {
            data["target_width"] = json!(tw);
            data["computed_height"] = json!((tw as f64 / r).round() as i64);
        }
        if let Some(th) = p.opt_int("target_height") {
            data["target_height"] = json!(th);
            data["computed_width"] = json!((th as f64 * r).round() as i64);
        }
        Ok(ToolOutput::data(data))
    })
}

fn canvas_resize() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.canvas_resize", "Resize Canvas / Pad", Category::Design)
        .describe("Extend or trim the canvas around images to an exact size, an aspect ratio or by padding, with a color or transparent fill")
        .keywords(&["canvas", "pad", "padding", "border", "extend", "letterbox", "square", "instagram", "frame"])
        .param(images_param())
        .param(ParamSpec::choice("mode", &[("aspect", "To aspect ratio"), ("size", "To exact size"), ("padding", "Add padding")]).default_str("aspect"))
        .param(ParamSpec::text("aspect").label("Aspect ratio").placeholder("1:1").default_str("1:1").show_if("mode", &["aspect"]))
        .param(ParamSpec::integer("width").min(1.0).show_if("mode", &["size"]))
        .param(ParamSpec::integer("height").min(1.0).show_if("mode", &["size"]))
        .param(ParamSpec::integer("padding").describe("Pixels on every side").range(0.0, 10000.0).default_int(50).show_if("mode", &["padding"]))
        .param(ParamSpec::choice("anchor", POSITIONS).describe("Where the original sits on the new canvas").default_str("center"))
        .param(ParamSpec::color("background").describe("Fill color (#00000000 = transparent)").default_str("#ffffff"))
        .param(ParamSpec::choice("format", &[("same", "Same as input"), ("png", "PNG"), ("jpg", "JPEG"), ("webp", "WebP")]).default_str("same").advanced())
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let bg = color_param(p, "background", "#ffffff")?;
        let arts = for_each_file(ctx, p.files("input"), "Extending", |f| {
            let img = load_image(ctx, f)?;
            let (w, h) = (img.width(), img.height());
            let (nw, nh) = match p.str_or("mode", "aspect") {
                "size" => (
                    opt_dim(p, "width")
                        .ok_or_else(|| ToolError::invalid("width", "is required"))?,
                    opt_dim(p, "height")
                        .ok_or_else(|| ToolError::invalid("height", "is required"))?,
                ),
                "padding" => {
                    let pd = p.int_or("padding", 50) as u32;
                    (w + 2 * pd, h + 2 * pd)
                }
                _ => {
                    let r = crate::edit::parse_ratio(p.str_or("aspect", "1:1"))
                        .ok_or_else(|| ToolError::invalid("aspect", "expected a ratio like 1:1"))?;
                    if (w as f64 / h as f64) < r {
                        (((h as f64) * r).round() as u32, h)
                    } else {
                        (w, ((w as f64) / r).round() as u32)
                    }
                }
            };
            check_pixels(ctx, nw as u64, nh as u64)?;
            let mut canvas = RgbaImage::from_pixel(nw, nh, bg);
            let (x, y) = anchor(p.str_or("anchor", "center"), nw, nh, w, h, 0);
            composite(&mut canvas, &img.to_rgba8(), x, y, 1.0, Blend::Normal);
            let mut fmt = pick_format(p.str_or("format", "same"), &f.name)?;
            if bg[3] < 255 && !fmt.supports_alpha() {
                fmt = OutFmt::Png;
            }
            save_output(
                ctx,
                &DynamicImage::ImageRgba8(canvas),
                &suffixed(&f.name, &format!("{nw}x{nh}"), Some(fmt.ext())),
                fmt,
                &SaveOpts {
                    quality: 92,
                    background: bg,
                    lossless: false,
                },
            )
        })?;
        Ok(ToolOutput::files(arts))
    })
}

fn compose() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.compose", "Overlay Images", Category::Design)
        .describe("Place one image on top of another with position, scale, opacity and blend mode (multiply, screen, overlay...)")
        .keywords(&["overlay", "compose", "composite", "layer", "blend", "superimpose", "place logo"])
        .param(ParamSpec::file("input").label("Base image").required().primary().accept(IMAGE_ACCEPT))
        .param(ParamSpec::file("overlay").label("Overlay image").required().accept(IMAGE_ACCEPT))
        .param(ParamSpec::choice("position", &[("center", "Center"), ("top-left", "Top left"), ("top", "Top"), ("top-right", "Top right"), ("left", "Left"), ("right", "Right"), ("bottom-left", "Bottom left"), ("bottom", "Bottom"), ("bottom-right", "Bottom right"), ("custom", "Custom x/y")]).default_str("center"))
        .param(ParamSpec::integer("x").default_int(0).show_if("position", &["custom"]))
        .param(ParamSpec::integer("y").default_int(0).show_if("position", &["custom"]))
        .param(ParamSpec::number("scale").label("Overlay width (% of base, 0 = original)").range(0.0, 400.0).default_num(0.0))
        .param(ParamSpec::number("opacity").range(0.0, 1.0).step(0.05).default_num(1.0))
        .param(ParamSpec::choice("blend", &[("normal", "Normal"), ("multiply", "Multiply"), ("screen", "Screen"), ("overlay", "Overlay"), ("darken", "Darken"), ("lighten", "Lighten"), ("difference", "Difference"), ("add", "Add")]).default_str("normal"))
        .param(ParamSpec::integer("margin").range(0.0, 10000.0).default_int(0).advanced())
        .param(ParamSpec::choice("format", &[("png", "PNG"), ("jpg", "JPEG"), ("webp", "WebP")]).default_str("png"))
        .output(OutputKind::Files);
    sync_tool(spec, |ctx, p| {
        let fb = p.file("input")?;
        let base = load_image(ctx, fb)?;
        let mut ov = load_image(ctx, p.file("overlay")?)?.to_rgba8();
        let scale = p.num_or("scale", 0.0);
        if scale > 0.0 {
            let tw = (base.width() as f64 * scale / 100.0).round().max(1.0) as u32;
            let th = (ov.height() as f64 * tw as f64 / ov.width() as f64)
                .round()
                .max(1.0) as u32;
            check_pixels(ctx, tw as u64, th as u64)?;
            ov = image::imageops::resize(&ov, tw, th, image::imageops::FilterType::Lanczos3);
        }
        let mut canvas = base.to_rgba8();
        let pos = p.str_or("position", "center");
        let (x, y) = if pos == "custom" {
            (p.int_or("x", 0), p.int_or("y", 0))
        } else {
            anchor(
                pos,
                canvas.width(),
                canvas.height(),
                ov.width(),
                ov.height(),
                p.int_or("margin", 0),
            )
        };
        composite(
            &mut canvas,
            &ov,
            x,
            y,
            p.num_or("opacity", 1.0) as f32,
            Blend::parse(p.str_or("blend", "normal")),
        );
        let fmt = OutFmt::parse(p.str_or("format", "png")).unwrap_or(OutFmt::Png);
        let a = save_output(
            ctx,
            &DynamicImage::ImageRgba8(canvas),
            &suffixed(&fb.name, "composed", Some(fmt.ext())),
            fmt,
            &SaveOpts::default(),
        )?;
        Ok(ToolOutput::files(vec![a]).with_data(json!({"regions": [{"x": x, "y": y, "width": ov.width(), "height": ov.height(), "label": "overlay"}]})))
    })
}

fn image_dimensions() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("design.image_dimensions", "Bulk Image Dimensions", Category::Design)
        .describe("List width, height, aspect ratio, megapixels, DPI and format of many images at once (reads headers only, fast)")
        .keywords(&["dimensions", "size", "resolution", "dpi", "batch", "list", "inventory", "audit"])
        .param(ParamSpec::files("input").label("Images").required().primary().accept(IMAGE_ACCEPT))
        .output(OutputKind::Data)
        .progress();
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let mut rows = vec![];
        let mut csv = String::from("file,format,width,height,aspect,megapixels,dpi,bytes\n");
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(i as f32 / files.len() as f32, "");
            let (fmt, dims) = if looks_like_svg(&f.path) {
                let d = std::fs::read(&f.path)
                    .ok()
                    .and_then(|b| crate::svg::parse_svg(&b).ok())
                    .map(|t| {
                        (
                            t.size().width().round() as u32,
                            t.size().height().round() as u32,
                        )
                    });
                ("SVG".to_string(), d)
            } else {
                match ImageReader::open(&f.path).and_then(|r| r.with_guessed_format()) {
                    Ok(r) => {
                        let fmt = r
                            .format()
                            .map(|x| format!("{x:?}").to_uppercase())
                            .unwrap_or_else(|| f.extension().to_uppercase());
                        (fmt, r.into_dimensions().ok())
                    }
                    Err(_) => (f.extension().to_uppercase(), None),
                }
            };
            let dpi = crate::convert::read_dpi(f)
                .map(|(x, y)| {
                    if (x - y).abs() < 0.5 {
                        format!("{x:.0}")
                    } else {
                        format!("{x:.0}×{y:.0}")
                    }
                })
                .unwrap_or_default();
            match dims {
                Some((w, h)) if w > 0 && h > 0 => {
                    let mp = ((w as f64 * h as f64) / 1e4).round() / 100.0;
                    let ar = ratio_string(w as u64, h as u64);
                    csv.push_str(&format!(
                        "\"{}\",{fmt},{w},{h},{ar},{mp},{dpi},{}\n",
                        f.name.replace('"', "\"\""),
                        f.size()
                    ));
                    rows.push(json!([
                        f.name,
                        fmt,
                        w,
                        h,
                        ar,
                        mp,
                        dpi,
                        human_size(f.size())
                    ]));
                }
                _ => rows.push(json!([
                    f.name,
                    fmt,
                    null,
                    null,
                    "unreadable",
                    null,
                    dpi,
                    human_size(f.size())
                ])),
            }
        }
        Ok(ToolOutput::data(json!({"table": {"columns": ["File", "Format", "Width", "Height", "Aspect", "MP", "DPI", "Size"], "rows": rows}})).with_text(csv, "text/csv"))
    })
}
