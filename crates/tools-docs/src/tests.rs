//! Integration tests running the PDF tools through the real engine.

use dogubako_core::testing::TestHarness;
use lopdf::{dictionary, Document, Object, Stream};
use serde_json::json;
use std::path::Path;

fn harness() -> TestHarness {
    TestHarness::new(crate::tools())
}

/// Build an n-page PDF where page i shows "Page i" (and optional big text).
fn make_pdf(path: &Path, n: u32, big: Option<&str>) {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"},
    );
    let mut kids = vec![];
    for i in 1..=n {
        let mut content = format!("BT /F1 24 Tf 72 700 Td (Page {i}) Tj ET\n");
        if let Some(t) = big {
            content.push_str(&format!("BT /F1 64 Tf 60 400 Td ({t}) Tj ET\n"));
        }
        let c = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => c,
            "Resources" => dictionary! {"Font" => dictionary! {"F1" => font_id}},
        });
        kids.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => kids, "Count" => n as i64}),
    );
    let cat = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id});
    doc.trailer.set("Root", cat);
    let info = doc.add_object(dictionary! {"Title" => Object::string_literal("Original")});
    doc.trailer.set("Info", info);
    doc.save(path).unwrap();
}

/// One-page PDF with a text field "name" and a checkbox "agree".
fn make_form(path: &Path) {
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let font_id = doc.add_object(
        dictionary! {"Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica"},
    );
    let on = doc.add_object(Stream::new(dictionary! {"Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()]}, b"0 0 1 rg 2 2 16 16 re f".to_vec()));
    let off = doc.add_object(Stream::new(dictionary! {"Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()]}, b"".to_vec()));
    let name = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("name"),
        "Rect" => vec![100.into(), 600.into(), 300.into(), 620.into()], "P" => page_id, "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
    });
    let agree = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("agree"),
        "Rect" => vec![100.into(), 550.into(), 120.into(), 570.into()], "P" => page_id, "V" => "Off", "AS" => "Off",
        "AP" => dictionary! {"N" => dictionary! {"Yes" => on, "Off" => off}},
    });
    let c = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 20 700 Td (Form) Tj ET".to_vec(),
    ));
    doc.objects.insert(page_id, Object::Dictionary(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => c, "Resources" => dictionary! {"Font" => dictionary! {"F1" => font_id}}, "Annots" => vec![name.into(), agree.into()],
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! {"Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1},
        ),
    );
    let form = dictionary! {"Fields" => vec![name.into(), agree.into()], "DA" => Object::string_literal("/Helv 0 Tf 0 g"), "DR" => dictionary! {"Font" => dictionary! {"Helv" => font_id}}};
    let cat =
        doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id, "AcroForm" => form});
    doc.trailer.set("Root", cat);
    doc.save(path).unwrap();
}

fn pages(p: &Path) -> usize {
    Document::load(p).unwrap().get_pages().len()
}

fn text_of(h: &TestHarness, p: &Path) -> String {
    let out = std::process::Command::new("pdftotext")
        .arg(p)
        .arg("-")
        .output();
    let _ = h;
    out.map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

#[test]
fn specs_are_consistent() {
    let tools = crate::tools();
    let mut ids = std::collections::HashSet::new();
    for t in &tools {
        let sp = t.spec();
        assert!(sp.id.starts_with("pdf."), "{}", sp.id);
        assert!(ids.insert(sp.id.clone()), "duplicate {}", sp.id);
        assert!(!sp.description.is_empty(), "{}", sp.id);
        for p in &sp.params {
            assert!(
                p.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}.{}",
                sp.id,
                p.name
            );
        }
    }
    assert!(tools.len() >= 25);
}

#[tokio::test]
async fn merge_split_extract() {
    let h = harness();
    if !h.available("pdf.merge") {
        return;
    }
    let a = h.path("a.pdf");
    let b = h.path("b.pdf");
    make_pdf(&a, 2, None);
    make_pdf(&b, 3, None);
    let out = h
        .run(
            "pdf.merge",
            json!({"input": [s(&a), s(&b)], "ranges": "1\n2-3"}),
        )
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 3);
    assert_eq!(out.data["pages"], 3);
    let out = h
        .run("pdf.merge", json!({"input": [s(&a), s(&b)]}))
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 5);
    let merged = out.artifacts[0].path.clone();
    // Bad range syntax never reaches qpdf.
    let err = h
        .run(
            "pdf.merge",
            json!({"input": [s(&a), s(&b)], "ranges": "1-9"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run("pdf.merge", json!({"input": [s(&a)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run(
            "pdf.split",
            json!({"input": s(&merged), "mode": "every", "every": 2}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 3);
    assert_eq!(pages(&out.artifacts[0].path), 2);
    assert_eq!(pages(&out.artifacts[2].path), 1);
    let out = h
        .run("pdf.split", json!({"input": s(&merged), "mode": "single"}))
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 5);
    let out = h
        .run(
            "pdf.split",
            json!({"input": s(&merged), "mode": "ranges", "ranges": "1-2; 5\n3-4"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 3);
    assert_eq!(pages(&out.artifacts[1].path), 1);

    let out = h
        .run(
            "pdf.extract_pages",
            json!({"input": s(&merged), "pages": "2,4-z"}),
        )
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 3);
    for bad in ["0", "1-99", "abc", "--help", "1;2"] {
        let err = h
            .run(
                "pdf.extract_pages",
                json!({"input": s(&merged), "pages": bad}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_input", "{bad}");
    }
    let out = h
        .run(
            "pdf.delete_pages",
            json!({"input": s(&merged), "pages": "1,3"}),
        )
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 3);
    let err = h
        .run(
            "pdf.delete_pages",
            json!({"input": s(&merged), "pages": "1-z"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    // Not a PDF.
    let junk = h.write("junk.pdf", b"hello, not a pdf");
    let err = h
        .run(
            "pdf.extract_pages",
            json!({"input": s(&junk), "pages": "1"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn reorder_rotate() {
    let h = harness();
    if !h.available("pdf.reorder") || !h.available("pdf.extract_text") {
        return;
    }
    let a = h.path("a.pdf");
    make_pdf(&a, 3, None);
    let out = h
        .run("pdf.reorder", json!({"input": s(&a), "reverse": true}))
        .await
        .unwrap();
    let t = text_of(&h, &out.artifacts[0].path);
    let (p3, p1) = (t.find("Page 3").unwrap(), t.find("Page 1").unwrap());
    assert!(p3 < p1, "reversed order expected: {t}");
    let out = h
        .run("pdf.reorder", json!({"input": s(&a), "order": "2,2,1"}))
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 3);
    let err = h
        .run("pdf.reorder", json!({"input": s(&a)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run(
            "pdf.rotate",
            json!({"input": [s(&a)], "angle": "90", "pages": "1"}),
        )
        .await
        .unwrap();
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    let ids: Vec<_> = doc.get_pages().into_values().collect();
    let rot = |id| {
        doc.get_dictionary(id)
            .unwrap()
            .get(b"Rotate")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0)
    };
    assert_eq!(rot(ids[0]), 90);
    assert_eq!(rot(ids[1]), 0);
}

#[tokio::test]
async fn protect_unlock_roundtrip() {
    let h = harness();
    if !h.available("pdf.protect") {
        return;
    }
    let a = h.path("a.pdf");
    make_pdf(&a, 2, None);
    let out = h.run("pdf.protect", json!({"input": [s(&a)], "user_password": "open sesame", "owner_password": "owner-pw", "allow": ["print"]})).await.unwrap();
    let locked = out.artifacts[0].path.clone();
    let raw = std::fs::read(&locked).unwrap();
    assert!(
        raw.windows(8).any(|w| w == b"/Encrypt"),
        "output must be encrypted"
    );
    // Same passwords are rejected.
    let err = h
        .run(
            "pdf.protect",
            json!({"input": [s(&a)], "user_password": "x", "owner_password": "x"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    // Native editing refuses encrypted input instead of stripping protection.
    let err = h
        .run(
            "pdf.metadata_edit",
            json!({"input": [s(&locked)], "title": "x"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run(
            "pdf.unlock",
            json!({"input": [s(&locked)], "password": "wrong"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input", "{err}");
    let err = h
        .run("pdf.unlock", json!({"input": [s(&locked)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let out = h
        .run(
            "pdf.unlock",
            json!({"input": [s(&locked)], "password": "open sesame"}),
        )
        .await
        .unwrap();
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    assert!(!doc.is_encrypted());
    assert_eq!(doc.get_pages().len(), 2);
}

#[tokio::test]
async fn compress_linearize_repair_flatten() {
    let h = harness();
    if !h.available("pdf.compress") {
        return;
    }
    let a = h.path("a.pdf");
    make_pdf(&a, 4, Some("BIG TEXT"));
    let out = h
        .run(
            "pdf.compress",
            json!({"input": [s(&a)], "preset": "screen"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].size <= std::fs::metadata(&a).unwrap().len());
    assert_eq!(pages(&out.artifacts[0].path), 4);
    assert_eq!(out.data["table"]["rows"].as_array().unwrap().len(), 1);

    let out = h
        .run("pdf.linearize", json!({"input": [s(&a)]}))
        .await
        .unwrap();
    let head = std::fs::read(&out.artifacts[0].path).unwrap();
    assert!(head[..1024.min(head.len())]
        .windows(11)
        .any(|w| w == b"/Linearized"));

    // Damage the xref offsets: qpdf reconstructs the file.
    let mut bytes = std::fs::read(&a).unwrap();
    let pos = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    bytes.truncate(pos);
    bytes.extend_from_slice(b"startxref\n999999\n%%EOF\n");
    let broken = h.write("broken.pdf", &bytes);
    let out = h
        .run("pdf.repair", json!({"input": [s(&broken)]}))
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 4);

    let form = h.path("form.pdf");
    make_form(&form);
    let out = h
        .run("pdf.flatten", json!({"input": [s(&form)]}))
        .await
        .unwrap();
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    let page = *doc.get_pages().values().next().unwrap();
    // The checkbox (which has an appearance) is merged into the page content.
    assert!(doc
        .get_page_annotations(page)
        .map(|a| a.len() < 2)
        .unwrap_or(true));
}

#[tokio::test]
async fn text_images_info() {
    let h = harness();
    if !h.available("pdf.extract_text") {
        return;
    }
    let a = h.path("doc.pdf");
    make_pdf(&a, 3, None);
    let out = h
        .run(
            "pdf.extract_text",
            json!({"input": [s(&a)], "first_page": 2, "last_page": 2}),
        )
        .await
        .unwrap();
    let t = out.text.unwrap();
    assert!(t.contains("Page 2") && !t.contains("Page 3"), "{t}");
    let err = h
        .run(
            "pdf.extract_text",
            json!({"input": [s(&a)], "first_page": 9}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run(
            "pdf.to_images",
            json!({"input": [s(&a)], "dpi": 72, "pages": "1-2"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    assert_eq!(
        image::image_dimensions(&out.artifacts[0].path).unwrap(),
        (612, 792)
    );
    let out = h
        .run(
            "pdf.to_images",
            json!({"input": [s(&a)], "dpi": 36, "format": "jpeg", "pages": "3"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    assert!(out.artifacts[0].name.ends_with(".jpg"));

    let out = h.run("pdf.info", json!({"input": s(&a)})).await.unwrap();
    assert_eq!(out.data["pages"], 3);
    assert_eq!(out.data["title"], "Original");
    assert!(out.data["page_sizes"][0]["size"]
        .as_str()
        .unwrap()
        .contains("612"));
    let b = h.path("b.pdf");
    make_pdf(&b, 2, None);
    let out = h
        .run(
            "pdf.compare_text",
            json!({"original": s(&a), "changed": s(&b)}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["identical"], false);
    assert!(out.data["diff"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["tag"] == "delete" && d["text"].as_str().unwrap().contains("Page 3")));
}

fn png_with_alpha(path: &Path) {
    let img = image::RgbaImage::from_fn(40, 20, |x, _| {
        image::Rgba([255, 0, 0, if x < 20 { 255 } else { 0 }])
    });
    img.save(path).unwrap();
}

#[tokio::test]
async fn from_images_and_extract_images() {
    let h = harness();
    let png = h.path("a.png");
    png_with_alpha(&png);
    let jpg = h.path("b.jpg");
    image::RgbImage::from_pixel(30, 60, image::Rgb([0, 128, 255]))
        .save(&jpg)
        .unwrap();
    let out = h
        .run(
            "pdf.from_images",
            json!({"input": [s(&png), s(&jpg)], "page_size": "a4", "margin": 10}),
        )
        .await
        .unwrap();
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    assert_eq!(doc.get_pages().len(), 2);
    let has = |filter: &[u8]| {
        doc.objects.values().any(|o| matches!(o, Object::Stream(s) if s.dict.get(b"Filter").ok().and_then(|f| f.as_name().ok()) == Some(filter) && s.dict.has(b"Width")))
    };
    assert!(has(b"DCTDecode"), "JPEG embedded directly");
    assert!(has(b"FlateDecode"), "PNG flate-compressed");
    assert!(
        doc.objects
            .values()
            .any(|o| matches!(o, Object::Stream(s) if s.dict.has(b"SMask"))),
        "alpha preserved as SMask"
    );
    // Landscape image on A4 → landscape page.
    let first = *doc.get_pages().values().next().unwrap();
    let mb = doc
        .get_dictionary(first)
        .unwrap()
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let w = mb[2].as_float().unwrap();
    assert!(w > 800.0, "landscape A4 expected, got width {w}");

    let bad = h.write("bad.png", b"\x89PNG\r\n\x1a\nnot really");
    let err = h
        .run("pdf.from_images", json!({"input": [s(&bad)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    if h.available("pdf.extract_images") {
        let pdf = out.artifacts[0].path.clone();
        let out = h
            .run("pdf.extract_images", json!({"input": s(&pdf)}))
            .await
            .unwrap();
        assert!(out.artifacts.len() >= 2, "{:?}", out.artifacts);
        assert!(out.artifacts.iter().any(|a| a.name.ends_with(".jpg")));
    }
}

#[tokio::test]
async fn watermark_and_page_numbers() {
    let h = harness();
    let a = h.path("doc.pdf");
    make_pdf(&a, 3, None);
    let out = h.run("pdf.watermark", json!({"input": [s(&a)], "text": "TOP SECRET", "pages": "2-3", "opacity": 0.5, "color": "#00ff00", "rotation": 0})).await.unwrap();
    let wm = out.artifacts[0].path.clone();
    let doc = Document::load(&wm).unwrap();
    assert_eq!(doc.get_pages().len(), 3);
    let ids: Vec<_> = doc.get_pages().into_values().collect();
    let content = |i: usize| String::from_utf8_lossy(&doc.get_page_content(ids[i])).to_string();
    assert!(!content(0).contains("TOP SECRET"));
    assert!(content(1).contains("(TOP SECRET) Tj"));
    assert!(content(1).contains("0 1 0 rg"));
    // Image watermark.
    let png = h.path("logo.png");
    png_with_alpha(&png);
    let out = h
        .run(
            "pdf.watermark",
            json!({"input": [s(&a)], "kind": "image", "image": s(&png), "scale": 30}),
        )
        .await
        .unwrap();
    assert_eq!(pages(&out.artifacts[0].path), 3);
    let err = h
        .run("pdf.watermark", json!({"input": [s(&a)], "text": "  "}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run("pdf.watermark", json!({"input": [s(&a)], "pages": "7"}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run(
            "pdf.page_numbers",
            json!({"input": [s(&a)], "format": "Page {n} of {total}", "skip_first": true}),
        )
        .await
        .unwrap();
    let numbered = out.artifacts[0].path.clone();
    let err = h
        .run(
            "pdf.page_numbers",
            json!({"input": [s(&a)], "format": "no placeholder"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    if h.available("pdf.extract_text") {
        let t = text_of(&h, &wm);
        assert!(
            t.contains("TOP SECRET"),
            "watermark should be real text: {t}"
        );
        let t = text_of(&h, &numbered);
        assert!(t.contains("Page 2 of 2"), "{t}");
        assert!(t.contains("Page 1 of 2"));
        assert!(!t.contains("of 3"));
        // Rendering still works after editing.
        let out = h
            .run("pdf.to_images", json!({"input": [s(&numbered)], "dpi": 20}))
            .await
            .unwrap();
        assert_eq!(out.artifacts.len(), 3);
    }
}

#[tokio::test]
async fn metadata() {
    let h = harness();
    let a = h.path("doc.pdf");
    make_pdf(&a, 1, None);
    let out = h
        .run(
            "pdf.metadata_edit",
            json!({"input": [s(&a)], "title": "Ünïcode title", "author": "Ada"}),
        )
        .await
        .unwrap();
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    let info = doc
        .trailer
        .get(b"Info")
        .and_then(|o| doc.dereference(o))
        .unwrap()
        .1
        .as_dict()
        .unwrap()
        .clone();
    assert_eq!(
        crate::common::decode_text(info.get(b"Title").unwrap().as_str().unwrap()),
        "Ünïcode title"
    );
    assert_eq!(out.data["metadata"]["Author"], "Ada");
    let out = h
        .run(
            "pdf.metadata_edit",
            json!({"input": [s(&a)], "clear_all": true}),
        )
        .await
        .unwrap();
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    assert!(doc.trailer.get(b"Info").is_err());

    // An indirect XMP stream must be gone from the file, not just unlinked.
    let x = h.path("xmp.pdf");
    let mut d = Document::load(&a).unwrap();
    let xmp = d.add_object(Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        b"<x:xmpmeta><dc:creator>SECRET-AUTHOR</dc:creator></x:xmpmeta>".to_vec(),
    ));
    d.catalog_mut()
        .unwrap()
        .set("Metadata", Object::Reference(xmp));
    d.save(&x).unwrap();
    let contains_marker = |path: &Path| {
        let doc = Document::load(path).unwrap();
        doc.objects.values().any(|o| match o {
            Object::Stream(st) => {
                let bytes = st
                    .decompressed_content()
                    .unwrap_or_else(|_| st.content.clone());
                bytes.windows(13).any(|w| w == b"SECRET-AUTHOR")
            }
            _ => false,
        })
    };
    assert!(contains_marker(&x));
    for params in [
        json!({"input": [s(&x)], "clear_all": true}),
        json!({"input": [s(&x)], "title": "New"}),
    ] {
        let out = h.run("pdf.metadata_edit", params).await.unwrap();
        assert!(!contains_marker(&out.artifacts[0].path));
    }
    // An indirect Info value must be gone once the field is removed or
    // replaced, not left behind as an orphaned object.
    let ind = h.path("indirect.pdf");
    let mut d = Document::load(&a).unwrap();
    let author = d.add_object(Object::string_literal("SECRET-NAME"));
    let info = d.add_object(dictionary! { "Author" => Object::Reference(author) });
    d.trailer.set("Info", Object::Reference(info));
    d.save(&ind).unwrap();
    let has_name = |path: &Path| {
        let doc = Document::load(path).unwrap();
        doc.objects.values().any(|o| match o {
            Object::String(b, _) => b.windows(11).any(|w| w == b"SECRET-NAME"),
            _ => false,
        })
    };
    assert!(has_name(&ind));
    for params in [
        json!({"input": [s(&ind)], "author": "-"}),
        json!({"input": [s(&ind)], "author": "Someone else"}),
    ] {
        let out = h.run("pdf.metadata_edit", params).await.unwrap();
        assert!(!has_name(&out.artifacts[0].path));
    }
    let err = h
        .run("pdf.metadata_edit", json!({"input": [s(&a)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn forms() {
    let h = harness();
    let f = h.path("form.pdf");
    make_form(&f);
    let out = h
        .run("pdf.form_fields", json!({"input": s(&f)}))
        .await
        .unwrap();
    assert_eq!(out.data["count"], 2);
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|r| r[0] == "agree" && r[1] == "checkbox" && r[3] == "Yes"));
    assert!(out.data["fill_template"].get("name").is_some());
    if !h.available("pdf.form_fill") {
        return;
    }
    let out = h
        .run(
            "pdf.form_fill",
            json!({"input": s(&f), "values": {"name": "Ada Lovelace", "agree": true}}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["filled"], 2);
    let doc = Document::load(&out.artifacts[0].path).unwrap();
    let fields = crate::forms::collect_fields(&doc);
    let name = fields.iter().find(|f| f.name == "name").unwrap();
    assert_eq!(name.value.as_deref(), Some("Ada Lovelace"));
    let agree = fields.iter().find(|f| f.name == "agree").unwrap();
    assert_eq!(agree.value.as_deref(), Some("Yes"));
    // Values passed as a JSON string also work; unknown fields are rejected.
    let err = h
        .run(
            "pdf.form_fill",
            json!({"input": s(&f), "values": "{\"nope\": 1}"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run(
            "pdf.form_fill",
            json!({"input": s(&f), "values": {"agree": "maybe"}}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    if h.available("pdf.extract_text") {
        let out = h
            .run(
                "pdf.form_fill",
                json!({"input": s(&f), "values": {"name": "Grace"}, "flatten": true}),
            )
            .await
            .unwrap();
        assert!(text_of(&h, &out.artifacts[0].path).contains("Grace"));
    }
}

#[tokio::test]
async fn ocr() {
    let h = harness();
    if !h.available("pdf.ocr") {
        return;
    }
    let a = h.path("scan.pdf");
    make_pdf(&a, 1, Some("HELLO WORLD"));
    // Rasterize first so there is no text layer, like a real scan.
    let imgs = h
        .run("pdf.to_images", json!({"input": [s(&a)], "dpi": 150}))
        .await
        .unwrap();
    let scan = h
        .run(
            "pdf.from_images",
            json!({"input": [s(&imgs.artifacts[0].path)], "dpi": 150}),
        )
        .await
        .unwrap();
    let scanned = scan.artifacts[0].path.clone();
    assert!(!text_of(&h, &scanned).contains("HELLO"));
    let out = h
        .run("pdf.ocr", json!({"input": [s(&scanned)], "output": "text"}))
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("HELLO"));
    let out = h
        .run("pdf.ocr", json!({"input": [s(&scanned)], "dpi": 200}))
        .await
        .unwrap();
    assert!(
        text_of(&h, &out.artifacts[0].path).contains("HELLO"),
        "searchable PDF"
    );
    let err = h
        .run(
            "pdf.ocr",
            json!({"input": [s(&scanned)], "languages": "eng;rm"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run(
            "pdf.ocr",
            json!({"input": [s(&scanned)], "languages": "zzz"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn markup_conversion() {
    let h = harness();
    if !h.available("pdf.convert_markup") {
        return;
    }
    let md = h.write("notes.md", b"# Title\n\nSome *text*.\n");
    let out = h
        .run(
            "pdf.convert_markup",
            json!({"input": [s(&md)], "to": "html"}),
        )
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("<h1"));
    assert!(out.artifacts[0].name.ends_with(".html"));
    let out = h
        .run(
            "pdf.convert_markup",
            json!({"text": "Hello **world**", "to": "docx"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].name.ends_with(".docx"));
    // Local images are not embedded (no file exfiltration through outputs).
    let secret_img = h.path("secret.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3]))
        .save(&secret_img)
        .unwrap();
    let md = format!("![alt text]({})\n", secret_img.display());
    let out = h
        .run("pdf.convert_markup", json!({"text": md, "to": "docx"}))
        .await
        .unwrap();
    let docx = std::fs::read(&out.artifacts[0].path).unwrap();
    assert!(
        !docx.windows(10).any(|w| w == b"word/media"),
        "image must not be embedded"
    );
    let out = h
        .run(
            "pdf.convert_markup",
            json!({"text": "# Book", "to": "epub"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].name.ends_with(".epub"));
    // --sandbox: documents cannot pull in other files.
    let secret = h.write("secret.txt", b"TOPSECRETVALUE");
    let tex = format!("\\input{{{}}}\n", secret.display());
    let out = h
        .run(
            "pdf.convert_markup",
            json!({"text": tex, "from": "latex", "to": "plain"}),
        )
        .await;
    if let Ok(o) = out {
        assert!(!std::fs::read_to_string(&o.artifacts[0].path)
            .unwrap()
            .contains("TOPSECRETVALUE"));
    }
    let err = h
        .run("pdf.convert_markup", json!({"to": "html"}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let unknown = h.write("x.weird", b"data");
    let err = h
        .run("pdf.convert_markup", json!({"input": [s(&unknown)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn office_conversion() {
    let h = harness();
    if !h.available("pdf.convert_document") {
        return;
    }
    let txt = h.write(
        "letter.txt",
        b"Dear reader,\nthis was converted by LibreOffice.\n",
    );
    let out = h
        .run(
            "pdf.convert_document",
            json!({"input": [s(&txt)], "format": "pdf"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].name.ends_with("letter.pdf"));
    assert!(std::fs::read(&out.artifacts[0].path)
        .unwrap()
        .starts_with(b"%PDF"));
    if h.available("pdf.markup_to_pdf") {
        let out = h
            .run(
                "pdf.markup_to_pdf",
                json!({"text": "# Heading\n\nBody text"}),
            )
            .await
            .unwrap();
        assert!(std::fs::read(&out.artifacts[0].path)
            .unwrap()
            .starts_with(b"%PDF"));
    }
}
