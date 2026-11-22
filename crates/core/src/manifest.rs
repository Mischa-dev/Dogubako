//! Declarative extension manifests: wrap a command-line program as a tool
//! without writing Rust.
//!
//! ```toml
//! [tool]
//! id = "image.cwebp"
//! name = "Encode WebP (cwebp)"
//! category = "image"
//! description = "Encode images to WebP with Google's cwebp"
//! version = "1.0.0"
//!
//! [command]
//! program = "cwebp"
//! args = ["-q", "{quality}", "{input}", "-o", "{output}",
//!         { when = "lossless", args = ["-lossless"] }]
//!
//! [output]
//! file = "{input.stem}.webp"
//!
//! [[params]]
//! name = "input"
//! type = "file"
//! accept = ["image/*"]
//! required = true
//!
//! [[params]]
//! name = "quality"
//! type = "integer"
//! min = 0
//! max = 100
//! default = 80
//! ```
//!
//! Security model:
//! * Every placeholder expands to exactly one argv element; no shell is used.
//! * Shell interpreters are refused as programs.
//! * Manifests are only loaded when their SHA-256 is listed in the trust
//!   store (`trusted.toml`), so dropping a file into the folder is not enough.
//! * Declared permissions are enforced by the engine like builtin tools.
//!
//! `requires = ["cups"]` in `[tool]` names dependencies from
//! [`crate::deps::KNOWN_DEPS`], checked like a Rust tool's `.requires(..)`. A
//! `program` that is one of their ids or executables is found through the
//! dependency registry (managed engines, `DOGUBAKO_<ID>_PATH`); another bare
//! program beside a required dependency's binary is taken from that folder.

use crate::deps::dep_spec;
use crate::error::{ToolError, ToolResult};
use crate::params::Params;
use crate::spec::{
    Category, Execution, OutputKind, ParamKind, ParamSpec, Permission, Platform, ToolSpec,
};
use crate::tool::{Tool, ToolContext, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FORBIDDEN_PROGRAMS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "cmd",
    "cmd.exe",
    "powershell",
    "powershell.exe",
    "pwsh",
    "env",
    "sudo",
    "doas",
    "su",
];

#[derive(Debug, Deserialize)]
struct ToolSection {
    id: String,
    name: String,
    category: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_version")]
    version: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    platforms: Vec<Platform>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    /// Dependency ids from [`crate::deps::KNOWN_DEPS`].
    #[serde(default)]
    requires: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".into()
}
fn default_timeout() -> u64 {
    300
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ArgItem {
    Plain(String),
    Group { when: String, args: Vec<String> },
}

#[derive(Debug, Deserialize)]
struct CommandSection {
    program: String,
    args: Vec<ArgItem>,
}

#[derive(Debug, Deserialize, Default)]
struct OutputSection {
    /// Output file name template; `{output}` in args expands to its path.
    file: Option<String>,
    /// Capture stdout as text with this MIME type.
    stdout: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    tool: ToolSection,
    command: CommandSection,
    #[serde(default)]
    output: OutputSection,
    #[serde(default)]
    params: Vec<ParamSpec>,
}

pub struct ExternalTool {
    spec: ToolSpec,
    program: String,
    /// The required dependency `program` belongs to and the program's name
    /// there, if any; the program is then found through the dependency
    /// registry.
    dep: Option<(String, String)>,
    args: Vec<ArgItem>,
    output: OutputSection,
}

/// Parse a manifest from TOML text.
pub fn parse_manifest(text: &str, source: &str) -> Result<ExternalTool, String> {
    let mut m: Manifest = toml::from_str(text).map_err(|e| format!("invalid manifest: {e}"))?;
    let category = Category::parse(&m.tool.category)
        .ok_or_else(|| format!("unknown category '{}'", m.tool.category))?;
    let prog_name = Path::new(&m.command.program)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if FORBIDDEN_PROGRAMS.contains(&prog_name.as_str())
        || prog_name.starts_with("python")
        || prog_name.starts_with("perl")
        || prog_name.starts_with("node")
    {
        return Err(format!(
            "program '{}' is an interpreter/shell and is not allowed in manifests",
            m.command.program
        ));
    }
    for p in &mut m.params {
        if p.label.is_empty() {
            let mut l = p.name.replace('_', " ");
            if let Some(f) = l.get(0..1) {
                l = f.to_uppercase() + &l[1..];
            }
            p.label = l;
        }
        if matches!(p.kind, ParamKind::Path { .. })
            && !m.tool.permissions.iter().any(|x| x == "host_fs_read")
        {
            m.tool.permissions.push("host_fs_read".into());
        }
    }
    let mut spec = ToolSpec::new(&m.tool.id, &m.tool.name, category)
        .describe(&m.tool.description)
        .execution(Execution::External);
    spec.version = m.tool.version.clone();
    spec.keywords = m.tool.keywords.clone();
    spec.params = m.params.clone();
    spec.timeout_secs = m.tool.timeout_secs;
    if !m.tool.platforms.is_empty() {
        spec.platforms = m.tool.platforms.clone();
    }
    for d in &m.tool.requires {
        if dep_spec(d).is_none() {
            return Err(format!("unknown dependency '{d}' in requires"));
        }
    }
    let reqs: Vec<&str> = m.tool.requires.iter().map(String::as_str).collect();
    spec = spec.requires(&reqs);
    let dep = program_dep(&m.command.program, &m.tool.requires);
    for p in &m.tool.permissions {
        let perm = Permission::parse(p).ok_or_else(|| format!("unknown permission '{p}'"))?;
        spec = spec.permission(perm);
    }
    spec.output.kind = if m.output.file.is_some() {
        OutputKind::Files
    } else if m.output.stdout.is_some() {
        OutputKind::Text
    } else {
        OutputKind::Data
    };
    spec.output.text_mime = m.output.stdout.clone();
    spec.source = format!("extension:{source}");
    // Validate placeholders reference known params.
    let known: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    let check = |s: &str| -> Result<(), String> {
        for ph in placeholders(s) {
            let base = ph.split('.').next().unwrap_or("");
            if !["output", "workdir"].contains(&base) && !known.contains(&base) {
                return Err(format!(
                    "placeholder '{{{ph}}}' does not match any parameter"
                ));
            }
        }
        Ok(())
    };
    for a in &m.command.args {
        match a {
            ArgItem::Plain(s) => check(s)?,
            ArgItem::Group { when, args } => {
                if !known.contains(&when.as_str()) {
                    return Err(format!(
                        "group condition '{when}' does not match any parameter"
                    ));
                }
                for s in args {
                    check(s)?;
                }
            }
        }
    }
    if let Some(f) = &m.output.file {
        check(f)?;
    }
    Ok(ExternalTool {
        spec,
        program: m.command.program,
        dep,
        args: m.command.args,
        output: m.output,
    })
}

/// The required dependency whose id or one of whose executables `program`
/// names (ignoring case and `.exe` on Windows), with the bare program name.
fn program_dep(program: &str, requires: &[String]) -> Option<(String, String)> {
    if Path::new(program).components().count() != 1 {
        return None;
    }
    let name = if cfg!(windows) {
        let lower = program.to_ascii_lowercase();
        lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
    } else {
        program.to_string()
    };
    requires
        .iter()
        .filter_map(|d| dep_spec(d))
        .find(|d| {
            d.id == name
                || d.binaries
                    .iter()
                    .chain(d.unix_only_binaries)
                    .any(|b| *b == name)
        })
        .map(|d| (d.id.to_string(), name))
}

fn placeholders(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        if let Some(j) = rest[i..].find('}') {
            out.push(rest[i + 1..i + j].to_string());
            rest = &rest[i + j + 1..];
        } else {
            break;
        }
    }
    out
}

fn value_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(value_str).collect::<Vec<_>>().join(","),
        other => other.to_string(),
    }
}

impl ExternalTool {
    fn expand(
        &self,
        s: &str,
        params: &Params,
        output: &Option<PathBuf>,
        ctx: &ToolContext,
    ) -> ToolResult<String> {
        let mut result = String::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            result.push_str(&rest[..i]);
            let j = rest[i..]
                .find('}')
                .ok_or_else(|| ToolError::failed("unterminated placeholder"))?;
            let ph = &rest[i + 1..i + j];
            let (base, attr) = ph
                .split_once('.')
                .map(|(a, b)| (a, Some(b)))
                .unwrap_or((ph, None));
            let value = if base == "output" {
                output
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .ok_or_else(|| {
                        ToolError::failed("manifest uses {output} without [output].file")
                    })?
            } else if base == "workdir" {
                ctx.work_dir().to_string_lossy().to_string()
            } else if let Some(f) = params.opt_file(base) {
                match attr {
                    Some("stem") => f.stem(),
                    Some("ext") => f.extension(),
                    Some("name") => f.name.clone(),
                    _ => f.path.to_string_lossy().to_string(),
                }
            } else {
                params
                    .get(base)
                    .map(value_str)
                    .ok_or_else(|| ToolError::invalid(base, "is required by the command"))?
            };
            result.push_str(&value);
            rest = &rest[i + j + 1..];
        }
        result.push_str(rest);
        Ok(result)
    }
}

#[async_trait::async_trait]
impl Tool for ExternalTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    async fn run(&self, ctx: ToolContext, params: Params) -> ToolResult<ToolOutput> {
        let mut cmd = if let Some((dep, name)) = &self.dep {
            ctx.command_program(dep, name)?
        } else if Path::new(&self.program).is_absolute() {
            ctx.command_path(PathBuf::from(&self.program), &self.program)
        } else {
            // Another program shipped beside a required dependency (e.g.
            // `pdftotext` next to Poppler's `pdftoppm`) comes from that folder.
            let beside = (Path::new(&self.program).components().count() == 1)
                .then(|| {
                    self.spec
                        .requires
                        .iter()
                        .find_map(|d| ctx.sibling_path(d, &self.program))
                })
                .flatten();
            let program = beside
                .or_else(|| which::which(&self.program).ok())
                .ok_or_else(|| ToolError::MissingDependency {
                    dependency: self.program.clone(),
                    hint: format!("'{}' was not found on PATH", self.program),
                })?;
            ctx.command_path(program, &self.program)
        };
        let output_path = match &self.output.file {
            Some(t) => {
                let name = self.expand(t, &params, &None, &ctx)?;
                Some(ctx.output_path(&name))
            }
            None => None,
        };
        let mut argv: Vec<String> = vec![];
        for a in &self.args {
            match a {
                ArgItem::Plain(s) => argv.push(self.expand(s, &params, &output_path, &ctx)?),
                ArgItem::Group { when, args } => {
                    let on = match params.get(when) {
                        Some(Value::Bool(b)) => *b,
                        Some(Value::String(s)) => !s.is_empty(),
                        Some(Value::Null) | None => false,
                        Some(_) => true,
                    };
                    if on {
                        for s in args {
                            argv.push(self.expand(s, &params, &output_path, &ctx)?);
                        }
                    }
                }
            }
        }
        cmd = cmd.args(&argv);
        if self.spec.permissions.contains(&Permission::Network) {
            cmd = cmd.allow_network();
        }
        let out = cmd.run().await?;
        let mut result = ToolOutput::new();
        if let Some(mime) = &self.output.stdout {
            result = result.with_text(out.stdout_text(), mime);
        }
        if let Some(p) = output_path {
            if p.exists() {
                result = result.with_artifact(ctx.artifact(&p)?);
            } else {
                return Err(ToolError::failed(format!(
                    "{} did not produce the expected output file",
                    self.program
                )));
            }
        }
        Ok(result)
    }
}

/// Trusted manifest hashes: `id -> sha256`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    #[serde(default)]
    pub trusted: BTreeMap<String, String>,
}

impl TrustStore {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(
            path,
            toml::to_string_pretty(self).map_err(std::io::Error::other)?,
        )
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionStatus {
    pub file: String,
    pub id: Option<String>,
    pub sha256: String,
    pub loaded: bool,
    pub error: Option<String>,
}

/// Load trusted extension manifests from a directory.
pub fn load_extensions(dir: &Path, trust_all: bool) -> (Vec<Arc<dyn Tool>>, Vec<ExtensionStatus>) {
    let trust = TrustStore::load(&dir.join("trusted.toml"));
    let mut tools: Vec<Arc<dyn Tool>> = vec![];
    let mut status = vec![];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (tools, status);
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "toml").unwrap_or(false))
        .collect();
    files.sort();
    for path in files {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        if file == "trusted.toml" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let sha = sha256_hex(text.as_bytes());
        match parse_manifest(&text, &file) {
            Ok(t) => {
                let id = t.spec.id.clone();
                let trusted =
                    trust_all || trust.trusted.get(&id).map(|h| *h == sha).unwrap_or(false);
                if trusted {
                    tools.push(Arc::new(t));
                    status.push(ExtensionStatus {
                        file,
                        id: Some(id),
                        sha256: sha,
                        loaded: true,
                        error: None,
                    });
                } else {
                    status.push(ExtensionStatus {
                        file,
                        id: Some(id.clone()),
                        sha256: sha,
                        loaded: false,
                        error: Some(format!(
                            "not trusted (run `dogubako extensions trust {id}` after reviewing it)"
                        )),
                    });
                }
            }
            Err(e) => status.push(ExtensionStatus {
                file,
                id: None,
                sha256: sha,
                loaded: false,
                error: Some(e),
            }),
        }
    }
    (tools, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str = r#"
[tool]
id = "text.ext_head"
name = "Head (extension)"
category = "text"
description = "First lines of a file"
[command]
program = "head"
args = ["-n", "{lines}", { when = "quiet", args = ["-q"] }, "--", "{input}"]
[output]
stdout = "text/plain"
[[params]]
name = "input"
type = "file"
required = true
[[params]]
name = "lines"
type = "integer"
min = 1
max = 1000
default = 2
[[params]]
name = "quiet"
type = "boolean"
"#;

    #[test]
    fn rejects_shells_and_unknown_placeholders() {
        let bad = M.replace("program = \"head\"", "program = \"/bin/sh\"");
        assert!(parse_manifest(&bad, "x")
            .err()
            .unwrap()
            .contains("not allowed"));
        let bad = M.replace("\"{lines}\"", "\"{nope}\"");
        assert!(parse_manifest(&bad, "x")
            .err()
            .unwrap()
            .contains("placeholder"));
        let bad = M.replace("category = \"text\"", "category = \"bogus\"");
        assert!(parse_manifest(&bad, "x").is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_with_argv_only() {
        let tool = parse_manifest(M, "test.toml").unwrap();
        let h = crate::testing::TestHarness::new(vec![std::sync::Arc::new(tool)]);
        // A file name with shell metacharacters must be passed verbatim.
        let p = h.write("a; rm -rf x $(id).txt", b"one\ntwo\nthree\n");
        let out = h
            .run("text.ext_head", serde_json::json!({"input": p, "lines": 2}))
            .await
            .unwrap();
        assert_eq!(out.text.as_deref(), Some("one\ntwo\n"));
        let err = h
            .run("text.ext_head", serde_json::json!({"input": p, "lines": 0}))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_input");
    }

    const LPSTAT: &str = r#"
[tool]
id = "system.ext_printers"
name = "Printers (extension)"
category = "system"
requires = ["cups"]
[command]
program = "lpstat"
args = ["-a"]
[output]
stdout = "text/plain"
"#;

    #[test]
    fn requires_accepts_known_deps_only() {
        let t = parse_manifest(LPSTAT, "x").unwrap();
        assert_eq!(t.spec.requires, vec!["cups".to_string()]);
        assert_eq!(t.dep, Some(("cups".into(), "lpstat".into())));
        let multi = LPSTAT.replace(r#"["cups"]"#, r#"["ffmpeg", "cups"]"#);
        assert_eq!(
            parse_manifest(&multi, "x")
                .unwrap()
                .dep
                .map(|d| d.0)
                .as_deref(),
            Some("cups")
        );
        let bad = LPSTAT.replace(r#"["cups"]"#, r#"["cups", "nope"]"#);
        assert!(parse_manifest(&bad, "x")
            .err()
            .unwrap()
            .contains("unknown dependency 'nope'"));
        // A program outside the required deps keeps the PATH lookup.
        assert_eq!(parse_manifest(M, "x").unwrap().dep, None);
        let other = LPSTAT.replace(r#"["cups"]"#, r#"["ffmpeg"]"#);
        assert_eq!(parse_manifest(&other, "x").unwrap().dep, None);
        let abs = LPSTAT.replace(r#""lpstat""#, r#""/opt/x/lpstat""#);
        assert_eq!(parse_manifest(&abs, "x").unwrap().dep, None);
    }

    #[test]
    fn missing_requirement_matches_a_rust_tool() {
        // Whichever of these is missing here, the manifest tool must report
        // it exactly like a Rust tool with the same `.requires(..)`.
        let deps = ["cups", "piper", "realesrgan"];
        let text = LPSTAT.replace(r#"["cups"]"#, r#"["cups", "piper", "realesrgan"]"#);
        let ext = parse_manifest(&text, "x").unwrap();
        let rust =
            ToolSpec::new("system.rust_printers", "Printers", Category::System).requires(&deps);
        let h = crate::testing::TestHarness::new(vec![std::sync::Arc::new(ext)]);
        let ext_spec = h.engine.registry.get("system.ext_printers").unwrap().spec();
        let a = serde_json::to_value(h.engine.availability(ext_spec)).unwrap();
        let b = serde_json::to_value(h.engine.availability(&rust)).unwrap();
        assert_eq!(a, b);
        if a["available"] == false {
            let hint = &a["missing"][0]["hint"];
            assert!(hint.as_str().is_some_and(|s| !s.is_empty()));
        }
    }

    #[cfg(unix)]
    fn fake_bins(names: &[&str]) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let bin = tempfile::tempdir().unwrap();
        for name in names {
            let p = bin.path().join(name);
            std::fs::write(&p, format!("#!/bin/sh\necho fake-{name} \"$@\"\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        bin
    }

    #[cfg(unix)]
    async fn run_with_bins(manifest: &str, bin: &Path) -> String {
        let tool = parse_manifest(manifest, "test.toml").unwrap();
        let id = tool.spec.id.clone();
        let h = crate::testing::TestHarness::with_bin_dirs(
            vec![std::sync::Arc::new(tool)],
            vec![bin.to_path_buf()],
        );
        assert!(h.available(&id));
        let out = h.run(&id, serde_json::json!({})).await.unwrap();
        out.text.unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn program_is_resolved_through_the_dependency_registry() {
        let bin = fake_bins(&["lp", "lpstat"]);
        assert_eq!(run_with_bins(LPSTAT, bin.path()).await, "fake-lpstat -a\n");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dependency_id_runs_the_binary_the_registry_found() {
        let bin = fake_bins(&["whisper-cli"]);
        let m = LPSTAT
            .replace(r#"["cups"]"#, r#"["whisper"]"#)
            .replace(r#""lpstat""#, r#""whisper""#);
        assert_eq!(run_with_bins(&m, bin.path()).await, "fake-whisper-cli -a\n");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn other_programs_beside_a_required_dep_are_used() {
        let bin = fake_bins(&["pdftoppm", "pdftotext"]);
        let m = LPSTAT
            .replace(r#"["cups"]"#, r#"["poppler"]"#)
            .replace(r#""lpstat""#, r#""pdftotext""#);
        assert_eq!(run_with_bins(&m, bin.path()).await, "fake-pdftotext -a\n");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_renamed_path_override_runs_as_the_main_program() {
        // Only this test sets DOGUBAKO_PANDOC_PATH.
        let bin = fake_bins(&["pandoc-3", "pandoc-lua"]);
        std::env::set_var("DOGUBAKO_PANDOC_PATH", bin.path().join("pandoc-3"));
        let main = LPSTAT
            .replace(r#"["cups"]"#, r#"["pandoc"]"#)
            .replace(r#""lpstat""#, r#""pandoc""#);
        let sibling = main.replace(r#"program = "pandoc""#, r#"program = "pandoc-lua""#);
        let empty = tempfile::tempdir().unwrap();
        let main_out = run_with_bins(&main, empty.path()).await;
        let sibling_out = run_with_bins(&sibling, empty.path()).await;
        std::env::remove_var("DOGUBAKO_PANDOC_PATH");
        assert_eq!(main_out, "fake-pandoc-3 -a\n");
        assert_eq!(sibling_out, "fake-pandoc-lua -a\n");
    }

    #[test]
    fn trust_store_requires_matching_hash() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("head.toml"), M).unwrap();
        let (tools, st) = load_extensions(dir.path(), false);
        assert!(tools.is_empty());
        assert!(!st[0].loaded);
        let mut ts = TrustStore::default();
        ts.trusted
            .insert("text.ext_head".into(), sha256_hex(M.as_bytes()));
        ts.save(&dir.path().join("trusted.toml")).unwrap();
        let (tools, _) = load_extensions(dir.path(), false);
        assert_eq!(tools.len(), 1);
        // Modifying the manifest revokes trust.
        std::fs::write(
            dir.path().join("head.toml"),
            M.replace("First lines", "Changed"),
        )
        .unwrap();
        let (tools, _) = load_extensions(dir.path(), false);
        assert!(tools.is_empty());
    }
}
