//! CLI integration tests (run the real binary).

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn cmd(home: &tempfile::TempDir) -> Command {
    let mut c = Command::cargo_bin("dogubako").unwrap();
    c.env("DOGUBAKO_HOME", home.path());
    c
}

#[test]
fn lists_and_searches_tools() {
    let h = tempfile::tempdir().unwrap();
    let out = cmd(&h).args(["tools", "list", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let tools = v.as_array().unwrap();
    assert!(tools.len() > 100, "expected a large catalog, got {}", tools.len());
    cmd(&h).args(["tools", "search", "case"]).assert().success().stdout(predicate::str::contains("text.case"));
    cmd(&h).args(["tools", "info", "text.case"]).assert().success().stdout(predicate::str::contains("Parameters"));
    cmd(&h).args(["tools", "info", "nope.nope"]).assert().code(2);
}

#[test]
fn runs_generated_command_with_stdin() {
    let h = tempfile::tempdir().unwrap();
    cmd(&h).args(["text", "case", "--case", "upper"]).write_stdin("hello cli").assert().success().stdout("HELLO CLI\n");
    cmd(&h).args(["text", "case", "hi there", "--case", "snake"]).assert().success().stdout("hi_there\n");
}

#[test]
fn json_mode_and_exit_codes() {
    let h = tempfile::tempdir().unwrap();
    let out = cmd(&h).args(["--json", "run", "text.case", "--set", "text=abc", "--set", "case=upper"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["text"], "ABC");
    // Invalid enum value is caught by clap for generated commands (usage error = 2)
    cmd(&h).args(["text", "case", "x", "--case", "nonsense"]).assert().code(2);
    // Engine validation error through the generic runner
    let out = cmd(&h).args(["--json", "run", "text.case", "--set", "bogus=1"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "invalid_input");
    cmd(&h).args(["run", "no.such_tool"]).assert().code(2).stderr(predicate::str::contains("unknown tool"));
}

#[test]
fn writes_output_files() {
    let h = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let input = work.path().join("a.txt");
    std::fs::write(&input, "hello").unwrap();
    let dest = work.path().join("out.gz");
    cmd(&h).current_dir(work.path()).args(["files", "compress"]).arg(&input).arg("-o").arg(&dest).assert().success();
    assert!(dest.is_file());
    // Never overwrite without --force
    cmd(&h).current_dir(work.path()).args(["files", "compress"]).arg(&input).arg("-o").arg(&dest).assert().success();
    assert!(work.path().join("out (1).gz").is_file());
}

#[test]
fn matrix_and_openapi() {
    let h = tempfile::tempdir().unwrap();
    cmd(&h).args(["tools", "matrix"]).assert().success().stdout(predicate::str::contains("| Category |"));
    let out = cmd(&h).args(["api", "openapi"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["openapi"], "3.1.0");
    assert!(v["paths"]["/api/v1/tools/text.case/run"].is_object());
}

#[test]
fn mcp_stdio_roundtrip() {
    let h = tempfile::tempdir().unwrap();
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#, "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"text_case","arguments":{"text":"mcp","case":"upper"}}}"#, "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"network_dns_lookup","arguments":{"name":"example.com"}}}"#, "\n",
    );
    let out = cmd(&h)
        .args(["mcp", "serve", "--allow", "text.*", "--output-dir"])
        .arg(h.path().join("out"))
        .write_stdin(input)
        .timeout(std::time::Duration::from_secs(30))
        .output()
        .unwrap();
    let lines: Vec<Value> = String::from_utf8_lossy(&out.stdout).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    let by_id = |id: i64| lines.iter().find(|l| l["id"] == id).cloned().unwrap_or_else(|| panic!("no response {id}: {lines:?}"));
    assert_eq!(by_id(1)["result"]["serverInfo"]["name"], "dogubako");
    let names: Vec<String> = by_id(2)["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert!(names.contains(&"text_case".to_string()));
    assert!(!names.iter().any(|n| n.starts_with("network_")));
    assert_eq!(by_id(3)["result"]["structuredContent"]["text"], "MCP");
    assert_eq!(by_id(4)["result"]["isError"], true);
}
