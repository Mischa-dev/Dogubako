//! PDF and document tools: structure (qpdf), rendering and text (Poppler),
//! compression (Ghostscript), OCR (Tesseract), native editing (lopdf) and
//! office / markup conversion (LibreOffice, Pandoc).

use dogubako_core::prelude::*;

mod common;
mod compress;
mod convert;
mod forms;
mod images;
mod meta;
mod ocr;
mod poppler;
mod ranges;
mod stamp;
mod structure;

#[cfg(test)]
mod tests;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = Vec::new();
    v.extend(structure::tools());
    v.extend(compress::tools());
    v.extend(poppler::tools());
    v.extend(images::tools());
    v.extend(ocr::tools());
    v.extend(stamp::tools());
    v.extend(meta::tools());
    v.extend(forms::tools());
    v.extend(convert::tools());
    v
}
