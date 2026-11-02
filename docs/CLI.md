# Command-line interface

The `dogubako` binary contains the whole engine, the server and the MCP server.
Tool commands are generated from the same specs as the UI, so every tool is
available with consistent options.

```
dogubako <category> <tool> [inputs…] [--option value…] [-o OUT] [--json] [-q]
```

## Discovering tools

```sh
dogubako tools list                     # all tools, grouped; · marks unavailable ones
dogubako tools list --category pdf --available
dogubako tools search compress video
dogubako tools info video.compress      # parameters, dependencies, availability
dogubako video compress --help          # generated help with all options
dogubako tools matrix                   # Markdown capability matrix
dogubako deps                           # which external engines are installed + install hints
```

## Examples

```sh
dogubako image convert photo.heic --format webp
dogubako image resize *.jpg --width 1600 -o resized/
dogubako image compress shot.png -o shot-small.png
dogubako video compress talk.mov
dogubako audio normalize podcast.wav
dogubako speech transcribe interview.m4a --formats srt,txt
dogubako pdf merge a.pdf b.pdf c.pdf -o merged.pdf
dogubako pdf ocr scan.pdf
echo '{"b":1,"a":[1,2]}' | dogubako dev json
dogubako dev hash "hello"
dogubako files hash big.iso --algorithms sha256,blake3
dogubako network cidr 10.0.0.0/22
dogubako run image.resize --set input=photo.png --set width=800     # generic form
dogubako run text.case --params '{"text":"Hello","case":"snake"}'
```

## Conventions

* **Inputs**: primary inputs are positional (files, or the main text). When
  the main text is omitted and stdin is not a terminal, it is read from stdin.
* **Options**: `--kebab-case` names; booleans as `--flag` or `--flag=false`;
  multi-choice as comma lists; JSON params as JSON strings.
* **Outputs**: files are written to the current directory by default; `-o FILE`
  for a single output, `-o DIR/` for several; existing files are never
  overwritten unless `--force` (a ` (1)` suffix is added instead); `-o -`
  writes a single output file to stdout. Text results print to stdout;
  informational messages go to stderr.
* `--json` prints `{ok, tool, text, data, artifacts:[{name, path, mime, size}], warnings, duration_ms}` or `{ok:false, error:{code, …}}`.
* `-q/--quiet` suppresses progress and messages. Progress bars only appear on a TTY.
* Ctrl-C cancels the running tool and kills its child processes.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Tool failed (processing error, external program error) |
| 2 | Invalid usage or input (unknown tool/param, validation) |
| 3 | Missing dependency / model / unsupported platform |
| 4 | Permission denied |
| 5 | Timeout or resource limit exceeded |
| 130 | Cancelled |

## Other commands

| Command | |
|---|---|
| `dogubako server start [--bind] [--data-dir] [--config] [--no-auth] [--public-url]` | Run the web service |
| `dogubako server config` / `server health` | Effective config / health check |
| `dogubako mcp serve [--allow] [--deny] [--permissions] [--fs-root] [--discovery] [--output-dir]` | MCP over stdio |
| `dogubako models list\|download <id>\|remove <id>` | Optional models (whisper, face detection, Piper voices) |
| `dogubako users list\|create\|policy\|set-password\|disable\|enable\|delete` | Server accounts and agents (offline, `--data-dir`) |
| `dogubako tokens list\|create --user U --name N [--allow …]\|revoke ID` | API tokens |
| `dogubako extensions list\|trust ID\|untrust ID` | Declarative tool extensions |
| `dogubako api openapi` | Print the OpenAPI document |

Environment: `DOGUBAKO_HOME` (base dir for models/extensions/bin),
`DOGUBAKO_MODELS_DIR`, `DOGUBAKO_<DEP>_PATH`, `DOGUBAKO_LOG`.
