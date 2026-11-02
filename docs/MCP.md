# MCP (Model Context Protocol)

Dogubako is an MCP server with two transports:

* **stdio** — `dogubako mcp serve`, for an agent on the same machine. File
  parameters take local paths; outputs go to an output directory and come back
  as paths plus `dogubako://files/<id>` resources.
* **Streamable HTTP** — `POST /mcp` on a self-hosted server, so many agents on
  different machines share one set of processing engines (FFmpeg, OCR,
  whisper.cpp, PDF tools…) without installing them. File parameters take stored
  file ids.

Agents with their own shell can of course keep using local programs directly;
MCP is an option, not a requirement.

## Discovery that scales

A catalog of a few hundred tools would flood an agent's context. Two modes:

* `full` (default): `tools/list` returns every tool **this principal is permitted to use** (paginated, 200 per page) plus the meta-tools.
* `compact`: only the meta-tools are listed:

| Meta-tool | Purpose |
|---|---|
| `dogubako_search_tools` | Keyword/category search → ids, descriptions, availability |
| `dogubako_describe_tool` | Full JSON input schema, output kind, dependencies, permissions, availability |
| `dogubako_run_tool` | Invoke any permitted tool by id; waits up to `wait_seconds`, else returns a `job_id` |
| `dogubako_job_status` | Poll / wait for a job; returns the result when finished |
| `dogubako_job_cancel` | Cancel a job |
| `dogubako_list_categories` | Categories with permitted tool counts |
| `dogubako_upload_file` | Upload a (base64) file → `file_id` (HTTP transport) |
| `dogubako_list_files` | Your files and output artifacts |

Choose the mode per server (`DOGUBAKO_MCP_DISCOVERY`), per agent policy
(`mcp_discovery`), per token scope, or per connection (`/mcp?discovery=compact`).
Unavailable tools are marked `[UNAVAILABLE on this host: Requires FFmpeg]` in
their description and in `_meta["dogubako/available"]`.

**Discovery is not authorization**: every call — direct, via
`dogubako_run_tool`, or for a tool the agent merely guessed — is checked by the
engine against the agent's policy and capabilities.

## Results

Tool results contain:

* a text block (the text result, or pretty JSON data, plus a list of output files),
* `resource_link` items (`dogubako://files/<id>`) for each output file — fetch with `resources/read` (≤ 16 MiB inline) or download over HTTP with the token (`GET /api/v1/files/<id>/content`),
* `structuredContent` with `{text, text_mime, data, artifacts:[{id, name, mime, size, url}], warnings, duration_ms}`.

Long jobs: direct calls wait up to 120 s (`dogubako_run_tool`: `wait_seconds`,
default 60). If still running you get `{job_id, status, progress}`; poll with
`dogubako_job_status` (with `wait_seconds` to long-poll). When the client sends a
`progressToken` and accepts `text/event-stream`, progress is streamed as
`notifications/progress` over SSE.

## Remote setup (HTTP)

1. As an admin, create an agent and choose what it may do (Admin → Users & agents),
   or on the server host:

   ```sh
   dogubako users create image-bot --role agent --allow 'image.*' --deny image.ocr
   dogubako users create media-bot --role agent --allow 'video.*' --allow 'speech.*' --allow 'audio.*'
   dogubako tokens create --user image-bot --name laptop-agent      # prints dgb_…
   ```

2. Configure the client.

**Claude Code**

```sh
claude mcp add --transport http dogubako https://tools.example.ts.net/mcp \
  --header "Authorization: Bearer dgb_…"
```

**Claude Desktop / generic JSON config**

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

Clients that only speak stdio can use a bridge such as `mcp-remote`:
`npx mcp-remote https://tools.example.ts.net/mcp --header "Authorization: Bearer dgb_…"`.

**Uploading inputs**: small files via `dogubako_upload_file`; large files over HTTP:

```sh
curl -H "Authorization: Bearer $TOKEN" -T video.mov "https://host/api/v1/files/raw?name=video.mov"
# → {"id":"f_…", ...}   then call video_compress with {"input": "f_…"}
```

### Worked example: centralized video compression

```
agent → tools/call video_compress {"input": "f_9c…", "preset": "balanced"}
server: authenticates token → checks policy allows video.compress → resolves f_9c… (owned by this agent)
        → runs FFmpeg (argv only, timeout, limits) with progress → stores output as f_1a…
agent ← {"content":[{"type":"text","text":"Output files:\n- clip-compressed.mp4 …"},
                    {"type":"resource_link","uri":"dogubako://files/f_1a…", ...}], ...}
```

## Local setup (stdio)

```json
{
  "mcpServers": {
    "dogubako": {
      "command": "dogubako",
      "args": ["mcp", "serve", "--allow", "image.*", "--allow", "pdf.*",
               "--permissions", "", "--fs-root", "/home/me/work",
               "--output-dir", "/home/me/work/dogubako-out"]
    }
  }
}
```

Claude Code: `claude mcp add dogubako -- dogubako mcp serve --discovery compact`.

Options: `--allow/--deny` tool patterns, `--permissions` (comma list; default all),
`--fs-root` restricts which local files can be used as inputs, `--discovery full|compact`.

## Protocol details

* Protocol versions: `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05` (negotiated).
* Capabilities: `tools`, `resources` (read + templates), `logging`.
* HTTP: `POST /mcp` (JSON or SSE response), `DELETE /mcp` ends a session, `GET /mcp` → 405 (no server-initiated stream). `Mcp-Session-Id` is issued on `initialize` and bound to the token.
* Tool names: ids with dots replaced by underscores (`video.compress` → `video_compress`); annotations carry `readOnlyHint`, `destructiveHint`, `openWorldHint`.
