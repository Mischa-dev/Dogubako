mod common;

use common::*;
use image::{GenericImageView, Rgba, RgbaImage};
use img_parts::{ImageEXIF, ImageICC};
use serde_json::json;

/// JPEG with an EXIF block (orientation 6 = rotate 90° CW), an XMP packet and a comment.
fn jpeg_with_metadata(h: &dogubako_core::testing::TestHarness) -> std::path::PathBuf {
    let p = save(h, "photo.jpg", &pattern(40, 20, false));
    let data = std::fs::read(&p).unwrap();
    let mut j = img_parts::jpeg::Jpeg::from_bytes(data.into()).unwrap();
    j.set_exif(Some(dogubako_tools_image::meta::orientation_exif(6).into()));
    j.set_icc_profile(Some(vec![1u8, 2, 3, 4].into()));
    let pos = j.segments().len() - 1;
    let mut xmp = b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta>secret location</x:xmpmeta>".to_vec();
    xmp.truncate(200);
    j.segments_mut().insert(
        pos.min(2),
        img_parts::jpeg::JpegSegment::new_with_contents(0xE1, xmp.into()),
    );
    j.segments_mut().insert(
        pos.min(2),
        img_parts::jpeg::JpegSegment::new_with_contents(0xFE, b"my comment".to_vec().into()),
    );
    let mut out = Vec::new();
    j.encoder().write_to(&mut out).unwrap();
    std::fs::write(&p, out).unwrap();
    p
}

#[tokio::test]
async fn metadata_and_strip() {
    let h = harness();
    let p = jpeg_with_metadata(&h);
    let out = h
        .run(
            "image.metadata",
            json!({"input": s(&p), "engine": "native"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["engine"], "native");
    assert!(
        out.data["exif"]["Orientation"]
            .as_str()
            .unwrap()
            .contains("row 0"),
        "{}",
        out.data["exif"]
    );
    // Auto-orientation is applied when decoding.
    let info = h.run("image.info", json!({"input": s(&p)})).await.unwrap();
    assert_eq!(
        (info.data["width"].as_u64(), info.data["height"].as_u64()),
        (Some(20), Some(40))
    );

    if h.engine.deps.path("exiftool").is_some() {
        let out = h
            .run(
                "image.metadata",
                json!({"input": s(&p), "engine": "exiftool"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["engine"], "exiftool");
        assert!(out.data["table"]["rows"].as_array().unwrap().len() > 3);
    }

    let out = h
        .run(
            "image.strip_metadata",
            json!({"input": [s(&p)], "keep_orientation": false, "keep_icc": false}),
        )
        .await
        .unwrap();
    let bytes = std::fs::read(&out.artifacts[0].path).unwrap();
    assert!(!bytes.windows(15).any(|w| w == b"secret location"));
    assert!(!bytes.windows(10).any(|w| w == b"my comment"));
    let j = img_parts::jpeg::Jpeg::from_bytes(bytes.clone().into()).unwrap();
    assert!(j.exif().is_none() && j.icc_profile().is_none());
    assert_eq!(
        open(&out.artifacts[0].path).dimensions(),
        (40, 20),
        "stripped image still decodes"
    );

    let out = h
        .run("image.strip_metadata", json!({"input": [s(&p)]}))
        .await
        .unwrap();
    let bytes = std::fs::read(&out.artifacts[0].path).unwrap();
    assert!(!bytes.windows(15).any(|w| w == b"secret location"));
    let j = img_parts::jpeg::Jpeg::from_bytes(bytes.into()).unwrap();
    assert!(j.exif().is_some(), "orientation kept");
    assert!(j.icc_profile().is_some(), "ICC kept");
    let ex = exif::Reader::new()
        .read_raw(j.exif().unwrap().to_vec())
        .unwrap();
    assert_eq!(
        ex.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .unwrap()
            .value
            .get_uint(0),
        Some(6)
    );
    assert_eq!(ex.fields().count(), 1);

    // PNG text chunks.
    let png = save(&h, "t.png", &pattern(10, 10, false));
    let mut pp = img_parts::png::Png::from_bytes(std::fs::read(&png).unwrap().into()).unwrap();
    let n = pp.chunks().len();
    pp.chunks_mut().insert(
        n - 1,
        img_parts::png::PngChunk::new(*b"tEXt", b"Author\0Someone".to_vec().into()),
    );
    let mut buf = Vec::new();
    pp.encoder().write_to(&mut buf).unwrap();
    std::fs::write(&png, buf).unwrap();
    let out = h
        .run("image.strip_metadata", json!({"input": [s(&png)]}))
        .await
        .unwrap();
    let bytes = std::fs::read(&out.artifacts[0].path).unwrap();
    assert!(!bytes.windows(7).any(|w| w == b"Someone"));
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (10, 10));
}

const INKSCAPE_SVG: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!-- Created with Inkscape (http://www.inkscape.org/) -->
<svg
   xmlns:dc="http://purl.org/dc/elements/1.1/"
   xmlns:cc="http://creativecommons.org/ns#"
   xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
   xmlns="http://www.w3.org/2000/svg"
   xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
   xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
   width="100" height="100" viewBox="0 0 100 100"
   inkscape:version="1.0" sodipodi:docname="drawing.svg">
  <sodipodi:namedview id="base" pagecolor="#ffffff" inkscape:zoom="1.4" />
  <metadata>
    <rdf:RDF><cc:Work rdf:about=""><dc:format>image/svg+xml</dc:format></cc:Work></rdf:RDF>
  </metadata>
  <g inkscape:label="Layer 1" inkscape:groupmode="layer" id="layer1">
    <rect x="10.123456" y="10.987654" width="40.0001" height="40" fill="#ff0000" />
    <path d="M60.5 60.25L90.333 60.25 90.333 90.0.5z" fill="#0000ff"/>
    <circle cx="25" cy="75" r="15" style="fill:#00ff00"></circle>
    <text x="55" y="30" font-size="10">A &amp; B  <tspan>C</tspan></text>
  </g>
</svg>
"##;

#[tokio::test]
async fn svg_optimize_and_render() {
    let h = harness();
    let p = h.write("drawing.svg", INKSCAPE_SVG.as_bytes());
    let out = h
        .run("image.svg_optimize", json!({"input": [s(&p)]}))
        .await
        .unwrap();
    let min = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(min.len() < INKSCAPE_SVG.len() / 2 + 200, "{min}");
    for gone in [
        "inkscape", "sodipodi", "metadata", "rdf:", "<!--", "xmlns:dc",
    ] {
        assert!(!min.contains(gone), "{gone} still in {min}");
    }
    assert!(
        min.contains("A &amp; B  <tspan>C</tspan>"),
        "text whitespace and entities preserved: {min}"
    );
    assert!(
        min.contains("<circle cx=\"25\" cy=\"75\" r=\"15\" style=\"fill:#00ff00\"/>"),
        "{min}"
    );
    assert!(
        min.contains("10.123456"),
        "numbers untouched without precision"
    );
    assert_eq!(out.text.as_deref(), Some(min.as_str()));

    // Rendering is identical before and after.
    let render = |svg: &[u8]| {
        let tree = dogubako_tools_image::svg::parse_svg(svg).unwrap();
        let mut pm = resvg::tiny_skia::Pixmap::new(100, 100).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pm.as_mut(),
        );
        pm.data().to_vec()
    };
    let (a, b) = (render(INKSCAPE_SVG.as_bytes()), render(min.as_bytes()));
    let diff = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert_eq!(diff, 0);

    // Rounding.
    let out = h
        .run(
            "image.svg_optimize",
            json!({"input": [s(&p)], "precision": 1}),
        )
        .await
        .unwrap();
    let r = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(
        r.contains("x=\"10.1\"") && r.contains("width=\"40\""),
        "{r}"
    );
    assert!(r.contains("M60.5 60.2L90.3 60.2 90.3 90 .5z"), "{r}");
    let c = render(r.as_bytes());
    let diff = a.iter().zip(&c).filter(|(x, y)| x != y).count();
    assert!(
        diff < a.len() / 20,
        "rounded render close to original ({diff})"
    );

    let bad = h.write("bad.svg", b"<svg><g></svg>");
    let err = h
        .run("image.svg_optimize", json!({"input": [s(&bad)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    assert_eq!(
        dogubako_tools_image::svg::round_numbers("1.0.5 -0.04,3e-5 2.25", 1),
        "1 .5 0,3e-5 2.2"
    );
}

#[tokio::test]
async fn svg_to_raster_sizes() {
    let h = harness();
    let p = h.write("c.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="50" height="25" viewBox="0 0 50 25"><rect width="50" height="25" fill="#ff0000"/><image href="/etc/passwd" width="10" height="10"/></svg>"##);
    let out = h
        .run("image.svg_to_raster", json!({"input": [s(&p)]}))
        .await
        .unwrap();
    let img = open(&out.artifacts[0].path).to_rgba8();
    assert_eq!(img.dimensions(), (50, 25));
    assert_eq!(img.get_pixel(25, 12).0, [255, 0, 0, 255]);
    let out = h
        .run(
            "image.svg_to_raster",
            json!({"input": [s(&p)], "width": 200, "format": "jpg"}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (200, 100));
    let out = h
        .run(
            "image.svg_to_raster",
            json!({"input": [s(&p)], "scale": 2, "format": "webp"}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (100, 50));
    let bad = h.write("x.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"><");
    assert_eq!(
        h.run("image.svg_to_raster", json!({"input": [s(&bad)]}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn palette_contrast_gradient_aspect() {
    let h = harness();
    let out = h
        .run(
            "design.palette",
            json!({"color": "#ff0000", "scheme": "complementary"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["colors"][1]["hex"], "#00ffff");
    let out = h
        .run(
            "design.palette",
            json!({"color": "#3366ff", "scheme": "shades", "count": 4}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["colors"].as_array().unwrap().len(), 4);
    let out = h
        .run(
            "design.palette",
            json!({"color": "#3366ff", "scheme": "triadic"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["colors"].as_array().unwrap().len(), 3);
    assert_eq!(
        h.run("design.palette", json!({"color": "nope"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );

    let out = h
        .run(
            "design.contrast",
            json!({"foreground": "#000000", "background": "#ffffff"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["ratio"], 21.0);
    assert_eq!(out.data["aaa_normal"], true);
    let out = h
        .run(
            "design.contrast",
            json!({"foreground": "#999999", "background": "#ffffff"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["aa_normal"], false);
    let sug = out.data["suggestions"][0]["hex"].as_str().unwrap();
    let out2 = h
        .run(
            "design.contrast",
            json!({"foreground": sug, "background": "#ffffff"}),
        )
        .await
        .unwrap();
    assert_eq!(out2.data["aa_normal"], true, "suggestion {sug} passes AA");

    let out = h
        .run(
            "design.gradient",
            json!({"stops": "#ff0000, #0000ff", "width": 100, "height": 10, "angle": 90}),
        )
        .await
        .unwrap();
    let g = open(&out.artifacts[0].path).to_rgba8();
    assert!(g.get_pixel(0, 5)[0] > 240 && g.get_pixel(99, 5)[2] > 240);
    assert!(out
        .text
        .unwrap()
        .contains("linear-gradient(90deg, #ff0000 0%, #0000ff 100%)"));
    let out = h.run("design.gradient", json!({"stops": "#ff0000, #00ff00 30%, #0000ff", "kind": "radial", "width": 20, "height": 20})).await.unwrap();
    assert!(out.data["css"]
        .as_str()
        .unwrap()
        .starts_with("radial-gradient"));
    assert_eq!(
        h.run("design.gradient", json!({"stops": "#fff"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );

    let out = h
        .run(
            "design.aspect_ratio",
            json!({"width": 1920, "height": 1080, "target_width": 1280}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["ratio"], "16:9");
    assert_eq!(out.data["computed_height"], 720);
    let out = h
        .run(
            "design.aspect_ratio",
            json!({"ratio": "4:3", "target_height": 600}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["computed_width"], 800);
    assert_eq!(
        h.run("design.aspect_ratio", json!({}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn colors_from_images() {
    let h = harness();
    let img = RgbaImage::from_fn(100, 100, |x, _| {
        if x < 70 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let p = save(&h, "two.png", &img);
    let out = h
        .run("image.dominant_colors", json!({"input": s(&p), "count": 2}))
        .await
        .unwrap();
    assert_eq!(out.data["colors"][0]["hex"], "#ff0000");
    assert_eq!(out.data["colors"][1]["hex"], "#0000ff");
    let pct = out.data["colors"][0]["percent"].as_f64().unwrap();
    assert!((pct - 70.0).abs() < 3.0, "{pct}");
    let out = h
        .run(
            "design.palette_extract",
            json!({"input": s(&p), "count": 3}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    assert!(out.data["css"].as_str().unwrap().contains("--palette-1"));
}

#[tokio::test]
async fn canvas_and_compose() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(100, 50, false));
    let out = h
        .run(
            "design.canvas_resize",
            json!({"input": [s(&a)], "aspect": "1:1", "background": "#00000000"}),
        )
        .await
        .unwrap();
    let c = open(&out.artifacts[0].path).to_rgba8();
    assert_eq!(c.dimensions(), (100, 100));
    assert_eq!(c.get_pixel(50, 5)[3], 0);
    let out = h
        .run(
            "design.canvas_resize",
            json!({"input": [s(&a)], "mode": "padding", "padding": 10}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (120, 70));
    let out = h.run("design.canvas_resize", json!({"input": [s(&a)], "mode": "size", "width": 60, "height": 60, "anchor": "top-left"})).await.unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (60, 60));

    let ov = save(
        &h,
        "ov.png",
        &RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 255])),
    );
    let out = h
        .run(
            "design.compose",
            json!({"input": s(&a), "overlay": s(&ov), "position": "custom", "x": 5, "y": 5}),
        )
        .await
        .unwrap();
    let c = open(&out.artifacts[0].path).to_rgba8();
    assert_eq!(c.get_pixel(8, 8).0, [0, 0, 0, 255]);
    let out = h.run("design.compose", json!({"input": s(&a), "overlay": s(&ov), "blend": "multiply", "opacity": 0.5, "scale": 50})).await.unwrap();
    assert_eq!(out.data["regions"][0]["width"], 50);
}

#[tokio::test]
async fn qr_roundtrip() {
    let h = harness();
    let text = "https://example.com/?q=dogubako&x=1";
    let out = h
        .run(
            "dev.qr_generate",
            json!({"text": text, "size": 300, "error_correction": "Q"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    assert_eq!(out.text_mime.as_deref(), Some("image/svg+xml"));
    assert!(out.text.as_ref().unwrap().starts_with("<svg"));
    assert_eq!(out.data["verified_scan"], true);
    let png = out
        .artifacts
        .iter()
        .find(|a| a.name.ends_with(".png"))
        .unwrap();
    let read = h
        .run("dev.qr_read", json!({"input": s(&png.path)}))
        .await
        .unwrap();
    assert_eq!(read.text.as_deref(), Some(text));
    assert_eq!(read.data["found"], 1);
    assert!(read.data["regions"][0]["width"].as_f64().unwrap() > 100.0);
    // The SVG renders and decodes too.
    let svg = out
        .artifacts
        .iter()
        .find(|a| a.name.ends_with(".svg"))
        .unwrap();
    let read = h
        .run("dev.qr_read", json!({"input": s(&svg.path)}))
        .await
        .unwrap();
    assert_eq!(read.text.as_deref(), Some(text));

    let logo = save(&h, "logo.png", &pattern(64, 64, false));
    for style in ["square", "rounded", "dots"] {
        let out = h.run("design.qr_style", json!({"text": text, "module_style": style, "eye_style": style, "logo": s(&logo), "foreground": "#1a237e", "size": 400})).await.unwrap();
        assert_eq!(out.data["error_correction"], "H");
        assert_eq!(out.data["verified_scan"], true, "style {style}");
        // Styled SVG decodes too.
        let svg = out
            .artifacts
            .iter()
            .find(|a| a.name.ends_with(".svg"))
            .unwrap();
        let read = h
            .run("dev.qr_read", json!({"input": s(&svg.path)}))
            .await
            .unwrap();
        assert_eq!(read.text.as_deref(), Some(text), "svg style {style}");
    }

    let blank = save(
        &h,
        "blank.png",
        &RgbaImage::from_pixel(50, 50, Rgba([255, 255, 255, 255])),
    );
    let read = h
        .run("dev.qr_read", json!({"input": s(&blank)}))
        .await
        .unwrap();
    assert_eq!(read.data["found"], 0);
    assert!(!read.warnings.is_empty());
    let long = "x".repeat(4000);
    assert_eq!(
        h.run(
            "dev.qr_generate",
            json!({"text": long, "error_correction": "H"})
        )
        .await
        .unwrap_err()
        .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn barcode_roundtrip() {
    let h = harness();
    for (fmt, text, expect) in [
        ("code128", "DOGUBAKO-123", "CODE_128"),
        ("ean13", "5901234123457", "EAN_13"),
        ("ean8", "96385074", "EAN_8"),
        ("code39", "HELLO", "CODE_39"),
        ("itf", "12345678", "ITF"),
        ("datamatrix", "Data Matrix test", "DATA_MATRIX"),
        ("pdf417", "PDF417 test", "PDF_417"),
        ("aztec", "Aztec test", "AZTEC"),
    ] {
        let out = h
            .run("dev.barcode_generate", json!({"text": text, "format": fmt}))
            .await
            .unwrap_or_else(|e| panic!("{fmt}: {e}"));
        assert_eq!(out.artifacts.len(), 2);
        let png = &out.artifacts[0].path;
        let read = h
            .run("dev.barcode_read", json!({"input": s(png)}))
            .await
            .unwrap();
        let codes = read.data["codes"].as_array().unwrap();
        assert!(
            codes
                .iter()
                .any(|c| c["text"] == text && c["format"] == expect),
            "{fmt}: {}",
            read.data
        );
    }
    let err = h
        .run(
            "dev.barcode_generate",
            json!({"text": "abc", "format": "ean13"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn face_detection_with_model() {
    let model = std::path::Path::new("/tmp/claude-0/-home-user-Dogubako/03b812fc-9984-5341-ac4d-33322a39447a/scratchpad/seeta_fd_frontal_v1.0.bin");
    let env_dir = std::env::var_os("DOGUBAKO_TEST_MODELS").map(std::path::PathBuf::from);
    let dir = tempfile::tempdir().unwrap();
    let models = match env_dir {
        Some(d) if d.join("seeta_fd_frontal_v1.0.bin").is_file() => d,
        _ => {
            if !model.is_file() {
                eprintln!("skipping: face model not available");
                return;
            }
            std::fs::copy(model, dir.path().join("seeta_fd_frontal_v1.0.bin")).unwrap();
            // SAFETY: tests only read this variable when constructing harnesses.
            unsafe { std::env::set_var("DOGUBAKO_TEST_MODELS", dir.path()) };
            dir.path().to_path_buf()
        }
    };
    let h = harness();
    assert!(
        h.engine.models.dir() == models.as_path() || h.engine.models.is_installed("seeta-face")
    );
    if !h.available("image.face_detect") {
        return;
    }
    let a = save(&h, "scene.png", &pattern(320, 240, false));
    let out = h
        .run("image.face_detect", json!({"input": s(&a)}))
        .await
        .unwrap();
    assert!(out.data["regions"].is_array());
    assert_eq!(
        out.data["faces"].as_u64(),
        Some(out.data["regions"].as_array().unwrap().len() as u64)
    );
    assert_eq!(out.artifacts.len(), 1);
    let out = h.run("image.face_blur", json!({"input": [s(&a)], "extra_regions": [{"x": 0, "y": 0, "width": 20, "height": 20}], "mode": "fill"})).await.unwrap();
    let img = open(&out.artifacts[0].path).to_rgb8();
    assert_eq!(img.get_pixel(5, 5).0, [0, 0, 0]);
    assert!(!out.data["regions"].as_array().unwrap().is_empty());
    // Optional real photo (e.g. rustface's assets/test/scientists.jpg) to check actual detection.
    if let Some(photo) = std::env::var_os("DOGUBAKO_TEST_FACE_IMAGE").filter(|p| std::path::Path::new(p).is_file()) {
        let photo = std::path::PathBuf::from(photo);
        let out = h.run("image.face_detect", json!({"input": s(&photo)})).await.unwrap();
        assert!(out.data["faces"].as_u64().unwrap() > 0, "{}", out.data);
        let out = h.run("image.face_blur", json!({"input": [s(&photo)], "mode": "pixelate"})).await.unwrap();
        assert!(out.data["faces"].as_u64().unwrap() > 0);
    }
}

#[tokio::test]
async fn ocr_with_tesseract() {
    let h = harness();
    if !h.available("image.ocr") {
        return;
    }
    let mut img = RgbaImage::from_pixel(900, 200, Rgba([255, 255, 255, 255]));
    let txt = dogubako_tools_image::util::render_text(
        "Hello Dogubako\nOptical text",
        56.0,
        Rgba([0, 0, 0, 255]),
    );
    dogubako_tools_image::util::composite(
        &mut img,
        &txt,
        30,
        20,
        1.0,
        dogubako_tools_image::util::Blend::Normal,
    );
    let p = save(&h, "text.png", &img);
    let out = h.run("image.ocr", json!({"input": s(&p)})).await.unwrap();
    let text = out.text.unwrap();
    assert!(
        text.contains("Hello") && text.contains("Dogubako"),
        "{text}"
    );
    assert!(out.data["regions"].as_array().unwrap().len() >= 3);
    assert!(out.data["regions"][0]["width"].as_i64().unwrap() > 10);
    let out = h
        .run("image.ocr", json!({"input": s(&p), "output": "hocr"}))
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("ocr_line"));
    assert_eq!(
        h.run("image.ocr", json!({"input": s(&p), "languages": "-l eng"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn optional_engines_report_missing_dependencies() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(8, 8, false));
    for id in ["image.remove_background", "image.upscale"] {
        if !h.available(id) {
            let err = h.run(id, json!({"input": [s(&a)]})).await.unwrap_err();
            assert_eq!(err.code(), "missing_dependency", "{id}");
        }
    }
}

#[test]
fn registry_specs_are_valid() {
    let tools = dogubako_tools_image::tools();
    let mut reg = dogubako_core::registry::Registry::new();
    reg.register_all(tools.clone());
    for t in &tools {
        let sp = t.spec();
        assert!(
            sp.id.starts_with(&format!("{}.", sp.category.id())),
            "{}",
            sp.id
        );
        assert!(!sp.description.is_empty(), "{}", sp.id);
    }
    assert!(tools.len() >= 43, "{}", tools.len());
}
