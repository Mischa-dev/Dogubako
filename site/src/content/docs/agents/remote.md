---
title: Remote agents (HTTP)
description: Connect AI agents to a shared Dogubako server over MCP Streamable HTTP, each with its own scoped token.
sidebar:
  order: 3
---

A self-hosted server serves MCP over Streamable HTTP at `/mcp`. Many agents
on different machines share one set of engines (FFmpeg, OCR, whisper.cpp, PDF
tools…) without installing anything themselves.

## 1. Create the agent

As an admin, in **Admin → Users & agents → New user or agent**, choose
**Agent**, pick what it may do, and create a token from its row menu. Or on
the server host:

```sh
dogubako users create image-bot --role agent --allow 'image.*' --deny image.ocr
dogubako users create media-bot --role agent --allow 'video.*' --allow 'audio.*' --allow 'speech.*'
dogubako tokens create --user media-bot --name laptop-agent      # prints dgb_… once
```

Agents start with no tools and no permissions, so grant only what they need.
See [Accounts and agents](/self-hosting/accounts-and-agents/#adding-agents).

## 2. Configure the client

**Claude Code**

```sh
claude mcp add --transport http dogubako https://tools.example.ts.net/mcp \
  --header "Authorization: Bearer dgb_…"
```

**Claude Desktop and other JSON-configured clients**

```json
{
  "mcpServers": {
    "dogubako": {
      "type": "http",
      "url": "https://tools.example.ts.net/mcp",
      "headers": { "Authorization": "Bearer dgb_…" }
    }
  }
}
```

**Clients that only speak stdio** can use a bridge such as `mcp-remote`:

```sh
npx mcp-remote https://tools.example.ts.net/mcp --header "Authorization: Bearer dgb_…"
```

Add `?discovery=compact` to the URL to use compact discovery for this
connection, whatever the agent's default.

## 3. Getting files to the server

On the HTTP transport, file parameters take **file ids** (`f_…`) of files
stored on the server, never paths on the agent's machine.

- **Small files:** the agent calls `dogubako_upload_file` with the content as
  base64 and gets a `file_id`.
- **Large files:** upload over HTTP with the same token, then pass the id:

  ```sh
  curl -H "Authorization: Bearer $TOKEN" -T video.mov \
    "https://tools.example.ts.net/api/v1/files/raw?name=video.mov"
  # → {"id":"f_…", …}   then call video_compress with {"input": "f_…"}
  ```

Outputs come back as `dogubako://files/<id>` resource links. Read small ones
with `resources/read` (up to 16 MiB inline), or download any size with
`GET /api/v1/files/<id>/content` and the token. Files expire after the
server's retention period.

## Worked example: central video compression

```
agent → tools/call video_compress {"input": "f_9c…", "level": "balanced"}

server: authenticates the token
        → checks the policy allows video.compress
        → resolves f_9c… (owned by this agent)
        → runs FFmpeg (argument vector only, timeout, resource limits) with progress
        → stores the output as f_1a…

agent ← {"content": [
           {"type": "text", "text": "Output files:\n- clip-compressed.mp4 …"},
           {"type": "resource_link", "uri": "dogubako://files/f_1a…", …}
         ], …}
```

## Security notes

- Every call is authorized against the agent's policy and the token's scope,
  whether the agent calls the tool directly, through `dogubako_run_tool`, or
  guesses a tool name it was never shown.
- MCP sessions (`Mcp-Session-Id`) are bound to the token that created them.
- The server checks the `Origin` header. Browser-based clients on other
  origins need `DOGUBAKO_ALLOWED_ORIGINS`.
- Revoke a token at any time in **Admin → Tokens**. Its sessions stop working
  immediately.
