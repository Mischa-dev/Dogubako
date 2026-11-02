//! Structural PDF operations backed by qpdf.

use crate::common::*;
use crate::ranges;
use dogubako_core::prelude::*;
use std::path::{Path, PathBuf};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        merge(),
        split(),
        extract_pages(),
        reorder(),
        rotate(),
        delete_pages(),
        repair(),
        linearize(),
        flatten(),
        protect(),
        unlock(),
    ]
}

pub fn pdf_input() -> ParamSpec {
    ParamSpec::file("input")
        .label("PDF")
        .accept(PDF_ACCEPT)
        .required()
        .primary()
}

pub fn pdf_inputs() -> ParamSpec {
    ParamSpec::files("input")
        .label("PDF files")
        .accept(PDF_ACCEPT)
        .required()
        .primary()
}

fn pdf_name(f: &InputFile, suffix: &str) -> String {
    suffixed(&f.name, suffix, Some("pdf"))
}

fn map_qpdf_err(param: &str) -> impl Fn(ToolError) -> ToolError + '_ {
    move |e| match e {
        ToolError::External {
            stderr,
            program,
            exit_code,
        } => {
            let low = stderr.to_ascii_lowercase();
            if low.contains("invalid password") {
                ToolError::invalid(param, "the password is incorrect")
            } else if low.contains("password") {
                ToolError::invalid(
                    param,
                    "the PDF is password protected; remove the password with pdf.unlock first",
                )
            } else if low.contains("not a pdf")
                || low.contains("can't find pdf header")
                || low.contains("unable to find trailer")
            {
                ToolError::invalid(param, format!("not a readable PDF: {stderr}"))
            } else {
                ToolError::External {
                    stderr,
                    program,
                    exit_code,
                }
            }
        }
        other => other,
    }
}

fn merge() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.merge", "Merge PDFs", Category::Pdf)
        .describe("Combine several PDFs into one document in the given order, optionally taking only some pages of each")
        .keywords(&["combine", "join", "concatenate", "append", "pdf"])
        .param(ParamSpec::files("input").label("PDF files (in order)").accept(PDF_ACCEPT).required().primary())
        .param(
            ParamSpec::textarea("ranges")
                .label("Page ranges")
                .describe("Optional: one page range per line, in file order (e.g. '1-3,5' or 'z-1'); blank line = all pages")
                .advanced(),
        )
        .param(ParamSpec::text("output_name").label("Output file name").default_str("merged.pdf").advanced())
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input");
        if files.len() < 2 {
            return Err(ToolError::invalid(
                "input",
                "select at least two PDFs to merge",
            ));
        }
        let lines: Vec<&str> = p.text("ranges").lines().collect();
        if lines.len() > files.len() {
            return Err(ToolError::invalid(
                "ranges",
                format!(
                    "{} range lines given for {} files",
                    lines.len(),
                    files.len()
                ),
            ));
        }
        let mut args: Vec<std::ffi::OsString> = vec!["--empty".into(), "--pages".into()];
        let mut total_pages = 0u32;
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, files.len() + 1, &f.name);
            let n = page_count(&ctx, &f.path)
                .await
                .map_err(|e| prefix_err(e, &f.name))?;
            let r = lines.get(i).map(|s| s.trim()).filter(|s| !s.is_empty());
            let pages = match r {
                Some(r) => ranges::parse(r, n).map_err(|e| {
                    ToolError::invalid("ranges", format!("line {} ({}): {e}", i + 1, f.name))
                })?,
                None => (1..=n).collect(),
            };
            total_pages += pages.len() as u32;
            args.push(arg_path(&f.path));
            args.push(ranges::to_qpdf(&pages).into());
        }
        let mut name = p.str_or("output_name", "merged.pdf").to_string();
        if !name.to_ascii_lowercase().ends_with(".pdf") {
            name.push_str(".pdf");
        }
        let out = ctx.output_path(&name);
        args.push("--".into());
        args.push(out.clone().into());
        ctx.progress(0.9, "Writing merged PDF");
        let res = qpdf(&ctx)?
            .args(args)
            .run()
            .await
            .map_err(map_qpdf_err("input"))?;
        let mut o = ToolOutput::files(vec![ctx.artifact(&out)?])
            .with_data(json!({"files": files.len(), "pages": total_pages}));
        o.warnings = qpdf_warnings(&res);
        Ok(o)
    })
}

fn prefix_err(e: ToolError, name: &str) -> ToolError {
    match e {
        ToolError::InvalidInput { param, message } => ToolError::InvalidInput {
            param,
            message: format!("{name}: {message}"),
        },
        other => other,
    }
}

/// Run qpdf selecting `pages` of `input` into `out`, keeping document-level data.
async fn select_pages(
    ctx: &ToolContext,
    input: &Path,
    pages: &[u32],
    out: &Path,
) -> ToolResult<Vec<String>> {
    let res = qpdf(ctx)?
        .arg(arg_path(input))
        .arg("--pages")
        .arg(".")
        .arg(ranges::to_qpdf(pages))
        .arg("--")
        .arg(out)
        .run()
        .await
        .map_err(map_qpdf_err("input"))?;
    Ok(qpdf_warnings(&res))
}

fn split() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.split", "Split PDF", Category::Pdf)
        .describe("Split a PDF into several files: every N pages, single pages, or custom page ranges")
        .keywords(&["separate", "burst", "pages", "divide", "pdf"])
        .param(pdf_input())
        .param(ParamSpec::choice("mode", &[("every", "Every N pages"), ("single", "Single pages"), ("ranges", "Custom ranges")]).default_str("every"))
        .param(ParamSpec::integer("every").label("Pages per file").range(1.0, 100000.0).default_int(1).show_if("mode", &["every"]))
        .param(
            ParamSpec::textarea("ranges")
                .label("Ranges")
                .describe("One output file per group; separate groups with ';' or new lines, e.g. '1-3; 4-6; 7-z'")
                .show_if("mode", &["ranges"]),
        )
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?;
        let n = page_count(&ctx, &f.path).await?;
        let mode = p.str_or("mode", "every");
        let mut arts = vec![];
        let mut warnings = vec![];
        if mode == "ranges" {
            let groups = ranges::parse_groups(p.text("ranges"), n).bad_input("ranges")?;
            for (i, g) in groups.iter().enumerate() {
                ctx.check_cancelled()?;
                ctx.progress(
                    i as f32 / groups.len() as f32,
                    format!("Part {}/{}", i + 1, groups.len()),
                );
                let label = if g.len() == 1 {
                    format!("p{}", g[0])
                } else {
                    format!("p{}-{}", g[0], g[g.len() - 1])
                };
                let out = ctx.output_path(&pdf_name(f, &label));
                warnings.extend(select_pages(&ctx, &f.path, g, &out).await?);
                arts.push(ctx.artifact_labeled(&out, &format!("part {}", i + 1))?);
            }
        } else {
            let every = if mode == "single" {
                1
            } else {
                p.int_or("every", 1).max(1) as u32
            };
            if every >= n {
                return Err(ToolError::invalid(
                    "every",
                    format!("the document has only {n} page(s); nothing to split"),
                ));
            }
            let dir = ctx.temp_dir("split")?;
            ctx.progress(0.1, "Splitting");
            let res = qpdf(&ctx)?
                .arg(arg_path(&f.path))
                .arg(format!("--split-pages={every}"))
                .arg(dir.join("part.pdf"))
                .run()
                .await
                .map_err(map_qpdf_err("input"))?;
            warnings.extend(qpdf_warnings(&res));
            let mut parts: Vec<PathBuf> = std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect();
            parts.sort();
            for (i, part) in parts.iter().enumerate() {
                let range = part
                    .file_stem()
                    .map(|s| s.to_string_lossy().trim_start_matches("part-").to_string())
                    .unwrap_or_default();
                let range = range
                    .split('-')
                    .map(|x| x.trim_start_matches('0'))
                    .collect::<Vec<_>>()
                    .join("-");
                let out = ctx.output_path(&pdf_name(f, &format!("p{range}")));
                std::fs::rename(part, &out).or_else(|_| std::fs::copy(part, &out).map(|_| ()))?;
                arts.push(ctx.artifact_labeled(&out, &format!("part {}", i + 1))?);
            }
        }
        let count = arts.len();
        let mut o = ToolOutput::files(arts).with_data(json!({"pages": n, "parts": count}));
        o.warnings = warnings;
        Ok(o)
    })
}

fn extract_pages() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.extract_pages", "Extract Pages", Category::Pdf)
        .describe("Create a new PDF containing only the selected pages")
        .keywords(&["select", "pick", "subset", "pages", "pdf"])
        .param(pdf_input())
        .param(
            ParamSpec::text("pages")
                .label("Pages")
                .describe("Page range, e.g. '1-3,5,8-z'")
                .placeholder("1-3,5")
                .required(),
        )
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .timeout(900);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?;
        let n = page_count(&ctx, &f.path).await?;
        let pages = ranges::parse(p.str("pages")?, n).bad_input("pages")?;
        let out = ctx.output_path(&pdf_name(f, "extract"));
        let w = select_pages(&ctx, &f.path, &pages, &out).await?;
        let mut o = ToolOutput::files(vec![ctx.artifact(&out)?])
            .with_data(json!({"pages": pages.len(), "source_pages": n}));
        o.warnings = w;
        Ok(o)
    })
}

fn reorder() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.reorder", "Reorder Pages", Category::Pdf)
        .describe("Rearrange the pages of a PDF in a new order, or reverse it (pages may be repeated or left out)")
        .keywords(&["order", "arrange", "reverse", "sort", "pages", "pdf"])
        .param(pdf_input())
        .param(ParamSpec::text("order").label("New order").describe("Page list in the desired order, e.g. '3,1,2,4-z'").placeholder("3,1,2,4-z"))
        .param(ParamSpec::boolean("reverse").label("Reverse all pages"))
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .timeout(900);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?;
        let n = page_count(&ctx, &f.path).await?;
        let pages = if p.bool("reverse") {
            match p.opt_str("order") {
                Some(o) => {
                    let mut v = ranges::parse(o, n).bad_input("order")?;
                    v.reverse();
                    v
                }
                None => (1..=n).rev().collect(),
            }
        } else {
            let order = p.opt_str("order").ok_or_else(|| {
                ToolError::invalid("order", "give a new page order or enable 'reverse'")
            })?;
            ranges::parse(order, n).bad_input("order")?
        };
        let out = ctx.output_path(&pdf_name(f, "reordered"));
        let w = select_pages(&ctx, &f.path, &pages, &out).await?;
        let mut o = ToolOutput::files(vec![ctx.artifact(&out)?])
            .with_data(json!({"order": ranges::to_qpdf(&pages), "pages": pages.len()}));
        o.warnings = w;
        Ok(o)
    })
}

fn rotate() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.rotate", "Rotate Pages", Category::Pdf)
        .describe("Rotate all or selected pages of one or more PDFs by 90, 180 or 270 degrees")
        .keywords(&["turn", "orientation", "landscape", "portrait", "pdf"])
        .param(pdf_inputs())
        .param(
            ParamSpec::choice(
                "angle",
                &[
                    ("90", "90° clockwise"),
                    ("180", "180°"),
                    ("270", "90° counter-clockwise"),
                ],
            )
            .default_str("90"),
        )
        .param(
            ParamSpec::text("pages")
                .label("Pages")
                .describe("Optional page range (default: all pages)")
                .placeholder("1-z"),
        )
        .param(
            ParamSpec::boolean("absolute")
                .label("Set absolute rotation")
                .describe(
                    "Set the rotation to exactly this angle instead of adding to the current one",
                )
                .advanced(),
        )
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input");
        let angle = p.str_or("angle", "90");
        let mut arts = vec![];
        let mut warnings = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, files.len(), &f.name);
            let n = page_count(&ctx, &f.path)
                .await
                .map_err(|e| prefix_err(e, &f.name))?;
            let range = match p.opt_str("pages") {
                Some(r) => ranges::to_qpdf(&ranges::parse(r, n).bad_input("pages")?),
                None => "1-z".to_string(),
            };
            let sign = if p.bool("absolute") { "" } else { "+" };
            let out = ctx.output_path(&pdf_name(f, "rotated"));
            let res = qpdf(&ctx)?
                .arg(arg_path(&f.path))
                .arg(format!("--rotate={sign}{angle}:{range}"))
                .arg(&out)
                .run()
                .await
                .map_err(map_qpdf_err("input"))?;
            warnings.extend(qpdf_warnings(&res));
            arts.push(ctx.artifact(&out)?);
        }
        let mut o = ToolOutput::files(arts);
        o.warnings = warnings;
        Ok(o)
    })
}

fn delete_pages() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.delete_pages", "Delete Pages", Category::Pdf)
        .describe("Remove selected pages from a PDF")
        .keywords(&["remove", "drop", "pages", "pdf"])
        .param(pdf_input())
        .param(
            ParamSpec::text("pages")
                .label("Pages to delete")
                .placeholder("2,4-6")
                .required(),
        )
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .timeout(900);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?;
        let n = page_count(&ctx, &f.path).await?;
        let del = page_filter(Some(p.str("pages")?), n, "pages")?;
        let keep: Vec<u32> = (1..=n).filter(|i| !del[(*i - 1) as usize]).collect();
        if keep.is_empty() {
            return Err(ToolError::invalid("pages", "this would delete every page"));
        }
        let out = ctx.output_path(&pdf_name(f, "trimmed"));
        let w = select_pages(&ctx, &f.path, &keep, &out).await?;
        let mut o = ToolOutput::files(vec![ctx.artifact(&out)?])
            .with_data(json!({"deleted": n as usize - keep.len(), "remaining": keep.len()}));
        o.warnings = w;
        Ok(o)
    })
}

/// Batch helper: run qpdf with `args` + input + output for every file.
async fn batch_qpdf(
    ctx: &ToolContext,
    files: &[InputFile],
    suffix: &str,
    args: &[&str],
) -> ToolResult<ToolOutput> {
    let mut arts = vec![];
    let mut warnings = vec![];
    let mut report = vec![];
    for (i, f) in files.iter().enumerate() {
        ctx.check_cancelled()?;
        progress_files(ctx, i, files.len(), &f.name);
        let out = ctx.output_path(&pdf_name(f, suffix));
        let res = qpdf(ctx)?
            .args(args)
            .arg(arg_path(&f.path))
            .arg(&out)
            .run()
            .await
            .map_err(map_qpdf_err("input"))
            .map_err(|e| prefix_err(e, &f.name))?;
        let w = qpdf_warnings(&res);
        report.push(json!({"file": f.name, "warnings": w.len(), "input_size": f.size(), "output_size": std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)}));
        warnings.extend(w.into_iter().map(|w| format!("{}: {w}", f.name)));
        arts.push(ctx.artifact(&out)?);
    }
    let mut o = ToolOutput::files(arts).with_data(json!({"files": report}));
    o.warnings = warnings;
    Ok(o)
}

fn repair() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.repair", "Repair PDF", Category::Pdf)
        .describe("Rebuild damaged PDFs (broken cross-reference tables, bad offsets) into clean, valid files")
        .keywords(&["fix", "recover", "corrupt", "broken", "damaged", "pdf"])
        .param(ParamSpec::files("input").label("PDF files").required().primary())
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        batch_qpdf(
            &ctx,
            p.files("input"),
            "repaired",
            &["--object-streams=preserve"],
        )
        .await
    })
}

fn linearize() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.linearize", "Optimize for Web", Category::Pdf)
        .describe("Linearize PDFs (fast web view) so the first page displays before the whole file has downloaded")
        .keywords(&["linearize", "fast web view", "optimize", "web", "pdf"])
        .param(pdf_inputs())
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        batch_qpdf(&ctx, p.files("input"), "web", &["--linearize"]).await
    })
}

fn flatten() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.flatten", "Flatten Forms & Annotations", Category::Pdf)
        .describe("Bake form field values and annotations into the page content so they can no longer be edited")
        .keywords(&["flatten", "forms", "annotations", "comments", "burn in", "pdf"])
        .param(pdf_inputs())
        .param(
            ParamSpec::choice("which", &[("all", "All annotations"), ("print", "Only those that print"), ("screen", "Only those shown on screen")])
                .default_str("all")
                .advanced(),
        )
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let which = format!("--flatten-annotations={}", p.str_or("which", "all"));
        batch_qpdf(
            &ctx,
            p.files("input"),
            "flattened",
            &["--generate-appearances", &which],
        )
        .await
    })
}

const PERMS: &[(&str, &str)] = &[
    ("print", "Print"),
    ("modify", "Modify content"),
    ("copy", "Copy text & images"),
    ("annotate", "Add comments"),
    ("forms", "Fill in forms"),
    ("assemble", "Assemble (insert, rotate, delete pages)"),
];

fn check_password(param: &str, s: &str) -> ToolResult<()> {
    if s.contains(['\n', '\r', '\0']) {
        return Err(ToolError::invalid(
            param,
            "password must not contain line breaks",
        ));
    }
    Ok(())
}

fn protect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.protect", "Protect PDF", Category::Pdf)
        .describe("Encrypt PDFs with AES-256: require a password to open and/or restrict printing, copying and editing")
        .keywords(&["encrypt", "password", "lock", "secure", "permissions", "aes", "pdf"])
        .param(pdf_inputs())
        .param(ParamSpec::secret("user_password").label("Open password").describe("Required to open the document (leave empty to allow opening without a password)"))
        .param(ParamSpec::secret("owner_password").label("Permissions password").describe("Required to change permissions or remove the protection").required())
        .param(ParamSpec::multi_choice("allow", PERMS).label("Allowed actions").default_value(json!(["print", "copy"])))
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let user = p.text("user_password").to_string();
        let owner = p.str("owner_password")?.to_string();
        check_password("user_password", &user)?;
        check_password("owner_password", &owner)?;
        if owner == user {
            return Err(ToolError::invalid(
                "owner_password",
                "must differ from the open password",
            ));
        }
        let allow = p.list("allow");
        let yn = |k: &str| {
            if allow.iter().any(|a| a == k) {
                "y"
            } else {
                "n"
            }
        };
        let files = p.files("input");
        let mut arts = vec![];
        let mut warnings = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, files.len(), &f.name);
            let out = ctx.output_path(&pdf_name(f, "protected"));
            // Options go through a private qpdf job-JSON file so passwords
            // never appear in the process list (`--job-json-file` is part of
            // qpdf's stable interface, unlike @argfile expansion which some
            // builds lack).
            let argfile = ctx.temp_path(&format!("encrypt-{i}.json"));
            let job = serde_json::json!({
                "inputFile": f.path.to_string_lossy(),
                "outputFile": out.to_string_lossy(),
                "warningExit0": "",
                "encrypt": {
                    "userPassword": user,
                    "ownerPassword": owner,
                    "256bit": {
                        "print": if allow.iter().any(|a| a == "print") { "full" } else { "none" },
                        "modifyOther": yn("modify"),
                        "extract": yn("copy"),
                        "annotate": yn("annotate"),
                        "form": yn("forms"),
                        "assemble": yn("assemble"),
                    }
                }
            });
            write_private(&argfile, job.to_string().as_bytes())?;
            let res = ctx
                .command("qpdf")?
                .arg(format!("--job-json-file={}", argfile.display()))
                .run()
                .await
                .map_err(map_qpdf_err("input"));
            let _ = std::fs::remove_file(&argfile);
            let res = res.map_err(|e| prefix_err(e, &f.name))?;
            warnings.extend(qpdf_warnings(&res));
            arts.push(ctx.artifact(&out)?);
        }
        let mut o = ToolOutput::files(arts).with_data(
            json!({"encryption": "AES-256", "open_password": !user.is_empty(), "allowed": allow}),
        );
        o.warnings = warnings;
        Ok(o)
    })
}

fn write_private(path: &Path, bytes: &[u8]) -> ToolResult<()> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path)?;
    use std::io::Write;
    f.write_all(bytes)?;
    Ok(())
}

fn unlock() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("pdf.unlock", "Remove PDF Password", Category::Pdf)
        .describe("Remove password protection from a document you are authorized to open (the correct password is required)")
        .keywords(&["decrypt", "unlock", "password", "unprotect", "pdf"])
        .param(pdf_inputs())
        .param(ParamSpec::secret("password").label("Password").describe("The document's open or permissions password").required())
        .requires(&["qpdf"])
        .output(OutputKind::Files)
        .progress()
        .timeout(1800);
    async_tool(spec, |ctx, p| async move {
        let pw = p.str("password")?.to_string();
        check_password("password", &pw)?;
        let files = p.files("input");
        let pwfile = ctx.temp_path("password.txt");
        write_private(&pwfile, pw.as_bytes())?;
        let mut arts = vec![];
        let mut warnings = vec![];
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancelled()?;
            progress_files(&ctx, i, files.len(), &f.name);
            let out = ctx.output_path(&pdf_name(f, "unlocked"));
            let res = qpdf(&ctx)?
                .arg(format!("--password-file={}", pwfile.display()))
                .arg("--decrypt")
                .arg(arg_path(&f.path))
                .arg(&out)
                .run()
                .await
                .map_err(map_qpdf_err("password"))
                .map_err(|e| prefix_err(e, &f.name))?;
            warnings.extend(qpdf_warnings(&res));
            arts.push(ctx.artifact(&out)?);
        }
        let _ = std::fs::remove_file(&pwfile);
        let mut o = ToolOutput::files(arts);
        o.warnings = warnings;
        Ok(o)
    })
}
