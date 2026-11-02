mod common;

use common::*;
use image::{GenericImageView, Rgba, RgbaImage};
use serde_json::json;

#[tokio::test]
async fn convert_many_formats_and_batch() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(64, 48, true));
    let b = save(&h, "b.png", &pattern(30, 20, false));
    for fmt in [
        "jpg", "webp", "avif", "gif", "bmp", "tiff", "ico", "tga", "qoi", "ppm", "png",
    ] {
        let out = h
            .run(
                "image.convert",
                json!({"input": [s(&a), s(&b)], "format": fmt, "quality": 70}),
            )
            .await
            .unwrap_or_else(|e| panic!("{fmt}: {e}"));
        assert_eq!(out.artifacts.len(), 2, "{fmt}");
        if fmt == "avif" {
            // AVIF is encode-only natively (decoding needs dav1d); check the container brand.
            let b = std::fs::read(&out.artifacts[0].path).unwrap();
            assert_eq!(&b[4..12], b"ftypavif");
            continue;
        }
        let img = open(&out.artifacts[0].path);
        assert_eq!(img.dimensions(), (64, 48), "{fmt}");
        assert!(
            out.artifacts[0].name.ends_with(&format!(".{fmt}")),
            "{}",
            out.artifacts[0].name
        );
        if fmt == "jpg" {
            assert!(!out.warnings.is_empty(), "alpha flatten warning expected");
            // Transparent corner flattened onto white.
            let p = img.get_pixel(1, 1);
            assert!(p[0] > 240 && p[1] > 240 && p[2] > 240);
        }
    }
    // Lossless WebP keeps exact pixels.
    let out = h
        .run(
            "image.convert",
            json!({"input": s(&b), "format": "webp", "lossless": true}),
        )
        .await
        .unwrap();
    assert_eq!(
        open(&out.artifacts[0].path).to_rgba8(),
        image::open(&b).unwrap().to_rgba8()
    );
}

#[tokio::test]
async fn malformed_and_unknown_inputs() {
    let h = harness();
    let bad = h.write("broken.png", b"\x89PNG\r\n\x1a\nnot really a png");
    let err = h
        .run("image.convert", json!({"input": s(&bad), "format": "jpg"}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input", "{err}");
    let err = h
        .run("image.convert", json!({"input": s(&bad), "format": "nope"}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run("image.resize", json!({"input": s(&bad)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn pixel_limit_is_enforced() {
    let mut h = harness();
    h.engine.config.limits.max_image_pixels = 1000;
    let a = save(&h, "big.png", &pattern(100, 100, false));
    let err = h
        .run("image.info", json!({"input": s(&a)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded", "{err}");
}

#[tokio::test]
async fn resize_modes() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(200, 100, false));
    let dims = |out: &dogubako_core::tool::ToolOutput| open(&out.artifacts[0].path).dimensions();
    let out = h
        .run("image.resize", json!({"input": s(&a), "width": 100}))
        .await
        .unwrap();
    assert_eq!(dims(&out), (100, 50));
    let out = h
        .run(
            "image.resize",
            json!({"input": s(&a), "width": 80, "height": 80, "fit": "contain"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out), (80, 40));
    let out = h
        .run(
            "image.resize",
            json!({"input": s(&a), "width": 80, "height": 80, "fit": "cover"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out), (80, 80));
    let out = h
        .run(
            "image.resize",
            json!({"input": s(&a), "width": 80, "height": 80, "fit": "fill"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out), (80, 80));
    let out = h
        .run(
            "image.resize",
            json!({"input": s(&a), "width": 33, "height": 77, "fit": "exact"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out), (33, 77));
    let out = h
        .run("image.resize", json!({"input": s(&a), "percent": 50}))
        .await
        .unwrap();
    assert_eq!(dims(&out), (100, 50));
    let out = h
        .run(
            "image.resize",
            json!({"input": s(&a), "width": 400, "height": 400, "no_upscale": true}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out), (200, 100));
    let out = h
        .run(
            "image.resize",
            json!({"input": s(&a), "width": 50, "format": "jpg"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].name.ends_with(".jpg"));
    let err = h
        .run("image.resize", json!({"input": s(&a)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let out = h
        .run(
            "image.thumbnail",
            json!({"input": [s(&a)], "size": 32, "square": true}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out), (32, 32));
}

#[tokio::test]
async fn crop_rotate_flip() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(100, 60, false));
    let out = h
        .run(
            "image.crop",
            json!({"input": s(&a), "x": 10, "y": 5, "width": 30, "height": 20}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (30, 20));
    assert_eq!(out.data["regions"][0]["x"], 10);
    // Clipped to the image.
    let out = h
        .run(
            "image.crop",
            json!({"input": s(&a), "x": 90, "y": 50, "width": 30, "height": 30}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (10, 10));
    let err = h
        .run(
            "image.crop",
            json!({"input": s(&a), "x": 200, "y": 0, "width": 10, "height": 10}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let out = h
        .run(
            "image.crop",
            json!({"input": s(&a), "mode": "aspect", "aspect": "1:1"}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (60, 60));
    let err = h
        .run(
            "image.crop",
            json!({"input": s(&a), "mode": "aspect", "aspect": "abc"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run("image.rotate_flip", json!({"input": s(&a), "angle": 90}))
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (60, 100));
    let out = h
        .run("image.rotate_flip", json!({"input": s(&a), "angle": 45}))
        .await
        .unwrap();
    let (w, hh) = open(&out.artifacts[0].path).dimensions();
    assert!(w > 100 && hh > 100);
    let out = h
        .run(
            "image.rotate_flip",
            json!({"input": s(&a), "angle": 0, "flip_horizontal": true}),
        )
        .await
        .unwrap();
    let orig = image::open(&a).unwrap().to_rgba8();
    let flipped = open(&out.artifacts[0].path).to_rgba8();
    assert_eq!(orig.get_pixel(0, 0), flipped.get_pixel(99, 0));
}

#[tokio::test]
async fn compress_png_and_jpeg() {
    let h = harness();
    // Highly compressible PNG saved without optimization.
    let flat = RgbaImage::from_pixel(300, 300, Rgba([10, 120, 200, 255]));
    let a = save(&h, "flat.png", &flat);
    let out = h
        .run("image.compress", json!({"input": [s(&a)]}))
        .await
        .unwrap();
    assert!(out.artifacts[0].size <= std::fs::metadata(&a).unwrap().len());
    assert_eq!(
        open(&out.artifacts[0].path).to_rgba8(),
        flat,
        "PNG compression must be lossless"
    );
    let j = save(&h, "photo.jpg", &pattern(256, 256, false));
    let out = h
        .run(
            "image.compress",
            json!({"input": [s(&j)], "quality": 30, "keep_if_larger": false}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].size < std::fs::metadata(&j).unwrap().len());
    let out = h
        .run(
            "image.compress",
            json!({"input": [s(&j)], "format": "webp", "max_dimension": 64}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (64, 64));
    assert!(out.data["saved_percent"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn adjust_filters() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(40, 40, false));
    let out = h
        .run("image.adjust", json!({"input": s(&a), "grayscale": true}))
        .await
        .unwrap();
    let img = open(&out.artifacts[0].path).to_rgb8();
    assert!(img.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
    let out = h
        .run("image.adjust", json!({"input": s(&a), "invert": true}))
        .await
        .unwrap();
    let inv = open(&out.artifacts[0].path).to_rgb8();
    let orig = image::open(&a).unwrap().to_rgb8();
    assert_eq!(inv.get_pixel(3, 3)[0], 255 - orig.get_pixel(3, 3)[0]);
    let out = h.run("image.adjust", json!({"input": s(&a), "brightness": 20, "contrast": 10, "saturation": -30, "hue": 90, "gamma": 1.5, "sepia": true, "blur": 1.5, "sharpen": 1.0})).await.unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (40, 40));
    let err = h
        .run("image.adjust", json!({"input": s(&a), "brightness": 500}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn denoise_removes_salt_and_pepper() {
    let h = harness();
    let mut img = RgbaImage::from_pixel(64, 64, Rgba([128, 128, 128, 255]));
    for i in 0..200u32 {
        let (x, y) = ((i * 37) % 64, (i * 91 + 7) % 64);
        img.put_pixel(
            x,
            y,
            if i % 2 == 0 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            },
        );
    }
    let a = save(&h, "noisy.png", &img);
    let out = h
        .run(
            "image.denoise",
            json!({"input": s(&a), "method": "median", "radius": 1}),
        )
        .await
        .unwrap();
    let clean = open(&out.artifacts[0].path).to_rgb8();
    let outliers = clean
        .pixels()
        .filter(|p| (p[0] as i32 - 128).abs() > 40)
        .count();
    assert!(outliers < 10, "outliers left: {outliers}");
    let out = h
        .run(
            "image.denoise",
            json!({"input": s(&a), "method": "bilateral"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
}

#[tokio::test]
async fn watermark_text_image_and_tiles() {
    let h = harness();
    let a = save(&h, "a.jpg", &pattern(300, 200, false));
    let logo = save(
        &h,
        "logo.png",
        &RgbaImage::from_pixel(50, 50, Rgba([255, 0, 255, 255])),
    );
    let out = h.run("image.watermark", json!({"input": [s(&a)], "text": "Hello Ünïcode", "opacity": 1.0, "position": "center", "font_size": 10})).await.unwrap();
    let w = open(&out.artifacts[0].path).to_rgb8();
    let orig = image::open(&a).unwrap().to_rgb8();
    let changed = w
        .pixels()
        .zip(orig.pixels())
        .filter(|(x, y)| (x[0] as i32 - y[0] as i32).abs() > 30)
        .count();
    assert!(changed > 200, "text must be drawn ({changed})");
    let out = h.run("image.watermark", json!({"input": [s(&a)], "kind": "image", "watermark": s(&logo), "opacity": 1.0, "position": "top-left", "margin": 0, "scale": 10})).await.unwrap();
    let w = open(&out.artifacts[0].path).to_rgb8();
    let p = w.get_pixel(5, 5);
    assert!(p[0] > 200 && p[1] < 60 && p[2] > 200, "{p:?}");
    let out = h
        .run(
            "image.watermark",
            json!({"input": [s(&a)], "tile": true, "rotation": -30}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    let err = h
        .run(
            "image.watermark",
            json!({"input": [s(&a)], "kind": "image"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn combine_split_sheets() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(40, 30, false));
    let b = save(&h, "b.png", &pattern(20, 30, false));
    let out = h
        .run(
            "image.combine",
            json!({"input": [s(&a), s(&b)], "spacing": 4}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (64, 30));
    let out = h
        .run(
            "image.combine",
            json!({"input": [s(&a), s(&b)], "direction": "vertical", "match_size": false}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (40, 60));
    let out = h
        .run(
            "image.combine",
            json!({"input": [s(&a), s(&b), s(&a)], "direction": "grid", "columns": 2}),
        )
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (80, 60));
    let err = h
        .run("image.combine", json!({"input": [s(&a)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    // match_size must not resize to a huge size before the limit check:
    // 10000x1 next to 1x10000 would scale the first to 1e8 x 1e4.
    let wide = save(&h, "wide.png", &pattern(10_000, 1, false));
    let tall = save(&h, "tall.png", &pattern(1, 10_000, false));
    for dir in ["horizontal", "vertical"] {
        let (x, y) = if dir == "horizontal" {
            (&wide, &tall)
        } else {
            (&tall, &wide)
        };
        let err = h
            .run(
                "image.combine",
                json!({"input": [s(x), s(y)], "direction": dir, "match_size": true}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code(), "limit_exceeded", "{dir}: {err}");
    }

    let out = h
        .run(
            "image.split",
            json!({"input": s(&a), "rows": 2, "columns": 3}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 6);
    let out = h
        .run(
            "image.split",
            json!({"input": s(&a), "mode": "size", "tile_width": 16, "tile_height": 16}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 6); // 3 x 2, edge tiles smaller

    let out = h
        .run(
            "image.contact_sheet",
            json!({"input": [s(&a), s(&b)], "columns": 2, "thumb_size": 64, "title": "Sheet"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    let out = h
        .run(
            "image.sprite_sheet",
            json!({"input": [s(&a), s(&b)], "layout": "horizontal", "padding": 2}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 3);
    let css = std::fs::read_to_string(&out.artifacts[1].path).unwrap();
    assert!(
        css.contains(".sprite-b") && css.contains("background-position: -42px 0;"),
        "{css}"
    );
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (62, 30));
    assert_eq!(out.data["frames"]["a"]["width"], 40);
}

#[tokio::test]
async fn compare_images() {
    let h = harness();
    let base = pattern(50, 50, false);
    let a = save(&h, "a.png", &base);
    let mut changed = base.clone();
    for y in 10..20 {
        for x in 30..40 {
            changed.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }
    let b = save(&h, "b.png", &changed);
    let out = h
        .run("image.compare", json!({"input": s(&a), "other": s(&a)}))
        .await
        .unwrap();
    assert_eq!(out.data["identical"], true);
    assert_eq!(out.data["ssim"], 1.0);
    let out = h
        .run("image.compare", json!({"input": s(&a), "other": s(&b)}))
        .await
        .unwrap();
    assert_eq!(out.data["differing_pixels"], 100);
    assert_eq!(out.data["regions"][0]["x"].as_f64(), Some(30.0));
    assert!(out.data["ssim"].as_f64().unwrap() < 1.0);
    let c = save(&h, "c.png", &pattern(25, 25, false));
    let out = h
        .run(
            "image.compare",
            json!({"input": s(&a), "other": s(&c), "resize_to_match": false}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["same_dimensions"], false);
}

#[tokio::test]
async fn redact_regions() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(80, 80, false));
    let out = h.run("image.redact", json!({"input": s(&a), "regions": [{"x": 10, "y": 10, "width": 20, "height": 20}], "mode": "fill", "color": "#000000"})).await.unwrap();
    let img = open(&out.artifacts[0].path).to_rgb8();
    assert_eq!(img.get_pixel(15, 15).0, [0, 0, 0]);
    assert_ne!(img.get_pixel(50, 50).0, [0, 0, 0]);
    for mode in ["blur", "pixelate"] {
        let out = h.run("image.redact", json!({"input": s(&a), "regions": "[{\"x\":0,\"y\":0,\"w\":40,\"h\":40}]", "mode": mode})).await.unwrap();
        assert_eq!(out.data["regions"][0]["width"], 40);
    }
    let err = h
        .run(
            "image.redact",
            json!({"input": s(&a), "regions": [{"x": 500, "y": 500, "width": 5, "height": 5}]}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run(
            "image.redact",
            json!({"input": s(&a), "regions": [{"x": 1}]}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn info_and_dimensions() {
    let h = harness();
    let a = save(&h, "a.png", &pattern(64, 32, true));
    let out = h.run("image.info", json!({"input": s(&a)})).await.unwrap();
    assert_eq!(out.data["width"], 64);
    assert_eq!(out.data["aspect_ratio"], "2:1");
    assert_eq!(out.data["has_alpha"], true);
    assert_eq!(out.data["format"], "PNG");
    let svg = h.write("v.svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#);
    let out = h
        .run(
            "design.image_dimensions",
            json!({"input": [s(&a), s(&svg)]}),
        )
        .await
        .unwrap();
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert_eq!(rows[0][2], 64);
    assert_eq!(rows[1][1], "SVG");
    assert_eq!(rows[1][2], 20);
    assert!(out.text.unwrap().starts_with("file,format"));
}

#[tokio::test]
async fn animation_frames() {
    let h = harness();
    let p = h.path("anim.gif");
    {
        let f = std::fs::File::create(&p).unwrap();
        let mut enc = image::codecs::gif::GifEncoder::new(f);
        let frames = (0..3).map(|i| {
            image::Frame::from_parts(
                RgbaImage::from_pixel(16, 16, Rgba([i * 80, 0, 0, 255])),
                0,
                0,
                image::Delay::from_numer_denom_ms(100, 1),
            )
        });
        enc.encode_frames(frames).unwrap();
    }
    let out = h
        .run("image.gif_frames", json!({"input": s(&p)}))
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 3);
    assert_eq!(out.data["table"]["rows"][0][1], 100);
    let out = h
        .run("image.gif_frames", json!({"input": s(&p), "every": 2}))
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    let still = save(&h, "still.png", &pattern(8, 8, false));
    let err = h
        .run("image.gif_frames", json!({"input": s(&still)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn icons_and_favicon() {
    let h = harness();
    let a = save(&h, "logo.png", &pattern(300, 200, true));
    let out = h
        .run(
            "image.icons",
            json!({"input": s(&a), "sizes": ["16", "32", "512"]}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 4);
    assert_eq!(open(&out.artifacts[2].path).dimensions(), (512, 512));
    let ico = out
        .artifacts
        .iter()
        .find(|a| a.name.ends_with(".ico"))
        .unwrap();
    assert_eq!(
        open(&ico.path).width(),
        32,
        "ICO decodes to its largest frame"
    );

    let svg = h.write("icon.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="5" fill="#0a0"/></svg>"##);
    let out = h
        .run(
            "image.favicon",
            json!({"input": s(&svg), "app_name": "Test <App>"}),
        )
        .await
        .unwrap();
    let names: Vec<&str> = out.artifacts.iter().map(|a| a.name.as_str()).collect();
    for n in [
        "favicon.ico",
        "favicon-16x16.png",
        "favicon-32x32.png",
        "apple-touch-icon.png",
        "android-chrome-192x192.png",
        "android-chrome-512x512.png",
        "site.webmanifest",
    ] {
        assert!(names.contains(&n), "{n} missing from {names:?}");
    }
    let apple = open(
        &out.artifacts
            .iter()
            .find(|a| a.name == "apple-touch-icon.png")
            .unwrap()
            .path,
    )
    .to_rgba8();
    assert_eq!(apple.dimensions(), (180, 180));
    assert_eq!(apple.get_pixel(90, 90).0, [0, 170, 0, 255]);
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            &out.artifacts
                .iter()
                .find(|a| a.name == "site.webmanifest")
                .unwrap()
                .path,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "Test <App>");
    assert!(out.text.unwrap().contains("apple-touch-icon"));
}

#[tokio::test]
async fn colorspace_with_imagemagick() {
    let h = harness();
    if !h.available("image.colorspace") {
        return;
    }
    let a = save(&h, "a.png", &pattern(32, 32, false));
    let out = h
        .run(
            "image.colorspace",
            json!({"input": s(&a), "colorspace": "CMYK"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].name.ends_with(".tiff"));
    let err = h
        .run(
            "image.colorspace",
            json!({"input": s(&a), "colorspace": "CMYK", "format": "png"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let out = h
        .run(
            "image.colorspace",
            json!({"input": s(&a), "colorspace": "Gray"}),
        )
        .await
        .unwrap();
    let g = open(&out.artifacts[0].path).to_rgb8();
    assert!(g.pixels().all(|p| p[0] == p[1] && p[1] == p[2]));
}

#[tokio::test]
async fn imagemagick_fallback_for_input_formats() {
    let h = harness();
    if h.engine.deps.path("imagemagick").is_none() {
        return;
    }
    // A PCX file is not supported by the image crate; ImageMagick decodes it.
    let a = save(&h, "a.png", &pattern(20, 10, false));
    let pcx = h.path("a.pcx");
    let st = std::process::Command::new(h.engine.deps.path("imagemagick").unwrap())
        .arg(&a)
        .arg(&pcx)
        .status()
        .unwrap();
    if !st.success() {
        return;
    }
    let out = h
        .run("image.convert", json!({"input": s(&pcx), "format": "png"}))
        .await
        .unwrap();
    assert_eq!(open(&out.artifacts[0].path).dimensions(), (20, 10));
}
