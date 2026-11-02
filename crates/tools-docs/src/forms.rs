//! AcroForm inspection and filling.

use crate::common::*;
use crate::structure::pdf_input;
use dogubako_core::prelude::*;
use lopdf::{Document, Object, ObjectId};
use std::collections::HashSet;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![form_fields(), form_fill()]
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: ObjectId,
    pub name: String,
    pub ft: String,
    pub flags: i64,
    pub value: Option<String>,
    pub options: Vec<String>,
    pub states: Vec<String>,
    pub widgets: Vec<ObjectId>,
    pub max_len: Option<i64>,
}

impl Field {
    pub fn kind(&self) -> &'static str {
        match self.ft.as_str() {
            "Tx" if self.flags & (1 << 12) != 0 => "text (multiline)",
            "Tx" => "text",
            "Btn" if self.flags & (1 << 16) != 0 => "pushbutton",
            "Btn" if self.flags & (1 << 15) != 0 => "radio",
            "Btn" => "checkbox",
            "Ch" if self.flags & (1 << 17) != 0 => "combo",
            "Ch" => "list",
            "Sig" => "signature",
            _ => "unknown",
        }
    }
    pub fn read_only(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn required(&self) -> bool {
        self.flags & 2 != 0
    }
}

fn acroform(doc: &Document) -> Option<&lopdf::Dictionary> {
    let cat = doc.catalog().ok()?;
    deref_dict(doc, cat.get(b"AcroForm").ok()?)
}

pub fn collect_fields(doc: &Document) -> Vec<Field> {
    let mut out = vec![];
    let Some(form) = acroform(doc) else {
        return out;
    };
    let Ok(fields) = form.get(b"Fields").map(|o| deref(doc, o)) else {
        return out;
    };
    let Ok(arr) = fields.as_array() else {
        return out;
    };
    let mut seen = HashSet::new();
    for f in arr {
        if let Object::Reference(id) = f {
            walk(doc, *id, "", None, 0, &mut out, &mut seen, 0);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn walk(
    doc: &Document,
    id: ObjectId,
    parent: &str,
    ft: Option<String>,
    ff: i64,
    out: &mut Vec<Field>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) {
    if depth > 32 || !seen.insert(id) || out.len() > 10_000 {
        return;
    }
    let Ok(d) = doc.get_dictionary(id) else {
        return;
    };
    let partial = d.get(b"T").ok().and_then(|o| obj_to_string(doc, o));
    let name = match (&partial, parent.is_empty()) {
        (Some(t), true) => t.clone(),
        (Some(t), false) => format!("{parent}.{t}"),
        (None, _) => parent.to_string(),
    };
    let ft = d.get(b"FT").ok().and_then(|o| obj_to_string(doc, o)).or(ft);
    let ff = d
        .get(b"Ff")
        .ok()
        .and_then(|o| deref(doc, o).as_i64().ok())
        .unwrap_or(ff);
    let kids: Vec<ObjectId> = d
        .get(b"Kids")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .map(|a| a.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    let field_kids: Vec<ObjectId> = kids
        .iter()
        .copied()
        .filter(|k| {
            doc.get_dictionary(*k)
                .map(|kd| kd.has(b"T"))
                .unwrap_or(false)
        })
        .collect();
    if !field_kids.is_empty() {
        for k in field_kids {
            walk(doc, k, &name, ft.clone(), ff, out, seen, depth + 1);
        }
        return;
    }
    let widgets = if kids.is_empty() { vec![id] } else { kids };
    let mut states = vec![];
    for w in &widgets {
        if let Some(n) = doc
            .get_dictionary(*w)
            .ok()
            .and_then(|wd| wd.get(b"AP").ok())
            .and_then(|ap| deref_dict(doc, ap))
            .and_then(|ap| ap.get(b"N").ok())
            .and_then(|n| deref_dict(doc, n))
        {
            for (k, _) in n.iter() {
                let s = String::from_utf8_lossy(k).to_string();
                if s != "Off" && !states.contains(&s) {
                    states.push(s);
                }
            }
        }
    }
    let options = d
        .get(b"Opt")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .map(|a| {
            a.iter()
                .filter_map(|o| match deref(doc, o) {
                    Object::Array(pair) => pair.first().and_then(|x| obj_to_string(doc, x)),
                    other => obj_to_string(doc, other),
                })
                .collect()
        })
        .unwrap_or_default();
    let value = d.get(b"V").ok().and_then(|v| match deref(doc, v) {
        Object::Array(a) => Some(
            a.iter()
                .filter_map(|x| obj_to_string(doc, x))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        other => obj_to_string(doc, other),
    });
    let max_len = d
        .get(b"MaxLen")
        .ok()
        .and_then(|o| deref(doc, o).as_i64().ok());
    out.push(Field {
        id,
        name,
        ft: ft.unwrap_or_default(),
        flags: ff,
        value,
        options,
        states,
        widgets,
        max_len,
    });
}

fn form_fields() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.form_fields", "List Form Fields", Category::Pdf)
        .describe("List the fillable form fields of a PDF with their names, types, current values and options")
        .keywords(&["form", "acroform", "fields", "fillable", "inspect", "pdf"])
        .param(pdf_input())
        .output(OutputKind::Data)
        .timeout(300);
    sync_tool(spec, |_ctx, p| {
        let f = p.file("input")?;
        let doc = load_pdf(&f.path, "input")?;
        let fields = collect_fields(&doc);
        let rows: Vec<Value> = fields
            .iter()
            .map(|fl| {
                let opts = if !fl.options.is_empty() {
                    fl.options.join(" | ")
                } else {
                    fl.states.join(" | ")
                };
                json!([
                    fl.name,
                    fl.kind(),
                    fl.value.clone().unwrap_or_default(),
                    opts,
                    fl.required(),
                    fl.read_only()
                ])
            })
            .collect();
        let template: serde_json::Map<String, Value> = fields
            .iter()
            .filter(|f| !matches!(f.kind(), "pushbutton" | "signature") && !f.read_only())
            .map(|f| {
                let v = match f.kind() {
                    "checkbox" => {
                        Value::Bool(f.value.as_deref().map(|v| v != "Off").unwrap_or(false))
                    }
                    _ => Value::String(f.value.clone().unwrap_or_default()),
                };
                (f.name.clone(), v)
            })
            .collect();
        let mut out = ToolOutput::data(json!({
            "count": fields.len(),
            "table": {"columns": ["Name", "Type", "Value", "Options", "Required", "Read-only"], "rows": rows},
            "fill_template": template,
        }));
        if fields.is_empty() {
            out = out.warn("This PDF has no fillable (AcroForm) fields");
        }
        Ok(out)
    })
}

fn truthy(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => Some(n.as_f64().unwrap_or(0.0) != 0.0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" | "1" | "x" | "checked" => Some(true),
            "false" | "no" | "off" | "0" | "" | "unchecked" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Null => Some(String::new()),
        _ => None,
    }
}

/// Apply `values` to the document's form; returns (filled, warnings).
pub fn fill(
    doc: &mut Document,
    values: &serde_json::Map<String, Value>,
) -> ToolResult<(usize, Vec<String>)> {
    let fields = collect_fields(doc);
    if fields.is_empty() {
        return Err(ToolError::invalid(
            "input",
            "this PDF has no fillable form fields",
        ));
    }
    let mut warnings = vec![];
    let mut filled = 0;
    let mut unknown = vec![];
    for (key, val) in values {
        let field = fields.iter().find(|f| f.name == *key).or_else(|| {
            let m: Vec<&Field> = fields
                .iter()
                .filter(|f| f.name.rsplit('.').next() == Some(key.as_str()))
                .collect();
            if m.len() == 1 {
                Some(m[0])
            } else {
                None
            }
        });
        let Some(field) = field else {
            unknown.push(key.clone());
            continue;
        };
        if field.read_only() {
            warnings.push(format!("'{key}' is read-only; skipped"));
            continue;
        }
        let bad = |msg: String| ToolError::invalid("values", format!("'{key}': {msg}"));
        match field.kind() {
            "checkbox" | "radio" => {
                let on_state = if field.kind() == "checkbox" {
                    let t = match truthy(val) {
                        Some(t) => t,
                        None => {
                            let s = value_text(val).unwrap_or_default();
                            if field.states.contains(&s) {
                                true
                            } else {
                                return Err(bad(format!(
                                    "expected true/false or one of: {}",
                                    field.states.join(", ")
                                )));
                            }
                        }
                    };
                    let s = value_text(val).filter(|s| field.states.contains(s));
                    if t {
                        Some(
                            s.or_else(|| field.states.first().cloned())
                                .unwrap_or_else(|| "Yes".into()),
                        )
                    } else {
                        None
                    }
                } else {
                    let s = value_text(val).unwrap_or_default();
                    if s.is_empty() || truthy(val) == Some(false) && !field.states.contains(&s) {
                        None
                    } else if field.states.contains(&s) {
                        Some(s)
                    } else {
                        return Err(bad(format!(
                            "'{s}' is not an option; choose one of: {}",
                            field.states.join(", ")
                        )));
                    }
                };
                let name = on_state.clone().unwrap_or_else(|| "Off".into());
                doc.get_dictionary_mut(field.id)
                    .ctx("field")?
                    .set("V", Object::Name(name.clone().into_bytes()));
                for w in &field.widgets {
                    let has = doc
                        .get_dictionary(*w)
                        .ok()
                        .and_then(|wd| wd.get(b"AP").ok())
                        .and_then(|ap| deref_dict(doc, ap))
                        .and_then(|ap| ap.get(b"N").ok())
                        .and_then(|n| deref_dict(doc, n))
                        .map(|n| n.has(name.as_bytes()))
                        .unwrap_or(false);
                    let st = if on_state.is_some() && has {
                        name.clone()
                    } else {
                        "Off".into()
                    };
                    if let Ok(wd) = doc.get_dictionary_mut(*w) {
                        wd.set("AS", Object::Name(st.into_bytes()));
                    }
                }
            }
            "text" | "text (multiline)" | "combo" | "list" => {
                let v: Object = match val {
                    Value::Array(items) if field.kind() == "list" => {
                        let strs: Vec<String> = items.iter().filter_map(value_text).collect();
                        for s in &strs {
                            if !field.options.is_empty() && !field.options.contains(s) {
                                return Err(bad(format!(
                                    "'{s}' is not an option; choose from: {}",
                                    field.options.join(", ")
                                )));
                            }
                        }
                        Object::Array(strs.iter().map(|s| text_string(s)).collect())
                    }
                    other => {
                        let mut s =
                            value_text(other).ok_or_else(|| bad("expected a text value".into()))?;
                        let editable = field.flags & (1 << 18) != 0;
                        if matches!(field.kind(), "combo" | "list")
                            && !editable
                            && !field.options.is_empty()
                            && !s.is_empty()
                            && !field.options.contains(&s)
                        {
                            return Err(bad(format!(
                                "'{s}' is not an option; choose from: {}",
                                field.options.join(", ")
                            )));
                        }
                        if let Some(m) = field.max_len.filter(|m| *m > 0) {
                            if s.chars().count() > m as usize {
                                s = s.chars().take(m as usize).collect();
                                warnings.push(format!(
                                    "'{key}' was truncated to its maximum length of {m}"
                                ));
                            }
                        }
                        text_string(&s)
                    }
                };
                doc.get_dictionary_mut(field.id).ctx("field")?.set("V", v);
                // Drop stale appearances so they are regenerated.
                for w in &field.widgets {
                    if let Ok(wd) = doc.get_dictionary_mut(*w) {
                        wd.remove(b"AP");
                    }
                }
            }
            other => return Err(bad(format!("{other} fields cannot be filled"))),
        }
        filled += 1;
    }
    if !unknown.is_empty() {
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).take(50).collect();
        return Err(ToolError::invalid(
            "values",
            format!(
                "unknown field(s): {} (available: {})",
                unknown.join(", "),
                names.join(", ")
            ),
        ));
    }
    // NeedAppearances so viewers (and qpdf) regenerate appearance streams.
    let form_ref = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .cloned();
    match form_ref {
        Some(Object::Reference(id)) => {
            doc.get_dictionary_mut(id)
                .ctx("AcroForm")?
                .set("NeedAppearances", Object::Boolean(true));
        }
        Some(Object::Dictionary(_)) => {
            if let Ok(Object::Dictionary(d)) =
                doc.catalog_mut().ctx("catalog")?.get_mut(b"AcroForm")
            {
                d.set("NeedAppearances", Object::Boolean(true));
            }
        }
        _ => {}
    }
    Ok((filled, warnings))
}

fn form_fill() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.form_fill", "Fill PDF Form", Category::Pdf)
        .describe("Fill text fields, checkboxes, radio buttons and choice fields of a PDF form from a JSON mapping of field names to values")
        .keywords(&["form", "fill", "acroform", "fields", "fillable", "pdf"])
        .param(pdf_input())
        .param(
            ParamSpec::json("values")
                .label("Field values")
                .describe("JSON object mapping field names to values, e.g. {\"name\": \"Ada\", \"agree\": true}. Get the names with pdf.form_fields.")
                .required(),
        )
        .param(ParamSpec::boolean("flatten").label("Flatten after filling").describe("Make the filled values part of the page so they can no longer be edited"))
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .timeout(600);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let values = p
            .json("values")
            .and_then(|v| v.as_object())
            .cloned()
            .ok_or_else(|| {
                ToolError::invalid("values", "expected a JSON object of field names to values")
            })?;
        if values.is_empty() {
            return Err(ToolError::invalid("values", "no field values given"));
        }
        let tmp = ctx.temp_path("filled.pdf");
        let tmp2 = tmp.clone();
        let (filled, warnings) = ctx
            .blocking(move || {
                let mut doc = load_pdf(&f.path, "input")?;
                let r = fill(&mut doc, &values)?;
                save_pdf(&mut doc, &tmp2)?;
                Ok(r)
            })
            .await?;
        let input = p.file("input")?;
        let out = ctx.output_path(&suffixed(&input.name, "filled", Some("pdf")));
        let res = qpdf(&ctx)?
            .arg("--generate-appearances")
            .arg_if(p.bool("flatten"), "--flatten-annotations=all")
            .arg(&tmp)
            .arg(&out)
            .run()
            .await?;
        let mut o = ToolOutput::files(vec![ctx.artifact(&out)?])
            .with_data(json!({"filled": filled, "flattened": p.bool("flatten")}));
        o.warnings = warnings;
        o.warnings.extend(qpdf_warnings(&res));
        Ok(o)
    })
}
