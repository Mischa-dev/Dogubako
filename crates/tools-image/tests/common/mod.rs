#![allow(dead_code)]

use dogubako_core::testing::TestHarness;
use image::{DynamicImage, Rgba, RgbaImage};
use std::path::{Path, PathBuf};

pub fn harness() -> TestHarness {
    TestHarness::new(dogubako_tools_image::tools())
}

/// Colorful test pattern: quadrants + gradient + a transparent corner if `alpha`.
pub fn pattern(w: u32, h: u32, alpha: bool) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let q = (x * 2 / w.max(1), y * 2 / h.max(1));
        let base: [u8; 3] = match q {
            (0, 0) => [220, 40, 40],
            (1, 0) => [40, 180, 60],
            (0, 1) => [40, 60, 220],
            _ => [240, 220, 40],
        };
        let g = ((x + y) % 32) as u8;
        let a = if alpha && x < w / 4 && y < h / 4 {
            0
        } else {
            255
        };
        Rgba([
            base[0].saturating_sub(g),
            base[1].saturating_sub(g),
            base[2].saturating_sub(g),
            a,
        ])
    })
}

pub fn save(h: &TestHarness, name: &str, img: &RgbaImage) -> PathBuf {
    let p = h.path(name);
    let dynimg = DynamicImage::ImageRgba8(img.clone());
    let dynimg = if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        DynamicImage::ImageRgb8(dynimg.to_rgb8())
    } else {
        dynimg
    };
    dynimg.save(&p).unwrap();
    p
}

pub fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

pub fn open(p: &Path) -> DynamicImage {
    image::ImageReader::open(p)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
}
