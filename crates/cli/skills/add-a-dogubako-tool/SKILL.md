---
name: add-a-dogubako-tool
description: Give your agents a new Dogubako tool ("I want my agents to do X"). Finds an installed or store tool first; otherwise writes a TOML manifest, installs it in your dev slot, tests it and asks the owner to promote it.
---

# Add a Dogubako tool

Follow these steps in order. Each step names the MCP tool first and the CLI
command second; use whichever you have. MCP names are tool ids with dots as
underscores (`data.weather` is `data_weather`).

## 1. Look for an existing tool

Search before writing anything:

- MCP: `dogubako_search_tools` with `{"query": "<what X is>", "include_available": true}`.
- CLI: `dogubako tools search <words>`, then `dogubako store search <words>`.

Read the `tools` array, not `total` (which counts installed matches only).
MCP sees store tools only once the store index is cached, so on a fresh
install an empty MCP result proves nothing: run `dogubako store search <words>`
(it fetches the index) or ask the owner to, before writing a new tool.

What you find decides the next step:

| Found | Do |
|---|---|
| An installed tool (`installed` is not false) | Use it: `dogubako_describe_tool`, then call it. Done |
| A turned-off pack (`pack` is set) | Ask the owner to run `dogubako packs enable <pack>`. Done |
| A store tool (`source` is the store) | `dogubako_store_install` with `{"id": "<id>"}` (CLI: `dogubako store install <id>`). If the report lists `unset_settings`, ask the owner to set them (step 5). Done |
| Nothing | Write a new tool (steps 2 to 5) |

## 2. Pick the instance

Install the tool on the Dogubako your agents call, since a dev tool is only
visible to whoever installed it:

- **A self-hosted server** (HTTP MCP at `https://<host>/mcp`): your token needs
  the `extensions_author` permission and the server needs
  `allow_extension_install`. If `dogubako_extension_install` is missing or
  returns `permission_denied`, ask an admin to grant it (Admin → Users →
  Capabilities, "Author tools (MCP)").
- **This machine** (stdio): `dogubako mcp serve --permissions network,extensions_author`,
  or the CLI commands below, which write to the local extensions folder.

Over stdio, send `initialize`, then one JSON-RPC line per call:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"dogubako_extension_status","arguments":{"id":"data.weather"}}}
```

## 3. Write the manifest

A tool is one TOML file. Pick the shape and start from its golden manifest:

- **Calls a web API** (one HTTP GET, JSON back): copy the weather tool,
  <https://docs.dogubako.app/store/data.weather/manifest.toml>.
- **Runs a program** (with per-install settings such as a printer queue): copy
  the print tool, <https://docs.dogubako.app/store/system.print/manifest.toml>.

### Worked example: HTTP GET (weather)

```toml
[tool]
id = "data.weather"              # <category>.<snake_name>, unique
name = "Weather"
category = "data"                # image video audio speech pdf files text dev data network design system
description = "Current conditions for a saved or given location (Open-Meteo, no API key)"
version = "1.0.0"

[http]                           # gets the network permission automatically
url = "https://api.open-meteo.com/v1/forecast?latitude={latitude|settings.latitude}&longitude={longitude|settings.longitude}&current=temperature_2m,weather_code"

[output]
json = { temperature = "$.current.temperature_2m", unit = "$.current_units.temperature_2m" }
text = "Now {temperature}{unit}"

[[params]]                       # per-run inputs; {latitude|settings.latitude} prefers this
name = "latitude"
type = "number"

[[params]]
name = "longitude"
type = "number"

[[settings]]                     # per-install values, set by the owner
name = "latitude"
type = "number"

[[settings]]
name = "longitude"
type = "number"

[[tests]]                        # runs offline with a recorded response
settings = { latitude = 40.01, longitude = -105.27 }
http = { json = { current = { temperature_2m = 21.5 }, current_units = { temperature_2m = "°C" } } }
expect = { text = "Now 21.5°C", json = { "$.temperature" = 21.5 } }
```

Only GET; scheme and host are written out, placeholders fill the path, query
and headers. An API key is a `[[settings]]` text with `secret = true`, used as
`headers = { Authorization = "Bearer {settings.api_key}" }`. Lists of results:
`[output.table]` with `rows = "$.items"` (`rows = "$"` when the body is the
array) and `columns = [{ name = "title", path = "$.title" }]`.

### Worked example: command with settings (print)

```toml
[tool]
id = "system.print"
name = "Print"
category = "system"
description = "Send a PDF to a CUPS printer queue. Returns the job ID once queued"
version = "1.0.0"
permissions = ["network"]        # lp talks to CUPS on localhost:631
platforms = ["linux", "macos"]
requires = ["cups"]              # dependency ids from `dogubako deps`
destructive = true               # it acts outside Dogubako: agents ask first
action = "Print"

[command]
program = "lp"                   # no shells or interpreters (sh, python, node...)
args = ["-d", "{settings.queue}", { when = "copies", args = ["-n", "{copies}"] }, "{file}"]

[output]
extract = { job_id = '(\S+-\d+)' }   # regex over stdout
text = "Queued, job {job_id}"

[[params]]
name = "file"
type = "file"
accept = ["application/pdf", ".pdf"]
required = true
primary = true

[[params]]
name = "copies"
type = "integer"
min = 1

[[settings]]
name = "queue"
type = "text"
required = true                  # unset: the tool shows "Set up Print in Settings"

[[tests]]                        # records the program's output; lp need not be installed
params = { file = "report.pdf" }
files = { file = "%PDF-1.4\n%%EOF\n" }
settings = { queue = "Office" }
command = { stdout = "request id is Office-12 (1 file(s))\n" }
expect = { text = "Queued, job Office-12" }

[[tests]]                        # the failure case: a required input is missing
settings = { queue = "Office" }
command = { stdout = "" }
expect = { error = "invalid_input" }
```

Each placeholder is exactly one argument. Use `{output}` with
`[output] file = "{input.stem}.webp"` for a file result, or `stdout = "text/plain"`.

Rules: always add `[[tests]]` with recorded `http` or `command` output (plus
an `expect = { error = "..." }` case); never hardcode an install's values,
make them `[[settings]]`; set `destructive = true` when running it changes
something outside Dogubako.

## 4. Install in your dev slot and test

- MCP: `dogubako_extension_install` with `{"manifest": "<TOML text>"}`; add
  `"replace": true` for each later version.
- CLI: `dogubako extensions install --dev <file>.toml` (add `--replace`).

Read the report and fix until `passed` is true (`loaded` alone is not
enough: a tool with failing tests still loads):

| Report | Fix |
|---|---|
| `parse_error` / `error` | The message names the key; fix the manifest and reinstall |
| a test `failed` | Compare its `error` with `expect`; fix the manifest or the test |
| a test `skipped` | `missing` programs or `unset_settings`: give the test its own `settings`, record `command`/`http` output |

Rerun tests with `dogubako_extension_test` (CLI: `dogubako extensions test <id>`)
and check loading with `dogubako_extension_status`. The tool is callable right
away by its MCP name (or `dogubako run <id> -s key=value`). Try one real run.

## 5. Hand it to the owner

You cannot make your tool trusted, and must not try. Tell the owner, in one
message:

1. The tool id, what it does, its permissions and `requires`.
2. Settings to fill in: Settings → Extensions, or `dogubako extensions config <id> key=value`.
3. To promote it (review the manifest first): the Promote button on the tool in
   the app, or `dogubako extensions promote <id>` (it asks to confirm; `--yes`
   skips).

Until then the tool works only for you and is marked Dev. A tool others would
want can go in the official store by pull request (`store/<id>/manifest.toml`).
