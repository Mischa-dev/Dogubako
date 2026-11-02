//! Metadata inspection (EXIF natively, everything via ExifTool) and
//! lossless metadata stripping.

use crate::util::*;
use dogubako_core::prelude::*;
use img_parts::{Bytes, ImageEXIF, ImageICC};
use std::io::BufReader;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![metadata(), strip_metadata()]
}

/// Absolute or `./`-prefixed path so external tools never see an option.
pub fn arg_path(p: &std::path::Path) -> std::ffi::OsString {
    if p.is_absolute() {
        p.as_os_str().to_os_string()
    } else {
        let mut s = std::ffi::OsString::from("./");
        s.push(p.as_os_str());
        s
    }
}

fn gps_decimal(ex: &exif::Exif) -> Option<(f64, f64)> {
    let coord = |tag, ref_tag| -> Option<f64> {
        let f = ex.get_field(tag, exif::In::PRIMARY)?;
        let v = match &f.value {
            exif::Value::Rational(v) if v.len() >= 3 => {
                v[0].to_f64() + v[1].to_f64() / 60.0 + v[2].to_f64() / 3600.0
            }
            _ => return None,
        };
        let r = ex
            .get_field(ref_tag, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string())
            .unwrap_or_default();
        Some(if r.contains('S') || r.contains('W') {
            -v
        } else {
            v
        })
    };
    Some((
        coord(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef)?,
        coord(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef)?,
    ))
}

pub fn read_exif(path: &std::path::Path) -> Option<exif::Exif> {
    let f = std::fs::File::open(path).ok()?;
    exif::Reader::new()
        .read_from_container(&mut BufReader::new(f))
        .ok()
}

fn metadata() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.metadata", "View Image Metadata", Category::Image)
        .describe("Inspect EXIF metadata (camera, lens, date, GPS location...) natively, or all metadata (XMP, IPTC, ICC, maker notes) with ExifTool")
        .keywords(&["exif", "metadata", "xmp", "iptc", "gps", "camera", "location", "exiftool", "inspect"])
        .param(image_param())
        .param(ParamSpec::choice("engine", &[("auto", "Auto (ExifTool if installed)"), ("native", "Built-in EXIF reader"), ("exiftool", "ExifTool (all metadata)")]).default_str("auto"))
        .output(OutputKind::Data);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let engine = p.str_or("engine", "auto").to_string();
        let use_exiftool = match engine.as_str() {
            "exiftool" => true,
            "native" => false,
            _ => ctx.deps().path("exiftool").is_some(),
        };
        let path = f.path.clone();
        let native = ctx
            .blocking(move || {
                let mut rows = vec![];
                let mut obj = serde_json::Map::new();
                let mut gps = None;
                if let Some(ex) = read_exif(&path) {
                    for field in ex.fields() {
                        let v = field.display_value().with_unit(&ex).to_string();
                        let ifd = if field.ifd_num == exif::In::THUMBNAIL {
                            "Thumbnail"
                        } else {
                            "Primary"
                        };
                        let name = field.tag.to_string();
                        rows.push(json!([ifd, name, v]));
                        if field.ifd_num == exif::In::PRIMARY {
                            obj.insert(name, Value::String(v));
                        }
                    }
                    gps = gps_decimal(&ex);
                }
                Ok((rows, obj, gps))
            })
            .await?;
        let (rows, obj, gps) = native;
        let mut data = json!({
            "file": f.name,
            "exif_fields": rows.len(),
            "gps": gps.map(|(lat, lon)| json!({"latitude": lat, "longitude": lon, "map": format!("https://www.openstreetmap.org/?mlat={lat:.6}&mlon={lon:.6}#map=16/{lat:.6}/{lon:.6}")})),
            "exif": obj,
        });
        let mut out_rows = rows;
        if use_exiftool {
            let out = ctx
                .command("exiftool")?
                .args(["-j", "-G1", "-a", "-s", "-x", "SourceFile"])
                .arg(arg_path(&f.path))
                .run()
                .await?;
            let parsed: Value = serde_json::from_slice(&out.stdout)
                .map_err(|e| ToolError::failed(format!("could not parse ExifTool output: {e}")))?;
            let first = parsed
                .as_array()
                .and_then(|a| a.first())
                .cloned()
                .unwrap_or(Value::Null);
            if let Value::Object(m) = &first {
                out_rows = m
                    .iter()
                    .map(|(k, v)| {
                        let (g, t) = k.split_once(':').unwrap_or(("", k));
                        json!([
                            g,
                            t,
                            match v {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            }
                        ])
                    })
                    .collect();
            }
            data["exiftool"] = first;
            data["engine"] = json!("exiftool");
        } else {
            data["engine"] = json!("native");
            if out_rows.is_empty() {
                data["note"] = json!("no EXIF metadata found");
            }
        }
        data["table"] = json!({"columns": ["Group", "Tag", "Value"], "rows": out_rows});
        Ok(ToolOutput::data(data))
    })
}

/// Minimal big-endian TIFF/EXIF blob with only an Orientation tag.
pub fn orientation_exif(orientation: u16) -> Vec<u8> {
    let mut v = b"MM\x00\x2a\x00\x00\x00\x08".to_vec();
    v.extend_from_slice(&1u16.to_be_bytes()); // one entry
    v.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
    v.extend_from_slice(&3u16.to_be_bytes()); // SHORT
    v.extend_from_slice(&1u32.to_be_bytes()); // count
    v.extend_from_slice(&orientation.to_be_bytes());
    v.extend_from_slice(&[0, 0]); // padding
    v.extend_from_slice(&0u32.to_be_bytes()); // next IFD
    v
}

fn read_orientation(path: &std::path::Path) -> Option<u16> {
    let ex = read_exif(path)?;
    let o = ex
        .get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
        .get_uint(0)? as u16;
    (o > 1 && o <= 8).then_some(o)
}

/// Bytes of the JPEG entropy stream up to and including the primary image's EOI.
fn truncate_after_eoi(entropy: &[u8]) -> usize {
    let mut i = 0;
    while i + 1 < entropy.len() {
        if entropy[i] == 0xFF {
            let m = entropy[i + 1];
            match m {
                0x00 | 0xFF | 0xD0..=0xD7 => i += if m == 0xFF { 1 } else { 2 },
                0xD9 => return i + 2,
                _ => {
                    if i + 3 >= entropy.len() {
                        return entropy.len();
                    }
                    let len = u16::from_be_bytes([entropy[i + 2], entropy[i + 3]]) as usize;
                    i += 2 + len;
                }
            }
        } else {
            i += 1;
        }
    }
    entropy.len()
}

pub struct StripOpts {
    pub keep_orientation: bool,
    pub keep_icc: bool,
}

/// Losslessly remove metadata from JPEG/PNG/WebP bytes. Returns None for other formats.
pub fn strip_native(
    data: Vec<u8>,
    orientation: Option<u16>,
    o: &StripOpts,
) -> ToolResult<Option<Vec<u8>>> {
    let orient = if o.keep_orientation {
        orientation
    } else {
        None
    };
    let bytes = Bytes::from(data);
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut j = img_parts::jpeg::Jpeg::from_bytes(bytes)
            .map_err(|e| ToolError::input(format!("invalid JPEG: {e}")))?;
        j.set_exif(None);
        if !o.keep_icc {
            j.set_icc_profile(None);
        }
        let keep_icc = o.keep_icc;
        j.segments_mut().retain(|s| {
            let m = s.marker();
            let c = s.contents();
            match m {
                0xE0 | 0xEE => true, // JFIF, Adobe (color transform)
                0xE2 => keep_icc && c.starts_with(b"ICC_PROFILE\0"), // ICC kept on request; MPF/FlashPix dropped
                0xE1 | 0xE3..=0xED | 0xEF | 0xFE => false, // EXIF/XMP, vendor APPn, IPTC, comments
                _ => true,
            }
        });
        // Drop trailing data after the primary image (MPF secondary images, thumbnails, junk).
        if let Some(pos) = j.segments().iter().position(|s| s.has_entropy()) {
            let seg = j.segments()[pos].clone();
            let contents = seg.contents().clone();
            let enc = seg.encoder().bytes();
            let entropy = enc.slice((4 + contents.len()).min(enc.len())..);
            let cut = truncate_after_eoi(&entropy);
            if cut < entropy.len() {
                let marker = j.segments()[pos].marker();
                j.segments_mut()[pos] = img_parts::jpeg::JpegSegment::new_with_entropy(
                    marker,
                    contents,
                    entropy.slice(..cut),
                );
            }
        }
        if let Some(or) = orient {
            j.set_exif(Some(Bytes::from(orientation_exif(or))));
        }
        return Ok(Some(j.encoder().bytes().to_vec()));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut png = img_parts::png::Png::from_bytes(bytes)
            .map_err(|e| ToolError::input(format!("invalid PNG: {e}")))?;
        let keep_icc = o.keep_icc;
        png.chunks_mut().retain(|c| {
            let k = c.kind();
            !(matches!(
                &k,
                b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf" | b"tIME" | b"dSIG"
            ) || (&k == b"iCCP" && !keep_icc))
        });
        if let Some(or) = orient {
            png.set_exif(Some(Bytes::from(orientation_exif(or))));
        }
        return Ok(Some(png.encoder().bytes().to_vec()));
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let mut w = img_parts::webp::WebP::from_bytes(bytes)
            .map_err(|e| ToolError::input(format!("invalid WebP: {e}")))?;
        w.set_exif(None);
        if !o.keep_icc {
            w.set_icc_profile(None);
        }
        w.remove_chunks_by_id(img_parts::webp::CHUNK_XMP);
        if let Some(or) = orient {
            w.set_exif(Some(Bytes::from(orientation_exif(or))));
        }
        return Ok(Some(w.encoder().bytes().to_vec()));
    }
    Ok(None)
}

fn strip_metadata() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("image.strip_metadata", "Remove Image Metadata", Category::Image)
        .describe("Remove EXIF, GPS location, XMP, IPTC and comments from photos without re-encoding (JPEG, PNG, WebP natively; other formats via ExifTool)")
        .keywords(&["strip", "remove exif", "privacy", "gps", "metadata", "clean", "anonymize", "location"])
        .param(images_param())
        .param(ParamSpec::boolean("keep_orientation").describe("Keep the EXIF orientation so photos stay upright").default_bool(true))
        .param(ParamSpec::boolean("keep_icc").label("Keep color profile").describe("Keep the embedded ICC color profile").default_bool(true))
        .output(OutputKind::Files)
        .progress();
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        if files.is_empty() {
            return Err(ToolError::invalid(
                "input",
                "at least one image is required",
            ));
        }
        let opts = StripOpts {
            keep_orientation: p.bool("keep_orientation"),
            keep_icc: p.bool("keep_icc"),
        };
        let mut arts = vec![];
        let mut rows = vec![];
        let n = files.len();
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(i as f32 / n as f32, format!("Stripping {}", f.name));
            let path = f.path.clone();
            let ko = opts.keep_orientation;
            let ki = opts.keep_icc;
            let native = ctx
                .blocking(move || {
                    let data = std::fs::read(&path)?;
                    strip_native(
                        data,
                        read_orientation(&path),
                        &StripOpts {
                            keep_orientation: ko,
                            keep_icc: ki,
                        },
                    )
                })
                .await?;
            let out = ctx.output_path(&suffixed(&f.name, "clean", None));
            let method = match native {
                Some(bytes) => {
                    std::fs::write(&out, bytes)?;
                    "native"
                }
                None => {
                    if ctx.deps().path("exiftool").is_none() {
                        return Err(ToolError::MissingDependency {
                            dependency: "exiftool".into(),
                            hint: format!("{}: stripping metadata from this format requires ExifTool (JPEG, PNG and WebP work without it)", f.name),
                        });
                    }
                    let src = safe_input_copy(&ctx, &f.path, "strip-in")?;
                    let mut cmd = ctx.command("exiftool")?.arg("-all=");
                    if opts.keep_icc {
                        cmd = cmd.arg("-tagsfromfile").arg("@").arg("-icc_profile");
                    }
                    if opts.keep_orientation {
                        cmd = cmd.arg("-tagsfromfile").arg("@").arg("-orientation");
                    }
                    let tmp =
                        ctx.temp_path(&format!("strip-out-{}", dogubako_core::util::random_id(4)));
                    let tmp = tmp.with_extension(f.extension());
                    cmd.arg("-o")
                        .arg(arg_path(&tmp))
                        .arg(arg_path(&src))
                        .run()
                        .await?;
                    std::fs::copy(&tmp, &out)?;
                    "exiftool"
                }
            };
            let a = ctx.artifact(&out)?;
            rows.push(json!([
                f.name,
                human_size(f.size()),
                human_size(a.size),
                human_size(f.size().saturating_sub(a.size)),
                method
            ]));
            arts.push(a);
        }
        ctx.progress(1.0, "done");
        Ok(ToolOutput::files(arts).with_data(json!({"table": {"columns": ["File", "Before", "After", "Removed", "Method"], "rows": rows}})))
    })
}
