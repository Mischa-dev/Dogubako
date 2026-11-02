---
title: CLI reference
description: Every dogubako command, global flag, environment variable and exit code.
---

For a guided introduction, see the [CLI overview](/cli/overview/).

## Global flags

These work with every command.

| Flag | Meaning |
|---|---|
| `--json` | Machine-readable JSON output |
| `-q`, `--quiet` | Suppress progress and informational messages |
| `-v`, `--verbose` | More log output (repeatable) |
| `-h`, `--help` | Help for the command |
| `-V`, `--version` | Print the version |

Tool commands also accept:

| Flag | Meaning |
|---|---|
| `-o`, `--out <PATH>` | Output file, folder (with a trailing `/`), or `-` for stdout |
| `-f`, `--force` | Overwrite existing output files |

## Tool commands

```
dogubako <category> <tool> [inputs…] [--option value…]
dogubako run <tool-id> [--set name=value…] [--params JSON]
```

Categories: `image`, `video`, `audio`, `speech`, `pdf`, `files`, `text`,
`dev`, `data`, `network`, `design`, `system`. Each tool's command and options
are listed in the [tool catalog](/tools/).

## Discovery

| Command | Does |
|---|---|
| `dogubako tools list [--category C] [--available]` | List tools, grouped by category |
| `dogubako tools search <words…>` | Search names, descriptions and keywords |
| `dogubako tools info <tool-id>` | Parameters, dependencies, availability and input schema |
| `dogubako tools matrix` | Markdown capability matrix of every tool |
| `dogubako deps` | Engine status and install commands |

## Server

| Command | Does |
|---|---|
| `dogubako server start [--bind ADDR] [--data-dir DIR] [--config FILE] [--public-url URL] [--no-auth]` | Run the web app, HTTP API and MCP endpoint |
| `dogubako server config` | Print the effective configuration as TOML |
| `dogubako server health [--addr ADDR]` | Exit 0 if a server responds (for health checks) |
| `dogubako api openapi` | Print the OpenAPI document |

## MCP

| Command | Does |
|---|---|
| `dogubako mcp serve [--allow P…] [--deny P…] [--permissions LIST] [--fs-root DIR…] [--output-dir DIR] [--discovery full\|compact]` | MCP over stdio. See [Local agents](/agents/local/) |

## Models

| Command | Does |
|---|---|
| `dogubako models list` | Available models, installed or not |
| `dogubako models download <id>` | Download and verify a model |
| `dogubako models remove <id>` | Delete a downloaded model |

## Accounts and tokens

These operate directly on a server's database. Pass `--data-dir` or set
`DOGUBAKO_DATA_DIR`.

| Command | Does |
|---|---|
| `dogubako users list` | List users and agents |
| `dogubako users create <name> [--role owner\|admin\|user\|agent] [--password-stdin] [--description D] [policy flags]` | Create an account. Prompts for a password unless it's an agent |
| `dogubako users policy <name> [policy flags]` | Replace an account's policy |
| `dogubako users set-password <name> [--password-stdin]` | Set a password |
| `dogubako users disable <name>`, `enable <name>`, `delete <name>` | Manage an account |
| `dogubako tokens list` | List tokens |
| `dogubako tokens create --user <name> --name <label> [--expires-days N] [policy flags]` | Create a token; policy flags narrow its scope. Prints the token once |
| `dogubako tokens revoke <id>` | Revoke a token |

Policy flags: `--allow <pattern>` and `--deny <pattern>` (repeatable),
`--permissions <list>`, `--max-jobs <n>`, `--quota-mb <n>`,
`--discovery full|compact`. See
[Permissions and policies](/reference/permissions-and-policies/).

## Extensions

| Command | Does |
|---|---|
| `dogubako extensions list [--dir DIR]` | Manifests and their trust status |
| `dogubako extensions trust <id> [--dir DIR]` | Trust a manifest, pinning its current SHA-256 |
| `dogubako extensions untrust <id> [--dir DIR]` | Stop trusting it |

## Environment variables

| Variable | Meaning |
|---|---|
| `DOGUBAKO_HOME` | Base folder for models, extensions and `bin` |
| `DOGUBAKO_MODELS_DIR` | Model folder |
| `DOGUBAKO_<ID>_PATH` | Explicit path to an engine, such as `DOGUBAKO_FFMPEG_PATH` |
| `DOGUBAKO_LOG` | Log filter, such as `debug` |
| `DOGUBAKO_DATA_DIR`, `DOGUBAKO_CONFIG`, `DOGUBAKO_BIND`, … | Server settings; see [Configuration](/reference/configuration/) |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Tool failed |
| `2` | Invalid usage or input |
| `3` | Missing engine or model, or unsupported platform |
| `4` | Permission denied |
| `5` | Timeout or resource limit exceeded |
| `130` | Cancelled |
