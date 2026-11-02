//! Integration tests through the real engine, including archive safety,
//! rename preview/apply and filesystem-root enforcement.

use dogubako_core::engine::{Engine, EngineConfig, RunRequest};
use dogubax_alias::*;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

mod dogubax_alias {
    pub use dogubako_core::error::ToolResult;
    pub use dogubako_core::params::LocalFileResolver;
    pub use dogubako_core::policy::{ExecLimits, FsPolicy, PermissionSet};
    pub use dogubako_core::registry::Registry;
    pub use dogubako_core::testing::TestHarness;
    pub use dogubako_core::tool::{Environment, ToolOutput};
}

fn harness() -> TestHarness {
    TestHarness::new(crate::tools())
}

fn s(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// Engine with custom limits / filesystem policy / permissions.
struct Custom {
    engine: Engine,
    dir: tempfile::TempDir,
}

impl Custom {
    fn new(limits: ExecLimits, fs: FsPolicy) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = Registry::new();
        reg.register_all(crate::tools());
        let mut cfg = EngineConfig::local(Environment::Cli);
        cfg.temp_root = dir.path().join("tmp");
        cfg.limits = limits;
        cfg.fs = fs;
        Custom {
            engine: Engine::new(reg, cfg, vec![]),
            dir,
        }
    }
    async fn run_with(
        &self,
        id: &str,
        params: Value,
        perms: PermissionSet,
    ) -> ToolResult<ToolOutput> {
        let resolver = LocalFileResolver::unrestricted();
        self.engine
            .run(RunRequest {
                tool_id: id,
                params: params.as_object().cloned().unwrap_or_default(),
                resolver: &resolver,
                out_dir: self
                    .dir
                    .path()
                    .join(format!("out-{}", dogubako_core::util::random_id(4))),
                permissions: perms,
                authorize: None,
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await
            .map(|(o, _)| o)
    }
    async fn run(&self, id: &str, params: Value) -> ToolResult<ToolOutput> {
        self.run_with(id, params, PermissionSet::all()).await
    }
}

fn small_limits(bytes: u64, entries: u64) -> ExecLimits {
    ExecLimits {
        max_extract_bytes: bytes,
        max_extract_entries: entries,
        ..ExecLimits::default()
    }
}

fn tree(root: &Path) {
    std::fs::create_dir_all(root.join("sub/deeper")).unwrap();
    std::fs::create_dir_all(root.join("empty1/empty2")).unwrap();
    std::fs::write(root.join("a.txt"), "hello world\nTODO: fix me\n").unwrap();
    std::fs::write(root.join("sub/b.txt"), "hello world\nTODO: fix me\n").unwrap(); // duplicate of a.txt
    std::fs::write(root.join("sub/deeper/big.bin"), vec![7u8; 300_000]).unwrap();
    std::fs::write(root.join("sub/zero.dat"), b"").unwrap();
    std::fs::write(root.join("notes.md"), "# Notes\nnothing to do\n").unwrap();
}

#[test]
fn specs_are_consistent() {
    use dogubako_core::spec::{ParamKind, Permission};
    for t in crate::tools() {
        let sp = t.spec();
        assert!(sp.id.starts_with("files."), "{}", sp.id);
        assert!(!sp.description.is_empty());
        let has_path = sp
            .params
            .iter()
            .any(|p| matches!(p.kind, ParamKind::Path { .. }));
        if has_path {
            assert!(
                sp.permissions.contains(&Permission::HostFsRead),
                "{} needs host_fs_read",
                sp.id
            );
        }
        if sp.permissions.contains(&Permission::HostFsWrite) {
            assert!(
                sp.destructive,
                "{} writes to the host and must be destructive",
                sp.id
            );
            assert!(
                sp.params
                    .iter()
                    .any(|p| p.name == "apply" && p.default == Some(json!(false))),
                "{} must preview by default",
                sp.id
            );
        }
    }
}

#[tokio::test]
async fn hashes() {
    let h = harness();
    let out = h.run("files.hash", json!({"text": "abc", "algorithms": ["md5", "sha1", "sha256", "sha3-256", "blake3", "crc32", "xxh3", "sha512"]})).await.unwrap();
    let r = &out.data["results"][0]["hashes"];
    assert_eq!(r["md5"], "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(r["sha1"], "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(
        r["sha256"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        r["sha3-256"],
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
    assert_eq!(
        r["blake3"],
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    assert_eq!(r["crc32"], "352441c2");
    assert!(r["sha512"]
        .as_str()
        .unwrap()
        .starts_with("ddaf35a193617aba"));
    assert_eq!(r["xxh3"].as_str().unwrap().len(), 16);
    let f = h.write("abc.txt", b"abc");
    let out = h
        .run(
            "files.hash",
            json!({"input": [s(&f)], "algorithms": "sha256,md5", "uppercase": true}),
        )
        .await
        .unwrap();
    assert_eq!(
        out.data["results"][0]["hashes"]["md5"],
        "900150983CD24FB0D6963F7D28E17F72"
    );
    let err = h.run("files.hash", json!({})).await.unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run("files.hash", json!({"text": "x", "algorithms": ["rot13"]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn checksum_verify() {
    let h = harness();
    let f = h.write("abc.txt", b"abc");
    let g = h.write("other.bin", b"zzz");
    let list = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  dir/abc.txt\n0000000000000000000000000000000000000000000000000000000000000000 *other.bin\n";
    let out = h
        .run(
            "files.checksum_verify",
            json!({"input": [s(&f), s(&g)], "checksums": list}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["ok"], 1);
    assert_eq!(out.data["mismatched"], 1);
    assert_eq!(out.data["all_ok"], false);
    // Single bare hash, algorithm detected by length (MD5, and BLAKE3 among 64-hex candidates).
    let out = h
        .run(
            "files.checksum_verify",
            json!({"input": [s(&f)], "checksums": "900150983cd24fb0d6963f7d28e17f72"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["all_ok"], true);
    let out = h.run("files.checksum_verify", json!({"input": [s(&f)], "checksums": "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"})).await.unwrap();
    assert_eq!(out.data["table"]["rows"][0][1], "blake3");
    let err = h
        .run(
            "files.checksum_verify",
            json!({"input": [s(&f)], "checksums": "abcdef1234"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run(
            "files.checksum_verify",
            json!({"input": [s(&f)], "checksums": "not a checksum"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn manifest_and_scans() {
    let h = harness();
    let root = h.path("tree");
    tree(&root);
    let outside = h.write("outside-secret.txt", b"secret");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("link-out")).unwrap();

    let out = h
        .run("files.checksum_manifest", json!({"path": s(&root)}))
        .await
        .unwrap();
    let sums = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(sums.contains("  sub/b.txt\n"), "{sums}");
    assert!(
        !sums.contains("link-out"),
        "symlinks are not followed by default"
    );
    assert_eq!(out.artifacts[0].name, "SHA256SUMS");

    let out = h
        .run("files.dir_size", json!({"path": s(&root)}))
        .await
        .unwrap();
    assert_eq!(out.data["files"], 5);
    assert_eq!(out.data["table"]["rows"][0][0], "sub/");
    let out = h
        .run(
            "files.large_files",
            json!({"path": s(&root), "min_size_mb": 0.1}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["matching_files"], 1);
    assert_eq!(out.data["table"]["rows"][0][0], "sub/deeper/big.bin");
    let err = h
        .run(
            "files.dir_size",
            json!({"path": s(&root), "max_entries": 3}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded");

    let out = h
        .run(
            "files.duplicates",
            json!({"path": s(&root), "min_size_kb": 0}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["groups"], 1);
    assert_eq!(out.data["wasted_space"], 25);
    let a = root.join("a.txt");
    let b = root.join("sub/b.txt");
    let out = h
        .run(
            "files.duplicates_uploaded",
            json!({"input": [s(&a), s(&b), s(&outside)]}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["groups"], 1);

    let out = h
        .run(
            "files.search",
            json!({"path": s(&root), "content": "todo", "name": "*.txt"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["files_matched"], 2);
    assert_eq!(out.data["table"]["rows"][0][1], 2);
    let out = h
        .run(
            "files.search",
            json!({"path": s(&root), "content": "todo", "case_sensitive": true}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["results"], 0);
    let out = h
        .run("files.search", json!({"path": s(&root), "name": "*.md"}))
        .await
        .unwrap();
    assert_eq!(out.data["files_matched"], 1);
    let err = h
        .run("files.search", json!({"path": s(&root), "content": "("}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run(
            "files.manifest",
            json!({"path": s(&root), "hash": "sha256"}),
        )
        .await
        .unwrap();
    let csv = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(csv.starts_with("path,size,modified,mime,hash\n"));
    assert!(csv.contains("a.txt,25,"));
    assert!(csv.contains("text/plain"));

    let out = h
        .run("files.empty", json!({"path": s(&root)}))
        .await
        .unwrap();
    assert_eq!(out.data["empty_files"], 1);
    assert_eq!(out.data["empty_folders"], 2);
    // Preview does not delete.
    let out = h
        .run("files.remove_empty", json!({"path": s(&root)}))
        .await
        .unwrap();
    assert_eq!(out.data["applied"], false);
    assert!(root.join("empty1/empty2").exists() && root.join("sub/zero.dat").exists());
    let out = h
        .run(
            "files.remove_empty",
            json!({"path": s(&root), "apply": true}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["deleted"], 3);
    assert!(!root.join("empty1").exists() && !root.join("sub/zero.dat").exists());
    assert!(root.join("a.txt").exists() && outside.exists());
}

#[tokio::test]
async fn compare_files_and_dirs() {
    let h = harness();
    let a = h.write("a.txt", b"one\ntwo\nthree\n");
    let b = h.write("b.txt", b"one\n2\nthree\nfour\n");
    let c = h.write("c.txt", b"one\ntwo\nthree\n");
    let out = h
        .run("files.compare", json!({"left": s(&a), "right": s(&c)}))
        .await
        .unwrap();
    assert_eq!(out.data["identical"], true);
    let out = h
        .run("files.compare", json!({"left": s(&a), "right": s(&b)}))
        .await
        .unwrap();
    assert_eq!(out.data["identical"], false);
    assert_eq!(out.data["first_difference"]["offset"], 4);
    assert!(out.data["diff"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["tag"] == "insert" && d["text"].as_str().unwrap().contains("four")));
    assert!(out.text.unwrap().contains("+2"));
    let x = h.write("x.bin", &[0, 1, 2, 3]);
    let y = h.write("y.bin", &[0, 1, 2]);
    let out = h
        .run("files.compare", json!({"left": s(&x), "right": s(&y)}))
        .await
        .unwrap();
    assert_eq!(out.data["first_difference"]["offset"], 3);
    assert!(out.data.get("diff").is_none());

    let l = h.path("left");
    let r = h.path("right");
    tree(&l);
    tree(&r);
    std::fs::write(r.join("a.txt"), "hello world\nTODO: fix ME\n").unwrap(); // same size, new content
    std::fs::write(r.join("only-right.txt"), "x").unwrap();
    std::fs::remove_file(r.join("notes.md")).unwrap();
    let out = h
        .run("files.compare_dirs", json!({"left": s(&l), "right": s(&r)}))
        .await
        .unwrap();
    assert_eq!(out.data["only_in_first"], 1);
    assert_eq!(out.data["only_in_second"], 1);
    assert_eq!(out.data["different"], 1);
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|r| r[0] == "a.txt" && r[1] == "content differs"));
}

#[tokio::test]
async fn inspection() {
    let h = harness();
    let mut png = std::io::Cursor::new(Vec::new());
    // Minimal valid PNG header bytes are enough for magic detection.
    png.write_all(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x08\x08\x02\0\0\0")
        .unwrap();
    let fake = h.write("photo.txt", png.get_ref());
    let js = h.write("data.json", b"{\"a\": [1, 2]}");
    let out = h
        .run("files.type_detect", json!({"input": [s(&fake), s(&js)]}))
        .await
        .unwrap();
    assert_eq!(out.data["files"][0]["mime"], "image/png");
    assert_eq!(out.data["files"][0]["extension_mismatch"], true);
    assert_eq!(out.data["files"][0]["dimensions"], "16×8");
    assert_eq!(out.data["files"][1]["mime"], "application/json");
    assert_eq!(out.data["files"][1]["extension_mismatch"], false);
    assert!(!out.warnings.is_empty());

    let out = h
        .run(
            "files.hexdump",
            json!({"input": s(&js), "offset": 2, "length": 3}),
        )
        .await
        .unwrap();
    assert!(out.text.unwrap().starts_with("00000002  61 22 3a"));
    let err = h
        .run("files.hexdump", json!({"input": s(&js), "offset": 999}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let exe = std::env::current_exe().unwrap();
    let out = h
        .run("files.binary_info", json!({"input": s(&exe)}))
        .await
        .unwrap();
    #[cfg(target_os = "linux")]
    {
        assert_eq!(out.data["format"], "ELF");
        assert_eq!(out.data["bits"], 64);
    }
    assert!(out.data["header_hex"]
        .as_str()
        .unwrap()
        .starts_with("00000000"));
    let err = h
        .run("files.binary_info", json!({"input": s(&js)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    let out = h
        .run("files.mime_types", json!({"query": "pdf, .webp image/png"}))
        .await
        .unwrap();
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert_eq!(rows[0][1], "application/pdf");
    assert!(rows[2][1].as_str().unwrap().contains("png"));
}

// ---------------------------------------------------------------------------
// Archives

#[tokio::test]
async fn archive_roundtrips() {
    let h = harness();
    let a = h.write("alpha.txt", b"alpha alpha alpha alpha");
    let b = h.write("beta.bin", &[1u8, 2, 3, 4, 5, 6, 7, 8]);
    let dup_dir = h.path("dupdir");
    std::fs::create_dir_all(&dup_dir).unwrap();
    let a2 = dup_dir.join("alpha.txt");
    std::fs::write(&a2, b"second alpha").unwrap();
    let mut formats = vec!["zip", "tar", "tar.gz", "tar.xz", "tar.zst", "tar.bz2"];
    if h.available("files.archive_create")
        && dogubako_core::deps::DepRegistry::new(vec![]).available("7zip")
    {
        formats.push("7z");
    }
    for fmt in formats {
        let out = h
            .run(
                "files.archive_create",
                json!({"input": [s(&a), s(&b), s(&a2)], "format": fmt, "name": "bundle"}),
            )
            .await
            .unwrap_or_else(|e| panic!("{fmt}: {e}"));
        let arc = out.artifacts[0].path.clone();
        assert!(out.artifacts[0].name.ends_with(fmt), "{fmt}");
        let out = h
            .run("files.archive_list", json!({"input": s(&arc)}))
            .await
            .unwrap_or_else(|e| panic!("list {fmt}: {e}"));
        assert_eq!(out.data["files"], 3, "{fmt}");
        assert_eq!(out.data["suspicious"], 0, "{fmt}");
        let out = h
            .run("files.archive_extract", json!({"input": s(&arc)}))
            .await
            .unwrap_or_else(|e| panic!("extract {fmt}: {e}"));
        assert_eq!(out.artifacts.len(), 3, "{fmt}");
        let mut names: Vec<String> = out
            .artifacts
            .iter()
            .map(|x| x.label.clone().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, vec!["alpha-2.txt", "alpha.txt", "beta.bin"], "{fmt}");
        let alpha = out
            .artifacts
            .iter()
            .find(|x| x.label.as_deref() == Some("alpha.txt"))
            .unwrap();
        assert_eq!(
            std::fs::read(&alpha.path).unwrap(),
            b"alpha alpha alpha alpha"
        );
        // Zip output mode keeps everything in one archive.
        let out = h
            .run(
                "files.archive_extract",
                json!({"input": s(&arc), "output": "zip"}),
            )
            .await
            .unwrap();
        assert_eq!(out.artifacts.len(), 1);
        assert!(out.artifacts[0].name.ends_with("-extracted.zip"));
    }
    // Encrypted zip (AES-256).
    let out = h
        .run(
            "files.archive_create",
            json!({"input": [s(&a)], "format": "zip", "password": "pw123"}),
        )
        .await
        .unwrap();
    let enc = out.artifacts[0].path.clone();
    let err = h
        .run("files.archive_extract", json!({"input": s(&enc)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let err = h
        .run(
            "files.archive_extract",
            json!({"input": s(&enc), "password": "nope"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let out = h
        .run(
            "files.archive_extract",
            json!({"input": s(&enc), "password": "pw123"}),
        )
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(&out.artifacts[0].path).unwrap(),
        b"alpha alpha alpha alpha"
    );
    let err = h
        .run(
            "files.archive_create",
            json!({"input": [s(&a)], "format": "tar", "password": "x"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    // Not an archive.
    let err = h
        .run("files.archive_extract", json!({"input": s(&a)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

#[tokio::test]
async fn single_file_compression() {
    let h = harness();
    let data: Vec<u8> = (0..50_000u32)
        .flat_map(|i| (i % 97).to_le_bytes())
        .collect();
    let f = h.write("data.csv", &data);
    for (fmt, ext) in [
        ("gzip", "gz"),
        ("zstd", "zst"),
        ("xz", "xz"),
        ("bzip2", "bz2"),
        ("brotli", "br"),
    ] {
        let out = h
            .run("files.compress", json!({"input": [s(&f)], "format": fmt}))
            .await
            .unwrap();
        let c = out.artifacts[0].path.clone();
        assert_eq!(out.artifacts[0].name, format!("data.csv.{ext}"));
        assert!(out.artifacts[0].size < data.len() as u64);
        let out = h
            .run("files.decompress", json!({"input": [s(&c)]}))
            .await
            .unwrap_or_else(|e| panic!("{fmt}: {e}"));
        assert_eq!(out.artifacts[0].name, "data.csv", "{fmt}");
        assert_eq!(
            std::fs::read(&out.artifacts[0].path).unwrap(),
            data,
            "{fmt}"
        );
        if fmt != "brotli" {
            // Single compressed files also go through archive_extract.
            let out = h
                .run("files.archive_extract", json!({"input": s(&c)}))
                .await
                .unwrap();
            assert_eq!(
                std::fs::read(&out.artifacts[0].path).unwrap(),
                data,
                "{fmt}"
            );
        }
    }
    let err = h
        .run(
            "files.decompress",
            json!({"input": [s(&h.write("x.gz", b"plain"))]}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
}

/// A zip with traversal, absolute, symlink and backslash-traversal entries.
fn evil_zip(path: &Path) {
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let o = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("../evil.txt", "x"),
        ("/abs-evil.txt", "x"),
        ("a/../../evil2.txt", "x"),
        ("..\\win-evil.txt", "x"),
        ("good/ok.txt", "fine"),
        ("C:/drive.txt", "x"),
    ] {
        w.start_file(name, o).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    }
    w.add_symlink("link", "/etc/passwd", o).unwrap();
    w.finish().unwrap();
}

/// Raw tar with malicious headers (the tar crate's safe setters refuse these).
fn evil_tar(path: &Path) {
    let mut out = Vec::new();
    let mut entry = |name: &str, kind: tar::EntryType, body: &[u8], link: &str| {
        let mut h = tar::Header::new_ustar();
        {
            let old = h.as_old_mut();
            old.name[..name.len()].copy_from_slice(name.as_bytes());
            old.linkname[..link.len()].copy_from_slice(link.as_bytes());
        }
        h.set_size(body.len() as u64);
        h.set_mode(0o644);
        h.set_entry_type(kind);
        h.set_cksum();
        out.extend_from_slice(h.as_bytes());
        out.extend_from_slice(body);
        out.resize(out.len().div_ceil(512) * 512, 0);
    };
    entry("../evil.txt", tar::EntryType::Regular, b"x", "");
    entry("/etc/evil-abs.txt", tar::EntryType::Regular, b"x", "");
    entry("link", tar::EntryType::Symlink, b"", "/etc");
    entry("link/through.txt", tar::EntryType::Regular, b"x", "");
    entry("hard", tar::EntryType::Link, b"", "/etc/passwd");
    entry("dev", tar::EntryType::Char, b"", "");
    entry("ok.txt", tar::EntryType::Regular, b"fine", "");
    out.extend_from_slice(&[0u8; 1024]);
    std::fs::write(path, out).unwrap();
}

#[tokio::test]
async fn malicious_archives_are_contained() {
    let h = harness();
    let z = h.path("evil.zip");
    evil_zip(&z);
    let out = h
        .run("files.archive_list", json!({"input": s(&z)}))
        .await
        .unwrap();
    assert!(
        out.data["suspicious"].as_u64().unwrap() >= 6,
        "{}",
        out.data
    );
    assert!(!out.warnings.is_empty());
    let out = h
        .run("files.archive_extract", json!({"input": s(&z)}))
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    assert_eq!(out.artifacts[0].label.as_deref(), Some("good/ok.txt"));
    assert_eq!(std::fs::read(&out.artifacts[0].path).unwrap(), b"fine");
    assert_eq!(out.data["skipped"], 6);

    let t = h.path("evil.tar");
    evil_tar(&t);
    let out = h
        .run("files.archive_list", json!({"input": s(&t)}))
        .await
        .unwrap();
    assert!(out.data["suspicious"].as_u64().unwrap() >= 5);
    let out = h
        .run("files.archive_extract", json!({"input": s(&t)}))
        .await
        .unwrap();
    let labels: Vec<_> = out
        .artifacts
        .iter()
        .map(|a| a.label.clone().unwrap())
        .collect();
    // "link/through.txt" is created as a plain directory + file, never through a link.
    assert!(labels.contains(&"ok.txt".to_string()));
    assert!(labels
        .iter()
        .all(|l| !l.contains("..") && !l.starts_with('/')));
    let reasons = out.data["skipped_entries"]["rows"].to_string();
    assert!(
        reasons.contains("symbolic link")
            && reasons.contains("hard link")
            && reasons.contains("device"),
        "{reasons}"
    );

    // Nothing escaped the sandboxed extraction directories.
    for p in [
        h.dir.path().join("evil.txt"),
        h.dir.path().join("evil2.txt"),
        h.dir.path().parent().unwrap().join("evil.txt"),
        PathBuf::from("/etc/evil-abs.txt"),
        PathBuf::from("/abs-evil.txt"),
    ] {
        assert!(!p.exists(), "{} must not exist", p.display());
    }
}

#[tokio::test]
async fn decompression_bombs_are_stopped() {
    let c = Custom::new(small_limits(1 << 20, 10), FsPolicy::unrestricted());
    // 8 MiB of zeros compresses to a few KB.
    let zeros = vec![0u8; 8 << 20];
    let zp = c.dir.path().join("bomb.zip");
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zp).unwrap());
        w.start_file(
            "zeros.bin",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        w.write_all(&zeros).unwrap();
        w.finish().unwrap();
    }
    assert!(std::fs::metadata(&zp).unwrap().len() < 100_000);
    let err = c
        .run("files.archive_extract", json!({"input": s(&zp)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded", "{err}");
    let out = c
        .run("files.archive_list", json!({"input": s(&zp)}))
        .await
        .unwrap();
    assert!(out.data["table"]["rows"][0][6]
        .as_str()
        .unwrap()
        .contains("ratio"));

    // A zip header that lies about the size: the counted bytes still stop it.
    let tgz = c.dir.path().join("bomb.tar.gz");
    {
        let enc = flate2::write::GzEncoder::new(
            std::fs::File::create(&tgz).unwrap(),
            flate2::Compression::best(),
        );
        let mut b = tar::Builder::new(enc);
        let mut hd = tar::Header::new_gnu();
        hd.set_size(zeros.len() as u64);
        hd.set_mode(0o644);
        hd.set_cksum();
        b.append_data(&mut hd, "zeros.bin", &zeros[..]).unwrap();
        b.into_inner().unwrap().finish().unwrap();
    }
    let err = c
        .run("files.archive_extract", json!({"input": s(&tgz)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded");
    let gz = c.dir.path().join("zeros.gz");
    {
        let mut enc = flate2::write::GzEncoder::new(
            std::fs::File::create(&gz).unwrap(),
            flate2::Compression::best(),
        );
        enc.write_all(&zeros).unwrap();
        enc.finish().unwrap();
    }
    let err = c
        .run("files.decompress", json!({"input": [s(&gz)]}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded");
    let err = c
        .run("files.archive_extract", json!({"input": s(&gz)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded");
    // The user can lower the limit further, but not raise it.
    let small = c.dir.path().join("small.gz");
    {
        let mut enc = flate2::write::GzEncoder::new(
            std::fs::File::create(&small).unwrap(),
            flate2::Compression::best(),
        );
        enc.write_all(&vec![0u8; 600_000]).unwrap();
        enc.finish().unwrap();
    }
    assert!(c
        .run("files.archive_extract", json!({"input": s(&small)}))
        .await
        .is_ok());
    let err = c
        .run(
            "files.archive_extract",
            json!({"input": s(&small), "max_size_mb": 0}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");

    // Too many entries.
    let many = c.dir.path().join("many.zip");
    {
        let mut w = zip::ZipWriter::new(std::fs::File::create(&many).unwrap());
        for i in 0..20 {
            w.start_file(
                format!("f{i}.txt"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            w.write_all(b"x").unwrap();
        }
        w.finish().unwrap();
    }
    let err = c
        .run("files.archive_extract", json!({"input": s(&many)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded");
}

#[tokio::test]
async fn sevenzip_archives() {
    let c = Custom::new(small_limits(1 << 20, 100), FsPolicy::unrestricted());
    if !c.engine.deps.available("7zip") {
        eprintln!("skipping: 7-Zip not installed");
        return;
    }
    let a = c.dir.path().join("a.txt");
    std::fs::write(&a, b"seven").unwrap();
    let out = c
        .run(
            "files.archive_create",
            json!({"input": [s(&a)], "format": "7z", "password": "pw"}),
        )
        .await
        .unwrap();
    let arc = out.artifacts[0].path.clone();
    let err = c
        .run("files.archive_extract", json!({"input": s(&arc)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input", "{err}");
    let out = c
        .run(
            "files.archive_extract",
            json!({"input": s(&arc), "password": "pw"}),
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&out.artifacts[0].path).unwrap(), b"seven");
    // A 7z bomb (declared sizes) is rejected before extraction.
    let big = c.dir.path().join("zeros.bin");
    std::fs::write(&big, vec![0u8; 4 << 20]).unwrap();
    let out = c
        .run(
            "files.archive_create",
            json!({"input": [s(&big)], "format": "7z"}),
        )
        .await
        .unwrap();
    let err = c
        .run(
            "files.archive_extract",
            json!({"input": s(&out.artifacts[0].path)}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "limit_exceeded");
    // Listing still flags dangerous entries.
    let z = c.dir.path().join("evil.zip");
    evil_zip(&z);
    let out = c
        .run("files.archive_list", json!({"input": s(&z)}))
        .await
        .unwrap();
    assert!(out.data["suspicious"].as_u64().unwrap() >= 5);
}

// ---------------------------------------------------------------------------
// Batch rename

fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn batch_rename_preview_apply_collisions() {
    let h = harness();
    let d = h.path("photos");
    std::fs::create_dir_all(&d).unwrap();
    for (n, body) in [
        ("IMG_001.JPG", "1"),
        ("IMG_002.JPG", "2"),
        ("notes.txt", "n"),
    ] {
        std::fs::write(d.join(n), body).unwrap();
    }
    // Preview: nothing changes on disk.
    let out = h.run("files.batch_rename", json!({"path": s(&d), "filter": "*.jpg", "template": "trip-{n:02}{ext}", "case": "lower"})).await.unwrap();
    assert_eq!(out.data["applied"], false);
    assert_eq!(out.data["changes"], 2);
    assert_eq!(out.data["table"]["rows"][0][1], "trip-01.JPG");
    assert_eq!(
        names_in(&d),
        vec!["IMG_001.JPG", "IMG_002.JPG", "notes.txt"]
    );
    // Apply.
    let out = h.run("files.batch_rename", json!({"path": s(&d), "filter": "*.jpg", "find": "IMG_(\\d+)", "replace": "photo-$1", "case": "lower", "apply": true})).await.unwrap();
    assert_eq!(out.data["renamed"], 2);
    assert_eq!(
        names_in(&d),
        vec!["notes.txt", "photo-001.JPG", "photo-002.JPG"]
    );
    assert_eq!(
        std::fs::read_to_string(d.join("photo-002.JPG")).unwrap(),
        "2"
    );
    // Collision: two files → same name; apply refuses and changes nothing.
    let out = h
        .run(
            "files.batch_rename",
            json!({"path": s(&d), "filter": "*.jpg", "template": "same{ext}"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["problems"], 2);
    let err = h
        .run(
            "files.batch_rename",
            json!({"path": s(&d), "filter": "*.jpg", "template": "same{ext}", "apply": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    assert_eq!(
        names_in(&d),
        vec!["notes.txt", "photo-001.JPG", "photo-002.JPG"]
    );
    // Existing file that is not being renamed is never overwritten.
    let err = h.run("files.batch_rename", json!({"path": s(&d), "filter": "photo-001.JPG", "template": "notes.txt", "apply": true})).await.unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    assert_eq!(std::fs::read_to_string(d.join("notes.txt")).unwrap(), "n");
    // Chains work (001→002 while 002→003) thanks to the two-phase rename.
    let out = h.run("files.batch_rename", json!({"path": s(&d), "filter": "*.JPG", "template": "photo-{n:03}{ext}", "start": 2, "apply": true})).await.unwrap();
    assert_eq!(out.data["renamed"], 2);
    assert_eq!(
        names_in(&d),
        vec!["notes.txt", "photo-002.JPG", "photo-003.JPG"]
    );
    assert_eq!(
        std::fs::read_to_string(d.join("photo-002.JPG")).unwrap(),
        "1"
    );
    assert_eq!(
        std::fs::read_to_string(d.join("photo-003.JPG")).unwrap(),
        "2"
    );
    // Invalid names and placeholders.
    let err = h
        .run(
            "files.batch_rename",
            json!({"path": s(&d), "template": "../escape{ext}", "apply": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_input");
    let out = h
        .run(
            "files.batch_rename",
            json!({"path": s(&d), "template": "{nope}"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["problems"], 3);
    // Explicit file list.
    let out = h
        .run(
            "files.batch_rename",
            json!({"files": s(&d.join("notes.txt")), "template": "{name}-v2{ext}", "apply": true}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["renamed"], 1);
    assert!(d.join("notes-v2.txt").exists());
}

// ---------------------------------------------------------------------------
// Filesystem roots and permissions

#[tokio::test]
async fn host_paths_respect_roots_and_permissions() {
    let base = tempfile::tempdir().unwrap();
    let allowed = base.path().join("allowed");
    let other = base.path().join("other");
    tree(&allowed);
    tree(&other);
    let c = Custom::new(
        ExecLimits::default(),
        FsPolicy::roots(vec![allowed.clone()]),
    );
    assert!(c
        .run("files.dir_size", json!({"path": s(&allowed)}))
        .await
        .is_ok());
    let err = c
        .run("files.dir_size", json!({"path": s(&other)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
    let err = c
        .run(
            "files.search",
            json!({"path": s(&allowed.join("../other")), "name": "*"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
    #[cfg(unix)]
    {
        // A symlink inside the root pointing outside is resolved and refused...
        std::os::unix::fs::symlink(&other, allowed.join("escape")).unwrap();
        let err = c
            .run(
                "files.dir_size",
                json!({"path": s(&allowed.join("escape"))}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code(), "permission_denied");
        // ...and never followed while walking, even when following is enabled.
        let out = c
            .run(
                "files.manifest",
                json!({"path": s(&allowed), "follow_symlinks": true}),
            )
            .await
            .unwrap();
        let csv = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
        assert!(!csv.contains("escape/"), "{csv}");
        let out = c
            .run("files.manifest", json!({"path": s(&allowed)}))
            .await
            .unwrap();
        assert!(!std::fs::read_to_string(&out.artifacts[0].path)
            .unwrap()
            .contains("escape"));
    }
    // Rename file lists are policy-checked too.
    let err = c
        .run(
            "files.batch_rename",
            json!({"files": s(&other.join("a.txt")), "template": "x{ext}", "apply": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
    assert!(other.join("a.txt").exists());
    // Without the host_fs permissions the tools are refused outright.
    let read_only = PermissionSet::from_iter([dogubako_core::spec::Permission::HostFsRead]);
    let err = c
        .run_with(
            "files.dir_size",
            json!({"path": s(&allowed)}),
            PermissionSet::none(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
    let err = c
        .run_with(
            "files.batch_rename",
            json!({"path": s(&allowed), "template": "{name}"}),
            read_only.clone(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
    assert!(c
        .run_with("files.duplicates", json!({"path": s(&allowed)}), read_only)
        .await
        .is_ok());
    // Uploaded-file tools need no host permissions.
    let f = allowed.join("a.txt");
    assert!(c
        .run_with(
            "files.hash",
            json!({"input": [s(&f)]}),
            PermissionSet::none()
        )
        .await
        .is_ok());
    // Host access disabled entirely.
    let c2 = Custom::new(ExecLimits::default(), FsPolicy::deny_all());
    let err = c2
        .run("files.dir_size", json!({"path": s(&allowed)}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "permission_denied");
}
