//! Images → PDF (native, lopdf) and shared image embedding.

use crate::common::*;
use dogubako_core::prelude::*;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::path::Path;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![from_images()]
}

/// An image XObject added to a document.
pub struct Embedded {
    pub id: ObjectId,
    pub width: u32,
    pub height: u32,
    /// EXIF orientation (1..=8), JPEG only.
    pub orientation: u8,
}

impl Embedded {
    /// Displayed size in pixels after orientation.
    pub fn display_px(&self) -> (u32, u32) {
        if self.orientation >= 5 {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// `cm` matrix drawing the image into rect (x, y, w, h) honoring orientation.
    pub fn matrix(&self, x: f32, y: f32, w: f32, h: f32) -> [f32; 6] {
        let o = self.orientation;
        let place = |u: f32, v: f32| -> (f32, f32) {
            let (c, r) = (u, 1.0 - v);
            let (vx, vy) = match o {
                2 => (1.0 - c, r),
                3 => (1.0 - c, 1.0 - r),
                4 => (c, 1.0 - r),
                5 => (r, c),
                6 => (1.0 - r, c),
                7 => (1.0 - r, 1.0 - c),
                8 => (r, 1.0 - c),
                _ => (c, r),
            };
            (x + vx * w, y + (1.0 - vy) * h)
        };
        let (e, f) = place(0.0, 0.0);
        let (a1, b1) = place(1.0, 0.0);
        let (c1, d1) = place(0.0, 1.0);
        [a1 - e, b1 - f, c1 - e, d1 - f, e, f]
    }
}

struct JpegInfo {
    width: u32,
    height: u32,
    components: u8,
    adobe: bool,
    orientation: u8,
}

/// Minimal JPEG marker scan: frame size, components, Adobe APP14, EXIF orientation.
fn jpeg_info(b: &[u8]) -> Option<JpegInfo> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    let mut adobe = false;
    let mut orientation = 1;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        let seg = b.get(i + 4..i + 2 + len)?;
        match marker {
            0xC0..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                if seg.len() < 6 {
                    return None;
                }
                let height = u16::from_be_bytes([seg[1], seg[2]]) as u32;
                let width = u16::from_be_bytes([seg[3], seg[4]]) as u32;
                return Some(JpegInfo {
                    width,
                    height,
                    components: seg[5],
                    adobe,
                    orientation,
                });
            }
            0xEE if seg.starts_with(b"Adobe") => adobe = true,
            0xE1 if seg.starts_with(b"Exif\0\0") => {
                orientation = exif_orientation(&seg[6..]).unwrap_or(1)
            }
            0xDA => return None,
            _ => {}
        }
        i += 2 + len;
    }
    None
}

fn exif_orientation(t: &[u8]) -> Option<u8> {
    let le = match t.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |o: usize| -> Option<u16> {
        let s = t.get(o..o + 2)?;
        Some(if le {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let u32_at = |o: usize| -> Option<u32> {
        let s = t.get(o..o + 4)?;
        Some(if le {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            u32::from_be_bytes([s[0], s[1], s[2], s[3]])
        })
    };
    let ifd = u32_at(4)? as usize;
    let n = u16_at(ifd)? as usize;
    for k in 0..n.min(512) {
        let e = ifd + 2 + k * 12;
        if u16_at(e)? == 0x0112 {
            let v = u16_at(e + 8)?;
            return if (1..=8).contains(&v) {
                Some(v as u8)
            } else {
                None
            };
        }
    }
    None
}

/// Add an image file to `doc` as an XObject: JPEG is embedded as-is
/// (DCTDecode); anything else is decoded and Flate-compressed, with an
/// SMask for transparency.
pub fn embed_image(
    doc: &mut Document,
    path: &Path,
    max_pixels: u64,
    param: &str,
) -> ToolResult<Embedded> {
    let bytes = std::fs::read(path)?;
    if let Some(j) = jpeg_info(&bytes) {
        if j.width == 0 || j.height == 0 {
            return Err(ToolError::invalid(param, "JPEG has no dimensions"));
        }
        if j.width as u64 * j.height as u64 > max_pixels {
            return Err(ToolError::limit(format!(
                "image is {}×{} pixels, above the limit",
                j.width, j.height
            )));
        }
        let cs = match j.components {
            1 => "DeviceGray",
            3 => "DeviceRGB",
            4 => "DeviceCMYK",
            c => {
                return Err(ToolError::invalid(
                    param,
                    format!("unsupported JPEG with {c} components"),
                ))
            }
        };
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"XObject".to_vec()));
        d.set("Subtype", Object::Name(b"Image".to_vec()));
        d.set("Width", Object::Integer(j.width as i64));
        d.set("Height", Object::Integer(j.height as i64));
        d.set("ColorSpace", Object::Name(cs.as_bytes().to_vec()));
        d.set("BitsPerComponent", Object::Integer(8));
        d.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        if j.components == 4 && j.adobe {
            d.set(
                "Decode",
                Object::Array(
                    [1, 0, 1, 0, 1, 0, 1, 0]
                        .iter()
                        .map(|v| Object::Integer(*v))
                        .collect(),
                ),
            );
        }
        let id = doc.add_object(Stream::new(d, bytes).with_compression(false));
        return Ok(Embedded {
            id,
            width: j.width,
            height: j.height,
            orientation: j.orientation,
        });
    }

    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ctx("reading image")?;
    if reader.format().is_none() {
        return Err(ToolError::invalid(param, "unrecognized image format"));
    }
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| ToolError::invalid(param, format!("cannot read image: {e}")))?;
    if w as u64 * h as u64 > max_pixels {
        return Err(ToolError::limit(format!(
            "image is {w}×{h} pixels, above the limit"
        )));
    }
    let img = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ctx("reading image")?
        .decode()
        .map_err(|e| ToolError::invalid(param, format!("cannot decode image: {e}")))?;
    let has_alpha = img.color().has_alpha();
    let gray = !img.color().has_color();
    let (data, cs) = if gray {
        (img.to_luma8().into_raw(), "DeviceGray")
    } else {
        (img.to_rgb8().into_raw(), "DeviceRGB")
    };
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"XObject".to_vec()));
    d.set("Subtype", Object::Name(b"Image".to_vec()));
    d.set("Width", Object::Integer(w as i64));
    d.set("Height", Object::Integer(h as i64));
    d.set("ColorSpace", Object::Name(cs.as_bytes().to_vec()));
    d.set("BitsPerComponent", Object::Integer(8));
    if has_alpha {
        let alpha: Vec<u8> = img.to_rgba8().pixels().map(|p| p.0[3]).collect();
        if alpha.iter().any(|a| *a != 255) {
            let mut md = Dictionary::new();
            md.set("Type", Object::Name(b"XObject".to_vec()));
            md.set("Subtype", Object::Name(b"Image".to_vec()));
            md.set("Width", Object::Integer(w as i64));
            md.set("Height", Object::Integer(h as i64));
            md.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            md.set("BitsPerComponent", Object::Integer(8));
            let mut ms = Stream::new(md, alpha);
            ms.compress().ctx("compressing mask")?;
            let mid = doc.add_object(ms);
            d.set("SMask", Object::Reference(mid));
        }
    }
    let mut s = Stream::new(d, data);
    s.compress().ctx("compressing image")?;
    let id = doc.add_object(s);
    Ok(Embedded {
        id,
        width: w,
        height: h,
        orientation: 1,
    })
}

const MM: f32 = 72.0 / 25.4;

fn from_images() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.from_images", "Images to PDF", Category::Pdf)
        .describe("Combine images (JPEG, PNG, WebP, TIFF, GIF, BMP) into a PDF, one image per page; JPEGs are embedded without recompression")
        .keywords(&["jpg to pdf", "png to pdf", "photos", "scan", "convert", "combine", "images"])
        .param(ParamSpec::files("input").label("Images (in order)").accept(&["image/*"]).required().primary())
        .param(ParamSpec::choice("page_size", &[("fit", "Fit to image"), ("a4", "A4"), ("letter", "US Letter"), ("legal", "US Legal"), ("a3", "A3"), ("a5", "A5")]).default_str("fit"))
        .param(ParamSpec::choice("orientation", &[("auto", "Auto (match image)"), ("portrait", "Portrait"), ("landscape", "Landscape")]).default_str("auto"))
        .param(ParamSpec::number("margin").label("Margin (mm)").range(0.0, 100.0).default_num(0.0))
        .param(ParamSpec::integer("dpi").label("Image DPI for 'fit'").describe("Pixels per inch used to size 'fit' pages").range(10.0, 2400.0).default_int(150).advanced())
        .param(ParamSpec::text("output_name").label("Output file name").default_str("images.pdf").advanced())
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let size = p.str_or("page_size", "fit");
        let orient = p.str_or("orientation", "auto");
        let margin = p.num_or("margin", 0.0) as f32 * MM;
        let dpi = p.int_or("dpi", 150) as f32;
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut kids = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(ctx, i, files.len(), &f.name);
            let img = embed_image(&mut doc, &f.path, ctx.limits().max_image_pixels, "input")
                .map_err(|e| match e {
                    ToolError::InvalidInput { param, message } => ToolError::InvalidInput {
                        param,
                        message: format!("{}: {message}", f.name),
                    },
                    o => o,
                })?;
            let (pw_px, ph_px) = img.display_px();
            let (iw, ih) = (pw_px as f32 * 72.0 / dpi, ph_px as f32 * 72.0 / dpi);
            let (mut pw, mut ph) = match size {
                "a4" => (595.28, 841.89),
                "letter" => (612.0, 792.0),
                "legal" => (612.0, 1008.0),
                "a3" => (841.89, 1190.55),
                "a5" => (419.53, 595.28),
                _ => (iw + 2.0 * margin, ih + 2.0 * margin),
            };
            if size != "fit" {
                let landscape = match orient {
                    "landscape" => true,
                    "portrait" => false,
                    _ => pw_px > ph_px,
                };
                if landscape != (pw > ph) {
                    std::mem::swap(&mut pw, &mut ph);
                }
            }
            let (aw, ah) = ((pw - 2.0 * margin).max(1.0), (ph - 2.0 * margin).max(1.0));
            let scale = if size == "fit" {
                1.0
            } else {
                (aw / iw).min(ah / ih)
            };
            let (dw, dh) = (iw * scale, ih * scale);
            let (x, y) = (margin + (aw - dw) / 2.0, margin + (ah - dh) / 2.0);
            let m = img.matrix(x, y, dw, dh);
            let ops = format!(
                "q {} {} {} {} {} {} cm /Im0 Do Q\n",
                pf(m[0]),
                pf(m[1]),
                pf(m[2]),
                pf(m[3]),
                pf(m[4]),
                pf(m[5])
            );
            let content = doc.add_object(Stream::new(Dictionary::new(), ops.into_bytes()));
            let mut xo = Dictionary::new();
            xo.set("Im0", Object::Reference(img.id));
            let mut res = Dictionary::new();
            res.set("XObject", Object::Dictionary(xo));
            let mut page = Dictionary::new();
            page.set("Type", Object::Name(b"Page".to_vec()));
            page.set("Parent", Object::Reference(pages_id));
            page.set(
                "MediaBox",
                Object::Array(vec![0.into(), 0.into(), Object::Real(pw), Object::Real(ph)]),
            );
            page.set("Resources", Object::Dictionary(res));
            page.set("Contents", Object::Reference(content));
            kids.push(Object::Reference(doc.add_object(page)));
        }
        let count = kids.len();
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Count", Object::Integer(count as i64));
        pages.set("Kids", Object::Array(kids));
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let mut cat = Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set("Pages", Object::Reference(pages_id));
        let cat_id = doc.add_object(cat);
        doc.trailer.set("Root", Object::Reference(cat_id));
        let mut info = Dictionary::new();
        info.set("Producer", text_string("Dogubako"));
        info.set("CreationDate", text_string(&pdf_date_now()));
        let info_id = doc.add_object(info);
        doc.trailer.set("Info", Object::Reference(info_id));
        let mut name = p.str_or("output_name", "images.pdf").to_string();
        if !name.to_ascii_lowercase().ends_with(".pdf") {
            name.push_str(".pdf");
        }
        let out = ctx.output_path(&name);
        let mut file = std::io::BufWriter::new(std::fs::File::create(&out)?);
        doc.save_to(&mut file).ctx("writing PDF")?;
        use std::io::Write;
        file.flush()?;
        Ok(ToolOutput::files(vec![ctx.artifact(&out)?]).with_data(json!({"pages": count})))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_matrix() {
        let e = Embedded {
            id: (1, 0),
            width: 40,
            height: 20,
            orientation: 1,
        };
        assert_eq!(
            e.matrix(10.0, 20.0, 40.0, 20.0),
            [40.0, 0.0, 0.0, 20.0, 10.0, 20.0]
        );
        // Rotated 90° clockwise: displayed 20×40.
        let e = Embedded {
            id: (1, 0),
            width: 40,
            height: 20,
            orientation: 6,
        };
        assert_eq!(e.display_px(), (20, 40));
        let m = e.matrix(0.0, 0.0, 20.0, 40.0);
        // Stored top-left (u=0,v=1) must land at displayed top-right (20,40).
        assert_eq!((m[2] + m[4], m[3] + m[5]), (20.0, 40.0));
    }

    #[test]
    fn jpeg_header() {
        let img = image::RgbImage::from_pixel(7, 5, image::Rgb([1, 2, 3]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        let j = jpeg_info(buf.get_ref()).unwrap();
        assert_eq!(
            (j.width, j.height, j.components, j.orientation),
            (7, 5, 3, 1)
        );
        assert!(jpeg_info(b"not a jpeg").is_none());
    }
}
