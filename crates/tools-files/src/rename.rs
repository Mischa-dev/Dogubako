//! Batch rename with preview, collision detection and safe two-phase apply.

use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![batch_rename()]
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub from: PathBuf,
    pub to: PathBuf,
    pub status: String,
}

pub struct Rules {
    pub template: String,
    pub find: Option<regex::Regex>,
    pub replace: String,
    pub case: String,
    pub start: i64,
    pub step: i64,
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut up = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if up {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            up = false;
        } else {
            out.push(c);
            up = true;
        }
    }
    out
}

/// Render the new file name for `path` (the `index`-th file in order).
pub fn render(rules: &Rules, path: &Path, index: usize) -> Result<String, String> {
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let (stem, ext) = match fname.rfind('.') {
        Some(i) if i > 0 => (fname[..i].to_string(), fname[i..].to_string()),
        _ => (fname.clone(), String::new()),
    };
    let mut name = match &rules.find {
        Some(re) => re.replace_all(&stem, rules.replace.as_str()).to_string(),
        None => stem.clone(),
    };
    name = match rules.case.as_str() {
        "lower" => name.to_lowercase(),
        "upper" => name.to_uppercase(),
        "title" => title_case(&name),
        _ => name,
    };
    let n = rules.start + rules.step * index as i64;
    let date = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%Y-%m-%d").to_string()
        })
        .ok();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let re =
        regex::Regex::new(r"\{(name|ext|n|date|parent|original)(?::(0?)(\d{1,2}))?\}").unwrap();
    let mut bad = None;
    let out = re.replace_all(&rules.template, |c: &regex::Captures| match &c[1] {
        "name" => name.clone(),
        "ext" => ext.clone(),
        "original" => fname.clone(),
        "parent" => parent.clone(),
        "date" => date.clone().unwrap_or_default(),
        "n" => {
            let width: usize = c
                .get(3)
                .map(|m| m.as_str().parse().unwrap_or(0))
                .unwrap_or(0);
            if c.get(2).map(|m| m.as_str() == "0").unwrap_or(false) || c.get(3).is_some() {
                format!("{n:0width$}")
            } else {
                n.to_string()
            }
        }
        _ => {
            bad = Some(c[0].to_string());
            String::new()
        }
    });
    if let Some(b) = bad {
        return Err(format!("unknown placeholder {b}"));
    }
    let out = out.to_string();
    if let Some(u) = regex::Regex::new(r"\{[a-z]+[^}]*\}").unwrap().find(&out) {
        return Err(format!("unknown placeholder {}", u.as_str()));
    }
    Ok(out)
}

pub fn validate_name(n: &str) -> Result<(), &'static str> {
    if n.is_empty() || n.trim().is_empty() {
        return Err("empty name");
    }
    if n == "." || n == ".." {
        return Err("reserved name");
    }
    if n.contains(['/', '\\', '\0']) || n.chars().any(|c| c.is_control()) {
        return Err("contains a path separator or control character");
    }
    if n.len() > 255 {
        return Err("name longer than 255 bytes");
    }
    Ok(())
}

/// Compute the rename plan and mark collisions / conflicts.
pub fn plan(rules: &Rules, sources: &[PathBuf]) -> Vec<Plan> {
    let src_set: HashSet<&Path> = sources.iter().map(|p| p.as_path()).collect();
    let mut plans: Vec<Plan> = sources
        .iter()
        .enumerate()
        .map(|(i, src)| match render(rules, src, i) {
            Err(e) => Plan {
                from: src.clone(),
                to: src.clone(),
                status: format!("invalid: {e}"),
            },
            Ok(n) => match validate_name(&n) {
                Err(e) => Plan {
                    from: src.clone(),
                    to: src.clone(),
                    status: format!("invalid: {e}"),
                },
                Ok(()) => {
                    let to = src.parent().unwrap_or(Path::new(".")).join(&n);
                    Plan {
                        status: if to == *src {
                            "unchanged".into()
                        } else {
                            "ok".into()
                        },
                        from: src.clone(),
                        to,
                    }
                }
            },
        })
        .collect();
    // Two sources mapping onto the same target (case-insensitively, to be
    // safe on case-insensitive filesystems).
    let mut targets: HashMap<String, usize> = HashMap::new();
    for p in &plans {
        if !p.status.starts_with("invalid") {
            *targets
                .entry(p.to.to_string_lossy().to_lowercase())
                .or_default() += 1;
        }
    }
    for p in plans.iter_mut() {
        if p.status.starts_with("invalid") {
            continue;
        }
        if targets[&p.to.to_string_lossy().to_lowercase()] > 1 {
            p.status = "collision: several files would get this name".into();
            continue;
        }
        if p.status == "ok" {
            let case_only =
                p.to.to_string_lossy().to_lowercase() == p.from.to_string_lossy().to_lowercase();
            // A target that exists and is not itself being renamed away.
            if std::fs::symlink_metadata(&p.to).is_ok()
                && !case_only
                && !src_set.contains(p.to.as_path())
            {
                p.status = "conflict: a file with this name already exists".into();
            }
        }
    }
    // A source that is the target of another rename but stays put itself.
    let staying: HashSet<PathBuf> = plans
        .iter()
        .filter(|p| p.status != "ok")
        .map(|p| p.from.clone())
        .collect();
    for p in plans.iter_mut() {
        if p.status == "ok" && staying.contains(&p.to) {
            p.status = "conflict: target is a file that is not being renamed".into();
        }
    }
    plans
}

fn temp_name(dir: &Path, i: usize) -> PathBuf {
    dir.join(format!(
        ".dgb-rename-{}-{i}",
        dogubako_core::util::random_id(6)
    ))
}

/// Two-phase rename (source → temp → target) so chains and swaps work;
/// rolls back on failure. Never overwrites.
pub fn apply(ctx: &ToolContext, plans: &[Plan]) -> ToolResult<usize> {
    let todo: Vec<&Plan> = plans.iter().filter(|p| p.status == "ok").collect();
    let mut moved: Vec<(PathBuf, PathBuf)> = vec![]; // (original, temp)
    let rollback = |moved: &[(PathBuf, PathBuf)]| {
        for (orig, tmp) in moved.iter().rev() {
            let _ = std::fs::rename(tmp, orig);
        }
    };
    for (i, p) in todo.iter().enumerate() {
        let dir = p.from.parent().unwrap_or(Path::new("."));
        let tmp = temp_name(dir, i);
        if let Err(e) = std::fs::rename(&p.from, &tmp) {
            rollback(&moved);
            return Err(ToolError::failed(format!(
                "renaming '{}' failed: {e}; no files were changed",
                p.from.display()
            )));
        }
        moved.push((p.from.clone(), tmp));
    }
    let mut done: Vec<(PathBuf, PathBuf)> = vec![]; // (target, temp)
    for (k, p) in todo.iter().enumerate() {
        ctx.check_cancelled().ok();
        let tmp = &moved[k].1;
        let res = ctx.fs_policy().check_new("path", &p.to).and_then(|target| {
            if std::fs::symlink_metadata(&target).is_ok() {
                return Err(ToolError::failed(format!(
                    "'{}' appeared while renaming; refusing to overwrite",
                    target.display()
                )));
            }
            std::fs::rename(tmp, &target).map_err(|e| {
                ToolError::failed(format!("renaming to '{}' failed: {e}", target.display()))
            })?;
            Ok(target)
        });
        match res {
            Ok(target) => done.push((target, tmp.clone())),
            Err(e) => {
                for (target, tmp) in done.iter().rev() {
                    let _ = std::fs::rename(target, tmp);
                }
                rollback(&moved);
                return Err(match e {
                    ToolError::Failed { message } => {
                        ToolError::failed(format!("{message}; all changes were rolled back"))
                    }
                    other => other,
                });
            }
        }
    }
    Ok(todo.len())
}

fn batch_rename() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.batch_rename", "Batch Rename", Category::Files)
        .describe("Rename many files at once with templates ({name}, {ext}, {n:03}, {date}), regex find/replace, case changes and numbering; previews by default")
        .keywords(&["rename", "batch rename", "bulk rename", "mass rename", "numbering", "regex", "files"])
        .param(ParamSpec::path("path", PathKind::Dir).label("Folder").describe("Rename the files directly inside this folder").primary())
        .param(ParamSpec::textarea("files").label("Or file paths").describe("One file path per line (instead of a folder)").advanced())
        .param(ParamSpec::text("filter").label("Only files matching").describe("Glob, e.g. '*.jpg' (folder mode)").placeholder("*.jpg"))
        .param(ParamSpec::text("template").label("New name").describe("Placeholders: {name} (name without extension), {ext} (extension incl. dot), {n} / {n:03} (counter), {date} (modified date), {parent}, {original}").default_str("{name}{ext}"))
        .param(ParamSpec::text("find").label("Find (regex)").describe("Applied to {name}"))
        .param(ParamSpec::text("replace").label("Replace with").describe("Use $1, $2 for capture groups"))
        .param(ParamSpec::choice("case", &[("keep", "Keep"), ("lower", "lowercase"), ("upper", "UPPERCASE"), ("title", "Title Case")]).default_str("keep"))
        .param(ParamSpec::integer("start").label("Counter start").default_int(1).advanced())
        .param(ParamSpec::integer("step").label("Counter step").default_int(1).advanced())
        .param(ParamSpec::choice("sort", &[("name", "Name"), ("modified", "Modified time"), ("size", "Size")]).label("Numbering order").default_str("name").advanced())
        .param(ParamSpec::boolean("apply").label("Rename now").describe("Off = preview only. On = perform the renames (never overwrites)."))
        .permission(Permission::HostFsRead)
        .permission(Permission::HostFsWrite)
        .destructive()
        .output(OutputKind::Data)
        .timeout(1800);
    sync_tool(spec, |ctx, p| {
        const MAX_FILES: usize = 100_000;
        let mut sources: Vec<PathBuf> = vec![];
        let mut warnings = vec![];
        if let Some(dir) = p.opt_path("path") {
            let filter = match p.opt_str("filter") {
                Some(g) => Some(
                    globset::GlobBuilder::new(g)
                        .case_insensitive(true)
                        .build()
                        .bad_input("filter")?
                        .compile_matcher(),
                ),
                None => None,
            };
            for e in std::fs::read_dir(&dir)? {
                let e = e?;
                let ft = e.file_type()?;
                if ft.is_symlink() {
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                if let Some(f) = &filter {
                    if !f.is_match(e.file_name()) {
                        continue;
                    }
                }
                sources.push(e.path());
                if sources.len() > MAX_FILES {
                    return Err(ToolError::limit(format!("more than {MAX_FILES} files")));
                }
            }
        }
        for line in p
            .text("files")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            let raw = Path::new(line);
            let meta = std::fs::symlink_metadata(raw)
                .map_err(|e| ToolError::invalid("files", format!("'{line}': {e}")))?;
            if meta.file_type().is_symlink() {
                warnings.push(format!("'{line}' is a symbolic link; skipped"));
                continue;
            }
            let canon = ctx.fs_policy().check("files", raw)?;
            if !canon.is_file() {
                return Err(ToolError::invalid(
                    "files",
                    format!("'{line}' is not a file"),
                ));
            }
            sources.push(canon);
            if sources.len() > MAX_FILES {
                return Err(ToolError::limit(format!("more than {MAX_FILES} files")));
            }
        }
        if p.opt_path("path").is_none() && p.text("files").trim().is_empty() {
            return Err(ToolError::invalid(
                "path",
                "choose a folder or list file paths",
            ));
        }
        let mut seen = HashSet::new();
        sources.retain(|s| seen.insert(s.clone()));
        if sources.is_empty() {
            return Err(ToolError::invalid("filter", "no files to rename"));
        }
        match p.str_or("sort", "name") {
            "modified" => sources.sort_by_key(|s| {
                (
                    std::fs::metadata(s).and_then(|m| m.modified()).ok(),
                    s.clone(),
                )
            }),
            "size" => sources.sort_by_key(|s| {
                (
                    std::fs::metadata(s).map(|m| m.len()).unwrap_or(0),
                    s.clone(),
                )
            }),
            _ => sources.sort_by(|a, b| {
                a.to_string_lossy()
                    .to_lowercase()
                    .cmp(&b.to_string_lossy().to_lowercase())
            }),
        }
        let rules = Rules {
            template: p.str_or("template", "{name}{ext}").to_string(),
            find: match p.opt_str("find") {
                Some(f) => Some(
                    regex::RegexBuilder::new(f)
                        .size_limit(1 << 20)
                        .build()
                        .bad_input("find")?,
                ),
                None => None,
            },
            replace: p.text("replace").to_string(),
            case: p.str_or("case", "keep").to_string(),
            start: p.int_or("start", 1),
            step: p.int_or("step", 1),
        };
        let plans = plan(&rules, &sources);
        let problems: Vec<&Plan> = plans
            .iter()
            .filter(|x| x.status != "ok" && x.status != "unchanged")
            .collect();
        let changes = plans.iter().filter(|x| x.status == "ok").count();
        let apply_now = p.bool("apply");
        let mut renamed = 0;
        if apply_now {
            if !problems.is_empty() {
                let list: Vec<String> = problems
                    .iter()
                    .take(10)
                    .map(|x| {
                        format!(
                            "{}: {}",
                            x.from.file_name().unwrap_or_default().to_string_lossy(),
                            x.status
                        )
                    })
                    .collect();
                return Err(ToolError::invalid(
                    "template",
                    format!(
                        "{} problem(s), nothing was renamed: {}",
                        problems.len(),
                        list.join("; ")
                    ),
                ));
            }
            renamed = apply(ctx, &plans)?;
        }
        let rows: Vec<Value> = plans
            .iter()
            .take(20_000)
            .map(|x| {
                let status = if apply_now && x.status == "ok" {
                    "renamed".to_string()
                } else {
                    x.status.clone()
                };
                json!([
                    x.from.file_name().unwrap_or_default().to_string_lossy(),
                    x.to.file_name().unwrap_or_default().to_string_lossy(),
                    status,
                    x.from.parent().map(|d| d.to_string_lossy().to_string())
                ])
            })
            .collect();
        let mut o = ToolOutput::data(json!({
            "applied": apply_now,
            "files": plans.len(),
            "changes": changes,
            "renamed": renamed,
            "problems": problems.len(),
            "table": table(&["Current name", "New name", "Status", "Folder"], rows),
        }));
        if !apply_now {
            if !problems.is_empty() {
                o = o.warn(format!(
                    "{} file(s) cannot be renamed as configured (see Status)",
                    problems.len()
                ));
            }
            if changes > 0 {
                o = o.warn(format!("Preview only: {changes} file(s) would be renamed. Enable 'apply' to rename them."));
            }
        }
        o.warnings.extend(warnings);
        Ok(o)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rules(t: &str) -> Rules {
        Rules {
            template: t.into(),
            find: None,
            replace: String::new(),
            case: "keep".into(),
            start: 1,
            step: 1,
        }
    }
    #[test]
    fn renders() {
        let p = Path::new("/x/photos/IMG_0042.JPG");
        assert_eq!(render(&rules("{name}{ext}"), p, 0).unwrap(), "IMG_0042.JPG");
        assert_eq!(
            render(&rules("trip-{n:03}{ext}"), p, 4).unwrap(),
            "trip-005.JPG"
        );
        assert_eq!(render(&rules("{parent}_{n}"), p, 0).unwrap(), "photos_1");
        let mut r = rules("{name}{ext}");
        r.find = Some(regex::Regex::new(r"IMG_(\d+)").unwrap());
        r.replace = "photo-$1".into();
        r.case = "upper".into();
        assert_eq!(render(&r, p, 0).unwrap(), "PHOTO-0042.JPG");
        assert!(render(&rules("{bogus}"), p, 0).is_err());
        assert_eq!(title_case("hello wORLD-foo"), "Hello World-Foo");
        assert!(validate_name("a/b").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name("ok.txt").is_ok());
    }
}
