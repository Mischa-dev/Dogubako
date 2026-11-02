//! Office (LibreOffice) and markup (Pandoc) document conversion.

use crate::common::*;
use dogubako_core::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![convert_document(), convert_markup(), markup_to_pdf()]
}

/// `file://` URL for a local path (percent-encoded).
pub fn file_url(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = if s.starts_with('/') {
        s
    } else {
        format!("/{s}")
    };
    let mut out = String::from("file://");
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Copy an input into a private directory under its (sanitized) original
/// name: LibreOffice and Pandoc use the extension to pick an import filter.
fn stage_input(ctx: &ToolContext, f: &InputFile, dir: &str) -> ToolResult<PathBuf> {
    let d = ctx.temp_dir(dir)?;
    let name = dogubako_core::policy::sanitize_filename(&f.name);
    let dst = d.join(name);
    std::fs::copy(&f.path, &dst)?;
    Ok(dst)
}

/// Convert `input` with LibreOffice; returns the produced file.
async fn soffice_convert(
    ctx: &ToolContext,
    input: &Path,
    target: &str,
    outdir: &Path,
) -> ToolResult<PathBuf> {
    let profile = ctx.temp_dir("lo-profile")?;
    let (filter, ext) = match target {
        "txt" => ("txt:Text (encoded):UTF8", "txt"),
        "csv" => ("csv:Text - txt - csv (StarCalc):44,34,76,1", "csv"),
        "html" => ("html", "html"),
        other => (other, other),
    };
    std::fs::create_dir_all(outdir)?;
    let res = ctx
        .command("libreoffice")?
        .args([
            "--headless",
            "--invisible",
            "--norestore",
            "--nologo",
            "--nodefault",
            "--nolockcheck",
            "--nofirststartwizard",
        ])
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .arg("--convert-to")
        .arg(filter)
        .arg("--outdir")
        .arg(outdir)
        .arg(arg_path(input))
        .timeout(Duration::from_secs(600))
        .run()
        .await?;
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let expected = outdir.join(format!("{stem}.{ext}"));
    if expected.is_file() {
        return Ok(expected);
    }
    // Fall back to whatever single file appeared.
    let found = std::fs::read_dir(outdir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.is_file());
    found.ok_or_else(|| {
        let msg = format!("{} {}", res.stdout_text().trim(), res.stderr.trim());
        ToolError::failed(format!(
            "LibreOffice could not convert this file to {target}. {}",
            msg.trim()
        ))
    })
}

const OFFICE_ACCEPT: &[&str] = &[
    ".doc", ".docx", ".odt", ".rtf", ".txt", ".html", ".htm", ".wpd", ".pages", ".xls", ".xlsx",
    ".ods", ".csv", ".ppt", ".pptx", ".odp", ".odg", ".vsd", ".pub", ".fodt", ".fods", ".fodp",
    ".dotx", ".xlsm", ".docm", ".pptm",
];

fn convert_document() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.convert_document", "Convert Office Documents", Category::Pdf)
        .describe("Convert Word, Excel, PowerPoint and OpenDocument files to PDF or other office formats with LibreOffice (offline)")
        .keywords(&["docx to pdf", "word to pdf", "excel to pdf", "powerpoint to pdf", "odt", "office", "libreoffice", "convert"])
        .param(ParamSpec::files("input").label("Documents").accept(OFFICE_ACCEPT).required().primary())
        .param(
            ParamSpec::choice(
                "format",
                &[
                    ("pdf", "PDF"),
                    ("docx", "Word (DOCX)"),
                    ("odt", "OpenDocument Text (ODT)"),
                    ("rtf", "Rich Text (RTF)"),
                    ("txt", "Plain text"),
                    ("html", "HTML"),
                    ("xlsx", "Excel (XLSX)"),
                    ("ods", "OpenDocument Spreadsheet (ODS)"),
                    ("csv", "CSV (first sheet)"),
                    ("pptx", "PowerPoint (PPTX)"),
                    ("odp", "OpenDocument Presentation (ODP)"),
                ],
            )
            .default_str("pdf"),
        )
        .requires(&["libreoffice"])
        .output(OutputKind::Files)
        .progress()
        .timeout(3600);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input");
        let fmt = p.str_or("format", "pdf").to_string();
        let mut arts = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, files.len(), &f.name);
            let staged = stage_input(&ctx, f, &format!("in{i}"))?;
            let outdir = ctx.temp_path(&format!("out{i}"));
            let produced = soffice_convert(&ctx, &staged, &fmt, &outdir)
                .await
                .map_err(|e| match e {
                    ToolError::Failed { message } => {
                        ToolError::failed(format!("{}: {message}", f.name))
                    }
                    o => o,
                })?;
            let ext = produced
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or(fmt.clone());
            let out = ctx.output_path(&with_extension(&f.name, &ext));
            std::fs::rename(&produced, &out)
                .or_else(|_| std::fs::copy(&produced, &out).map(|_| ()))?;
            arts.push(ctx.artifact(&out)?);
        }
        Ok(ToolOutput::files(arts))
    })
}

const MARKUP_FROM: &[(&str, &str)] = &[
    ("auto", "Detect from extension"),
    ("markdown", "Markdown (Pandoc)"),
    ("gfm", "GitHub Markdown"),
    ("commonmark", "CommonMark"),
    ("html", "HTML"),
    ("docx", "Word (DOCX)"),
    ("odt", "OpenDocument (ODT)"),
    ("epub", "EPUB"),
    ("rst", "reStructuredText"),
    ("latex", "LaTeX"),
    ("org", "Org mode"),
    ("textile", "Textile"),
    ("mediawiki", "MediaWiki"),
    ("docbook", "DocBook"),
    ("rtf", "RTF"),
    ("ipynb", "Jupyter notebook"),
    ("csv", "CSV"),
];

const MARKUP_TO: &[(&str, &str)] = &[
    ("html", "HTML"),
    ("docx", "Word (DOCX)"),
    ("odt", "OpenDocument (ODT)"),
    ("epub", "EPUB"),
    ("markdown", "Markdown (Pandoc)"),
    ("gfm", "GitHub Markdown"),
    ("rst", "reStructuredText"),
    ("latex", "LaTeX"),
    ("org", "Org mode"),
    ("asciidoc", "AsciiDoc"),
    ("mediawiki", "MediaWiki"),
    ("textile", "Textile"),
    ("rtf", "RTF"),
    ("docbook", "DocBook"),
    ("plain", "Plain text"),
    ("typst", "Typst"),
];

fn ext_for_writer(w: &str) -> &'static str {
    match w {
        "html" => "html",
        "docx" => "docx",
        "odt" => "odt",
        "epub" => "epub",
        "markdown" | "gfm" => "md",
        "rst" => "rst",
        "latex" => "tex",
        "org" => "org",
        "asciidoc" => "adoc",
        "mediawiki" => "wiki",
        "textile" => "textile",
        "rtf" => "rtf",
        "docbook" => "xml",
        "typst" => "typ",
        _ => "txt",
    }
}

fn reader_for_ext(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "md" | "markdown" | "mkd" | "txt" => "markdown",
        "html" | "htm" | "xhtml" => "html",
        "docx" => "docx",
        "odt" => "odt",
        "epub" => "epub",
        "rst" => "rst",
        "tex" | "latex" => "latex",
        "org" => "org",
        "textile" => "textile",
        "wiki" | "mediawiki" => "mediawiki",
        "dbk" | "docbook" => "docbook",
        "rtf" => "rtf",
        "ipynb" => "ipynb",
        "csv" => "csv",
        _ => return None,
    })
}

/// Metadata keys that make Pandoc writers read local files.
const RISKY_META: &[&str] = &[
    "cover-image",
    "epub-cover-image",
    "css",
    "stylesheet",
    "epub-fonts",
    "include-before",
    "include-after",
    "bibliography",
    "csl",
    "reference-doc",
    "template",
    "epub-embed-font",
];

/// Replace images pointing at files/URLs with their alt text and drop
/// metadata that makes writers read files, so a document cannot embed
/// arbitrary local files in the output.
pub fn sanitize_ast(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if m.get("t").and_then(|t| t.as_str()) == Some("Image") {
                let keep = m
                    .get("c")
                    .and_then(|c| c.get(2))
                    .and_then(|t| t.get(0))
                    .and_then(|u| u.as_str())
                    .map(|u| u.starts_with("data:"))
                    .unwrap_or(false);
                if !keep {
                    let c = m.get("c").cloned().unwrap_or(Value::Null);
                    let attr = c.get(0).cloned().unwrap_or(json!(["", [], []]));
                    let alt = c.get(1).cloned().unwrap_or(json!([]));
                    m.clear();
                    m.insert("t".into(), json!("Span"));
                    m.insert("c".into(), json!([attr, alt]));
                }
            }
            if let Some(Value::Object(meta)) = m.get_mut("meta") {
                for k in RISKY_META {
                    meta.remove(*k);
                }
            }
            for (_, x) in m.iter_mut() {
                sanitize_ast(x);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(sanitize_ast),
        _ => {}
    }
}

/// Pandoc with `--sandbox`: documents cannot make it read other files or the
/// network. Writers that need Pandoc's data files (DOCX, EPUB) do not work
/// under `--sandbox` in some Pandoc versions; for those the document is read
/// in the sandbox into Pandoc's AST, sanitized, and then written.
async fn pandoc(
    ctx: &ToolContext,
    input: &Path,
    from: &str,
    to: &str,
    standalone: bool,
    out: &Path,
) -> ToolResult<Vec<String>> {
    let two_step = matches!(to, "docx" | "epub");
    let mut warnings = vec![];
    let (src, reader) = if two_step {
        let ast = ctx.temp_path(&format!("ast-{}.json", dogubako_core::util::random_id(4)));
        let res = ctx
            .command("pandoc")?
            .arg("--sandbox")
            .args(["-f", from, "-t", "json", "-o"])
            .arg(&ast)
            .arg("--")
            .arg(arg_path(input))
            .timeout(Duration::from_secs(600))
            .run()
            .await?;
        warnings.extend(
            res.stderr
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(10)
                .map(|l| l.to_string()),
        );
        let mut v: Value =
            serde_json::from_slice(&std::fs::read(&ast)?).ctx("parsing Pandoc AST")?;
        sanitize_ast(&mut v);
        std::fs::write(&ast, serde_json::to_vec(&v)?)?;
        (ast, "json")
    } else {
        (input.to_path_buf(), from)
    };
    let empty = ctx.temp_dir("no-resources")?;
    let res = ctx
        .command("pandoc")?
        .arg_if(!two_step, "--sandbox")
        .arg(format!("--resource-path={}", empty.display()))
        .args(["-f", reader, "-t", to])
        .arg_if(standalone, "--standalone")
        .arg("-o")
        .arg(out)
        .arg("--")
        .arg(arg_path(&src))
        .timeout(Duration::from_secs(600))
        .run()
        .await?;
    warnings.extend(
        res.stderr
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(20)
            .map(|l| l.to_string()),
    );
    Ok(warnings)
}

fn markup_inputs(ctx: &ToolContext, p: &Params) -> ToolResult<Vec<(String, PathBuf, String)>> {
    let from = p.str_or("from", "auto");
    let mut v = vec![];
    let text = p.text("text");
    if !text.trim().is_empty() {
        let reader = if from == "auto" { "markdown" } else { from };
        let path = ctx.temp_path("input.txt");
        std::fs::write(&path, text)?;
        v.push(("document".to_string(), path, reader.to_string()));
    }
    for (i, f) in p.files("input").iter().enumerate() {
        let reader = if from == "auto" {
            reader_for_ext(&f.extension()).ok_or_else(|| {
                ToolError::invalid(
                    "from",
                    format!(
                        "cannot detect the format of '{}'; choose the input format",
                        f.name
                    ),
                )
            })?
        } else {
            from
        };
        v.push((
            f.name.clone(),
            stage_input(ctx, f, &format!("in{i}"))?,
            reader.to_string(),
        ));
    }
    if v.is_empty() {
        return Err(ToolError::invalid(
            "input",
            "provide input files or paste text",
        ));
    }
    Ok(v)
}

fn markup_spec(id: &str, name: &str, desc: &str) -> ToolSpec {
    ToolSpec::new(id, name, Category::Pdf)
        .describe(desc)
        .param(ParamSpec::files("input").label("Documents").primary())
        .param(
            ParamSpec::textarea("text")
                .label("Or paste text")
                .syntax("markdown"),
        )
        .param(
            ParamSpec::choice("from", MARKUP_FROM)
                .label("Input format")
                .default_str("auto"),
        )
}

fn convert_markup() -> Arc<dyn Tool> {
    let spec = markup_spec("pdf.convert_markup", "Convert Markup Documents", "Convert between Markdown, HTML, DOCX, ODT, EPUB, reStructuredText, LaTeX, Org and more with Pandoc")
        .keywords(&["pandoc", "markdown", "html", "docx", "epub", "rst", "latex", "org", "convert"])
        .param(ParamSpec::choice("to", MARKUP_TO).label("Output format").default_str("html"))
        .param(ParamSpec::boolean("standalone").label("Standalone document").describe("Produce a complete document with header (HTML/LaTeX/...)").default_bool(true).advanced())
        .requires(&["pandoc"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let to = p.str_or("to", "html").to_string();
        let inputs = markup_inputs(&ctx, &p)?;
        let mut arts = vec![];
        let mut warnings = vec![];
        let mut preview = None;
        for (i, (name, path, reader)) in inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, inputs.len(), name);
            let out = ctx.output_path(&with_extension(name, ext_for_writer(&to)));
            warnings.extend(pandoc(&ctx, path, reader, &to, p.bool("standalone"), &out).await?);
            if inputs.len() == 1 && !matches!(to.as_str(), "docx" | "odt" | "epub") {
                preview = std::fs::read_to_string(&out)
                    .ok()
                    .filter(|s| s.len() < 2 * 1024 * 1024);
            }
            arts.push(ctx.artifact(&out)?);
        }
        let mut o = ToolOutput::files(arts);
        if let Some(t) = preview {
            let mime = match to.as_str() {
                "html" => "text/html",
                "markdown" | "gfm" => "text/markdown",
                _ => "text/plain",
            };
            o = o.with_text(t, mime);
        }
        o.warnings = warnings;
        Ok(o)
    })
}

fn markup_to_pdf() -> Arc<dyn Tool> {
    let spec = markup_spec("pdf.markup_to_pdf", "Markdown to PDF", "Turn Markdown, HTML, reStructuredText and other markup into PDF (Pandoc → DOCX → LibreOffice)")
        .keywords(&["markdown to pdf", "md to pdf", "html to pdf", "pandoc", "libreoffice", "convert"])
        .requires(&["pandoc", "libreoffice"])
        .output(OutputKind::Files)
        .progress()
        .timeout(3600);
    async_tool(spec, |ctx, p| async move {
        let inputs = markup_inputs(&ctx, &p)?;
        let mut arts = vec![];
        let mut warnings = vec![];
        for (i, (name, path, reader)) in inputs.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, inputs.len(), name);
            let mid_dir = ctx.temp_dir(&format!("docx{i}"))?;
            let stem = Path::new(name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "document".into());
            let docx = mid_dir.join(dogubako_core::policy::sanitize_filename(&format!(
                "{stem}.docx"
            )));
            warnings.extend(pandoc(&ctx, path, reader, "docx", true, &docx).await?);
            let produced =
                soffice_convert(&ctx, &docx, "pdf", &ctx.temp_path(&format!("pdf{i}"))).await?;
            let out = ctx.output_path(&with_extension(name, "pdf"));
            std::fs::rename(&produced, &out)
                .or_else(|_| std::fs::copy(&produced, &out).map(|_| ()))?;
            arts.push(ctx.artifact(&out)?);
        }
        let mut o = ToolOutput::files(arts);
        o.warnings = warnings;
        Ok(o)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls() {
        assert_eq!(file_url(Path::new("/tmp/a b/lo")), "file:///tmp/a%20b/lo");
        assert_eq!(reader_for_ext("md"), Some("markdown"));
        assert_eq!(ext_for_writer("latex"), "tex");
        let mut ast = json!({"meta": {"cover-image": {"t": "MetaString", "c": "/etc/passwd"}, "title": 1}, "blocks": [{"t": "Para", "c": [
            {"t": "Image", "c": [["", [], []], [{"t": "Str", "c": "alt"}], ["/etc/passwd", ""]]},
            {"t": "Image", "c": [["", [], []], [], ["data:image/png;base64,AAAA", ""]]}
        ]}]});
        sanitize_ast(&mut ast);
        assert!(ast["meta"].get("cover-image").is_none());
        assert_eq!(ast["blocks"][0]["c"][0]["t"], "Span");
        assert_eq!(ast["blocks"][0]["c"][1]["t"], "Image");
    }
}
