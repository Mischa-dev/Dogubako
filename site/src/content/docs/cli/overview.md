---
title: CLI overview
description: How the dogubako command works, how to find tools, and how inputs, options and outputs behave.
sidebar:
  order: 1
---

The `dogubako` binary contains the whole engine, the web server and the MCP
server. Every tool is a subcommand generated from the same spec as the app's
form, so every option the app has, the CLI has too.

```
dogubako <category> <tool> [inputs…] [--option value…] [-o OUT] [--json] [-q]
```

For example:

```sh
dogubako image convert photo.heic --format webp
dogubako pdf merge a.pdf b.pdf c.pdf -o merged.pdf
echo '{"b":1,"a":[1,2]}' | dogubako dev json
```

## Finding tools

```sh
dogubako tools list                        # every tool, grouped by category; · marks unavailable ones
dogubako tools list --category pdf --available
dogubako tools search compress video       # keyword search
dogubako tools info video.compress         # options, dependencies and availability
dogubako video compress --help             # the generated help, with every option
dogubako deps                              # which engines are installed, with install commands
```

The command for a tool is its category, then its name with underscores turned
into dashes: `image.strip_metadata` is `dogubako image strip-metadata`. The
[tool catalog](/tools/) shows the command and every option for each tool.

## Inputs

- **Primary inputs are positional.** For file tools that means the files
  (`dogubako image resize *.jpg --width 1600`); for text tools, the main text
  (`dogubako text case "Hello World" --case snake`).
- **Standard input.** When a text tool's main text is omitted and stdin is not
  a terminal, it is read from stdin: `cat data.json | dogubako dev json`.
- **Many files at once.** Tools that make sense in bulk (convert, resize,
  compress…) take several files and produce one output per input, with
  progress per file.

## Options

- Options use `--kebab-case`: the parameter `max_dimension` is
  `--max-dimension`.
- Booleans are `--flag` or `--flag=false`.
- Negative numbers need the `=` form, so they aren't mistaken for flags:
  `--offset=-2`.
- Multiple choices are comma lists: `--formats srt,txt`.
- JSON parameters take a JSON string: `--schema '{"type":"object"}'`.
- A few parameter names clash with global flags (`json`, `quiet`, `verbose`,
  `out`, `force`, `help`, `version`). Those get a `param-` prefix, for
  example `--param-json`.

## Outputs

- **Files** are written to the current folder by default, named after the
  input: `photo.png` compressed becomes `photo-compressed.png`.
- `-o FILE` names a single output; `-o DIR/` (with the trailing slash) puts
  several outputs in a folder.
- Existing files are **never overwritten** unless you pass `--force`;
  otherwise a ` (1)` suffix is added.
- `-o -` writes a single output file to stdout, for piping.
- **Text results** print to stdout. Informational messages and progress go to
  stderr, so pipes stay clean.
- `--json` prints a machine-readable result instead; see
  [Scripting](/cli/scripting/).
- `-q` / `--quiet` suppresses progress and messages. Progress bars only
  appear on a terminal.

Press <kbd>Ctrl</kbd> <kbd>C</kbd> to cancel. The running tool and any external
programs it started are stopped.

## The generic form

Any tool can also be run by id with `dogubako run`, which is handy when you
generate parameters programmatically:

```sh
dogubako run image.resize --set input=photo.png --set width=800
dogubako run text.case --params '{"text":"Hello World","case":"snake"}'
```

## Other commands

| Command | Does |
|---|---|
| `dogubako server start` | Run the web service. See [Self-hosting](/self-hosting/overview/) |
| `dogubako mcp serve` | Run the MCP server over stdio. See [Local agents](/agents/local/) |
| `dogubako deps` | Show engine status and install hints |
| `dogubako models list\|download\|remove` | Manage [local models](/app/engines-and-models/#models) |
| `dogubako users …`, `dogubako tokens …` | Manage server accounts and API tokens offline |
| `dogubako extensions list\|trust\|untrust` | Manage [extensions](/develop/extensions/) |
| `dogubako api openapi` | Print the OpenAPI document |
| `dogubako tools matrix` | Print the Markdown [capability matrix](/reference/capabilities/) |

The [CLI reference](/reference/cli/) lists every command, flag, environment
variable and exit code.
