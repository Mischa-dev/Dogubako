# Developing tools

Every Dogubako tool is described once, by a declarative `ToolSpec`, and
implemented once. The desktop app, web UI, CLI, HTTP API and MCP server are all
generated from that spec and all execute through the same engine
(`dogubako_core::engine::Engine::run`), which enforces authorization,
permissions, dependency checks, validation, timeouts and cancellation.

There are two ways to add a tool:

1. **Rust tool** in one of the `crates/tools-*` crates (full power, tested, shipped with the app).
2. **Declarative extension**: a TOML manifest wrapping a command-line program — no compilation.

## Anatomy of a Rust tool

```rust
use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![reverse_lines()]
}

fn reverse_lines() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.reverse_lines", "Reverse Lines", Category::Text)
        .describe("Reverse the order of lines")
        .keywords(&["flip", "invert order"])
        .param(ParamSpec::textarea("text").required().primary())
        .param(ParamSpec::boolean("skip_empty").label("Skip empty lines"))
        .text_output("text/plain")
        .ui("live"); // fast & pure: the UI re-runs it as you type
    sync_tool(spec, |_ctx, p| {
        let mut lines: Vec<&str> = p.text("text").lines().collect();
        if p.bool("skip_empty") {
            lines.retain(|l| !l.trim().is_empty());
        }
        lines.reverse();
        Ok(ToolOutput::text(lines.join("\n")))
    })
}
```

That is all. The tool is now:

* in the UI catalog, command palette and search, with a generated form;
* `dogubako text reverse-lines "a\nb" --skip-empty` and `dogubako run text.reverse_lines --set text=...`;
* `POST /api/v1/tools/text.reverse_lines/run` with an OpenAPI schema;
* an MCP tool `text_reverse_lines` with a JSON input schema (subject to per-agent policies).

### Spec reference (`crates/core/src/spec.rs`)

| Builder | Meaning |
|---|---|
| `ToolSpec::new(id, name, category)` | `id` is `<category>.<snake_name>`, stable forever |
| `.describe()`, `.keywords()` | Search & discovery (humans and agents) |
| `.param(ParamSpec::…)` | `text`, `textarea`, `secret`, `integer`, `number`, `boolean`, `choice`, `multi_choice`, `color`, `file`, `files`, `path`, `json` |
| param modifiers | `.required()`, `.default_*()`, `.range(lo, hi)`, `.accept(&["image/*", ".pdf"])`, `.advanced()`, `.primary()` (positional CLI arg), `.show_if(param, values)`, `.syntax("json")` |
| `.text_output(mime)` / `.output(OutputKind::Files\|Data)` | Result kind |
| `.requires(&["ffmpeg"])` | External programs (see `crates/core/src/deps.rs`) |
| `.requires_model("whisper")` | Needs at least one model of this group |
| `.permission(Permission::Network)` | Capabilities beyond processing its inputs: `network`, `host_fs_read`, `host_fs_write`, `system_info`, `process_info` |
| `.platforms(&[Platform::Linux])` | Restrict platforms |
| `.timeout(secs)`, `.progress()` | Resource expectations |
| `.destructive()` | Changes state outside the job (e.g. batch rename with `apply`) |
| `.ui("face-redact")` | Use a specialized frontend component |

### Implementation helpers (`ToolContext`)

* `p.text()`, `p.str()`, `p.opt_int()`, `p.num_or()`, `p.bool()`, `p.list()`, `p.json()`, `p.file()`, `p.files()`, `p.path()` — typed, already validated values.
* `ctx.output_path(name)` → unique, sanitized path inside the run's output directory; `ctx.artifact(path)` to return it; `ctx.write_output(name, bytes)`.
* `ctx.command("ffmpeg")?.arg("-i").arg(&input.path)….run().await?` — safe external execution: argument vectors only (never a shell), cleared environment, private `HOME`/`TMPDIR`, timeouts, cancellation that kills the whole process group, Unix rlimits, optional bubblewrap sandbox, bounded output capture. `on_stdout_line` / `on_stderr_line` for progress parsing.
* `ctx.progress(fraction, msg)`, `ctx.check_cancelled()?` in loops.
* `ctx.blocking(|| …)` for CPU-heavy code inside async tools (`sync_tool` already runs on the blocking pool).
* `ctx.http_client()` — SSRF-guarded HTTP client honoring the instance's network policy; `dogubako_core::net::resolve_checked` for raw sockets.
* `ctx.limits()` — decompression/extraction/pixel limits to respect.

### Data conventions for rich rendering

Return structured `data` and the UI renders it automatically:
`data.table = {columns, rows}`, `data.colors = [{hex, name}]`,
`data.diff = [{tag, text}]`, `data.regions = [{x, y, width, height, label, score}]`,
`data.matches = [{start, end, text, groups}]`. Text outputs with mime
`text/markdown`, `text/html` (rendered in a sandboxed frame), `image/svg+xml`
and `application/json` get previews.

### Testing

```rust
#[tokio::test]
async fn reverses() {
    let h = dogubako_core::testing::TestHarness::new(super::tools());
    let out = h.run("text.reverse_lines", serde_json::json!({"text": "a\nb"})).await.unwrap();
    assert_eq!(out.text.as_deref(), Some("b\na"));
    // Malformed input is rejected by the engine before your code runs:
    assert_eq!(h.run("text.reverse_lines", serde_json::json!({"txt": 1})).await.unwrap_err().code(), "invalid_input");
}
```

Tests that need external programs should skip when unavailable:
`if !h.available("video.compress") { return; }`.

### Adding an external dependency

Add a `DepSpec` to `KNOWN_DEPS` in `crates/core/src/deps.rs` (binary names,
version probe, license, install hints for apt/dnf/pacman/brew/winget). It then
appears in `dogubako deps`, the Settings → Dependencies page and the API, and
tools declaring it are marked unavailable with an install hint when it is
missing. Users can point to a binary explicitly with `DOGUBAKO_<ID>_PATH`.

## Declarative extensions

Wrap any command-line program with a TOML manifest
(see [`extensions/cwebp.toml`](../extensions/cwebp.toml)):

```toml
[tool]
id = "image.cwebp"
name = "Encode WebP (cwebp)"
category = "image"
description = "Encode an image to WebP with Google's cwebp"
version = "1.0.0"
permissions = []            # e.g. ["network"]
timeout_secs = 300

[command]
program = "cwebp"
args = ["-q", "{quality}", { when = "lossless", args = ["-lossless"] }, "{input}", "-o", "{output}"]

[output]
file = "{input.stem}.webp"  # or: stdout = "text/plain"

[[params]]
name = "input"
type = "file"
accept = ["image/*"]
required = true
primary = true

[[params]]
name = "quality"
type = "integer"
min = 0
max = 100
default = 80
```

Placeholders: `{param}`, `{file_param}` (path), `{file_param.stem|ext|name}`,
`{output}`, `{workdir}`. **Each placeholder expands into exactly one argument**
— there is no shell, so file names like `a; rm -rf ~.png` are harmless. Argument
groups `{ when = "flag", args = [...] }` are included only when the parameter is
true / non-empty. Parameters use the same schema as Rust tools (`type` =
`text`, `secret`, `integer`, `number`, `boolean`, `enum` (`options = [{value, label}]`),
`color`, `file`, `path`, `json`).

### Trust and permissions

* Manifests live in the per-user extensions folder (`~/.local/share/dogubako/extensions`,
  `~/Library/Application Support/Dogubako/extensions`, `%APPDATA%\Dogubako\extensions`)
  or `<data_dir>/extensions` on a server.
* A manifest is **only loaded after it is explicitly trusted**:
  `dogubako extensions trust <id>` pins its SHA-256 in `trusted.toml`. Any
  edit invalidates trust until it is reviewed and trusted again.
* Shells and script interpreters (`sh`, `bash`, `cmd`, `powershell`, `python`,
  `node`, `perl`, `env`, `sudo`…) are refused as programs.
* Extension tools run through the same engine, sandbox and limits as builtins;
  their declared permissions are enforced, and per-agent policies apply
  (`allow = ["image.cwebp"]`).
* Tool ids may not collide with builtins.

Compiled extensions are also supported the normal Rust way: add a crate that
exposes `pub fn tools() -> Vec<Arc<dyn Tool>>` and register it in `crates/tools`.
