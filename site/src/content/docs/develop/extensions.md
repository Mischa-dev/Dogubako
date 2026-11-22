---
title: Extensions
description: Wrap any command-line program as a Dogubako tool with a TOML manifest, no Rust required.
sidebar:
  order: 4
---

A declarative extension turns a command-line program into a full Dogubako
tool: it appears in the app with a form, in the CLI, the API and MCP, and runs
through the same engine, sandbox, limits and policies as built-in tools.

Before writing one, check that the tool doesn't exist yet: built-in tools
are grouped in **packs** (one per category; a turned-off pack hides its tools
until `dogubako packs enable <pack>`), and the [official store](#official-tool-store)
has reviewed extensions you install in one step. `dogubako tools search <words>`
and `dogubako store search <words>` cover both.

## Let an agent build it

`dogubako skill` prints a short agent skill (`SKILL.md`) that takes an agent
from "I want my agents to do X" to a working tool: it searches installed tools,
packs and the store first, then writes a manifest from one of the two worked
examples below, installs it in its [dev slot](#dev-slot), tests it and asks you
to promote it. Save it where your agent reads skills:

```sh
mkdir -p .agents/skills/add-a-dogubako-tool
dogubako skill > .agents/skills/add-a-dogubako-tool/SKILL.md
```

Over MCP the agent needs the `extensions_author` permission
(`dogubako mcp serve --permissions network,extensions_author`, or "Author tools
(MCP)" for its user or token on a server with `allow_extension_install`). It then
uses `dogubako_extension_install`, `dogubako_extension_test`,
`dogubako_extension_status` and `dogubako_store_install`; it can never promote
its own tool. See [Local agents](/agents/local/).

## Example

This manifest wraps Google's `cwebp` encoder (a fuller version ships as
[`extensions/cwebp.toml`](https://github.com/mischa-dev/dogubako/blob/main/extensions/cwebp.toml)):

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

[[params]]
name = "lossless"
type = "boolean"
```

## Install it in one step

```sh
dogubako extensions install extensions/cwebp.toml   # or an https:// URL
dogubako image cwebp photo.png --quality 90
```

Install validates the manifest, shows its permissions and `requires` (and asks
unless `--yes`), copies it into the extensions folder, pins its SHA-256,
reloads and runs its `[[tests]]`. `--replace` updates an installed copy;
`dogubako extensions uninstall <id>` removes it (settings are kept unless
`--purge`). On a server the same is `POST /api/v1/extensions/install`, off
unless `allow_extension_install` is set.

A manifest is **only loaded while trusted**: install pins its hash in
`trusted.toml`, and any later edit invalidates the trust until you review it
and run `dogubako extensions trust <id>` (also how you trust a file you copied
into the folder by hand).

### Dev slot

While you or an agent are still writing a tool, install it in the **dev slot**
instead: `dogubako extensions install --dev wip.toml`, or copy it to
`extensions/dev/`. Dev tools load without a trust pin and reload on every
edit, but only their author (the user or API token that installed them) can see
or run them, they are marked **Dev**, and they can't shadow a built-in or
trusted tool. When it is ready, a human owner reviews and promotes it, which
moves it to the trusted folder and pins its hash; an API token or MCP agent
cannot:

```sh
dogubako extensions promote image.cwebp   # or the Promote button in the app
```

### Official tool store

Reviewed tools are published at
<https://docs.dogubako.app/store/index.json>. Installing one checks its
manifest against the index's SHA-256, then installs, trusts and tests it:

```sh
dogubako store search weather
dogubako store install data.weather
dogubako store list            # installed store tools and available updates
dogubako store update          # install newer versions
```

The same is in Settings → Tool store and, for agents, `dogubako_store_install`
over MCP. Store tools are added by pull request as `store/<id>/manifest.toml`;
see [`store/README.md`](https://github.com/mischa-dev/dogubako/blob/main/store/README.md).

Extension folders:

| Where | Folder |
|---|---|
| Linux | `~/.local/share/dogubako/extensions` |
| macOS | `~/Library/Application Support/Dogubako/extensions` |
| Windows | `%APPDATA%\Dogubako\extensions` |
| Server | `<data_dir>/extensions`. Trust with `dogubako extensions --dir <data_dir>/extensions trust <id>`, then restart |

## Manifest reference

### `[tool]`

| Key | Required | Meaning |
|---|---|---|
| `id` | yes | `<category>.<name>`. Must not collide with a built-in tool |
| `name` | yes | Display name |
| `category` | yes | One of the twelve [categories](/start/concepts/#categories) |
| `description` | | One sentence |
| `version` | | Default `0.1.0` |
| `keywords` | | Extra search words |
| `permissions` | | Capabilities the tool needs, such as `["network"]` |
| `platforms` | | Limit to `linux`, `macos` and/or `windows` |
| `requires` | | Programs the tool needs, as dependency ids such as `["cups"]` (listed by `dogubako deps`). A missing one marks the tool unavailable with an install hint; an unknown id is an error |
| `timeout_secs` | | Default 300 |
| `destructive` | | `true` when running the tool has an effect outside Dogubako (prints, sends, deletes). Agents ask before running it and MCP marks it `destructiveHint` |
| `action` | | Label of the button that runs the tool, such as `"Print"`, instead of Run (at most 24 characters) |

### `[command]`

| Key | Meaning |
|---|---|
| `program` | The program to run, found like an engine (`PATH`, Dogubako's `bin` folder). When it is a required dependency's id or one of its programs (`whisper`, or `lpstat` for `cups`), it is found the same way that dependency is, including Dogubako's own engines and `DOGUBAKO_<ID>_PATH`; another program installed next to a required dependency (`pdftotext` with `poppler`) is taken from that folder |
| `args` | A list of arguments. Strings may contain placeholders; `{ when = "param", args = [...] }` groups are included only when the parameter is true or non-empty |

### `[output]`

| Key | Meaning |
|---|---|
| `file` | The output file name template. `{output}` in `args` expands to its path |
| `stdout` | Capture standard output as text with this MIME type |
| `extract` | Data fields read from standard output, each a regular expression (its first group, or the whole match, is the value) or a list of `{ regex, value }` cases where the first match wins and a case without `regex` is the fallback. `{param}` and `{settings.name}` in a pattern match the value literally. A field nothing matches fails the run |
| `text` | Plain-text result built from `extract` fields and parameters, such as `"Queued, job {job_id}"` (instead of `stdout`) |

```toml
[output]
extract = { job_id = 'request id is (\S+)' }
text = "Queued, job {job_id}"
```

### `[[params]]`

Parameters use the same schema as Rust tools. `type` is one of `text`,
`secret`, `integer`, `number`, `boolean`, `enum` (with
`options = [{value, label}]`), `color`, `file`, `path` or `json`. Common keys
are `label`, `description`, `required`, `default`, `min`, `max`, `accept`,
`advanced` and `primary`.

### `[http]`

Instead of `[command]`, a manifest can send one GET request and read the JSON
back; it gets the `network` permission automatically. The weather tool
([`store/data.weather/manifest.toml`](https://github.com/mischa-dev/dogubako/blob/main/store/data.weather/manifest.toml))
is the full example:

```toml
[http]
url = "https://api.open-meteo.com/v1/forecast?latitude={latitude|settings.latitude}&longitude={longitude|settings.longitude}&current=temperature_2m"
headers = { Authorization = "Bearer {settings.api_key}" }   # optional

[output]
json = { temperature = "$.current.temperature_2m" }   # JSON paths into data
text = "Now {temperature}"
```

The scheme and host are written out; placeholders fill only the path, query
and headers, and are percent-encoded. `{param|settings.name}` takes the
parameter when a run gives it and the setting otherwise. Requests follow the
instance's network policy, and a non-2xx status fails the run. Lists of results
use `[output.table]` (`rows = "$.items"`,
`columns = [{ name = "title", path = "$.title" }]`), and `[output.labels.<set>]`
maps codes to labels.

### `[[settings]]`

Values that belong to one installation (a printer queue, a saved location, an
API key) are settings, used as `{settings.<name>}`. They take the same keys as
`[[params]]`, plus `secret = true` for text:

```toml
[[settings]]
name = "queue"
type = "text"
required = true
```

Values are stored outside the manifest, so changing them keeps the trust pin
and sharing a manifest never shares them. A tool with a required setting unset
shows "Set up <tool> in Settings" and doesn't run. Set them in Settings →
Extensions or with `dogubako extensions config <id> key=value` (`key=-` reads
the value from stdin). The print tool
([`store/system.print/manifest.toml`](https://github.com/mischa-dev/dogubako/blob/main/store/system.print/manifest.toml))
is the full command-with-settings example.

### `[[tests]]`

Tests run after every install and with `dogubako extensions test <id>`. They
give sample `params`, input `files`, their own `settings`, and a recorded
`command = { stdout = "..." }` or `http = { json = {...} }` response, so they
run offline without the program installed:

```toml
[[tests]]
params = { file = "report.pdf" }
files = { file = "%PDF-1.4\n%%EOF\n" }
settings = { queue = "Office" }
command = { stdout = "request id is Office-12 (1 file(s))\n" }
expect = { text = "Queued, job Office-12", json = { "$.job_id" = "Office-12" } }
```

`expect` checks `text`, `contains`, `json` paths, `files` or
`error = "<code>"`. A test whose tool is unavailable here is reported
`skipped`, never passed. The full reference is
[`docs/TOOL_DEVELOPMENT.md`](https://github.com/mischa-dev/dogubako/blob/main/docs/TOOL_DEVELOPMENT.md#declarative-extensions).

### Placeholders

| Placeholder | Expands to |
|---|---|
| `{param}` | The parameter's value |
| `{file_param}` | The file's path |
| `{file_param.stem}`, `.ext`, `.name` | Parts of the file's name |
| `{output}` | The output file's path |
| `{workdir}` | The run's scratch folder |

**Each placeholder expands into exactly one argument.** There is no shell, so
a file called `a; rm -rf ~.png` is just an odd file name.

## Safety rules

- Shells and script interpreters are refused as programs: `sh`, `bash`, `zsh`,
  `cmd`, `powershell`, `python`, `node`, `perl`, `env`, `sudo` and similar.
- Extension tools run through the same engine, sandbox and limits as
  built-ins. Their declared permissions are enforced, and per-agent policies
  apply (`allow = ["image.cwebp"]`).
- Tool ids may not collide with built-in tools.

## Compiled extensions

For anything a manifest can't express, write a Rust crate that exposes
`pub fn tools() -> Vec<Arc<dyn Tool>>` and register it in `crates/tools`. See
[Writing a Rust tool](/develop/writing-tools/).
