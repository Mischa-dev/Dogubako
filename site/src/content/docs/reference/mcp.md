---
title: MCP reference
description: Protocol versions, capabilities, transports, sessions, tool naming and resources of Dogubako's MCP server.
---

For setup, see [Local agents](/agents/local/) and
[Remote agents](/agents/remote/). For discovery modes, meta-tools and result
shapes, see [Discovery and results](/agents/discovery-and-results/).

## Protocol

| | |
|---|---|
| Protocol versions | `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05` (negotiated on `initialize`) |
| Server capabilities | `tools`, `resources` (read and templates), `logging` |
| Transports | stdio (`dogubako mcp serve`) and Streamable HTTP (`/mcp` on a server) |

## Streamable HTTP

| Request | Behavior |
|---|---|
| `POST /mcp` | JSON-RPC request. The response is JSON, or a server-sent event stream when the client accepts `text/event-stream` and progress is requested |
| `DELETE /mcp` | Ends the session given in `Mcp-Session-Id` |
| `GET /mcp` | `405`: there is no server-initiated stream |

- **Authentication:** `Authorization: Bearer dgb_…` is required.
- **Sessions:** `Mcp-Session-Id` is issued on `initialize` and bound to the
  token that created it. Using it with another token fails.
- **Origin checks:** requests with an `Origin` header must come from the
  server's own origin or one listed in `allowed_origins`.
- **Per-connection discovery:** `/mcp?discovery=compact` or `?discovery=full`.

## Tools

- **Names** are tool ids with dots replaced by underscores
  (`video.compress` → `video_compress`).
- **Input schemas** are generated from each tool's spec. On the HTTP
  transport, file parameters are file ids (`f_…`); over stdio they are local
  paths.
- **Output schemas** describe `structuredContent`.
- **Annotations:** `title`, `readOnlyHint`, `destructiveHint`,
  `idempotentHint` and `openWorldHint`.
- **`_meta`:** `dogubako/id`, `dogubako/category` and `dogubako/available`.
- **Pagination:** `tools/list` returns 200 tools per page with a cursor.
- **Unavailable tools** carry `[UNAVAILABLE on this host: <reason>]` in their
  description.

## Resources

Output files are resources with URIs `dogubako://files/<id>`. Tool results
include them as `resource_link` items. `resources/read` returns content up to
16 MiB inline; larger files should be downloaded with
`GET /api/v1/files/<id>/content` using the same token.

## Progress and long jobs

- Direct tool calls wait up to 120 seconds for the job, then return
  `{job_id, status, progress}`.
- `dogubako_run_tool` waits up to `wait_seconds` (default 60).
- With a `progressToken`, progress arrives as `notifications/progress`.
- `dogubako_job_status` with `wait_seconds` long-polls until the job ends.

## Errors

A failed tool call returns `isError: true` with the tagged error in
`structuredContent.error`. See [Errors](/reference/errors/#mcp-errors).
