//! Native page stamping: text/image watermarks and page numbers.

use crate::common::*;
use crate::images::embed_image;
use crate::structure::pdf_inputs;
use dogubako_core::prelude::*;
use lopdf::{Dictionary, Object};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![watermark(), page_numbers()]
}

fn gs_dict(opacity: f32) -> Object {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"ExtGState".to_vec()));
    d.set("ca", Object::Real(opacity));
    d.set("CA", Object::Real(opacity));
    Object::Dictionary(d)
}

/// Text matrix for drawing at user point (x, y) rotated by `deg` CCW.
fn tm(deg: f32, x: f32, y: f32) -> String {
    let (s, c) = deg.to_radians().sin_cos();
    format!(
        "{} {} {} {} {} {} Tm",
        pf(c),
        pf(s),
        pf(-s),
        pf(c),
        pf(x),
        pf(y)
    )
}

fn watermark() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.watermark", "Watermark PDF", Category::Pdf)
        .describe("Stamp a text or image watermark on all or selected pages, with opacity, rotation, size and colour")
        .keywords(&["watermark", "stamp", "confidential", "draft", "overlay", "logo", "pdf"])
        .param(pdf_inputs())
        .param(ParamSpec::choice("kind", &[("text", "Text"), ("image", "Image")]).default_str("text"))
        .param(ParamSpec::text("text").label("Text").default_str("CONFIDENTIAL").show_if("kind", &["text"]))
        .param(ParamSpec::file("image").label("Watermark image").accept(&["image/*"]).show_if("kind", &["image"]))
        .param(ParamSpec::number("font_size").label("Font size (pt)").range(4.0, 400.0).default_num(60.0).show_if("kind", &["text"]))
        .param(ParamSpec::color("color").label("Colour").default_str("#ff0000").show_if("kind", &["text"]))
        .param(ParamSpec::number("scale").label("Image width (% of page)").range(1.0, 100.0).default_num(50.0).show_if("kind", &["image"]))
        .param(ParamSpec::number("opacity").label("Opacity").range(0.0, 1.0).step(0.05).default_num(0.3))
        .param(ParamSpec::number("rotation").label("Rotation (°)").range(-360.0, 360.0).default_num(45.0))
        .param(ParamSpec::choice("position", &[("center", "Center"), ("top", "Top"), ("bottom", "Bottom")]).default_str("center").advanced())
        .param(ParamSpec::text("pages").label("Pages").describe("Optional page range (default: all)").placeholder("1-z"))
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let kind = p.str_or("kind", "text");
        let opacity = p.num_or("opacity", 0.3) as f32;
        let rotation = p.num_or("rotation", 45.0) as f32;
        let position = p.str_or("position", "center");
        let text = winansi(p.text("text"));
        if kind == "text" && p.text("text").trim().is_empty() {
            return Err(ToolError::invalid("text", "watermark text is empty"));
        }
        let image = if kind == "image" {
            Some(p.file("image")?.clone())
        } else {
            None
        };
        let size = p.num_or("font_size", 60.0) as f32;
        let (r, g, b) = rgb(p.str_or("color", "#ff0000"));
        let mut arts = vec![];
        for (i, file) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(ctx, i, files.len(), &file.name);
            let mut doc = load_pdf(&file.path, "input")?;
            let pages: Vec<_> = doc.get_pages().into_values().collect();
            let sel = page_filter(p.opt_str("pages"), pages.len() as u32, "pages")?;
            let img = match &image {
                Some(f) => Some(embed_image(
                    &mut doc,
                    &f.path,
                    ctx.limits().max_image_pixels,
                    "image",
                )?),
                None => None,
            };
            let font_id = doc.add_object(helvetica_font_dict());
            let gs_id = doc.add_object(gs_dict(opacity));
            let mut stamped = 0;
            for (k, page) in pages.iter().enumerate() {
                if !sel[k] {
                    continue;
                }
                let geom = PageGeom::of(&doc, *page);
                let (vw, vh) = geom.visible();
                let vy = match position {
                    "top" => vh * 0.85,
                    "bottom" => vh * 0.15,
                    _ => vh / 2.0,
                };
                let (cx, cy) = geom.to_user(vw / 2.0, vy);
                let theta = geom.rotate as f32 + rotation;
                let mut res = owned_resources(&doc, *page);
                let gs_name = unique_name(&res, b"ExtGState", "DgbGS");
                add_resource(
                    &mut res,
                    b"ExtGState",
                    gs_name.clone(),
                    Object::Reference(gs_id),
                );
                let gs = String::from_utf8_lossy(&gs_name).to_string();
                let ops = if let Some(img) = &img {
                    let (pxw, pxh) = img.display_px();
                    let w = vw * p.num_or("scale", 50.0) as f32 / 100.0;
                    let h = w * pxh as f32 / pxw.max(1) as f32;
                    let name = unique_name(&res, b"XObject", "DgbIm");
                    add_resource(
                        &mut res,
                        b"XObject",
                        name.clone(),
                        Object::Reference(img.id),
                    );
                    let (s, c) = theta.to_radians().sin_cos();
                    let m = img.matrix(-w / 2.0, -h / 2.0, w, h);
                    format!(
                        "q /{gs} gs {} {} {} {} {} {} cm {} {} {} {} {} {} cm /{} Do Q\n",
                        pf(c),
                        pf(s),
                        pf(-s),
                        pf(c),
                        pf(cx),
                        pf(cy),
                        pf(m[0]),
                        pf(m[1]),
                        pf(m[2]),
                        pf(m[3]),
                        pf(m[4]),
                        pf(m[5]),
                        String::from_utf8_lossy(&name)
                    )
                    .into_bytes()
                } else {
                    let fname = unique_name(&res, b"Font", "DgbF");
                    add_resource(&mut res, b"Font", fname.clone(), Object::Reference(font_id));
                    let tw = helvetica_width(&text, size);
                    let (s, c) = theta.to_radians().sin_cos();
                    // Offset so the text is centred on (cx, cy).
                    let (ox, oy) = (-tw / 2.0, -size * 0.35);
                    let (x, y) = (cx + ox * c - oy * s, cy + ox * s + oy * c);
                    let mut ops = format!(
                        "q /{gs} gs {} {} {} rg BT /{} {} Tf {} ",
                        pf(r),
                        pf(g),
                        pf(b),
                        String::from_utf8_lossy(&fname),
                        pf(size),
                        tm(theta, x, y)
                    )
                    .into_bytes();
                    ops.extend(pdf_literal(&text));
                    ops.extend_from_slice(b" Tj ET Q\n");
                    ops
                };
                overlay_page(&mut doc, *page, res, ops)?;
                stamped += 1;
            }
            let out = ctx.output_path(&suffixed(&file.name, "watermarked", Some("pdf")));
            save_pdf(&mut doc, &out)?;
            let mut a = ctx.artifact(&out)?;
            a.label = Some(format!("{stamped} pages"));
            arts.push(a);
        }
        let mut o = ToolOutput::files(arts);
        if kind == "text" && text.contains(&b'?') && !p.text("text").contains('?') {
            o = o.warn("Some characters are not available in the standard Helvetica font and were replaced by '?'");
        }
        Ok(o)
    })
}

fn page_numbers() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.page_numbers", "Add Page Numbers", Category::Pdf)
        .describe("Number the pages of PDFs with a custom format like 'Page {n} of {total}' at a chosen position")
        .keywords(&["page numbers", "numbering", "pagination", "footer", "header", "bates", "pdf"])
        .param(pdf_inputs())
        .param(
            ParamSpec::choice(
                "position",
                &[
                    ("bottom-center", "Bottom center"),
                    ("bottom-right", "Bottom right"),
                    ("bottom-left", "Bottom left"),
                    ("top-center", "Top center"),
                    ("top-right", "Top right"),
                    ("top-left", "Top left"),
                ],
            )
            .default_str("bottom-center"),
        )
        .param(ParamSpec::text("format").label("Format").describe("Use {n} for the page number and {total} for the total").default_str("{n}"))
        .param(ParamSpec::integer("start").label("Start at").default_int(1).range(-1_000_000.0, 1_000_000_000.0))
        .param(ParamSpec::number("font_size").label("Font size (pt)").range(4.0, 72.0).default_num(10.0))
        .param(ParamSpec::color("color").label("Colour").default_str("#000000").advanced())
        .param(ParamSpec::number("margin").label("Margin (pt)").range(0.0, 300.0).default_num(28.0).advanced())
        .param(ParamSpec::boolean("skip_first").label("Skip first page").describe("Leave the first page (e.g. a cover) unnumbered; numbering starts on page 2"))
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let fmt = p.str_or("format", "{n}").to_string();
        if !fmt.contains("{n}") && !fmt.contains("{total}") {
            return Err(ToolError::invalid(
                "format",
                "the format must contain {n} or {total}",
            ));
        }
        let start = p.int_or("start", 1);
        let size = p.num_or("font_size", 10.0) as f32;
        let margin = p.num_or("margin", 28.0) as f32;
        let (r, g, b) = rgb(p.str_or("color", "#000000"));
        let pos = p.str_or("position", "bottom-center").to_string();
        let skip = p.bool("skip_first");
        let mut arts = vec![];
        for (i, file) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(ctx, i, files.len(), &file.name);
            let mut doc = load_pdf(&file.path, "input")?;
            let pages: Vec<_> = doc.get_pages().into_values().collect();
            let font_id = doc.add_object(helvetica_font_dict());
            let numbered = pages.len() - usize::from(skip && !pages.is_empty());
            let total = start + numbered as i64 - 1;
            for (k, page) in pages.iter().enumerate() {
                if skip && k == 0 {
                    continue;
                }
                let n = start + (k - usize::from(skip)) as i64;
                let label = winansi(
                    &fmt.replace("{n}", &n.to_string())
                        .replace("{total}", &total.to_string()),
                );
                let geom = PageGeom::of(&doc, *page);
                let (vw, vh) = geom.visible();
                let tw = helvetica_width(&label, size);
                let vx = if pos.ends_with("left") {
                    margin
                } else if pos.ends_with("right") {
                    vw - margin - tw
                } else {
                    (vw - tw) / 2.0
                };
                let vy = if pos.starts_with("top") {
                    vh - margin - size * 0.75
                } else {
                    margin
                };
                let (x, y) = geom.to_user(vx, vy);
                let mut res = owned_resources(&doc, *page);
                let fname = unique_name(&res, b"Font", "DgbF");
                add_resource(&mut res, b"Font", fname.clone(), Object::Reference(font_id));
                let mut ops = format!(
                    "q {} {} {} rg BT /{} {} Tf {} ",
                    pf(r),
                    pf(g),
                    pf(b),
                    String::from_utf8_lossy(&fname),
                    pf(size),
                    tm(geom.rotate as f32, x, y)
                )
                .into_bytes();
                ops.extend(pdf_literal(&label));
                ops.extend_from_slice(b" Tj ET Q\n");
                overlay_page(&mut doc, *page, res, ops)?;
            }
            let out = ctx.output_path(&suffixed(&file.name, "numbered", Some("pdf")));
            save_pdf(&mut doc, &out)?;
            arts.push(ctx.artifact(&out)?);
        }
        Ok(ToolOutput::files(arts))
    })
}
