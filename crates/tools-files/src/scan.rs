//! Folder scanners: sizes, large files, duplicates, empty items, search, manifests.

use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        dir_size(),
        large_files(),
        duplicates(),
        duplicates_uploaded(),
        empty(),
        remove_empty(),
        search(),
        manifest(),
    ]
}

/// (device, inode) identity of a file on Unix, to avoid counting hard links twice.
#[cfg(unix)]
fn file_id(m: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    if m.nlink() > 1 {
        Some((m.dev(), m.ino()))
    } else {
        None
    }
}
#[cfg(not(unix))]
fn file_id(_m: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

fn dir_size() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.dir_size", "Folder Size", Category::Files)
        .describe(
            "Measure a folder's total size and see which subfolders and files take the most space",
        )
        .keywords(&[
            "disk usage",
            "du",
            "folder size",
            "space",
            "storage",
            "what is using space",
        ])
        .param(dir_param("path", "Folder"))
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Data)
        .progress()
        .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let opts = WalkOpts::from_params(p);
        // child name -> (bytes, files, is_dir)
        let mut children: BTreeMap<String, (u64, u64, bool)> = BTreeMap::new();
        let mut seen = HashSet::new();
        let (mut total, mut files, mut dirs) = (0u64, 0u64, 0u64);
        let stats = walk(ctx, &root, &opts, |e| {
            let r = rel(&root, e.path());
            let top = r.split('/').next().unwrap_or("").to_string();
            let ft = e.file_type();
            if ft.is_dir() {
                dirs += 1;
                if e.depth() == 1 {
                    children.entry(top).or_insert((0, 0, true));
                }
            } else if ft.is_file() {
                let m = match e.metadata() {
                    Ok(m) => m,
                    Err(_) => return Ok(()),
                };
                if let Some(id) = file_id(&m) {
                    if !seen.insert(id) {
                        return Ok(());
                    }
                }
                files += 1;
                total += m.len();
                let c = children.entry(top).or_insert((0, 0, e.depth() > 1));
                c.0 += m.len();
                c.1 += 1;
            }
            Ok(())
        })?;
        let mut v: Vec<_> = children.into_iter().collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.1 .0));
        let rows: Vec<Value> = v
            .iter()
            .take(5000)
            .map(|(name, (bytes, n, is_dir))| {
                json!([
                    if *is_dir {
                        format!("{name}/")
                    } else {
                        name.clone()
                    },
                    bytes,
                    human_size(*bytes),
                    n,
                    format!("{:.1}%", *bytes as f64 * 100.0 / total.max(1) as f64)
                ])
            })
            .collect();
        let mut o = ToolOutput::data(json!({
            "path": root.to_string_lossy(),
            "total_size": total,
            "total_size_human": human_size(total),
            "files": files,
            "folders": dirs,
            "table": table(&["Item", "Bytes", "Size", "Files", "Share"], rows),
        }));
        o.warnings = stats.warnings();
        Ok(o)
    })
}

fn large_files() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.large_files", "Find Large Files", Category::Files)
        .describe("Find the biggest files in a folder (recursively) above a minimum size")
        .keywords(&[
            "large files",
            "big files",
            "disk space",
            "cleanup",
            "largest",
        ])
        .param(dir_param("path", "Folder"))
        .param(
            ParamSpec::number("min_size_mb")
                .label("Minimum size (MB)")
                .min(0.0)
                .default_num(100.0),
        )
        .param(
            ParamSpec::integer("limit")
                .label("Show top")
                .range(1.0, 10000.0)
                .default_int(50),
        )
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Data)
        .progress()
        .timeout(3600);
    sync_tool(spec, |ctx, p| {
        use std::cmp::Reverse;
        let root = p.path("path")?;
        let min = (p.num_or("min_size_mb", 100.0) * 1024.0 * 1024.0) as u64;
        let limit = p.int_or("limit", 50).max(1) as usize;
        let opts = WalkOpts::from_params(p);
        let mut heap: std::collections::BinaryHeap<Reverse<(u64, String, String)>> =
            Default::default();
        let mut matched = 0u64;
        let stats = walk(ctx, &root, &opts, |e| {
            if e.file_type().is_file() {
                if let Ok(m) = e.metadata() {
                    if m.len() >= min {
                        matched += 1;
                        let when = m.modified().map(fmt_time).unwrap_or_default();
                        heap.push(Reverse((m.len(), rel(&root, e.path()), when)));
                        if heap.len() > limit {
                            heap.pop();
                        }
                    }
                }
            }
            Ok(())
        })?;
        let mut v: Vec<_> = heap.into_iter().map(|Reverse(x)| x).collect();
        v.sort_by_key(|x| std::cmp::Reverse(x.0));
        let total: u64 = v.iter().map(|x| x.0).sum();
        let rows: Vec<Value> = v
            .into_iter()
            .map(|(s, path, when)| json!([path, s, human_size(s), when]))
            .collect();
        let mut o = ToolOutput::data(json!({
            "matching_files": matched,
            "shown_size": human_size(total),
            "table": table(&["Path", "Bytes", "Size", "Modified"], rows),
        }));
        o.warnings = stats.warnings();
        Ok(o)
    })
}

struct Cand {
    path: PathBuf,
    label: String,
    size: u64,
    id: Option<(u64, u64)>,
}

fn partial_hash(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut got = 0;
    while got < buf.len() {
        let n = f.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(*blake3::hash(&buf[..got]).as_bytes())
}

/// Group candidates by size → partial hash → full BLAKE3.
fn find_duplicates(ctx: &ToolContext, cands: Vec<Cand>) -> ToolResult<ToolOutput> {
    let mut by_size: HashMap<u64, Vec<Cand>> = HashMap::new();
    for c in cands {
        by_size.entry(c.size).or_default().push(c);
    }
    let groups_by_size: Vec<Vec<Cand>> = by_size.into_values().filter(|g| g.len() > 1).collect();
    let to_hash: u64 = groups_by_size
        .iter()
        .map(|g| g.len() as u64)
        .sum::<u64>()
        .max(1);
    let mut hashed = 0u64;
    let mut dups: Vec<(String, u64, Vec<String>, usize)> = vec![];
    let mut errors = 0;
    for g in groups_by_size {
        let mut by_partial: HashMap<[u8; 32], Vec<Cand>> = HashMap::new();
        for c in g {
            hashed += 1;
            if hashed.is_multiple_of(32) {
                ctx.check_cancelled()?;
                ctx.progress(hashed as f32 / to_hash as f32, c.label.clone());
            }
            match partial_hash(&c.path) {
                Ok(h) => by_partial.entry(h).or_default().push(c),
                Err(_) => errors += 1,
            }
        }
        for (_, pg) in by_partial.into_iter().filter(|(_, v)| v.len() > 1) {
            let mut by_full: HashMap<String, Vec<Cand>> = HashMap::new();
            for c in pg {
                let h = if c.size <= 64 * 1024 {
                    String::from("partial")
                } else {
                    blake3_file(ctx, &c.path)?
                };
                by_full.entry(h).or_default().push(c);
            }
            for (h, fg) in by_full.into_iter().filter(|(_, v)| v.len() > 1) {
                let size = fg[0].size;
                let hash = if h == "partial" {
                    blake3_file(ctx, &fg[0].path)?
                } else {
                    h
                };
                let unique_inodes = {
                    let mut ids = HashSet::new();
                    fg.iter()
                        .filter(|c| c.id.map(|i| ids.insert(i)).unwrap_or(true))
                        .count()
                };
                let mut paths: Vec<String> = fg.into_iter().map(|c| c.label).collect();
                paths.sort();
                dups.push((hash, size, paths, unique_inodes));
            }
        }
    }
    dups.sort_by(|a, b| {
        (b.1 * (b.3 as u64 - 1))
            .cmp(&(a.1 * (a.3 as u64 - 1)))
            .then(a.2.cmp(&b.2))
    });
    let wasted: u64 = dups.iter().map(|d| d.1 * (d.3 as u64 - 1)).sum();
    let mut rows = vec![];
    for (i, (hash, size, paths, _)) in dups.iter().enumerate() {
        for path in paths {
            if rows.len() < 50_000 {
                rows.push(json!([i + 1, path, size, human_size(*size), &hash[..16]]));
            }
        }
    }
    let groups: Vec<Value> = dups
        .iter()
        .take(5000)
        .map(|(h, s, p, u)| json!({"hash": h, "size": s, "files": p, "hard_linked": p.len() - u}))
        .collect();
    let mut o = ToolOutput::data(json!({
        "groups": dups.len(),
        "duplicate_files": dups.iter().map(|d| d.2.len() - 1).sum::<usize>(),
        "wasted_space": wasted,
        "wasted_space_human": human_size(wasted),
        "table": table(&["Group", "Path", "Bytes", "Size", "BLAKE3"], rows),
        "duplicate_groups": groups,
    }));
    if errors > 0 {
        o = o.warn(format!("{errors} files could not be read"));
    }
    Ok(o)
}

fn duplicates() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.duplicates", "Find Duplicate Files", Category::Files)
        .describe("Find files with identical content in a folder (size, then BLAKE3 hash) and how much space they waste; nothing is deleted")
        .keywords(&["duplicates", "dupes", "same files", "identical", "dedupe", "cleanup", "wasted space"])
        .param(dir_param("path", "Folder"))
        .param(ParamSpec::number("min_size_kb").label("Ignore files smaller than (KB)").min(0.0).default_num(1.0))
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Data)
        .progress()
        .timeout(6 * 3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let min = (p.num_or("min_size_kb", 1.0) * 1024.0) as u64;
        let opts = WalkOpts::from_params(p);
        let mut cands = vec![];
        let stats = walk(ctx, &root, &opts, |e| {
            if e.file_type().is_file() {
                if let Ok(m) = e.metadata() {
                    if m.len() >= min.max(1) {
                        cands.push(Cand {
                            path: e.path().to_path_buf(),
                            label: rel(&root, e.path()),
                            size: m.len(),
                            id: file_id(&m),
                        });
                    }
                }
            }
            Ok(())
        })?;
        let mut o = find_duplicates(ctx, cands)?;
        o.warnings.extend(stats.warnings());
        Ok(o)
    })
}

fn duplicates_uploaded() -> Arc<dyn Tool> {
    let spec = ToolSpec::new(
        "files.duplicates_uploaded",
        "Find Duplicates Among Files",
        Category::Files,
    )
    .describe("Find which of the given files have identical content (BLAKE3)")
    .keywords(&["duplicates", "identical", "same content", "dedupe"])
    .param(
        ParamSpec::files("input")
            .label("Files")
            .required()
            .primary(),
    )
    .output(OutputKind::Data)
    .progress()
    .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let cands = p
            .files("input")
            .iter()
            .map(|f| Cand {
                path: f.path.clone(),
                label: f.name.clone(),
                size: f.size(),
                id: None,
            })
            .collect();
        find_duplicates(ctx, cands)
    })
}

struct EmptyScan {
    files: Vec<(String, PathBuf)>,
    dirs: Vec<(String, PathBuf, usize)>,
    stats: WalkStats,
}

/// Empty files, and folders that contain nothing but (possibly nested) empty folders.
fn scan_empty(ctx: &ToolContext, root: &Path, opts: &WalkOpts) -> ToolResult<EmptyScan> {
    let mut files = vec![];
    let mut all_dirs: Vec<(String, PathBuf, usize)> = vec![];
    let mut nonempty: HashSet<PathBuf> = HashSet::new();
    let stats = walk(ctx, root, opts, |e| {
        let ft = e.file_type();
        if ft.is_dir() {
            all_dirs.push((rel(root, e.path()), e.path().to_path_buf(), e.depth()));
        } else {
            if ft.is_file() && e.metadata().map(|m| m.len() == 0).unwrap_or(false) {
                files.push((rel(root, e.path()), e.path().to_path_buf()));
            }
            // Anything that is not a directory makes all ancestors non-empty.
            let mut cur = e.path().parent();
            while let Some(d) = cur {
                if d == root || !nonempty.insert(d.to_path_buf()) {
                    break;
                }
                cur = d.parent();
            }
        }
        Ok(())
    })?;
    if stats.truncated {
        return Err(ToolError::limit(
            "too many entries to decide which folders are empty; raise 'max_entries'",
        ));
    }
    let dirs = all_dirs
        .into_iter()
        .filter(|(_, p, _)| !nonempty.contains(p))
        .collect();
    Ok(EmptyScan { files, dirs, stats })
}

fn empty_params(spec: ToolSpec) -> ToolSpec {
    spec.param(dir_param("path", "Folder"))
        .param(
            ParamSpec::choice(
                "kind",
                &[
                    ("both", "Empty files and folders"),
                    ("files", "Empty files only"),
                    ("dirs", "Empty folders only"),
                ],
            )
            .default_str("both"),
        )
        .params(walk_params())
}

fn empty() -> Arc<dyn Tool> {
    let spec = empty_params(
        ToolSpec::new("files.empty", "Find Empty Files & Folders", Category::Files)
            .describe("List zero-byte files and folders that contain nothing (or only other empty folders)")
            .keywords(&["empty", "zero byte", "empty folders", "cleanup"]),
    )
    .permission(Permission::HostFsRead)
    .output(OutputKind::Data)
    .progress()
    .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let scan = scan_empty(ctx, &root, &WalkOpts::from_params(p))?;
        let kind = p.str_or("kind", "both");
        let mut rows = vec![];
        if kind != "dirs" {
            rows.extend(scan.files.iter().map(|(r, _)| json!([r, "empty file"])));
        }
        if kind != "files" {
            rows.extend(
                scan.dirs
                    .iter()
                    .map(|(r, _, _)| json!([format!("{r}/"), "empty folder"])),
            );
        }
        rows.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
        let mut o = ToolOutput::data(
            json!({"empty_files": scan.files.len(), "empty_folders": scan.dirs.len(), "table": table(&["Path", "Type"], rows)}),
        );
        o.warnings = scan.stats.warnings();
        Ok(o)
    })
}

fn remove_empty() -> Arc<dyn Tool> {
    let spec = empty_params(
        ToolSpec::new(
            "files.remove_empty",
            "Remove Empty Files & Folders",
            Category::Files,
        )
        .describe(
            "Delete zero-byte files and empty folders; shows a preview unless 'apply' is enabled",
        )
        .keywords(&["delete empty", "remove empty folders", "cleanup", "prune"]),
    )
    .param(
        ParamSpec::boolean("apply")
            .label("Delete now")
            .describe("Off = preview only. On = permanently delete the listed items."),
    )
    .permission(Permission::HostFsRead)
    .permission(Permission::HostFsWrite)
    .destructive()
    .output(OutputKind::Data)
    .progress()
    .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let scan = scan_empty(ctx, &root, &WalkOpts::from_params(p))?;
        let kind = p.str_or("kind", "both");
        let apply = p.bool("apply");
        let mut rows = vec![];
        let (mut removed, mut failed) = (0, 0);
        let mut act = |rel: String, path: &Path, is_dir: bool| {
            let status = if !apply {
                "would delete".to_string()
            } else {
                // Re-validate at deletion time: still inside the allowed
                // roots, not a link, and still empty.
                let ok = path
                    .parent()
                    .map(|par| ctx.fs_policy().check("path", par).is_ok())
                    .unwrap_or(false);
                let meta = std::fs::symlink_metadata(path);
                let res = match (ok, meta) {
                    (false, _) => Err("outside the allowed folders".to_string()),
                    (true, Ok(m)) if is_dir && m.is_dir() => {
                        std::fs::remove_dir(path).map_err(|e| e.to_string())
                    }
                    (true, Ok(m)) if !is_dir && m.is_file() && m.len() == 0 => {
                        std::fs::remove_file(path).map_err(|e| e.to_string())
                    }
                    (true, Ok(_)) => Err("changed since the scan; skipped".to_string()),
                    (true, Err(e)) => Err(e.to_string()),
                };
                match res {
                    Ok(()) => {
                        removed += 1;
                        "deleted".to_string()
                    }
                    Err(e) => {
                        failed += 1;
                        format!("failed: {e}")
                    }
                }
            };
            rows.push(json!([
                if is_dir { format!("{rel}/") } else { rel },
                if is_dir { "folder" } else { "file" },
                status
            ]));
        };
        if kind != "dirs" {
            for (r, path) in scan.files.iter() {
                act(r.clone(), path, false);
            }
        }
        if kind != "files" {
            let mut dirs = scan.dirs.clone();
            dirs.sort_by_key(|x| std::cmp::Reverse(x.2)); // deepest first
            for (r, path, _) in dirs {
                act(r, &path, true);
            }
        }
        let count = rows.len();
        let mut o = ToolOutput::data(json!({
            "applied": apply,
            "items": count,
            "deleted": removed,
            "failed": failed,
            "table": table(&["Path", "Type", "Status"], rows),
        }));
        if !apply && count > 0 {
            o = o.warn(format!(
                "Preview only: {count} item(s) would be deleted. Enable 'apply' to delete them."
            ));
        }
        o.warnings.extend(scan.stats.warnings());
        Ok(o)
    })
}

fn search() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.search", "Search Files", Category::Files)
        .describe("Search a folder for files by name (glob or regex) and/or by content (regex), with line numbers")
        .keywords(&["find", "grep", "search", "locate", "ripgrep", "content", "filename"])
        .param(dir_param("path", "Folder"))
        .param(ParamSpec::text("name").label("File name pattern").describe("Glob like '*.rs, *.toml' (or a regex when 'name_regex' is on)").placeholder("*.txt"))
        .param(ParamSpec::boolean("name_regex").label("Name pattern is a regex").advanced())
        .param(ParamSpec::text("content").label("Content (regex)").placeholder("TODO|FIXME"))
        .param(ParamSpec::boolean("case_sensitive").label("Case sensitive"))
        .param(ParamSpec::number("max_file_size_mb").label("Skip files larger than (MB)").min(0.001).default_num(10.0).advanced())
        .param(ParamSpec::integer("max_results").label("Maximum results").range(1.0, 100000.0).default_int(1000).advanced())
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Data)
        .progress()
        .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let ci = !p.bool("case_sensitive");
        let name = p.opt_str("name").map(str::trim).filter(|s| !s.is_empty());
        let content = p.opt_str("content");
        if name.is_none() && content.is_none() {
            return Err(ToolError::invalid(
                "name",
                "give a file name pattern and/or a content pattern",
            ));
        }
        enum NameMatch {
            Glob(globset::GlobSet),
            Re(regex::Regex),
        }
        let name_m = match name {
            None => None,
            Some(n) if p.bool("name_regex") => Some(NameMatch::Re(
                regex::RegexBuilder::new(n)
                    .case_insensitive(ci)
                    .size_limit(1 << 20)
                    .build()
                    .bad_input("name")?,
            )),
            Some(n) => {
                let mut b = globset::GlobSetBuilder::new();
                for g in n.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
                    b.add(
                        globset::GlobBuilder::new(g)
                            .case_insensitive(ci)
                            .literal_separator(false)
                            .build()
                            .bad_input("name")?,
                    );
                }
                Some(NameMatch::Glob(b.build().bad_input("name")?))
            }
        };
        let re = match content {
            Some(c) => Some(
                regex::RegexBuilder::new(c)
                    .case_insensitive(ci)
                    .size_limit(1 << 20)
                    .build()
                    .bad_input("content")?,
            ),
            None => None,
        };
        let max_size = (p.num_or("max_file_size_mb", 10.0) * 1024.0 * 1024.0) as u64;
        let max_results = p.int_or("max_results", 1000).max(1) as usize;
        let mut opts = WalkOpts::from_params(p);
        opts.truncate_ok = true;
        let mut rows = vec![];
        let (mut files_matched, mut skipped_big, mut skipped_bin) = (0u64, 0u64, 0u64);
        let mut truncated = false;
        let stats = walk(ctx, &root, &opts, |e| {
            if truncated || !e.file_type().is_file() {
                return Ok(());
            }
            let fname = e.file_name().to_string_lossy();
            let r = rel(&root, e.path());
            let name_ok = match &name_m {
                None => true,
                Some(NameMatch::Glob(g)) => g.is_match(fname.as_ref()) || g.is_match(&r),
                Some(NameMatch::Re(x)) => x.is_match(&fname),
            };
            if !name_ok {
                return Ok(());
            }
            let Some(re) = &re else {
                files_matched += 1;
                let m = e.metadata().ok();
                rows.push(json!([
                    r,
                    Value::Null,
                    m.as_ref().map(|m| human_size(m.len())),
                    m.and_then(|m| m.modified().ok()).map(fmt_time)
                ]));
                truncated = rows.len() >= max_results;
                return Ok(());
            };
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if size > max_size {
                skipped_big += 1;
                return Ok(());
            }
            let Ok(bytes) = std::fs::read(e.path()) else {
                return Ok(());
            };
            if looks_binary(&bytes[..bytes.len().min(8192)]) {
                skipped_bin += 1;
                return Ok(());
            }
            let text = String::from_utf8_lossy(&bytes);
            let mut any = false;
            for (i, line) in text.lines().enumerate() {
                if re.is_match(line) {
                    any = true;
                    let shown: String = line.trim().chars().take(300).collect();
                    rows.push(json!([r, i + 1, shown, Value::Null]));
                    if rows.len() >= max_results {
                        truncated = true;
                        break;
                    }
                }
            }
            if any {
                files_matched += 1;
            }
            Ok(())
        })?;
        let columns: &[&str] = if re.is_some() {
            &["Path", "Line", "Text", ""]
        } else {
            &["Path", "", "Size", "Modified"]
        };
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|r| {
                let a = r.as_array().cloned().unwrap_or_default();
                if re.is_some() {
                    json!([a[0], a[1], a[2]])
                } else {
                    json!([a[0], a[2], a[3]])
                }
            })
            .collect();
        let columns: Vec<&str> = columns.iter().copied().filter(|c| !c.is_empty()).collect();
        let mut o = ToolOutput::data(json!({
            "files_matched": files_matched,
            "results": rows.len(),
            "skipped_large": skipped_big,
            "skipped_binary": skipped_bin,
            "table": {"columns": columns, "rows": rows},
        }));
        o.warnings = stats.warnings();
        if truncated {
            o.warnings
                .push(format!("stopped after {max_results} results"));
        }
        Ok(o)
    })
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn manifest() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.manifest", "File Manifest", Category::Files)
        .describe("Create a CSV or JSON inventory of a folder: path, size, modification time, MIME type and optional hash")
        .keywords(&["inventory", "manifest", "file list", "catalog", "csv", "listing", "index"])
        .param(dir_param("path", "Folder"))
        .param(ParamSpec::choice("format", &[("csv", "CSV"), ("json", "JSON")]).default_str("csv"))
        .param(ParamSpec::choice("hash", &[("none", "No hash"), ("sha256", "SHA-256"), ("blake3", "BLAKE3"), ("md5", "MD5"), ("xxh3", "XXH3")]).default_str("none"))
        .param(ParamSpec::boolean("include_dirs").label("Include folders").advanced())
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let opts = WalkOpts::from_params(p);
        let algo = p.str_or("hash", "none").to_string();
        let dirs = p.bool("include_dirs");
        let mut items: Vec<(String, PathBuf, bool, u64, String)> = vec![];
        let stats = walk(ctx, &root, &opts, |e| {
            let ft = e.file_type();
            if ft.is_file() || (dirs && ft.is_dir()) {
                let m = e.metadata().ok();
                items.push((
                    rel(&root, e.path()),
                    e.path().to_path_buf(),
                    ft.is_dir(),
                    m.as_ref()
                        .map(|m| if m.is_file() { m.len() } else { 0 })
                        .unwrap_or(0),
                    m.and_then(|m| m.modified().ok())
                        .map(fmt_time_utc)
                        .unwrap_or_default(),
                ));
            }
            Ok(())
        })?;
        items.sort_by(|a, b| a.0.cmp(&b.0));
        let n = items.len().max(1);
        let mut records = vec![];
        for (i, (r, path, is_dir, size, mtime)) in items.iter().enumerate() {
            if i % 64 == 0 {
                ctx.check_cancelled()?;
                ctx.progress(i as f32 / n as f32, r.clone());
            }
            let mime = if *is_dir {
                "inode/directory".to_string()
            } else {
                dogubako_core::util::mime_for_path(path)
            };
            let hash = if algo != "none" && !is_dir {
                hash_file(ctx, path, std::slice::from_ref(&algo))
                    .ok()
                    .map(|mut v| v.remove(0))
            } else {
                None
            };
            records.push((r.clone(), *size, mtime.clone(), mime, hash));
        }
        let base = root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "folder".into());
        let art = if p.str_or("format", "csv") == "json" {
            let v: Vec<Value> = records.iter().map(|(r, s, m, mi, h)| json!({"path": r, "size": s, "modified": m, "mime": mi, "hash": h})).collect();
            ctx.write_output(
                &format!("{base}-manifest.json"),
                serde_json::to_string_pretty(&json!({"root": base, "hash": algo, "files": v}))?
                    .as_bytes(),
            )?
        } else {
            let mut s = String::from(if algo != "none" {
                "path,size,modified,mime,hash\n"
            } else {
                "path,size,modified,mime\n"
            });
            for (r, size, m, mi, h) in &records {
                s.push_str(&format!(
                    "{},{},{},{}",
                    csv_field(r),
                    size,
                    m,
                    csv_field(mi)
                ));
                if algo != "none" {
                    s.push(',');
                    s.push_str(h.as_deref().unwrap_or(""));
                }
                s.push('\n');
            }
            ctx.write_output(&format!("{base}-manifest.csv"), s.as_bytes())?
        };
        let total: u64 = records.iter().map(|r| r.1).sum();
        let preview: Vec<Value> = records
            .iter()
            .take(200)
            .map(|(r, s, m, mi, _)| json!([r, s, m, mi]))
            .collect();
        let mut o = ToolOutput::files(vec![art]).with_data(json!({"entries": records.len(), "total_size": human_size(total), "table": table(&["Path", "Size", "Modified", "MIME"], preview)}));
        o.warnings = stats.warnings();
        Ok(o)
    })
}
