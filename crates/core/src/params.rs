//! Parameter validation, coercion and typed access.

use crate::error::{ToolError, ToolResult};
use crate::policy::FsPolicy;
use crate::spec::{ParamKind, ParamSpec, PathKind, ToolSpec};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A resolved input file.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InputFile {
    pub path: PathBuf,
    /// Original, user-facing file name (used to derive output names).
    pub name: String,
}

impl InputFile {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "input".into());
        InputFile { path, name }
    }
    /// File stem of the original name, e.g. `photo` for `photo.jpg`.
    pub fn stem(&self) -> String {
        Path::new(&self.name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".into())
    }
    /// Lower-case extension of the original name, without dot.
    pub fn extension(&self) -> String {
        Path::new(&self.name)
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }
    pub fn size(&self) -> u64 {
        std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0)
    }
}

/// Resolves file parameter values (local paths, stored file ids...) into files.
pub trait FileResolver: Send + Sync {
    fn resolve(&self, param: &str, value: &str) -> ToolResult<InputFile>;
}

/// Resolves values as local filesystem paths, optionally restricted to roots.
pub struct LocalFileResolver {
    pub policy: FsPolicy,
}

impl LocalFileResolver {
    pub fn unrestricted() -> Self {
        LocalFileResolver {
            policy: FsPolicy::unrestricted(),
        }
    }
}

impl FileResolver for LocalFileResolver {
    fn resolve(&self, param: &str, value: &str) -> ToolResult<InputFile> {
        let path = self.policy.check(param, Path::new(value))?;
        if !path.is_file() {
            return Err(ToolError::invalid(
                param,
                format!("'{value}' is not a readable file"),
            ));
        }
        let mut f = InputFile::from_path(&path);
        // Keep the name the user typed, not the canonical target of a symlink.
        if let Some(n) = Path::new(value).file_name() {
            f.name = n.to_string_lossy().to_string();
        }
        Ok(f)
    }
}

/// Validated, defaulted parameters plus resolved files.
#[derive(Debug, Clone, Default)]
pub struct Params {
    values: Map<String, Value>,
    files: HashMap<String, Vec<InputFile>>,
}

impl Params {
    /// Construct directly (mostly for tests).
    pub fn from_values(values: Map<String, Value>) -> Self {
        Params {
            values,
            files: HashMap::new(),
        }
    }

    pub fn raw(&self) -> &Map<String, Value> {
        &self.values
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name).filter(|v| !v.is_null())
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn str(&self, name: &str) -> ToolResult<&str> {
        self.opt_str(name)
            .ok_or_else(|| ToolError::invalid(name, "is required"))
    }

    /// Returns the string, treating empty strings as absent.
    pub fn opt_str(&self, name: &str) -> Option<&str> {
        self.get(name)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
    }

    /// String value, possibly empty (for text inputs where empty is meaningful).
    pub fn text(&self, name: &str) -> &str {
        self.get(name).and_then(|v| v.as_str()).unwrap_or("")
    }

    pub fn str_or<'a>(&'a self, name: &str, default: &'a str) -> &'a str {
        self.opt_str(name).unwrap_or(default)
    }

    pub fn int(&self, name: &str) -> ToolResult<i64> {
        self.opt_int(name)
            .ok_or_else(|| ToolError::invalid(name, "is required"))
    }

    pub fn opt_int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(|v| v.as_i64())
    }

    pub fn int_or(&self, name: &str, d: i64) -> i64 {
        self.opt_int(name).unwrap_or(d)
    }

    pub fn num(&self, name: &str) -> ToolResult<f64> {
        self.opt_num(name)
            .ok_or_else(|| ToolError::invalid(name, "is required"))
    }

    pub fn opt_num(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(|v| v.as_f64())
    }

    pub fn num_or(&self, name: &str, d: f64) -> f64 {
        self.opt_num(name).unwrap_or(d)
    }

    pub fn bool(&self, name: &str) -> bool {
        self.get(name).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    /// Values of a multi-choice enum.
    pub fn list(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => vec![],
        }
    }

    pub fn json(&self, name: &str) -> Option<&Value> {
        self.get(name)
    }

    /// First file of a file parameter.
    pub fn file(&self, name: &str) -> ToolResult<&InputFile> {
        self.files
            .get(name)
            .and_then(|v| v.first())
            .ok_or_else(|| ToolError::invalid(name, "a file is required"))
    }

    pub fn opt_file(&self, name: &str) -> Option<&InputFile> {
        self.files.get(name).and_then(|v| v.first())
    }

    pub fn files(&self, name: &str) -> &[InputFile] {
        self.files.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Host filesystem path parameter (already policy-checked).
    pub fn path(&self, name: &str) -> ToolResult<PathBuf> {
        self.opt_str(name)
            .map(PathBuf::from)
            .ok_or_else(|| ToolError::invalid(name, "a path is required"))
    }

    pub fn opt_path(&self, name: &str) -> Option<PathBuf> {
        self.opt_str(name).map(PathBuf::from)
    }

    /// All resolved input files across parameters.
    pub fn all_files(&self) -> impl Iterator<Item = &InputFile> {
        self.files.values().flatten()
    }

    /// Value map with secrets redacted, for logs and job records.
    pub fn redacted(&self, spec: &ToolSpec) -> Value {
        let mut m = self.values.clone();
        for p in &spec.params {
            if p.is_secret() && m.contains_key(&p.name) {
                m.insert(p.name.clone(), Value::String("••••••".into()));
            }
        }
        Value::Object(m)
    }
}

fn coerce_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" | "" => Some(false),
            _ => None,
        },
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        _ => None,
    }
}

/// Normalize a color string to `#rrggbb` or `#rrggbbaa`.
pub fn normalize_hex_color(s: &str) -> Option<String> {
    let h = s.trim().trim_start_matches('#');
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let expanded = match h.len() {
        3 | 4 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 | 8 => h.to_string(),
        _ => return None,
    };
    Some(format!("#{}", expanded.to_lowercase()))
}

/// Maximum accepted size of a single text parameter (bytes).
pub const DEFAULT_MAX_TEXT: usize = 32 * 1024 * 1024;

/// Validate raw input values against the spec, apply defaults, coerce
/// string-typed inputs (from CLIs / loosely typed clients) and resolve files.
pub fn validate(
    spec: &ToolSpec,
    raw: &Map<String, Value>,
    resolver: &dyn FileResolver,
    fs_policy: &FsPolicy,
) -> ToolResult<Params> {
    for key in raw.keys() {
        if spec.param_spec(key).is_none() {
            let known: Vec<&str> = spec.params.iter().map(|p| p.name.as_str()).collect();
            return Err(ToolError::invalid(
                key,
                format!("unknown parameter (expected one of: {})", known.join(", ")),
            ));
        }
    }
    let mut out = Params::default();
    for p in &spec.params {
        let v = raw
            .get(&p.name)
            .filter(|v| !v.is_null() && !(v.is_string() && v.as_str() == Some("") && !is_text(p)));
        let v = match v {
            Some(v) => v.clone(),
            None => match &p.default {
                Some(d) => d.clone(),
                None => {
                    if p.required {
                        return Err(ToolError::invalid(&p.name, "is required"));
                    }
                    continue;
                }
            },
        };
        match &p.kind {
            ParamKind::File { multiple, .. } => {
                let ids: Vec<String> = match &v {
                    Value::String(s) => vec![s.clone()],
                    Value::Array(a) => a
                        .iter()
                        .map(|x| {
                            x.as_str().map(|s| s.to_string()).ok_or_else(|| {
                                ToolError::invalid(&p.name, "expected file references")
                            })
                        })
                        .collect::<ToolResult<_>>()?,
                    _ => {
                        return Err(ToolError::invalid(
                            &p.name,
                            "expected a file reference or list of file references",
                        ))
                    }
                };
                if ids.is_empty() {
                    if p.required {
                        return Err(ToolError::invalid(&p.name, "at least one file is required"));
                    }
                    continue;
                }
                if !multiple && ids.len() > 1 {
                    return Err(ToolError::invalid(&p.name, "only one file is accepted"));
                }
                let mut files = Vec::with_capacity(ids.len());
                for id in &ids {
                    let f = resolver.resolve(&p.name, id)?;
                    check_accept(p, &f)?;
                    files.push(f);
                }
                out.files.insert(p.name.clone(), files);
                out.values.insert(
                    p.name.clone(),
                    Value::Array(ids.into_iter().map(Value::String).collect()),
                );
            }
            _ => {
                let cv = coerce(p, v, fs_policy)?;
                out.values.insert(p.name.clone(), cv);
            }
        }
    }
    Ok(out)
}

fn is_text(p: &ParamSpec) -> bool {
    matches!(p.kind, ParamKind::Text { .. } | ParamKind::Secret)
}

fn check_accept(p: &ParamSpec, f: &InputFile) -> ToolResult<()> {
    let ParamKind::File { accept, .. } = &p.kind else {
        return Ok(());
    };
    if accept.is_empty() {
        return Ok(());
    }
    let ext = f.extension();
    let mime = mime_guess::from_ext(&ext).first_raw().unwrap_or("");
    let sniffed = crate::util::sniff_mime(&f.path);
    let ok = accept.iter().any(|a| {
        let a = a.trim().to_ascii_lowercase();
        if let Some(e) = a.strip_prefix('.') {
            return e == ext;
        }
        let matches_mime = |m: &str| {
            if let Some(prefix) = a.strip_suffix("/*") {
                m.starts_with(&format!("{prefix}/"))
            } else {
                m == a
            }
        };
        a == "*/*" || matches_mime(mime) || sniffed.as_deref().map(matches_mime).unwrap_or(false)
    });
    if ok {
        Ok(())
    } else {
        Err(ToolError::invalid(
            &p.name,
            format!(
                "'{}' is not an accepted file type (accepts: {})",
                f.name,
                accept.join(", ")
            ),
        ))
    }
}

fn coerce(p: &ParamSpec, v: Value, fs_policy: &FsPolicy) -> ToolResult<Value> {
    let name = &p.name;
    match &p.kind {
        ParamKind::Text { max_len, .. } => {
            let s = match v {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                other => {
                    return Err(ToolError::invalid(
                        name,
                        format!("expected text, got {}", type_name(&other)),
                    ))
                }
            };
            let limit = max_len.unwrap_or(DEFAULT_MAX_TEXT);
            if s.len() > limit {
                return Err(ToolError::invalid(
                    name,
                    format!("text is too long ({} bytes, max {limit})", s.len()),
                ));
            }
            Ok(Value::String(s))
        }
        ParamKind::Secret => match v {
            Value::String(s) if s.len() <= 4096 => Ok(Value::String(s)),
            _ => Err(ToolError::invalid(
                name,
                "expected a string of at most 4096 bytes",
            )),
        },
        ParamKind::Integer { min, max } => {
            let i = match &v {
                Value::Number(n) => n
                    .as_i64()
                    // Whole floats are accepted only inside the i64 range
                    // (-2^63 <= f < 2^63); `as` would silently saturate.
                    .or_else(|| {
                        n.as_f64()
                            .filter(|f| {
                                f.fract() == 0.0
                                    && *f >= -9_223_372_036_854_775_808.0
                                    && *f < 9_223_372_036_854_775_808.0
                            })
                            .map(|f| f as i64)
                    }),
                Value::String(s) => s.trim().parse::<i64>().ok(),
                _ => None,
            }
            .ok_or_else(|| ToolError::invalid(name, format!("expected an integer, got {v}")))?;
            if let Some(m) = min {
                if i < *m {
                    return Err(ToolError::invalid(name, format!("must be at least {m}")));
                }
            }
            if let Some(m) = max {
                if i > *m {
                    return Err(ToolError::invalid(name, format!("must be at most {m}")));
                }
            }
            Ok(Value::from(i))
        }
        ParamKind::Number { min, max, .. } => {
            let f = match &v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => s.trim().parse::<f64>().ok(),
                _ => None,
            }
            .filter(|f| f.is_finite())
            .ok_or_else(|| ToolError::invalid(name, format!("expected a number, got {v}")))?;
            if let Some(m) = min {
                if f < *m {
                    return Err(ToolError::invalid(name, format!("must be at least {m}")));
                }
            }
            if let Some(m) = max {
                if f > *m {
                    return Err(ToolError::invalid(name, format!("must be at most {m}")));
                }
            }
            Ok(Value::from(f))
        }
        ParamKind::Boolean => coerce_bool(&v)
            .map(Value::Bool)
            .ok_or_else(|| ToolError::invalid(name, "expected true or false")),
        ParamKind::Enum { options, multi } => {
            let check = |s: &str| -> ToolResult<String> {
                options
                    .iter()
                    .find(|o| o.value == s)
                    .or_else(|| options.iter().find(|o| o.value.eq_ignore_ascii_case(s)))
                    .map(|o| o.value.clone())
                    .ok_or_else(|| {
                        let allowed: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
                        ToolError::invalid(
                            name,
                            format!("'{s}' is not one of: {}", allowed.join(", ")),
                        )
                    })
            };
            if *multi {
                let items: Vec<String> = match v {
                    Value::Array(a) => a
                        .iter()
                        .map(|x| x.as_str().unwrap_or_default().to_string())
                        .collect(),
                    Value::String(s) => s
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect(),
                    _ => return Err(ToolError::invalid(name, "expected a list of options")),
                };
                let vals = items
                    .iter()
                    .map(|s| check(s).map(Value::String))
                    .collect::<ToolResult<Vec<_>>>()?;
                Ok(Value::Array(vals))
            } else {
                let s = match &v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => return Err(ToolError::invalid(name, "expected one option")),
                };
                Ok(Value::String(check(&s)?))
            }
        }
        ParamKind::Color => {
            let s = v
                .as_str()
                .ok_or_else(|| ToolError::invalid(name, "expected a color string"))?;
            normalize_hex_color(s).map(Value::String).ok_or_else(|| {
                ToolError::invalid(name, format!("'{s}' is not a hex color like #3366ff"))
            })
        }
        ParamKind::Json { .. } => match v {
            Value::String(s) => serde_json::from_str(&s)
                .map_err(|e| ToolError::invalid(name, format!("invalid JSON: {e}"))),
            other => Ok(other),
        },
        ParamKind::Path { kind } => {
            let s = v
                .as_str()
                .ok_or_else(|| ToolError::invalid(name, "expected a path"))?;
            let canon = fs_policy.check(name, Path::new(s))?;
            match kind {
                PathKind::Dir if !canon.is_dir() => {
                    return Err(ToolError::invalid(
                        name,
                        format!("'{s}' is not a directory"),
                    ))
                }
                PathKind::File if !canon.is_file() => {
                    return Err(ToolError::invalid(name, format!("'{s}' is not a file")))
                }
                _ => {}
            }
            Ok(Value::String(canon.to_string_lossy().to_string()))
        }
        ParamKind::File { .. } => unreachable!(),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
