//! Hashes, checksum verification and checksum manifests.

use crate::util::*;
use dogubako_core::prelude::*;
use std::collections::HashMap;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![hash(), checksum_verify(), checksum_manifest()]
}

fn hash() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.hash", "File Hash", Category::Files)
        .describe("Compute MD5, SHA-1, SHA-256, SHA-512, SHA3-256, BLAKE3, CRC32 or XXH3 checksums of files or text")
        .keywords(&["checksum", "hash", "md5", "sha256", "sha1", "sha512", "blake3", "crc32", "xxhash", "digest", "integrity"])
        .param(ParamSpec::files("input").label("Files").primary())
        .param(ParamSpec::textarea("text").label("Or text").describe("Hash this text (UTF-8) instead of / in addition to files"))
        .param(ParamSpec::multi_choice("algorithms", ALGOS).label("Algorithms").default_value(json!(["sha256"])))
        .param(ParamSpec::boolean("uppercase").label("Uppercase hex").advanced())
        .output(OutputKind::Data)
        .progress()
        .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let algos = p.list("algorithms");
        if algos.is_empty() {
            return Err(ToolError::invalid(
                "algorithms",
                "choose at least one algorithm",
            ));
        }
        let files = p.files("input");
        let text = p.text("text");
        if files.is_empty() && text.is_empty() {
            return Err(ToolError::invalid("input", "provide files or text to hash"));
        }
        let up = p.bool("uppercase");
        let fix = |s: String| if up { s.to_uppercase() } else { s };
        let mut rows = vec![];
        let mut results = vec![];
        let mut lines = vec![];
        if !text.is_empty() {
            let h = hash_reader(ctx, text.as_bytes(), &algos, |_| {})?;
            let mut m = serde_json::Map::new();
            for (a, v) in algos.iter().zip(h) {
                let v = fix(v);
                rows.push(json!(["(text)", a, v, text.len()]));
                lines.push(format!("{a}  {v}  (text)"));
                m.insert(a.clone(), Value::String(v));
            }
            results.push(json!({"name": "(text)", "size": text.len(), "hashes": m}));
        }
        let total: u64 = files.iter().map(|f| f.size()).sum::<u64>().max(1);
        let mut done = 0u64;
        for f in files {
            let file = std::fs::File::open(&f.path)?;
            let size = f.size();
            let h = hash_reader(ctx, file, &algos, |b| {
                if b % (64 << 20) < (1 << 20) {
                    ctx.progress((done + b) as f32 / total as f32, f.name.clone());
                }
            })?;
            done += size;
            let mut m = serde_json::Map::new();
            for (a, v) in algos.iter().zip(h) {
                let v = fix(v);
                rows.push(json!([f.name, a, v, size]));
                lines.push(format!("{v}  {}", f.name));
                m.insert(a.clone(), Value::String(v));
            }
            results.push(json!({"name": f.name, "size": size, "hashes": m}));
        }
        Ok(ToolOutput::text(lines.join("\n")).with_data(json!({
            "results": results,
            "table": table(&["Name", "Algorithm", "Hash", "Size"], rows),
        })))
    })
}

/// One expected checksum: (hash, file name or None).
#[derive(Debug, PartialEq)]
pub struct Expected {
    pub hash: String,
    pub name: Option<String>,
    pub algo: Option<String>,
}

/// Parse `sha256sum`/`md5sum` style (`hash  name`, `hash *name`), BSD style
/// (`SHA256 (name) = hash`) or a bare hash.
pub fn parse_checklist(text: &str) -> Vec<Expected> {
    let bsd = regex::Regex::new(r"^(?i)(MD5|SHA1|SHA256|SHA512|SHA3-256|BLAKE3|CRC32|XXH3)\s*\((.+)\)\s*=\s*([0-9a-fA-F]+)$").unwrap();
    let gnu = regex::Regex::new(r"^\\?([0-9a-fA-F]{8,128})\s+[ *]?(.+)$").unwrap();
    let mut v = vec![];
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(c) = bsd.captures(l) {
            v.push(Expected {
                hash: c[3].to_lowercase(),
                name: Some(c[2].to_string()),
                algo: Some(c[1].to_lowercase()),
            });
        } else if let Some(c) = gnu.captures(l) {
            v.push(Expected {
                hash: c[1].to_lowercase(),
                name: Some(c[2].trim().to_string()),
                algo: None,
            });
        } else if l.len() >= 8 && l.chars().all(|c| c.is_ascii_hexdigit()) {
            v.push(Expected {
                hash: l.to_lowercase(),
                name: None,
                algo: None,
            });
        }
    }
    v
}

fn base_name(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

fn checksum_verify() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.checksum_verify", "Verify Checksums", Category::Files)
        .describe("Check files against a pasted checksum list (sha256sum/md5sum/BSD format) or a single expected hash; the algorithm is detected from the hash length")
        .keywords(&["verify", "checksum", "integrity", "sha256sum", "md5sum", "download", "compare hash"])
        .param(ParamSpec::files("input").label("Files").required().primary())
        .param(ParamSpec::textarea("checksums").label("Checksums").describe("Paste a SHA256SUMS-style list or a single hash").required())
        .output(OutputKind::Data)
        .progress()
        .timeout(3600);
    sync_tool(spec, |ctx, p| {
        let files = p.files("input");
        let expected = parse_checklist(p.text("checksums"));
        if expected.is_empty() {
            return Err(ToolError::invalid("checksums", "no checksums recognized"));
        }
        for e in &expected {
            let ok_len = match &e.algo {
                Some(a) => Hasher::new(a).is_some(),
                None => !algos_for_len(e.hash.len()).is_empty(),
            };
            if !ok_len {
                return Err(ToolError::invalid(
                    "checksums",
                    format!(
                        "cannot determine the algorithm of '{}' ({} hex digits)",
                        e.hash,
                        e.hash.len()
                    ),
                ));
            }
        }
        let single = expected.len() == 1 && expected[0].name.is_none();
        // Several entries can share a basename (a/config.json, b/config.json).
        let mut by_name: HashMap<&str, Vec<&Expected>> = HashMap::new();
        for e in &expected {
            if let Some(n) = e.name.as_deref() {
                by_name.entry(base_name(n)).or_default().push(e);
            }
        }
        let mut rows = vec![];
        let (mut ok, mut bad, mut missing, mut ambiguous) = (0, 0, 0, 0);
        for (i, f) in files.iter().enumerate() {
            ctx.progress(i as f32 / files.len() as f32, f.name.clone());
            let exp = if single {
                Some(&expected[0])
            } else {
                match by_name.get(f.name.as_str()).map(Vec::as_slice) {
                    None | Some([]) => None,
                    Some([e]) => Some(*e),
                    Some(entries) => {
                        // Same checksum everywhere: no ambiguity. Otherwise
                        // match by path (local inputs carry their full path).
                        let local = f.path.to_string_lossy().replace('\\', "/");
                        let by_path: Vec<&&Expected> = entries
                            .iter()
                            .filter(|e| {
                                let n = e.name.as_deref().unwrap_or("").replace('\\', "/");
                                let n = n.trim_start_matches("./");
                                local.ends_with(&format!("/{n}")) || local == n
                            })
                            .collect();
                        if entries.iter().all(|e| e.hash == entries[0].hash) {
                            Some(entries[0])
                        } else if by_path.len() == 1 {
                            Some(*by_path[0])
                        } else {
                            ambiguous += 1;
                            let names: Vec<&str> =
                                entries.iter().filter_map(|e| e.name.as_deref()).collect();
                            rows.push(json!([
                                f.name,
                                "",
                                "",
                                "",
                                format!(
                                    "ambiguous: {} different checksums listed for this name ({})",
                                    entries.len(),
                                    names.join(", ")
                                )
                            ]));
                            continue;
                        }
                    }
                }
            };
            let Some(exp) = exp else {
                missing += 1;
                rows.push(json!([f.name, "", "", "", "no checksum listed"]));
                continue;
            };
            let algos: Vec<String> = match &exp.algo {
                Some(a) => vec![a.clone()],
                None => algos_for_len(exp.hash.len())
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            };
            let got = hash_file(ctx, &f.path, &algos)?;
            match algos.iter().zip(&got).find(|(_, g)| **g == exp.hash) {
                Some((a, g)) => {
                    ok += 1;
                    rows.push(json!([f.name, a, exp.hash, g, "OK"]));
                }
                None => {
                    bad += 1;
                    rows.push(json!([
                        f.name,
                        algos.join("/"),
                        exp.hash,
                        got.join(" / "),
                        "MISMATCH"
                    ]));
                }
            }
        }
        let listed_not_given: Vec<&str> = if single {
            vec![]
        } else {
            by_name
                .keys()
                .copied()
                .filter(|n| !files.iter().any(|f| f.name == *n))
                .collect()
        };
        let all_ok = bad == 0 && missing == 0 && ambiguous == 0;
        let mut out = ToolOutput::text(if all_ok {
            format!("All {ok} file(s) OK")
        } else if ambiguous > 0 {
            format!("{ok} OK, {bad} mismatched, {missing} without checksum, {ambiguous} ambiguous")
        } else {
            format!("{ok} OK, {bad} mismatched, {missing} without checksum")
        })
        .with_data(json!({
            "ok": ok,
            "mismatched": bad,
            "unlisted": missing,
            "ambiguous": ambiguous,
            "all_ok": all_ok,
            "listed_but_not_provided": listed_not_given,
            "table": table(&["File", "Algorithm", "Expected", "Actual", "Result"], rows),
        }));
        if bad > 0 {
            out = out.warn(format!("{bad} file(s) do not match the expected checksum"));
        }
        Ok(out)
    })
}

fn checksum_manifest() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("files.checksum_manifest", "Checksum Manifest", Category::Files)
        .describe("Hash every file in a folder and produce a SHA256SUMS-style manifest (plus JSON) for later verification")
        .keywords(&["sha256sums", "manifest", "checksums", "folder", "integrity", "backup", "verify"])
        .param(dir_param("path", "Folder"))
        .param(ParamSpec::choice("algorithm", ALGOS).default_str("sha256"))
        .params(walk_params())
        .permission(Permission::HostFsRead)
        .output(OutputKind::Files)
        .progress()
        .timeout(6 * 3600);
    sync_tool(spec, |ctx, p| {
        let root = p.path("path")?;
        let algo = p.str_or("algorithm", "sha256").to_string();
        let opts = WalkOpts::from_params(p);
        let mut files = vec![];
        let stats = walk(ctx, &root, &opts, |e| {
            if e.file_type().is_file() {
                files.push((
                    rel(&root, e.path()),
                    e.path().to_path_buf(),
                    e.metadata().map(|m| m.len()).unwrap_or(0),
                ));
            }
            Ok(())
        })?;
        files.sort();
        let total: u64 = files.iter().map(|f| f.2).sum::<u64>().max(1);
        let mut done = 0u64;
        let mut lines = String::new();
        let mut entries = vec![];
        let mut errors = 0;
        for (r, path, size) in &files {
            ctx.check_cancelled()?;
            ctx.progress(done as f32 / total as f32, r.clone());
            match hash_file(ctx, path, std::slice::from_ref(&algo)) {
                Ok(h) => {
                    lines.push_str(&format!("{}  {}\n", h[0], r));
                    entries.push(json!({"path": r, "size": size, "hash": h[0]}));
                }
                Err(ToolError::Cancelled) => return Err(ToolError::Cancelled),
                Err(_) => errors += 1,
            }
            done += size;
        }
        let base = root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "folder".into());
        let sums_name = format!("{}SUMS", algo.to_uppercase().replace('-', ""));
        let a = ctx.write_output(&sums_name, lines.as_bytes())?;
        let manifest = json!({"root": base, "algorithm": algo, "files": entries, "generated": fmt_time_utc(std::time::SystemTime::now())});
        let b = ctx.write_output(
            &format!("{base}-manifest.json"),
            serde_json::to_string_pretty(&manifest)?.as_bytes(),
        )?;
        let mut o = ToolOutput::files(vec![a, b])
            .with_data(json!({"files": entries.len(), "total_size": human_size(total)}));
        o.warnings = stats.warnings();
        if errors > 0 {
            o.warnings
                .push(format!("{errors} file(s) could not be read"));
        }
        Ok(o)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checklist_formats() {
        let v = parse_checklist("# c\nd41d8cd98f00b204e9800998ecf8427e  empty.txt\nD41D8CD98F00B204E9800998ECF8427E *dir/bin.dat\nSHA256 (x y.txt) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nabcdef12\n");
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].name.as_deref(), Some("empty.txt"));
        assert_eq!(v[1].name.as_deref(), Some("dir/bin.dat"));
        assert_eq!(v[1].hash, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(v[2].algo.as_deref(), Some("sha256"));
        assert_eq!(v[2].name.as_deref(), Some("x y.txt"));
        assert_eq!(
            v[3],
            Expected {
                hash: "abcdef12".into(),
                name: None,
                algo: None
            }
        );
    }
}
