//! SVG rasterization (resvg) and conservative SVG minification.

use crate::util::*;
use dogubako_core::prelude::*;
use image::{Rgba, RgbaImage};
use quick_xml::events::Event;
use quick_xml::Reader;
use resvg::tiny_skia;
use resvg::usvg;
use std::collections::HashSet;
use std::sync::OnceLock;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![svg_to_raster(), svg_optimize()]
}

fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(FONT_BYTES.to_vec());
        db.load_system_fonts();
        db.set_sans_serif_family("DejaVu Sans");
        Arc::new(db)
    })
    .clone()
}

/// Parse SVG with external references disabled (only embedded data: images).
pub fn parse_svg(data: &[u8]) -> Result<usvg::Tree, String> {
    let opt = usvg::Options {
        font_family: "DejaVu Sans".into(),
        fontdb: fontdb(),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        resources_dir: None,
        ..Default::default()
    };
    usvg::Tree::from_data(data, &opt).map_err(|e| format!("invalid SVG: {e}"))
}

/// Render a parsed tree to a straight-alpha RGBA image of exactly w×h.
pub fn render_tree(
    ctx: &ToolContext,
    tree: &usvg::Tree,
    w: u32,
    h: u32,
    bg: Option<Rgba<u8>>,
) -> ToolResult<RgbaImage> {
    check_pixels(ctx, w as u64, h as u64)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(w, h).ok_or_else(|| ToolError::input("invalid output size"))?;
    if let Some(c) = bg {
        pixmap.fill(tiny_skia::Color::from_rgba8(c[0], c[1], c[2], c[3]));
    }
    let size = tree.size();
    let t = tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(tree, t, &mut pixmap.as_mut());
    let mut img = RgbaImage::new(w, h);
    for (o, p) in img.pixels_mut().zip(pixmap.pixels()) {
        let c = p.demultiply();
        *o = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(img)
}

fn svg_to_raster() -> Arc<dyn Tool> {
    let spec = ToolSpec::new(
        "image.svg_to_raster",
        "SVG to PNG/JPEG/WebP",
        Category::Image,
    )
    .describe("Rasterize SVG files to PNG, JPEG, WebP or other formats at any size or scale")
    .keywords(&["svg", "rasterize", "png", "vector", "render", "export"])
    .param(
        ParamSpec::files("input")
            .label("SVG files")
            .required()
            .primary()
            .accept(&[".svg", ".svgz", "image/svg+xml"]),
    )
    .param(
        ParamSpec::choice(
            "format",
            &[
                ("png", "PNG"),
                ("jpg", "JPEG"),
                ("webp", "WebP"),
                ("avif", "AVIF"),
                ("bmp", "BMP"),
                ("tiff", "TIFF"),
                ("ico", "ICO"),
            ],
        )
        .default_str("png"),
    )
    .param(
        ParamSpec::integer("width")
            .describe("Output width in pixels (keeps aspect ratio if height is empty)")
            .min(1.0)
            .max(20000.0),
    )
    .param(
        ParamSpec::integer("height")
            .describe("Output height in pixels")
            .min(1.0)
            .max(20000.0),
    )
    .param(
        ParamSpec::number("scale")
            .describe(
                "Scale factor relative to the SVG's own size (used when width/height are empty)",
            )
            .range(0.01, 64.0)
            .default_num(1.0),
    )
    .param(
        ParamSpec::color("background")
            .describe("Background color; leave empty for transparent")
            .advanced(),
    )
    .param(quality_param().advanced())
    .output(OutputKind::Files)
    .progress();
    sync_tool(spec, |ctx, p| {
        let fmt = OutFmt::parse(p.str_or("format", "png"))
            .ok_or_else(|| ToolError::invalid("format", "unsupported"))?;
        let bg = match p.opt_str("background") {
            Some(c) => Some(parse_color("background", c)?),
            None => None,
        };
        let files = p.files("input");
        let mut dims = vec![];
        let arts = for_each_file(ctx, files, "Rendering", |f| {
            let data = std::fs::read(&f.path)?;
            let tree = parse_svg(&data)
                .map_err(|e| ToolError::invalid("input", format!("{}: {e}", f.name)))?;
            let s = tree.size();
            let (w, h) = match (opt_dim(p, "width"), opt_dim(p, "height")) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => (
                    w,
                    ((w as f32) * s.height() / s.width()).round().max(1.0) as u32,
                ),
                (None, Some(h)) => (
                    ((h as f32) * s.width() / s.height()).round().max(1.0) as u32,
                    h,
                ),
                (None, None) => {
                    let k = p.num_or("scale", 1.0) as f32;
                    (
                        (s.width() * k).round().max(1.0) as u32,
                        (s.height() * k).round().max(1.0) as u32,
                    )
                }
            };
            let img = render_tree(ctx, &tree, w, h, bg)?;
            dims.push(json!([f.name, w, h]));
            let opts = SaveOpts {
                quality: p.int_or("quality", 85) as u8,
                background: bg.unwrap_or(Rgba([255, 255, 255, 255])),
                lossless: false,
            };
            save_output(
                ctx,
                &image::DynamicImage::ImageRgba8(img),
                &with_extension(&f.name, fmt.ext()),
                fmt,
                &opts,
            )
        })?;
        Ok(ToolOutput::files(arts)
            .with_data(json!({"table": {"columns": ["Input", "Width", "Height"], "rows": dims}})))
    })
}

// ---------------------------------------------------------------------------
// SVG optimizer

const EDITOR_NS: &[&str] = &[
    "http://www.inkscape.org/namespaces/inkscape",
    "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd",
    "http://inkscape.sourceforge.net/DTD/sodipodi-0.dtd",
    "http://www.bohemiancoding.com/sketch/ns",
    "http://www.serif.com/",
    "http://ns.adobe.com/AdobeIllustrator/10.0/",
    "http://ns.adobe.com/AdobeSVGViewerExtensions/3.0/",
    "http://ns.adobe.com/Extensibility/1.0/",
    "http://ns.adobe.com/Flows/1.0/",
    "http://ns.adobe.com/ImageReplacement/1.0/",
    "http://ns.adobe.com/GenericCustomNamespace/1.0/",
    "http://ns.adobe.com/XPath/1.0/",
    "http://ns.adobe.com/SaveForWeb/1.0/",
    "http://ns.adobe.com/Variables/1.0/",
    "http://www.vector.evaxdesign.sk",
    "http://www.figma.com/figma/ns",
];

const PRESERVE_WS: &[&str] = &[
    "text",
    "tspan",
    "textPath",
    "style",
    "script",
    "title",
    "desc",
    "pre",
    "foreignObject",
];

const NUMERIC_ATTRS: &[&str] = &[
    "d",
    "points",
    "viewBox",
    "transform",
    "gradientTransform",
    "patternTransform",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "fx",
    "fy",
    "width",
    "height",
    "stroke-width",
    "stroke-dashoffset",
    "stroke-dasharray",
    "opacity",
    "fill-opacity",
    "stroke-opacity",
    "stop-opacity",
    "font-size",
];

pub struct OptimizeOptions {
    pub precision: Option<usize>,
    pub remove_metadata: bool,
    pub remove_editor_data: bool,
    pub remove_comments: bool,
    pub collapse_whitespace: bool,
}

fn local(name: &str) -> &str {
    name.rsplit_once(':').map(|(_, l)| l).unwrap_or(name)
}

fn prefix(name: &str) -> Option<&str> {
    name.split_once(':').map(|(p, _)| p)
}

enum Ev {
    Start(String, Vec<(String, String)>, bool),
    End(String),
    Text(String),
    Comment(String),
    DocType(String),
    Raw(String),
    Skip,
}

fn to_ev(ev: Event<'_>) -> Result<Ev, String> {
    let attrs = |e: &quick_xml::events::BytesStart<'_>| -> Result<Vec<(String, String)>, String> {
        e.attributes()
            .with_checks(false)
            .map(|a| {
                a.map(|a| (a.key.as_ref().to_string(), a.value.to_string()))
                    .map_err(|e| format!("bad attribute: {e}"))
            })
            .collect()
    };
    Ok(match ev {
        Event::Start(e) => Ev::Start(e.name().as_ref().to_string(), attrs(&e)?, false),
        Event::Empty(e) => Ev::Start(e.name().as_ref().to_string(), attrs(&e)?, true),
        Event::End(e) => Ev::End(e.name().as_ref().to_string()),
        Event::Text(t) => Ev::Text(t.as_ref().to_string()),
        Event::GeneralRef(r) => Ev::Raw(format!("&{};", r.as_ref())),
        Event::CData(c) => Ev::Raw(format!("<![CDATA[{}]]>", c.as_ref())),
        Event::Comment(c) => Ev::Comment(c.as_ref().to_string()),
        Event::DocType(d) => Ev::DocType(d.as_ref().to_string()),
        Event::PI(pi) => Ev::Raw(format!("<?{}?>", pi.as_ref())),
        Event::Decl(_) | Event::Eof => Ev::Skip,
    })
}

fn is_metadata_ns(px: &str) -> bool {
    matches!(px, "rdf" | "cc" | "dc" | "dcterms")
}

/// Conservative SVG minifier. Works on the token stream, preserving
/// attribute values verbatim (raw, still escaped) except for optional
/// number rounding.
pub fn optimize_svg(input: &str, o: &OptimizeOptions) -> Result<String, String> {
    let read_all = || -> Result<Vec<Ev>, String> {
        let mut r = Reader::from_str(input);
        let mut v = vec![];
        loop {
            let ev = r
                .read_event()
                .map_err(|e| format!("XML error at byte {}: {e}", r.buffer_position()))?;
            if matches!(ev, Event::Eof) {
                break;
            }
            v.push(to_ev(ev)?);
        }
        Ok(v)
    };
    let input_events = read_all()?;

    // Editor namespace prefixes declared anywhere.
    let mut editor_prefixes: HashSet<String> = HashSet::new();
    if o.remove_editor_data {
        for ev in &input_events {
            if let Ev::Start(_, attrs, _) = ev {
                for (k, v) in attrs {
                    if let Some(px) = k.strip_prefix("xmlns:") {
                        if EDITOR_NS
                            .iter()
                            .any(|ns| v.trim_end_matches('/') == ns.trim_end_matches('/'))
                        {
                            editor_prefixes.insert(px.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut used_prefixes: HashSet<String> = HashSet::new();
    let mut stack: Vec<bool> = vec![]; // preserve-whitespace flag per open element
    let mut skip_depth = 0usize;
    let mut events: Vec<Ev> = vec![];
    let mut saw_root = false;

    for ev in input_events {
        match ev {
            Ev::Start(name, attrs, empty) => {
                if skip_depth > 0 {
                    if !empty {
                        skip_depth += 1;
                    }
                    continue;
                }
                let lname = local(&name).to_string();
                let px = prefix(&name).map(|p| p.to_string());
                let remove = (o.remove_metadata && px.is_none() && lname == "metadata")
                    || (o.remove_editor_data
                        && px
                            .as_ref()
                            .map(|p| editor_prefixes.contains(p))
                            .unwrap_or(false));
                if remove {
                    if !empty {
                        skip_depth = 1;
                    }
                    continue;
                }
                saw_root = true;
                if let Some(p) = px {
                    used_prefixes.insert(p);
                }
                let mut preserve =
                    PRESERVE_WS.contains(&lname.as_str()) || stack.last().copied().unwrap_or(false);
                let mut kept = vec![];
                for (k, v) in attrs {
                    if k == "xml:space" && v == "preserve" {
                        preserve = true;
                    }
                    if let Some(p) = prefix(&k) {
                        if p == "xmlns" {
                            kept.push((k, v));
                            continue;
                        }
                        if o.remove_editor_data && editor_prefixes.contains(p) {
                            continue;
                        }
                        used_prefixes.insert(p.to_string());
                    }
                    let v = match o.precision {
                        Some(pr) if k == "d" => round_path_numbers(&v, pr),
                        Some(pr) if NUMERIC_ATTRS.contains(&k.as_str()) => round_numbers(&v, pr),
                        _ => v,
                    };
                    kept.push((k, v));
                }
                events.push(Ev::Start(name, kept, empty));
                if !empty {
                    stack.push(preserve);
                }
            }
            Ev::End(name) => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                stack.pop();
                events.push(Ev::End(name));
            }
            Ev::Text(t) => {
                if skip_depth > 0 {
                    continue;
                }
                let preserve = stack.last().copied().unwrap_or(false);
                let ws_only = t.chars().all(|c| c.is_ascii_whitespace());
                if ws_only && (stack.is_empty() || (o.collapse_whitespace && !preserve)) {
                    continue;
                }
                events.push(Ev::Text(t));
            }
            Ev::Comment(c) => {
                if skip_depth > 0 || o.remove_comments {
                    continue;
                }
                events.push(Ev::Comment(c));
            }
            Ev::Raw(s) => {
                if skip_depth == 0 {
                    events.push(Ev::Raw(s));
                }
            }
            Ev::DocType(d) => {
                // Keep a DOCTYPE only when it declares entities (internal subset).
                if d.contains('[') {
                    events.push(Ev::DocType(d));
                }
            }
            Ev::Skip => {}
        }
    }
    if !saw_root {
        return Err("no SVG root element found".into());
    }

    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Ev::Start(name, attrs, empty) => {
                out.push('<');
                out.push_str(name);
                for (k, v) in attrs {
                    if let Some(px) = k.strip_prefix("xmlns:") {
                        if editor_prefixes.contains(px)
                            || (o.remove_metadata
                                && !used_prefixes.contains(px)
                                && is_metadata_ns(px))
                        {
                            continue;
                        }
                    }
                    let q = if v.contains('"') { '\'' } else { '"' };
                    out.push(' ');
                    out.push_str(k);
                    out.push('=');
                    out.push(q);
                    out.push_str(v);
                    out.push(q);
                }
                if *empty {
                    out.push_str("/>");
                } else if matches!(events.get(i + 1), Some(Ev::End(n)) if n == name) {
                    out.push_str("/>");
                    i += 1;
                } else {
                    out.push('>');
                }
            }
            Ev::End(name) => {
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
            Ev::Text(t) => out.push_str(t),
            Ev::Comment(c) => {
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->");
            }
            Ev::Raw(s) => out.push_str(s),
            Ev::DocType(d) => {
                out.push_str("<!DOCTYPE ");
                out.push_str(d);
                out.push('>');
            }
            Ev::Skip => {}
        }
        i += 1;
    }
    Ok(out)
}

/// Round decimal numbers in an attribute value to `precision` decimals,
/// leaving everything else (commands, units, separators) untouched.
pub fn round_numbers(s: &str, precision: usize) -> String {
    round_impl(s, precision, false)
}

/// Like [`round_numbers`], for path data (`d`): elliptical-arc flags are
/// single-character tokens that may touch the next number ("0110.5" is
/// flags 0, 1 and x 10.5), so they are copied through unparsed.
pub fn round_path_numbers(s: &str, precision: usize) -> String {
    round_impl(s, precision, true)
}

fn round_impl(s: &str, precision: usize, path: bool) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    // Current path command and how many numbers it has consumed.
    let mut cmd = 0u8;
    let mut argn = 0usize;
    while i < b.len() {
        let c = b[i];
        if path && c.is_ascii_alphabetic() {
            cmd = c;
            argn = 0;
            out.push(c as char);
            i += 1;
            continue;
        }
        if path && (cmd == b'A' || cmd == b'a') && matches!(argn % 7, 3 | 4) {
            if c == b'0' || c == b'1' {
                out.push(c as char);
                i += 1;
                argn += 1;
                continue;
            }
            if c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'+' {
                // Not a valid flag: leave the rest untouched rather than guess.
                out.push_str(&s[i..]);
                return out;
            }
        }
        let starts_num = c.is_ascii_digit()
            || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()))
            || ((c == b'-' || c == b'+')
                && b.get(i + 1)
                    .is_some_and(|d| d.is_ascii_digit() || *d == b'.'));
        // Path commands (letters) may directly precede numbers; ids/refs ("#a1", "x_2") may not.
        let prev_alpha = i > 0 && (b[i - 1] == b'#' || b[i - 1] == b'_');
        if !starts_num || prev_alpha {
            out.push(c as char);
            i += 1;
            continue;
        }
        let st = i;
        if c == b'-' || c == b'+' {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        let mut has_exp = false;
        if i < b.len()
            && (b[i] == b'e' || b[i] == b'E')
            && b.get(i + 1)
                .is_some_and(|d| d.is_ascii_digit() || *d == b'-' || *d == b'+')
        {
            has_exp = true;
            i += 1;
            if b[i] == b'-' || b[i] == b'+' {
                i += 1;
            }
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        let tok = &s[st..i];
        argn += 1;
        if has_exp || !tok.contains('.') {
            out.push_str(tok);
            continue;
        }
        let v: f64 = match tok.parse() {
            Ok(v) => v,
            Err(_) => {
                out.push_str(tok);
                continue;
            }
        };
        let mut r = format!("{:.*}", precision, v);
        if r.contains('.') {
            r = r.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        if r == "-0" {
            r = "0".into();
        }
        // Drop redundant leading zeros: "0.5" → ".5", "-0.5" → "-.5".
        if let Some(rest) = r.strip_prefix("0.") {
            r = format!(".{rest}");
        } else if let Some(rest) = r.strip_prefix("-0.") {
            r = format!("-.{rest}");
        }
        // A sign may have been the only separator ("1.2+3.4", "5-0.001"):
        // if rounding dropped it, keep the numbers apart with a space.
        if r.starts_with(|ch: char| ch.is_ascii_digit() || ch == '.')
            && out.ends_with(|ch: char| ch.is_ascii_digit() || ch == '.')
        {
            out.push(' ');
        }
        out.push_str(&r);
        // "1.0.5" → "1" + ".5" would merge into "1.5": insert a separator.
        if !r.contains('.') && i < b.len() && (b[i] == b'.' || b[i].is_ascii_digit()) {
            out.push(' ');
        }
    }
    out
}

fn svg_optimize() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.svg_optimize", "Optimize SVG", Category::Image)
        .describe("Safely minify SVG files: strip comments, metadata, editor data (Inkscape, Illustrator, Sketch...) and whitespace, optionally round numbers")
        .keywords(&["svg", "minify", "svgo", "compress", "clean", "vector"])
        .param(ParamSpec::files("input").label("SVG files").required().primary().accept(&[".svg", "image/svg+xml"]))
        .param(ParamSpec::integer("precision").describe("Round coordinates to this many decimals (empty = keep numbers exactly)").range(0.0, 8.0))
        .param(ParamSpec::boolean("remove_metadata").describe("Remove <metadata> blocks (RDF, licenses embedded by editors)").default_bool(true))
        .param(ParamSpec::boolean("remove_editor_data").describe("Remove Inkscape/Sodipodi/Illustrator/Sketch/Figma namespaced elements and attributes").default_bool(true))
        .param(ParamSpec::boolean("remove_comments").default_bool(true))
        .param(ParamSpec::boolean("collapse_whitespace").describe("Remove whitespace between tags (text content is preserved)").default_bool(true))
        .output(OutputKind::Files)
        .progress();
    sync_tool(spec, |ctx, p| {
        let opts = OptimizeOptions {
            precision: p.opt_int("precision").map(|v| v as usize),
            remove_metadata: p.bool("remove_metadata"),
            remove_editor_data: p.bool("remove_editor_data"),
            remove_comments: p.bool("remove_comments"),
            collapse_whitespace: p.bool("collapse_whitespace"),
        };
        let files = p.files("input");
        let mut warnings = vec![];
        let mut last_text = String::new();
        let arts = for_each_file(ctx, files, "Optimizing", |f| {
            let raw = std::fs::read(&f.path)?;
            let text = String::from_utf8(raw).map_err(|_| {
                ToolError::invalid("input", format!("{}: SVG must be UTF-8 text", f.name))
            })?;
            let text = text.trim_start_matches('\u{feff}');
            let mut out = optimize_svg(text, &opts)
                .map_err(|e| ToolError::invalid("input", format!("{}: {e}", f.name)))?;
            // Safety net: the optimized SVG must still parse; otherwise keep the original.
            if parse_svg(text.as_bytes()).is_ok() && parse_svg(out.as_bytes()).is_err() {
                warnings.push(format!(
                    "{}: optimized output failed validation; original kept",
                    f.name
                ));
                out = text.to_string();
            }
            last_text = out.clone();
            ctx.write_output(&suffixed(&f.name, "min", Some("svg")), out.as_bytes())
        })?;
        let data = size_table(files, &arts);
        let mut o = ToolOutput::files(arts).with_data(json!({"table": data}));
        if files.len() == 1 {
            o = o.with_text(last_text, "image/svg+xml");
        }
        for w in warnings {
            o = o.warn(w);
        }
        Ok(o)
    })
}
