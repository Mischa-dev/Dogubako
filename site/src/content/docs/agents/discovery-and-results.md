---
title: Discovery and results
description: Full and compact tool discovery, the meta-tools, long-running jobs and what tool results contain.
sidebar:
  order: 4
---

## Discovery modes

A catalog of 249 tools would flood an agent's context, so there are two
modes.

- **`full`** (default): `tools/list` returns every tool this agent is
  **permitted** to use, 200 per page, plus the meta-tools. Best when an agent
  has a small, focused policy.
- **`compact`**: only the meta-tools are listed. The agent searches for what
  it needs. Best for agents with broad access.

Choose the mode, from most to least specific:

1. per connection: `/mcp?discovery=compact`, or `--discovery compact` for
   stdio;
2. per token scope;
3. per agent policy (`mcp_discovery`);
4. per server: `DOGUBAKO_MCP_DISCOVERY`.

**Discovery is not authorization.** Whatever is listed, every call is checked
against the agent's policy and capabilities.

## Meta-tools

| Meta-tool | Purpose |
|---|---|
| `dogubako_search_tools` | Keyword or category search, returning ids, descriptions and availability |
| `dogubako_describe_tool` | The full JSON input schema, output kind, dependencies, permissions and availability of one tool |
| `dogubako_run_tool` | Run any permitted tool by id. Waits up to `wait_seconds` (default 60), else returns a `job_id` |
| `dogubako_job_status` | Poll or wait for a job; returns the result when finished |
| `dogubako_job_cancel` | Cancel a job |
| `dogubako_list_categories` | Categories with the number of tools this agent may use |
| `dogubako_upload_file` | Upload a base64 file and get a `file_id` (HTTP transport) |
| `dogubako_list_files` | Your files and output artifacts |

A compact session typically goes: search → describe → run → (job status) →
read the output resource.

## Results

A successful tool result contains:

- a **text** block: the tool's text result, or its data as pretty JSON, plus
  a list of output files;
- a **`resource_link`** for each output file (`dogubako://files/<id>`). Fetch
  it with `resources/read` (up to 16 MiB inline) or over HTTP with
  `GET /api/v1/files/<id>/content`;
- **`structuredContent`**: `{text, text_mime, data, artifacts: [{id, name, mime, size, url}], warnings, duration_ms}`.

A failed call has `isError: true`; see [Errors](/reference/errors/#mcp-errors).

## Long jobs and progress

- Direct tool calls wait up to 120 seconds. `dogubako_run_tool` waits up to
  its `wait_seconds` (default 60).
- If the job is still running, the result is `{job_id, status, progress}`.
  Poll with `dogubako_job_status`, passing `wait_seconds` to long-poll.
- When the client sends a `progressToken` and accepts `text/event-stream`,
  progress is streamed as `notifications/progress`.

## Tool names and hints

MCP tool names are tool ids with dots replaced by underscores
(`pdf.merge` → `pdf_merge`). Annotations tell the client how careful to be:

| Annotation | Set when |
|---|---|
| `readOnlyHint` | The tool only reads its inputs and returns new outputs |
| `destructiveHint` | The tool can change state outside the job, such as batch rename with `apply` |
| `openWorldHint` | The tool talks to the network |
