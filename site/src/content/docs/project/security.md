---
title: Security model
description: How Dogubako protects your files and your server, from authentication to sandboxed engines and SSRF guards.
---

Dogubako processes arbitrary user files and can expose processing capabilities
to remote users and AI agents. Security is enforced in the engine, not the UI.

## Trust boundaries

| Mode | Who is trusted | Enforcement |
|---|---|---|
| Desktop app / CLI | The local user (same as their shell) | Input validation, safe process execution, archive limits |
| `dogubako mcp serve` (stdio) | The local agent, optionally narrowed with `--allow/--deny/--permissions/--fs-root` | Engine authorization + permissions + path roots |
| Self-hosted server | Nobody by default; every request is authenticated and authorized | Accounts, sessions, scoped API tokens, per-principal policies, file ownership, quotas, audit log |

Running behind Tailscale, a VPN or a LAN is **network access, not
authorization**. Authentication stays required unless you explicitly bind to
loopback with `--no-auth`. The server refuses to start without authentication
on a non-loopback address unless `DOGUBAKO_ALLOW_INSECURE_NO_AUTH=1`.

## Authentication

* Passwords: Argon2id (RustCrypto `argon2`, default parameters), minimum 10 characters. Unknown usernames run a dummy verification to equalize timing.
* Browser sessions: 256-bit random id in an `HttpOnly; SameSite=Strict` cookie (`Secure` when `DOGUBAKO_SECURE_COOKIES=true`); only a SHA-256 of the id is stored. Every state-changing request with a session must carry the per-session `X-CSRF-Token`.
* API tokens: `dgb_` + 256 random bits, shown once, stored as SHA-256, optional expiry, revocable, with an optional **scope** (allow/deny patterns + capabilities) that further narrows the owner's policy. Tokens cannot mint tokens; scoped tokens never carry admin rights.
* First run: an owner account can only be created with the one-time setup token printed in the server log (or via `dogubako users create --role owner` on the host).
* Login and setup attempts are rate limited per IP and per username.

## Authorization

Roles: `owner`, `admin`, `user`, `agent`. Each principal has a **policy**:

```json
{ "allow": ["image.*", "video.compress"], "deny": ["image.ocr"],
  "permissions": ["network"], "max_concurrent_jobs": 2, "quota_mb": 2048,
  "mcp_discovery": "compact" }
```

* Agents start with **no** tools and **no** capabilities (least privilege).
* Users default to all tools plus `network`; capabilities such as `system_info`, `process_info`, `host_fs_read`, `host_fs_write` must be granted explicitly.
* Host filesystem capabilities only work when the administrator configured `fs_roots`; paths are canonicalized (symlinks resolved) and must stay inside a root.
* The engine checks, for **every** invocation regardless of interface: the tool is permitted by the principal's policy ∩ token scope, the tool's declared permissions are granted, the platform is supported and dependencies exist. Hidden tools are not merely unlisted — calling them is refused (tested for HTTP, MCP direct calls and the MCP generic runner).
* Administration endpoints require an admin principal; only owners can manage owners.

## Files and isolation

* Files are addressed by random 128-bit ids (`f_…`), never by host paths. Every lookup checks ownership; other principals get `404`.
* On the server, file parameters accept only stored file ids — host paths like `/etc/passwd` are rejected.
* Upload names are sanitized (`../`, separators, control characters, Windows device names).
* Each job runs in a fresh scratch directory and output directory; tools can only return artifacts inside their output directory (checked by the engine).
* Inline previews are limited to passive types and served with `Content-Security-Policy: sandbox`, `X-Content-Type-Options: nosniff`; everything else downloads as an attachment. JSON/CSV previews are served as `text/plain`.
* Retention: uploads and outputs expire (default 24 h); job records after 14 days; per-user quotas; periodic cleanup.

## Executing external programs

* Argument vectors only — no shell is ever invoked. Placeholders in extension manifests expand to exactly one argument. User-controlled values are validated against allow-lists (codecs, formats, page ranges) and never placed where they could be parsed as options.
* Environment cleared to a small allow-list; `HOME`/`TMPDIR` point to the job's scratch directory.
* Wall-clock timeouts and cancellation kill the whole process group (Unix).
* Unix resource limits per process: address space (`DOGUBAKO_EXEC_MEMORY_MB`), CPU seconds, max file size, no core dumps.
* Global and per-principal concurrency limits; bounded stdout capture.
* Optional **bubblewrap sandbox** (`sandbox = "bwrap"`, Linux): no network, private `/tmp`, PID/IPC/UTS namespaces, read-only system directories, and only the job's inputs, scratch and output directories visible. Requires unprivileged user namespaces; inside Docker the container itself is the isolation boundary (the compose file drops all capabilities, uses a read-only root filesystem, `no-new-privileges`, and CPU/memory/PID limits).
* Windows: timeouts and cancellation work; rlimits and bwrap are not available. macOS: rlimits work, bwrap is not available.
* **In-process engines that cannot observe cancellation** (currently jq via jaq, where a filter can loop forever without output) run in a worker process: the host binary re-executes itself as `dogubako __dogubako-worker jq`, under the same limits, timeout and cancellation as any external program. When cancelled or timed out, other blocking work that cannot be aborted is awaited before the job's concurrency slot is released.
* Command arguments are never logged (some carry secrets such as archive passwords).

## Untrusted content

* Archive extraction rejects absolute paths, `..` components, links escaping the destination and device files, and enforces total-bytes and entry-count limits while streaming (decompression bombs).
* Image decoding is bounded by `max_image_pixels`.
* XML parsing does not expand external entities. Markdown → HTML output is sanitized (ammonia) and previews render in sandboxed iframes.
* PDF password removal requires the user to supply the password.

## Network tools and SSRF

On hosted instances `allow_private_network = false` (default):

* Every DNS resolution made by tools is filtered; hosts resolving to loopback, RFC 1918, link-local (incl. `169.254.169.254`), CGNAT/Tailscale `100.64/10`, ULA, multicast, documentation or other non-public ranges are refused. The filtered addresses are the ones connected to, which also defeats DNS rebinding.
* IP-literal URLs and every redirect hop are checked; redirects are limited.
* Raw TCP/TLS tools (`tls_inspect`, `port_check`, `whois`, `connectivity`) resolve through the same policy. Port checks are capped and rate bounded.
* Network tools require the `network` capability.

## MCP

* No shell-command tool exists. Agents can only invoke registered tools.
* Remote MCP requires a bearer API token; sessions (`Mcp-Session-Id`) are bound to the token that created them.
* `Origin` headers are validated (DNS-rebinding protection); configure `allowed_origins` for browser-based clients.
* Discoverability (full vs. compact listing) is independent from authorization.

## Audit log

Logins (success/failure/rate-limited), setup, token creation/revocation, user
and policy changes, uploads, model downloads, every tool run (tool, principal,
outcome, interface) and denied invocations are recorded and viewable in
**Admin → Audit log** (`GET /api/v1/admin/audit`).

## Reporting vulnerabilities

Please report security issues privately via GitHub security advisories on the
repository rather than public issues.
