---
title: Writing a Rust tool
description: Add a built-in Dogubako tool with a declarative spec and one function, then test it.
sidebar:
  order: 3
---

Every tool is described once, by a declarative `ToolSpec`, and implemented
once. The app, CLI, HTTP API and MCP server are all generated from that spec,
and every run goes through `Engine::run`, which enforces authorization,
permissions, dependency checks, validation, timeouts and cancellation. Your
code only receives validated parameters.

There are two ways to add a tool:

1. **A Rust tool** in one of the `crates/tools-*` crates: full power, tested,
   shipped with the app. This page.
2. **A declarative extension**: a TOML manifest that wraps a command-line
   program, with no compilation. See [Extensions](/develop/extensions/).

## Anatomy of a tool

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
        .ui("live"); // fast and pure: the UI re-runs it as you type
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

That's all. The tool is now:

- in the app's catalog, search and drop-screen suggestions, with a generated
  form;
- `dogubako text reverse-lines "a\nb" --skip-empty` and
  `dogubako run text.reverse_lines --set text=…`;
- `POST /api/v1/tools/text.reverse_lines/run`, with an OpenAPI schema;
- the MCP tool `text_reverse_lines`, subject to per-agent policies.

Each tool crate exposes `pub fn tools() -> Vec<Arc<dyn Tool>>`, and
`crates/tools` aggregates them.

## Spec reference

Defined in `crates/core/src/spec.rs`.

| Builder | Meaning |
|---|---|
| `ToolSpec::new(id, name, category)` | `id` is `<category>.<snake_name>` and stable forever |
| `.describe()`, `.keywords()` | Search and discovery, for people and agents |
| `.param(ParamSpec::…)` | `text`, `textarea`, `secret`, `integer`, `number`, `boolean`, `choice`, `multi_choice`, `color`, `file`, `files`, `path`, `json` |
| Parameter modifiers | `.required()`, `.default_*()`, `.range(lo, hi)`, `.accept(&["image/*", ".pdf"])`, `.advanced()`, `.primary()` (a positional CLI argument), `.show_if(param, values)`, `.syntax("json")` |
| `.text_output(mime)`, `.output(OutputKind::Files \| Data)` | The kind of result |
| `.requires(&["ffmpeg"])` | External programs; ids are in `crates/core/src/deps.rs` |
| `.requires_model("whisper")` | Needs at least one model of this group |
| `.permission(Permission::Network)` | Capabilities beyond processing its inputs: `network`, `host_fs_read`, `host_fs_write`, `system_info`, `process_info` |
| `.platforms(&[Platform::Linux])` | Restrict platforms |
| `.timeout(secs)`, `.progress()` | Resource expectations |
| `.destructive()` | Changes state outside the job, such as batch rename with `apply` |
| `.ui("face-redact")` | Use a specialized frontend component |

## Implementation helpers

`ToolContext` and `Params` live in `crates/core/src/tool.rs` and
`crates/core/src/params.rs`.

- `p.text()`, `p.str()`, `p.opt_int()`, `p.num_or()`, `p.bool()`, `p.list()`,
  `p.json()`, `p.file()`, `p.files()`, `p.path()`: typed values, already
  validated.
- `ctx.output_path(name)` gives a unique, sanitized path inside the run's
  output folder. Return it with `ctx.artifact(path)`, or use
  `ctx.write_output(name, bytes)`. Derive names from inputs with
  `dogubako_core::util::suffixed(&file.name, "compressed", Some("webp"))`.
- `ctx.command("ffmpeg")?.arg("-i").arg(&input.path)….run().await?` runs an
  external program safely: argument vectors only (never a shell), a cleared
  environment, a private `HOME` and `TMPDIR`, timeouts, cancellation that
  kills the whole process group, Unix resource limits, the optional
  bubblewrap sandbox and bounded output capture. Use `on_stdout_line` and
  `on_stderr_line` to parse progress.
- `ctx.progress(fraction, msg)`, and `ctx.check_cancelled()?` inside loops.
- `ctx.blocking(|| …)` for CPU-heavy code inside an async tool. `sync_tool`
  already runs on the blocking pool.
- `ctx.http_client()` is an SSRF-guarded HTTP client that honors the
  instance's network policy; `dogubako_core::net::resolve_checked` for raw
  sockets.
- `ctx.limits()` holds the decompression, extraction and pixel limits to
  respect.

## Rich results

Return structured `data` and the app renders it automatically:

| Field | Rendered as |
|---|---|
| `data.table = {columns, rows}` | A sortable table |
| `data.colors = [{hex, name}]` | Color swatches |
| `data.diff = [{tag, text}]` | An inline diff (`equal`, `insert`, `delete`) |
| `data.regions = [{x, y, width, height, label, score}]` | Boxes over the first input image |
| `data.matches = [{start, end, text, groups}]` | Regex highlights over the input text |
| Any other object | A key and value list with a JSON tree |

Text outputs with MIME `text/markdown`, `text/html` (rendered in a sandboxed
frame), `image/svg+xml` and `application/json` get previews.

## Rules of the house

- **No fake tools.** Every registered tool must really work. If something
  can't be done properly, don't register it.
- **Reuse mature libraries and programs.** Prefer well-maintained crates and
  external programs through `ctx.command("<dep-id>")`. Declare external
  dependencies with `.requires(…)` so the engine reports them, rather than
  failing at registration. Licenses must be compatible with AGPL-3.0-or-later.
- **Security.** Never use a shell. Pass paths as separate arguments, and guard
  any user-controlled value that an external program could mistake for an
  option. Only read files given as parameters. Tools that touch host paths use
  `ParamSpec::path` and declare `HostFsRead` or `HostFsWrite`; destructive
  operations preview by default and need an explicit `apply` or `confirm`.
  Archive extraction must reject traversal and enforce extraction limits.
  Image decoding must respect `max_image_pixels`. Network tools must use
  `ctx.http_client()` or `resolve_checked` and declare `Network`.
- **Long work** reports progress, checks for cancellation, and sets a sensible
  timeout. FFmpeg tools use `-progress pipe:1 -nostats` and parse
  `out_time_us`.
- **Batch.** Prefer `ParamSpec::files("input")` for operations that make sense
  in bulk, with one output per input.
- **Spec quality.** A clear one-sentence description, useful keywords,
  labels, defaults, `.advanced()` for rare options, `.primary()` on the main
  inputs, `.accept()` on file parameters and `.show_if()` for conditional
  options.

## Testing

```rust
#[tokio::test]
async fn reverses() {
    let h = dogubako_core::testing::TestHarness::new(super::tools());
    let out = h.run("text.reverse_lines", serde_json::json!({"text": "a\nb"})).await.unwrap();
    assert_eq!(out.text.as_deref(), Some("b\na"));
    // Malformed input is rejected by the engine before your code runs:
    assert_eq!(
        h.run("text.reverse_lines", serde_json::json!({"txt": 1})).await.unwrap_err().code(),
        "invalid_input"
    );
}
```

Tests that need external programs should skip when they're missing:

```rust
if !h.available("video.compress") { return; }
```

Generate fixtures in the test (the `image` crate, `ffmpeg -f lavfi -i testsrc`,
`lopdf`…) rather than committing binary files.

## Adding an external dependency

Add a `DepSpec` to `KNOWN_DEPS` in `crates/core/src/deps.rs`: binary names, a
version probe, the license and install hints for apt, dnf, pacman, brew and
winget. It then appears in `dogubako deps`, **Settings → Dependencies** and
the API, and tools that declare it are marked unavailable with an install
hint when it's missing. Users can point to a binary with
`DOGUBAKO_<ID>_PATH`.

After adding or changing tools, regenerate the docs catalog with
`npm run gen:tools` in `site/` (see [The docs site](/develop/docs-site/)).
