//! JSON Schema generation from tool specs (for MCP, OpenAPI and clients).

use crate::spec::{ParamKind, ParamSpec, PathKind, ToolSpec};
use serde_json::{json, Map, Value};

/// JSON Schema (draft 2020-12 compatible) for a tool's input object.
///
/// `file_mode` describes how file parameters are referenced:
/// "path" for local paths, "id" for stored file ids.
pub fn input_schema(spec: &ToolSpec, file_mode: &str) -> Value {
    let mut props = Map::new();
    let mut required = vec![];
    for p in &spec.params {
        props.insert(p.name.clone(), param_schema(p, file_mode));
        if p.required && p.default.is_none() {
            required.push(Value::String(p.name.clone()));
        }
    }
    let mut s = json!({
        "type": "object",
        "properties": Value::Object(props),
        "additionalProperties": false,
    });
    if !required.is_empty() {
        s["required"] = Value::Array(required);
    }
    s
}

pub fn param_schema(p: &ParamSpec, file_mode: &str) -> Value {
    let mut s = match &p.kind {
        ParamKind::Text { max_len, .. } => {
            let mut s = json!({"type": "string"});
            if let Some(m) = max_len {
                s["maxLength"] = json!(m);
            }
            s
        }
        ParamKind::Secret => json!({"type": "string", "writeOnly": true}),
        ParamKind::Integer { min, max } => {
            let mut s = json!({"type": "integer"});
            if let Some(m) = min {
                s["minimum"] = json!(m);
            }
            if let Some(m) = max {
                s["maximum"] = json!(m);
            }
            s
        }
        ParamKind::Number { min, max, .. } => {
            let mut s = json!({"type": "number"});
            if let Some(m) = min {
                s["minimum"] = json!(m);
            }
            if let Some(m) = max {
                s["maximum"] = json!(m);
            }
            s
        }
        ParamKind::Boolean => json!({"type": "boolean"}),
        ParamKind::Enum { options, multi } => {
            let vals: Vec<Value> = options.iter().map(|o| json!(o.value)).collect();
            if *multi {
                json!({"type": "array", "items": {"type": "string", "enum": vals}, "uniqueItems": true})
            } else {
                json!({"type": "string", "enum": vals})
            }
        }
        ParamKind::Color => {
            json!({"type": "string", "pattern": "^#?([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"})
        }
        ParamKind::File { multiple, accept } => {
            let what = if file_mode == "path" {
                "Local file path"
            } else {
                "File id returned by the upload endpoint or a previous job (e.g. 'f_...')"
            };
            let item = json!({"type": "string", "description": what});
            let mut s = if *multiple {
                json!({"type": "array", "items": item, "minItems": 1})
            } else {
                item
            };
            if !accept.is_empty() {
                s["x-accept"] = json!(accept);
            }
            s
        }
        ParamKind::Path { kind } => {
            let k = match kind {
                PathKind::Dir => "directory",
                PathKind::File => "file",
                PathKind::Any => "file or directory",
            };
            json!({"type": "string", "description": format!("Host {k} path (must be inside allowed folders)")})
        }
        ParamKind::Json { schema } => schema.clone().unwrap_or_else(|| json!({})),
    };
    let mut desc = p.label.clone();
    if !p.description.is_empty() {
        desc = format!("{}. {}", p.label, p.description);
    }
    if let Some(existing) = s.get("description").and_then(|d| d.as_str()) {
        desc = format!("{desc} ({existing})");
    }
    s["description"] = json!(desc);
    if let Some(d) = &p.default {
        if !p.is_file() {
            s["default"] = d.clone();
        }
    }
    s
}

/// Schema for the common tool result envelope.
pub fn output_schema(spec: &ToolSpec) -> Value {
    json!({
        "type": "object",
        "properties": {
            "text": {"type": "string", "description": "Primary text result"},
            "text_mime": {"type": "string"},
            "data": spec.output.data_schema.clone().unwrap_or_else(|| json!({"description": "Structured result"})),
            "artifacts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "name": {"type": "string"},
                        "mime": {"type": "string"},
                        "size": {"type": "integer"},
                        "url": {"type": "string"},
                        "path": {"type": "string"}
                    }
                }
            },
            "warnings": {"type": "array", "items": {"type": "string"}}
        }
    })
}
