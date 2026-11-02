---
title: Local agents (stdio)
description: Run Dogubako's MCP server over stdio for an agent on the same machine, and limit what it can do.
sidebar:
  order: 2
---

`dogubako mcp serve` speaks MCP over stdin and stdout. File parameters take
local paths, and outputs are written to an output folder and returned as
paths plus `dogubako://files/<id>` resources.

## Claude Code

```sh
claude mcp add dogubako -- dogubako mcp serve --discovery compact
```

Compact discovery keeps the agent's context small: it sees a handful of
meta-tools and searches for the right tool when it needs one.

## Claude Desktop and other JSON-configured clients

```json
{
  "mcpServers": {
    "dogubako": {
      "command": "dogubako",
      "args": ["mcp", "serve", "--discovery", "compact"]
    }
  }
}
```

If the client can't find `dogubako`, use the full path to the binary as
`command`.

## Limiting what the agent can do

By default a local agent can use every tool and every permission, like a
program you run yourself. Narrow it with flags:

| Flag | Effect |
|---|---|
| `--allow <pattern>` | Only these tools (repeatable). Default `*` |
| `--deny <pattern>` | Never these tools (repeatable). Deny wins |
| `--permissions <list>` | Capabilities to grant, comma-separated: `network`, `host_fs_read`, `host_fs_write`, `system_info`, `process_info`. Default: all. `""` grants none |
| `--fs-root <dir>` | Only files and folders inside these directories can be used as inputs (repeatable) |
| `--output-dir <dir>` | Where output files are written. Default: `mcp-output` in your [Dogubako data folder](/app/desktop/#where-files-go) |
| `--discovery full\|compact` | How tools are listed (default `full`) |

For example, an agent that may only work with images and PDFs inside one
project folder, with no network or host access:

```json
{
  "mcpServers": {
    "dogubako": {
      "command": "dogubako",
      "args": [
        "mcp", "serve",
        "--allow", "image.*", "--allow", "pdf.*",
        "--permissions", "",
        "--fs-root", "/home/me/work",
        "--output-dir", "/home/me/work/dogubako-out"
      ]
    }
  }
}
```

Patterns are `*`, a category such as `image.*` or `category:image`, or a tool
id such as `video.compress`.

## Try it

Ask the agent something like:

- "Compress `~/work/slides.pdf` with Dogubako and tell me how much smaller it
  got."
- "Use Dogubako to transcribe `interview.m4a` to SRT."
- "Which Dogubako tools can extract text from images?"

With compact discovery, the agent calls `dogubako_search_tools`, then
`dogubako_describe_tool`, then `dogubako_run_tool`.
