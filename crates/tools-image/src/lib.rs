//! Image, design and code (QR/barcode) tools.
//!
//! Core image work is native (image, imageproc, resvg, oxipng, webp,
//! rustface, rqrr, rxing); ImageMagick, ExifTool, Tesseract, rembg and
//! Real-ESRGAN are used when installed for formats and features beyond that.

use dogubako_core::prelude::*;

pub mod ai;
pub mod codes;
pub mod convert;
pub mod design;
pub mod edit;
pub mod face;
pub mod meta;
pub mod svg;
pub mod util;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = Vec::new();
    v.extend(convert::tools());
    v.extend(edit::tools());
    v.extend(meta::tools());
    v.extend(svg::tools());
    v.extend(face::tools());
    v.extend(ai::tools());
    v.extend(design::tools());
    v.extend(codes::tools());
    v
}
