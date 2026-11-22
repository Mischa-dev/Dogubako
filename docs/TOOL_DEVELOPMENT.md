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
requires = []               # dependency ids from KNOWN_DEPS, e.g. ["cups"]
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

`requires` takes the same dependency ids as `.requires(&[..])` (an unknown id is
a parse error): a missing one marks the tool unavailable with the usual install
hint. A `program` that is a required dependency's id or one of its executables
(`whisper`, `lpstat` for `cups`) is found through the dependency registry, so
managed engines and `DOGUBAKO_<ID>_PATH` apply; any other bare program next to a
required dependency's binary (`pdftotext` beside Poppler's `pdftoppm`) is run
from that folder before PATH is searched.

`[output] extract` reads data fields from stdout with regular expressions and
`text` turns them into a result line (the official `system.print` tool):

```toml
[output]
extract = { job_id = 'request id is (\S+)' }   # first group, or the whole match
text = "Queued, job {job_id}"                  # extract fields and params

# Cases: the first whose regex matches gives its value; no regex = fallback.
# {param} / {settings.x} in a pattern match the value literally.
[output.extract]
state = [{ regex = '(?m)^printer \S+ now printing {job_id}\.', value = "printing" }, { value = "not found" }]
```

A field nothing matches fails the run. `[tool] destructive = true` marks a tool
whose run has an effect outside Dogubako (agents ask first, MCP sets
`destructiveHint`), and `action = "Print"` relabels its Run button.

### HTTP requests

For weather, feeds and small APIs, a manifest can send one GET request with
`[http]` instead of running a program (a manifest has `[command]` or `[http]`,
never both). It gets the `network` permission automatically:

```toml
[tool]
id = "data.weather"
name = "Weather"
category = "data"

[http]
url = "https://api.open-meteo.com/v1/forecast?latitude={settings.lat}&longitude={settings.lon}&current=temperature_2m"
timeout_secs = 30   # optional

[output]
json = { temp = "$.current.temperature_2m", unit = "$.current_units.temperature_2m" }

[[settings]]
name = "lat"
type = "text"
required = true

[[settings]]
name = "lon"
type = "text"
required = true
```

Add `headers = { Authorization = "Bearer {settings.api_key}" }` under `[http]`
(with an `api_key` setting declared `secret = true`) for APIs that need a key;
header values may use parameters and settings.

* Only GET; there is no scripting. The scheme and host must be written out —
  placeholders (`{param}`, `{settings.<name>}`) may only fill the path, query
  and header values, and are percent-encoded in the URL. File and path
  parameters cannot be sent.
* Requests use the instance's network policy: private addresses, denied hosts
  and redirects to them are refused; with `headers`, redirects must stay on
  the same origin. Responses larger than `max_stdout_bytes` are refused.
* Output: `json = { field = "<JSON path>" }` picks values into `data` (a path
  that matches nothing is an error); `[output.table]` with `rows = "$.items"`
  and `columns = [{ name = "title", path = "$.title" }]` returns
  a sortable `data.table` (a `rows` path naming one array uses its items; a
  path with `*`, `..` or a filter makes each match a row); otherwise the body
  is returned as text in the response's charset (`body = "<mime>"` overrides
  its content type).
* A non-2xx status or a timeout fails with an `external_failure` error naming
  the status and a short excerpt of the body.
* Manifest `[[tests]]` run offline with a recorded response (`http = {...}`,
  see [Tests](#tests)); in Rust,
  `parse_manifest(..)?.with_recorded_response(RecordedResponse::json(&value))`
  (see `crates/core/src/manifest_http.rs`).

### Settings

Values that belong to one installation (a printer queue, a saved location, an
API key) are declared as `[[settings]]` and used as `{settings.<name>}`; a group
can also use `when = "settings.<name>"`. Settings take the same fields as
`[[params]]` (except `file`, `path`, `json` and multi-choice `enum`) plus `secret = true` for text:

```toml
[[settings]]
name = "queue"
type = "text"
required = true

[[settings]]
name = "api_key"
type = "text"
secret = true
```

Values are stored **outside the manifest**, in `<extensions>/settings/<id>.toml`
(secrets in `secrets/<id>.toml`, owner-only on Unix), so editing them keeps the
trust pin and sharing a manifest never shares its settings. Each installation
has its own values. A tool with a required setting unset is shown unavailable
("Set up <tool> in Settings") and is rejected before it runs. Edit them in
Settings → Extensions (desktop, or admins on a server), with
`dogubako extensions config <id> key=value` (`key=` clears, `key=-` reads the
value from stdin), or
`PUT /api/v1/extensions/<id>/settings`. Secret values are never echoed back.
Unknown `{settings.x}` names are rejected when the manifest is parsed.

### Tests

Optional `[[tests]]` run right after install and with
`dogubako extensions test <id>`: sample `params`, sample input `files`
(contents keyed by file parameter; the parameter's value, if given, names the
file) and checks in `expect`:

```toml
[[tests]]
name = "encodes a sample"
params = { quality = 50, input = "a.png" }
files = { input = "..." }
expect = { files = 1 }

[[tests]]
params = { word = '{"ok":true}' }
expect = { text = "...", contains = ["ok"], json = { "$.ok" = true } }
# expect = { error = "invalid_input" } passes only if the run fails that way
```

`json` maps a path (`$.a.b[0]`) to the expected value, checked against JSON
text output (or the structured data). A test whose tool is unavailable here
(missing program, unset required setting) is reported `skipped`, never passed.
Tests run with every permission but only see their own sample files.

`[http]` manifests can record the response a test gets instead of calling the
network, and any test can give its own `settings` instead of the user's
(`settings = {}` uses the manifest defaults):

```toml
[[tests]]
params = { city = "Boulder" }
settings = { lat = "40.0" }
http = { json = { current = { temp = 21.5 } } }   # or body = "...", status = 500, content_type = "..."
expect = { json = { "$.temp" = 21.5 } }
```

`[command]` manifests can record the program's output the same way, so the test
runs without the program installed (arguments are still built and checked):

```toml
[[tests]]
params = { file = "a.pdf" }
files = { file = "%PDF-1.4" }
settings = { queue = "Office" }
command = { stdout = "request id is Office-12 (1 file(s))" }   # exit_code = 1 to test a failure
expect = { text = "Queued, job Office-12" }
```

### Install, uninstall, test

```
dogubako extensions install ./weather.toml      # or an https:// URL
dogubako extensions test text.weather
dogubako extensions uninstall text.weather [--purge]
```

Install validates the manifest, shows its permissions and requirements (and
asks unless `--yes`/`--json`), copies it to `<extensions>/<id>.toml`, pins its
SHA-256, reloads and runs its tests, returning
`{id, version, trusted, loaded, tests: [{name, status, error}], passed, error}`
(exit code 1 when `passed` is false). An id that collides with a built-in or
another extension fails and changes nothing; `--replace` updates an installed
extension. A failing test leaves the extension installed and reports it
failing. Uninstall removes the manifest and its trust entry and reloads;
settings are kept unless `--purge`. The same exist as admin endpoints
(`POST /api/v1/extensions/install`, off unless the server sets
`allow_extension_install`, `DELETE /api/v1/extensions/<id>`,
`POST /api/v1/extensions/<id>/test`) and desktop commands
(`install_extension`, `uninstall_extension`, `test_extension`).

### Official tool store

Dogubako ships with the full toolbox; the store adds reviewed extension tools
on top, so you only install what you need. Store tools live in the repository
as `store/<id>/manifest.toml` (folder named after the tool id, with offline
`[[tests]]`). `dogubako store build` writes the index and manifests to
`site/public/store/`, published with the docs site at
<https://docs.dogubako.app/store/index.json>; CI fails when it is stale and
runs every store tool's tests (`dogubako store test`). Bump `version` when you
change a manifest.

```
dogubako store search weather       # cached index when offline
dogubako store install data.weather # SHA-256 must match the index, then install/trust/test
dogubako store list                 # installed store tools, versions and updates
dogubako store update [id]          # install newer versions
```

The same is in Settings → Tool store (desktop and server), over HTTP
(`GET /api/v1/store?q=`, `GET /api/v1/store/installed`,
`POST /api/v1/store/install {id}`, `POST /api/v1/store/update {id?}`; admin,
installs need `allow_extension_install`) and as desktop commands
(`store_search`, `store_installed`, `store_install`, `store_update`). The index
is fetched through the network policy and cached in
`<extensions>/store/`. `DOGUBAKO_STORE_URL` (server: `store_url`) points at
another index for self-hosting; empty turns the store off. The store is
official only: tools are added by pull request.

### Trust and permissions

* Manifests live in the per-user extensions folder (`~/.local/share/dogubako/extensions`,
  `~/Library/Application Support/Dogubako/extensions`, `%APPDATA%\Dogubako\extensions`)
  or `<data_dir>/extensions` on a server.
* A manifest is **only loaded after it is explicitly trusted**:
  `dogubako extensions install` or `dogubako extensions trust <id>` pins its
  SHA-256 in `trusted.toml`. Any
  edit invalidates trust until it is reviewed and trusted again.
* Shells and script interpreters (`sh`, `bash`, `cmd`, `powershell`, `python`,
  `node`, `perl`, `env`, `sudo`…) are refused as programs.
* Extension tools run through the same engine, sandbox and limits as builtins;
  their declared permissions are enforced, and per-agent policies apply
  (`allow = ["image.cwebp"]`).
* Tool ids may not collide with builtins.
* No restart is needed: the desktop app and `dogubako mcp serve` reload when the
  extensions folder changes, and a server admin reloads with **Settings →
  Dependencies → Reload tools** or `POST /api/v1/admin/reload`. Runs already
  started finish with the version of the tool they started with.

Compiled extensions are also supported the normal Rust way: add a crate that
exposes `pub fn tools() -> Vec<Arc<dyn Tool>>` and register it in `crates/tools`.
