//! File and folder comparison.

use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::BTreeMap;
use std::io::Read;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![compare(), compare_dirs()]
}

const MAX_TEXT_DIFF: u64 = 8 * 1024 * 1024;

fn compare() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.compare", "Compare Files", Category::Files)
        .describe("Check whether two files are identical; shows the first differing byte, and a line diff for text files")
        .keywords(&["compare", "diff", "identical", "cmp", "difference", "same file"])
        .param(ParamSpec::file("left").label("First file").required().primary())
        .param(ParamSpec::file("right").label("Second file").required().primary())
        .output(OutputKind::Data)
        .ui("diff")
        .timeout(1800);
    sync_tool(spec, |ctx, p| {
        let a = p.file("left")?;
        let b = p.file("right")?;
        let (sa, sb) = (a.size(), b.size());
        let mut fa = std::io::BufReader::new(std::fs::File::open(&a.path)?);
        let mut fb = std::io::BufReader::new(std::fs::File::open(&b.path)?);
        let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
        let mut offset = 0u64;
        let mut first_diff: Option<(u64, Option<u8>, Option<u8>)> = None;
        loop {
            let na = read_full(&mut fa, &mut ba)?;
            let nb = read_full(&mut fb, &mut bb)?;
            let n = na.min(nb);
            if let Some(i) = (0..n).find(|&i| ba[i] != bb[i]) {
                first_diff = Some((offset + i as u64, Some(ba[i]), Some(bb[i])));
                break;
            }
            if na != nb {
                first_diff = Some((
                    offset + n as u64,
                    ba.get(n).copied().filter(|_| na > n),
                    bb.get(n).copied().filter(|_| nb > n),
                ));
                break;
            }
            if na == 0 {
                break;
            }
            offset += n as u64;
            if offset.is_multiple_of(64 << 20) {
                ctx.check_cancelled()?;
            }
        }
        let identical = first_diff.is_none();
        let mut data = json!({
            "identical": identical,
            "left_size": sa,
            "right_size": sb,
        });
        if let Some((off, x, y)) = first_diff {
            data["first_difference"] = json!({
                "offset": off,
                "offset_hex": format!("0x{off:x}"),
                "left_byte": x.map(|v| format!("0x{v:02x}")),
                "right_byte": y.map(|v| format!("0x{v:02x}")),
            });
        }
        let mut text = if identical {
            "The files are identical.".to_string()
        } else {
            format!(
                "The files differ (first difference at byte {}).",
                first_diff.unwrap().0
            )
        };
        if !identical && sa <= MAX_TEXT_DIFF && sb <= MAX_TEXT_DIFF {
            let (ta, tb) = (std::fs::read(&a.path)?, std::fs::read(&b.path)?);
            if let (Ok(ta), Ok(tb)) = (String::from_utf8(ta), String::from_utf8(tb)) {
                if !ta.contains('\0') && !tb.contains('\0') {
                    let diff = similar::TextDiff::from_lines(&ta, &tb);
                    let mut chunks: Vec<Value> = vec![];
                    let (mut ins, mut del) = (0, 0);
                    for ch in diff.iter_all_changes() {
                        let tag = match ch.tag() {
                            similar::ChangeTag::Equal => "equal",
                            similar::ChangeTag::Insert => {
                                ins += 1;
                                "insert"
                            }
                            similar::ChangeTag::Delete => {
                                del += 1;
                                "delete"
                            }
                        };
                        match chunks.last_mut() {
                            Some(last) if last["tag"] == tag => {
                                let s =
                                    last["text"].as_str().unwrap_or("").to_string() + ch.value();
                                last["text"] = Value::String(s);
                            }
                            _ => chunks.push(json!({"tag": tag, "text": ch.value()})),
                        }
                    }
                    data["lines_added"] = json!(ins);
                    data["lines_removed"] = json!(del);
                    data["similarity"] = json!((diff.ratio() * 1000.0).round() / 10.0);
                    data["diff"] = Value::Array(chunks);
                    text = diff
                        .unified_diff()
                        .context_radius(3)
                        .header(&a.name, &b.name)
                        .to_string();
                }
            }
        }
        let mime = if data.get("diff").is_some() {
            "text/x-diff"
        } else {
            "text/plain"
        };
        Ok(ToolOutput::text_as(text, mime).with_data(data))
    })
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        let n = r.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(got)
}

struct Meta {
    size: u64,
    is_dir: bool,
    path: std::path::PathBuf,
}

fn snapshot(
    ctx: &ToolContext,
    root: &std::path::Path,
    opts: &WalkOpts,
) -> ToolResult<(BTreeMap<String, Meta>, WalkStats)> {
    let mut m = BTreeMap::new();
    let stats = walk(ctx, root, opts, |e| {
        let ft = e.file_type();
        if ft.is_file() || ft.is_dir() {
            m.insert(
                rel(root, e.path()),
                Meta {
                    size: if ft.is_file() {
                        e.metadata().map(|x| x.len()).unwrap_or(0)
                    } else {
                        0
                    },
                    is_dir: ft.is_dir(),
                    path: e.path().to_path_buf(),
                },
            );
        }
        Ok(())
    })?;
    Ok((m, stats))
}

fn compare_dirs() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.compare_dirs", "Compare Folders", Category::Files)
        .describe("Compare two folders: files only on one side, files that differ (size or content hash) and identical files")
        .keywords(&["compare", "folders", "directories", "diff", "sync", "backup", "dircmp"])
        .param(dir_param("left", "First folder"))
        .param(dir_param("right", "Second folder"))
        .param(ParamSpec::boolean("compare_content").label("Compare file contents").describe("Hash same-sized files (BLAKE3) to detect changed content").default_bool(true))
        .param(ParamSpec::boolean("show_identical").label("List identical files").advanced())
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Data)
        .progress()
        .timeout(6 * 3600);
    sync_tool(spec, |ctx, p| {
        let (l, r) = (p.path("left")?, p.path("right")?);
        let opts = WalkOpts::from_params(p);
        let (a, sa) = snapshot(ctx, &l, &opts)?;
        let (b, sb) = snapshot(ctx, &r, &opts)?;
        let content = p.bool("compare_content");
        let show_same = p.bool("show_identical");
        let mut rows = vec![];
        let (mut only_l, mut only_r, mut differ, mut same) = (0, 0, 0, 0);
        let both: Vec<&String> = a.keys().filter(|k| b.contains_key(*k)).collect();
        let total = both.len().max(1);
        for (k, ma) in &a {
            if !b.contains_key(k) {
                only_l += 1;
                rows.push(json!([
                    k,
                    "only in first",
                    if ma.is_dir { None } else { Some(ma.size) },
                    Value::Null
                ]));
            }
        }
        for (k, mb) in &b {
            if !a.contains_key(k) {
                only_r += 1;
                rows.push(json!([
                    k,
                    "only in second",
                    Value::Null,
                    if mb.is_dir { None } else { Some(mb.size) }
                ]));
            }
        }
        for (i, k) in both.iter().enumerate() {
            let (ma, mb) = (&a[*k], &b[*k]);
            if i % 64 == 0 {
                ctx.check_cancelled()?;
                ctx.progress(i as f32 / total as f32, k.to_string());
            }
            let status = if ma.is_dir != mb.is_dir {
                "type differs (file vs folder)"
            } else if ma.is_dir {
                continue;
            } else if ma.size != mb.size {
                "size differs"
            } else if content && blake3_file(ctx, &ma.path)? != blake3_file(ctx, &mb.path)? {
                "content differs"
            } else {
                "identical"
            };
            if status == "identical" {
                same += 1;
                if !show_same {
                    continue;
                }
            } else {
                differ += 1;
            }
            rows.push(json!([k, status, ma.size, mb.size]));
        }
        rows.sort_by(|x, y| x[0].as_str().cmp(&y[0].as_str()));
        let truncated = rows.len() > 50_000;
        rows.truncate(50_000);
        let mut o = ToolOutput::data(json!({
            "only_in_first": only_l,
            "only_in_second": only_r,
            "different": differ,
            "identical": same,
            "equal": only_l == 0 && only_r == 0 && differ == 0,
            "table": table(&["Path", "Status", "First size", "Second size"], rows),
        }));
        o.warnings = sa.warnings();
        o.warnings.extend(sb.warnings());
        if truncated {
            o.warnings
                .push("only the first 50,000 rows are shown".into());
        }
        if !content {
            o.warnings.push(
                "contents were not compared; same-sized files are reported as identical".into(),
            );
        }
        Ok(o)
    })
}
