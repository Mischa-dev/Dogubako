---
title: AI agents and MCP
description: Give AI agents Dogubako's tools over the Model Context Protocol, locally or from a shared server.
sidebar:
  order: 1
---

Dogubako is an [MCP](https://modelcontextprotocol.io) server. Any MCP client,
such as Claude Code, Claude Desktop or your own agent, can discover and run
Dogubako's tools with typed JSON schemas, progress and file outputs.

Agents with their own shell can of course keep calling local programs
directly. MCP is an option, not a requirement, and is most useful when you
want an agent to have **well-defined, policed tools instead of a shell**.

## Two ways to connect

| | Local (stdio) | Remote (HTTP) |
|---|---|---|
| Command or URL | `dogubako mcp serve` | `https://your-server/mcp` |
| Who runs the tools | The agent's own machine | Your Dogubako server |
| Files | Local paths in, output files in a folder | Uploaded files by id, outputs stored on the server |
| Engines | Whatever is installed locally | Installed once on the server, shared by every agent |
| Limits | Flags: `--allow`, `--deny`, `--permissions`, `--fs-root` | The agent account's policy and token scope |
| Guide | [Local agents](/agents/local/) | [Remote agents](/agents/remote/) |

## The idea: one server, many scoped agents

On a shared server, each agent gets its own account and token. For example:

- an **image bot** may resize, compress and blur images, and nothing else;
- a **media bot** may transcode video and transcribe audio, with one job at a
  time and a 4 GB quota;
- a **research agent** may use network tools such as DNS and TLS checks.

None of them can see the others' files, use tools they weren't granted, or
touch the host. Hidden tools aren't just unlisted: calling one is refused,
however the agent found out about it.

## What agents see

- **Tools** named after their ids with dots replaced by underscores:
  `video.compress` is `video_compress`. Each has a JSON input schema generated
  from its spec, and annotations (`readOnlyHint`, `destructiveHint`,
  `openWorldHint`) so clients can decide what needs confirmation.
- **Meta-tools** to search, describe and run tools by id, and to manage jobs
  and files. With **compact discovery**, these are the only tools listed, so a
  catalog of 249 tools doesn't flood the agent's context. See
  [Discovery and results](/agents/discovery-and-results/).
- **Resources** for output files, as `dogubako://files/<id>`.

Unavailable tools (missing engine, wrong platform) are marked
`[UNAVAILABLE on this host: …]` in their description, so agents don't waste a
call.

## Next

- [Local agents](/agents/local/): Claude Code, Claude Desktop and other
  stdio clients.
- [Remote agents](/agents/remote/): create an agent account and connect over
  HTTP.
- [MCP reference](/reference/mcp/): protocol versions, capabilities and
  transport details.
