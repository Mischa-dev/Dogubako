//! Document metadata editing (Info dictionary + XMP).

use crate::common::*;
use crate::structure::pdf_inputs;
use dogubako_core::prelude::*;
use lopdf::{Dictionary, Object};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![metadata_edit()]
}

const FIELDS: &[(&str, &str)] = &[
    ("title", "Title"),
    ("author", "Author"),
    ("subject", "Subject"),
    ("keywords", "Keywords"),
    ("creator", "Creator"),
    ("producer", "Producer"),
];

fn metadata_edit() -> Arc<dyn Tool> {
    let mut spec = ToolSpec::new("pdf.metadata_edit", "Edit PDF Metadata", Category::Pdf)
        .describe("Set or clear document properties (title, author, subject, keywords, creator, producer); can strip all metadata including XMP")
        .keywords(&["metadata", "properties", "title", "author", "privacy", "strip", "xmp", "pdf"])
        .param(pdf_inputs())
        .param(ParamSpec::boolean("clear_all").label("Remove all metadata").describe("Delete the document information dictionary and XMP metadata before applying any new values"));
    for (k, label) in FIELDS {
        spec = spec.param(
            ParamSpec::text(k)
                .label(label)
                .describe("Leave empty to keep the current value; use a single '-' to remove it"),
        );
    }
    let spec = spec.output(OutputKind::Files).progress().timeout(900);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let clear = p.bool("clear_all");
        let changes: Vec<(&str, &str, Option<String>)> = FIELDS
            .iter()
            .filter_map(|(k, key)| {
                let v = p.text(k);
                if v.is_empty() {
                    None
                } else if v.trim() == "-" {
                    Some((*k, *key, None))
                } else {
                    Some((*k, *key, Some(v.to_string())))
                }
            })
            .collect();
        if !clear && changes.is_empty() {
            return Err(ToolError::input(
                "nothing to do: enter at least one value or enable 'Remove all metadata'",
            ));
        }
        let mut arts = vec![];
        let mut warnings = vec![];
        let mut result = json!({});
        for (i, file) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(ctx, i, files.len(), &file.name);
            let mut doc = load_pdf(&file.path, "input")?;
            let had_xmp = doc.catalog().map(|c| c.has(b"Metadata")).unwrap_or(false);
            if clear {
                if let Ok(Object::Reference(id)) = doc.trailer.get(b"Info") {
                    let id = *id;
                    doc.objects.remove(&id);
                }
                doc.trailer.remove(b"Info");
                if let Ok(cat) = doc.catalog_mut() {
                    cat.remove(b"Metadata");
                    cat.remove(b"PieceInfo");
                }
            } else if had_xmp {
                // XMP would contradict the edited Info values in most readers.
                if let Ok(cat) = doc.catalog_mut() {
                    cat.remove(b"Metadata");
                }
                warnings.push(format!(
                    "{}: removed the XMP metadata stream so it cannot contradict the new values",
                    file.name
                ));
            }
            if !changes.is_empty() {
                let info_id = match doc.trailer.get(b"Info") {
                    Ok(Object::Reference(id)) if doc.get_dictionary(*id).is_ok() => *id,
                    Ok(Object::Dictionary(d)) => {
                        let d = d.clone();
                        let id = doc.add_object(d);
                        doc.trailer.set("Info", Object::Reference(id));
                        id
                    }
                    _ => {
                        let id = doc.add_object(Dictionary::new());
                        doc.trailer.set("Info", Object::Reference(id));
                        id
                    }
                };
                let info = doc.get_dictionary_mut(info_id).ctx("info dictionary")?;
                for (_, key, val) in &changes {
                    match val {
                        Some(v) => info.set(key.as_bytes().to_vec(), text_string(v)),
                        None => {
                            info.remove(key.as_bytes());
                        }
                    }
                }
                info.set("ModDate", text_string(&pdf_date_now()));
            }
            // Removing references is not enough: old Info/XMP objects and
            // indirect values of removed or replaced fields would still be
            // written (recoverable). Drop everything no longer reachable.
            doc.prune_objects();
            // Report the resulting metadata.
            let mut now = serde_json::Map::new();
            if let Some(info) = doc
                .trailer
                .get(b"Info")
                .ok()
                .and_then(|o| deref_dict(&doc, o).cloned())
            {
                for (k, v) in info.iter() {
                    if let Some(s) = obj_to_string(&doc, v) {
                        now.insert(String::from_utf8_lossy(k).to_string(), Value::String(s));
                    }
                }
            }
            result = Value::Object(now);
            let out = ctx.output_path(&suffixed(
                &file.name,
                if clear && changes.is_empty() {
                    "clean"
                } else {
                    "edited"
                },
                Some("pdf"),
            ));
            save_pdf(&mut doc, &out)?;
            arts.push(ctx.artifact(&out)?);
        }
        let mut o = ToolOutput::files(arts).with_data(json!({"metadata": result}));
        o.warnings = warnings;
        Ok(o)
    })
}
