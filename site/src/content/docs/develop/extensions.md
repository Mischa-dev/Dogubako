---
title: Extensions
description: Wrap any command-line program as a Dogubako tool with a TOML manifest, no Rust required.
sidebar:
  order: 4
---

A declarative extension turns a command-line program into a full Dogubako
tool: it appears in the app with a form, in the CLI, the API and MCP, and runs
through the same engine, sandbox, limits and policies as built-in tools.

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

## Install and trust it

```sh
cp extensions/cwebp.toml ~/.local/share/dogubako/extensions/
dogubako extensions list
dogubako extensions trust image.cwebp      # pins the manifest's SHA-256
dogubako image cwebp photo.png --quality 90
```

A manifest is **only loaded after you trust it**. Trusting pins its SHA-256 in
`trusted.toml`; any later edit invalidates the trust until you review and
trust it again.

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
| `timeout_secs` | | Default 300 |

### `[command]`

| Key | Meaning |
|---|---|
| `program` | The program to run, found like an engine (`PATH`, Dogubako's `bin` folder) |
| `args` | A list of arguments. Strings may contain placeholders; `{ when = "param", args = [...] }` groups are included only when the parameter is true or non-empty |

### `[output]`

Exactly one of:

| Key | Meaning |
|---|---|
| `file` | The output file name template. `{output}` in `args` expands to its path |
| `stdout` | Capture standard output as text with this MIME type |

### `[[params]]`

Parameters use the same schema as Rust tools. `type` is one of `text`,
`secret`, `integer`, `number`, `boolean`, `enum` (with
`options = [{value, label}]`), `color`, `file`, `path` or `json`. Common keys
are `label`, `description`, `required`, `default`, `min`, `max`, `accept`,
`advanced` and `primary`.

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
