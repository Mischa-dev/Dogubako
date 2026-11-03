---
title: Permissions and policies
description: How Dogubako decides who may run which tool, with roles, policies, token scopes, capabilities and tool patterns.
---

Authorization happens in the engine, for every run, whatever the interface:
the web app, the HTTP API, MCP (direct tool calls and the generic
`dogubako_run_tool`) or the stdio server.

## The rule

A run is allowed when **all** of these hold:

1. The tool is allowed by the account's **policy**.
2. If a scoped token is used, the tool is also allowed by the token's
   **scope**: effective access is policy ∩ scope.
3. Every **permission** the tool declares is granted, by both the policy and
   the scope.
4. The tool supports this **platform**, and its **engines and models** are
   installed.

Otherwise the run is refused with `permission_denied` (or
`missing_dependency`, `missing_model` or `unsupported` for step 4), and the
refusal is written to the audit log.

## Policies

```json
{
  "allow": ["image.*", "video.compress"],
  "deny": ["image.ocr"],
  "permissions": ["network"],
  "max_concurrent_jobs": 2,
  "quota_mb": 2048,
  "max_upload_mb": 512,
  "mcp_discovery": "compact"
}
```

| Field | Meaning |
|---|---|
| `allow` | Tool patterns the account may run. Empty means nothing |
| `deny` | Tool patterns it may never run. Deny wins over allow |
| `permissions` | Capabilities granted (below) |
| `max_concurrent_jobs` | Jobs at once for this account. Default: the server's `per_user_concurrency` |
| `quota_mb` | Storage quota. Default: the server's `user_quota_mb` |
| `max_upload_mb` | Largest upload. Default: the server's `max_upload_mb` |
| `mcp_discovery` | `full` or `compact`. Default: the server's `mcp_discovery` |

### Patterns

| Pattern | Matches |
|---|---|
| `*` | Every tool |
| `image.*` | Every tool in the `image` category |
| `category:image` | The same, spelled out |
| `video.compress` | Exactly one tool |

### Defaults by role

| Role | `allow` | `permissions` |
|---|---|---|
| `owner`, `admin` | `*` | all |
| `user` | `*` | `network` |
| `agent` | nothing | none |

## Permissions

Most tools only read the inputs they're given and need no permission. These
capabilities unlock the rest:

| Permission | Lets a tool… | Tools |
|---|---|---|
| `network` | Make outbound requests: DNS, HTTP, TLS, WHOIS, port checks | DNS lookup, reverse DNS, TLS inspect, HTTP headers, favicon fetch, connectivity, port check, RDAP, WHOIS, domain inspect |
| `host_fs_read` | Read folders on the host, inside the configured `fs_roots` | Checksum manifest, folder compare, folder size, large files, duplicates, empty items, search, file manifest, remove empty, batch rename |
| `host_fs_write` | Change files on the host, inside `fs_roots` | Remove empty folders, batch rename |
| `system_info` | Read hardware and OS information | Every System tool except Processes |
| `process_info` | List running processes | Processes |

Host filesystem permissions only work when an administrator has set
`fs_roots`. Paths are resolved, symlinks included, and must stay inside a
root. Tools that change host files show a preview by default and need an
explicit **apply** option to make changes.

Network tools on a server also respect `allow_private_network` (off by
default), which refuses private, loopback, link-local and other non-public
addresses, including through redirects and DNS rebinding.

## Token scopes

A token can carry a scope with the same shape as a policy. It can only narrow
what the account may do, never widen it:

```sh
dogubako tokens create --user alice --name merge-script --allow pdf.merge --permissions ""
```

Scoped tokens never carry admin rights, and no token can create tokens.

## Local interfaces

- **Desktop app and CLI:** you are trusted like your own shell. Every tool and
  permission is available.
- **`dogubako mcp serve`:** everything by default; narrow it with `--allow`,
  `--deny`, `--permissions` and `--fs-root`. See
  [Local agents](/agents/local/#limiting-what-the-agent-can-do).
