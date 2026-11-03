---
title: Scripting
description: Use dogubako in shell scripts and CI with JSON output, exit codes, stdin and stdout.
sidebar:
  order: 3
---

The CLI is designed to be scripted: results go to stdout, messages to
stderr, every failure has a distinct exit code, and `--json` gives you a
stable machine-readable result.

## JSON output

Add `--json` to any tool command:

```sh
echo '{"b":1}' | dogubako dev json --json
```

```json
{
  "ok": true,
  "tool": "dev.json",
  "text": "{\n  \"b\": 1\n}",
  "text_mime": "application/json",
  "data": { "valid": true, "root": "object with 1 keys", "depth": 1, "values": 2 },
  "artifacts": [],
  "warnings": [],
  "duration_ms": 0
}
```

File outputs are listed in `artifacts` as `{name, path, mime, size}`, with
`path` pointing at the file written on disk.

A run that completes reports `ok: true` even when its *finding* is negative,
for example a schema validation that found errors (`data.valid` is `false`).
Check the tool's `data` for that.

When the run itself fails you get `ok: false` and a tagged error, with the same codes the
[HTTP API](/reference/errors/) uses:

```json
{"ok":false,"error":{"code":"missing_dependency","dependency":"qpdf","hint":"Install qpdf (sudo apt install qpdf) or see https://github.com/qpdf/qpdf/releases"}}
```

With `jq`:

```sh
# The path of every output file
dogubako image resize *.jpg --width 800 -o small/ --json | jq -r '.artifacts[].path'

# Fail a CI step when a JSON file doesn't match its schema
dogubako data validate payload.json --schema "$(cat schema.json)" --json \
  | jq -e '.data.valid' > /dev/null
```

`dogubako tools list --json`, `tools info <id> --json`, `deps --json` and
`models list --json` are machine-readable too.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | The tool failed (a processing error, or the external program failed) |
| `2` | Invalid usage or input: unknown tool or option, or a value that failed validation |
| `3` | Missing engine or model, or the tool doesn't run on this platform |
| `4` | Permission denied |
| `5` | Timeout or resource limit exceeded |
| `130` | Cancelled (<kbd>Ctrl</kbd> <kbd>C</kbd>) |

```sh
if ! dogubako pdf compress big.pdf -q; then
  case $? in
    3) echo "Install the PDF engines first: dogubako deps" ;;
    5) echo "Took too long" ;;
  esac
fi
```

## Pipes

- **In:** a text tool reads its main text from stdin when you don't pass it
  and stdin isn't a terminal.
- **Out:** text results go to stdout. For a tool that writes a single file,
  `-o -` streams that file to stdout instead of writing it.

```sh
curl -s https://api.example.com/items | dogubako dev json > items.pretty.json
dogubako image convert logo.svg --format png -o - | pngquant - > logo.png
```

## Quiet and verbose

- `-q` / `--quiet` hides progress and informational messages.
- `-v` raises log verbosity (repeat it for more). `DOGUBAKO_LOG=debug` sets
  the log filter directly.

## Batch jobs

Tools that accept many files process them in one run and report progress per
file, which is faster than a shell loop:

```sh
dogubako image compress photos/*.jpg --quality 80 -o out/
```

For a whole tree, combine with `find` and `xargs`:

```sh
find . -name '*.png' -print0 | xargs -0 dogubako image convert --format webp -o webp/
```
