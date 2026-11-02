//! 7z / RAR (and anything else 7-Zip reads) via the external 7-Zip binary.

use super::formats::Entry;
use super::safe::{safe_rel_path, Extracted, Extractor};
use dogubako_core::prelude::*;
use std::path::Path;
use std::time::Duration;

fn check_pw(pw: &str) -> ToolResult<()> {
    if pw.contains(['\n', '\r', '\0']) || pw.starts_with('-') && pw.len() == 1 {
        return Err(ToolError::invalid("password", "invalid password"));
    }
    Ok(())
}

fn map_err(e: ToolError) -> ToolError {
    match e {
        ToolError::External { stderr, .. }
            if stderr.to_lowercase().contains("wrong password")
                || stderr.contains("Can not open encrypted archive") =>
        {
            ToolError::invalid(
                "password",
                "the archive is encrypted or the password is wrong",
            )
        }
        ToolError::External { stderr, .. }
            if stderr.contains("Cannot open the file as archive")
                || stderr.contains("Can not open the file as archive") =>
        {
            ToolError::invalid("input", "7-Zip cannot open this file as an archive")
        }
        other => other,
    }
}

pub async fn list(
    ctx: &ToolContext,
    path: &Path,
    password: Option<&str>,
) -> ToolResult<Vec<Entry>> {
    let mut cmd = ctx
        .command("7zip")?
        .args(["l", "-slt", "-ba", "-sccUTF-8", "-bd"]);
    // Always pass -p so 7-Zip never waits for a password prompt.
    let pw = password.unwrap_or("");
    check_pw(pw)?;
    cmd = cmd.arg(format!("-p{pw}"));
    let out = cmd
        .arg("--")
        .arg(path)
        .timeout(Duration::from_secs(600))
        .run()
        .await
        .map_err(map_err)?;
    Ok(parse_slt(&out.stdout_text()))
}

pub fn parse_slt(text: &str) -> Vec<Entry> {
    let mut v = vec![];
    let mut cur: Option<Entry> = None;
    for line in text.lines() {
        let Some((k, val)) = line.split_once(" = ") else {
            continue;
        };
        let (k, val) = (k.trim(), val.trim_end());
        if k == "Path" {
            if let Some(e) = cur.take() {
                v.push(e);
            }
            cur = Some(Entry {
                name: val.to_string(),
                size: 0,
                compressed: None,
                modified: None,
                kind: "file",
                link: None,
                encrypted: false,
            });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        match k {
            "Size" => e.size = val.parse().unwrap_or(0),
            "Packed Size" => e.compressed = val.parse().ok(),
            "Modified" => e.modified = Some(val.split('.').next().unwrap_or(val).to_string()),
            "Folder" if val == "+" => e.kind = "dir",
            "Encrypted" => e.encrypted = val == "+",
            "Symbolic Link" if !val.is_empty() => {
                e.kind = "symlink";
                e.link = Some(val.to_string());
            }
            "Hard Link" if !val.is_empty() => {
                e.kind = "hardlink";
                e.link = Some(val.to_string());
            }
            "Attributes" => {
                if val.starts_with('D') && e.kind == "file" {
                    e.kind = "dir";
                }
                for tok in val.split_whitespace() {
                    if tok.len() == 10 && tok[1..].chars().all(|c| "rwxsStT-".contains(c)) {
                        match tok.as_bytes()[0] {
                            b'l' => e.kind = "symlink",
                            b'c' | b'b' => e.kind = "device",
                            b'p' | b's' => e.kind = "special",
                            b'd' => e.kind = "dir",
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(e) = cur.take() {
        v.push(e);
    }
    v
}

/// Extract only validated regular files, then re-verify what is on disk.
pub async fn extract(
    ctx: &ToolContext,
    ex: &mut Extractor<'_>,
    path: &Path,
    password: Option<&str>,
) -> ToolResult<()> {
    let entries = list(ctx, path, password).await?;
    if entries.len() as u64 > ex.max_entries {
        return Err(ToolError::limit(format!(
            "archive lists {} entries (limit {})",
            entries.len(),
            ex.max_entries
        )));
    }
    let mut wanted = vec![];
    let mut declared = 0u64;
    for e in &entries {
        ex.count_entry()?;
        match e.kind {
            "file" => match safe_rel_path(&e.name) {
                Ok(_) if e.name.contains(['\n', '\r']) => {
                    ex.skip(&e.name, "name contains line breaks")
                }
                Ok(_) => {
                    declared = declared.saturating_add(e.size);
                    wanted.push(e.name.clone());
                }
                Err(why) => ex.skip(&e.name, why),
            },
            "dir" => {}
            "symlink" => ex.skip(&e.name, "symbolic link (not extracted)"),
            "hardlink" => ex.skip(&e.name, "hard link (not extracted)"),
            other => ex.skip(&e.name, format!("{other} entry (not extracted)")),
        }
    }
    if declared > ex.max_bytes {
        return Err(ToolError::limit(format!(
            "archive declares {} of data, above the limit of {}",
            human_size(declared),
            human_size(ex.max_bytes)
        )));
    }
    if wanted.is_empty() {
        return Ok(());
    }
    let list = ctx.temp_path("7z-include.txt");
    std::fs::write(&list, wanted.join("\n"))?;
    let pw = password.unwrap_or("");
    check_pw(pw)?;
    let dir = ex.root.clone();
    let mut out_arg = std::ffi::OsString::from("-o");
    out_arg.push(&dir);
    let mut inc = std::ffi::OsString::from("-i@");
    inc.push(&list);
    ctx.command("7zip")?
        .args(["x", "-y", "-bd", "-spd", "-scsUTF-8", "-sccUTF-8", "-aos"])
        .arg(format!("-p{pw}"))
        .arg(inc)
        .arg(out_arg)
        .arg("--")
        .arg(path)
        .timeout(Duration::from_secs(6 * 3600))
        .run()
        .await
        .map_err(map_err)?;
    // Re-verify the result on disk: only regular files inside the root,
    // counted by their real size.
    let mut total = 0u64;
    let mut files = vec![];
    for e in walkdir::WalkDir::new(&dir).follow_links(false) {
        let e = e.map_err(|e| ToolError::failed(format!("reading extracted files: {e}")))?;
        let ft = e.file_type();
        let rel = crate::util::rel(&dir, e.path());
        if ft.is_symlink() || !(ft.is_file() || ft.is_dir()) {
            let _ = std::fs::remove_file(e.path());
            ex.skip(&rel, "link or special file removed after extraction");
            continue;
        }
        if ft.is_file() {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            total += size;
            if total > ex.max_bytes {
                return Err(ToolError::limit(format!(
                    "extracted data exceeds the limit of {} (possible decompression bomb)",
                    human_size(ex.max_bytes)
                )));
            }
            files.push(Extracted { rel, size });
        }
    }
    ex.total_bytes = total;
    files.sort_by(|a, b| a.rel.cmp(&b.rel));
    ex.files = files;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slt() {
        let t = "Path = a.txt\nFolder = -\nSize = 12\nPacked Size = 10\nModified = 2024-01-02 03:04:05.1234567\nAttributes = A_ -rw-r--r--\nEncrypted = -\n\nPath = dir\nFolder = +\nSize = 0\nAttributes = D_ drwxr-xr-x\n\nPath = link\nSize = 4\nAttributes = A_ lrwxrwxrwx\n";
        let v = parse_slt(t);
        assert_eq!(v.len(), 3);
        assert_eq!(
            (v[0].kind, v[0].size, v[0].compressed),
            ("file", 12, Some(10))
        );
        assert_eq!(v[0].modified.as_deref(), Some("2024-01-02 03:04:05"));
        assert_eq!(v[1].kind, "dir");
        assert_eq!(v[2].kind, "symlink");
    }
}
