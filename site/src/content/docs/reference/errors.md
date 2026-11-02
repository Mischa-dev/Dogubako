---
title: Errors
description: Every Dogubako error code, with its fields, HTTP status and CLI exit code.
---

Tool errors are the same everywhere. Each interface maps them to its own
form: an HTTP status, an MCP error result, or a CLI exit code.

## Tool errors

Every tool error is an object tagged by `code`, with extra fields that depend
on the code.

| `code` | Fields | HTTP | CLI exit | Meaning |
|---|---|---:|---:|---|
| `invalid_input` | `param`, `message` | 400 | 2 | A parameter is missing, has the wrong type, is out of range, or the input can't be read |
| `not_found` | `id` | 404 | 2 | No such tool (or you aren't allowed to see it) |
| `missing_dependency` | `dependency`, `hint` | 501 | 3 | An engine isn't installed. `hint` says how to install it |
| `missing_model` | `message` | 501 | 3 | The tool needs a model that hasn't been downloaded |
| `unsupported` | `message` | 501 | 3 | Not possible on this platform or with this engine build |
| `permission_denied` | `message` | 403 | 4 | The account, token or command-line flags don't allow this tool or capability |
| `timeout` | `seconds` | 504 | 5 | The job ran longer than its time limit |
| `limit_exceeded` | `message` | 413 | 5 | A resource limit was hit: decompression size, entry count, image pixels… |
| `cancelled` | — | 499 | 130 | The job was cancelled |
| `external_failure` | `program`, `exit_code`, `stderr` | 422 | 1 | An external program failed. `stderr` holds the end of its error output |
| `failed` | `message` | 422 | 1 | Any other processing failure |

Example:

```json
{
  "code": "invalid_input",
  "param": "quality",
  "message": "must be at most 100"
}
```

## HTTP API errors

The HTTP API wraps errors in an envelope. For tool errors, `detail` carries
the tagged object above:

```json
{
  "error": {
    "code": "invalid_input",
    "message": "invalid input for 'quality': must be at most 100",
    "detail": { "code": "invalid_input", "param": "quality", "message": "must be at most 100" }
  }
}
```

A job that fails after it started has the tagged error in `job.error` and
status `failed` (or `cancelled`).

Errors that come from the API itself rather than a tool use these codes:

| `code` | HTTP | Meaning |
|---|---:|---|
| `bad_request` | 400 | Malformed request body or query |
| `unauthorized` | 401 | Missing, expired or revoked session or token |
| `forbidden` | 403 | Signed in, but not allowed (for example, a non-admin calling `/admin`), or a missing CSRF token |
| `not_found` | 404 | No such resource, or it belongs to someone else |
| `conflict` | 409 | The resource already exists, such as a username |
| `too_large` | 413 | The upload is too large, or it would exceed your storage quota |
| `rate_limited` | 429 | Too many sign-in or setup attempts; wait and retry |
| `internal` | 500 | A bug. The server log has the details |

## MCP errors

Over MCP, a failed tool call returns a result with `isError: true`, a text
block such as `Error (missing_dependency): …`, and the tagged error in
`structuredContent.error`. Protocol problems, such as an unknown
method or malformed request, are JSON-RPC errors.

## CLI

With `--json`, a failure prints `{"ok": false, "error": {…}}` with the tagged
error. Without it, a one-line message goes to stderr. The exit code follows
the table above; see [Scripting](/cli/scripting/#exit-codes).
