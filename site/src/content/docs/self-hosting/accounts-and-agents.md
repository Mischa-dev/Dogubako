---
title: Accounts and agents
description: Create the owner, add users and AI agents, decide what each may do, and issue API tokens.
sidebar:
  order: 5
---

Every request to a Dogubako server is made by an **account**. People sign in
with a password; scripts and AI agents use **API tokens**.

## Roles

| Role | Can |
|---|---|
| `owner` | Everything, including managing other owners. Created at first setup |
| `admin` | Manage users, agents and tokens, see the audit log, download models |
| `user` | Use the tools their policy allows, manage their own files and tokens |
| `agent` | A non-human account with no password. Uses tokens only, and starts with no tools at all |

## First run

On first start the server prints a one-time **setup token** in its log. Open
the server in a browser, paste the token, and choose the owner's username and
password (at least 10 characters). You can also create the owner on the host
without the web UI:

```sh
dogubako users create alice --role owner --data-dir ./data
```

## Adding people

In the app, go to **Admin → Users & agents → New user or agent**. Or on the
host:

```sh
dogubako users create bob --role user --data-dir ./data          # prompts for a password
echo "$PASSWORD" | dogubako users create bob --password-stdin --data-dir ./data
```

Users get **all tools** and the `network` permission by default. Permissions
that touch the host (`host_fs_read`, `host_fs_write`, `system_info`,
`process_info`) must be granted explicitly.

## Adding agents

An agent is an account for an AI agent or a bot. It starts with **no tools and
no permissions**, so you grant exactly what it needs:

```sh
dogubako users create image-bot --role agent --allow 'image.*' --deny image.ocr --data-dir ./data
dogubako users create media-bot --role agent \
  --allow 'video.*' --allow 'audio.*' --allow 'speech.*' --max-jobs 1 --quota-mb 4096 --data-dir ./data
dogubako tokens create --user media-bot --name laptop --data-dir ./data     # prints dgb_… once
```

In the app, **Admin → Users & agents → New user or agent**, with **Agent** as
the account type, has a policy editor for categories, individual tools,
permissions, concurrency, quota and MCP discovery mode. Then use **Create API
token** in the agent's row menu, and follow [Remote agents](/agents/remote/)
to connect it.

## Policies

Each account's **policy** is:

```json
{
  "allow": ["image.*", "video.compress"],
  "deny": ["image.ocr"],
  "permissions": ["network"],
  "max_concurrent_jobs": 2,
  "quota_mb": 2048,
  "mcp_discovery": "compact"
}
```

- `allow` and `deny` take patterns: `*`, a category (`image.*` or
  `category:image`) or a tool id. Deny wins.
- `permissions` grants [capabilities](/reference/permissions-and-policies/#permissions)
  such as `network` or `host_fs_read`.
- `max_concurrent_jobs`, `quota_mb` and `max_upload_mb` override the server
  defaults for this account.
- `mcp_discovery` is `full` or `compact`; see
  [Discovery](/agents/discovery-and-results/).

Change a policy in the app, or with `dogubako users policy <name> --allow … --deny …`
(which replaces it). The full rules are in
[Permissions and policies](/reference/permissions-and-policies/).

## API tokens

Tokens look like `dgb_…`, are shown once, and are sent as
`Authorization: Bearer dgb_…`.

- **Your own tokens:** **Account → API tokens** (from the menu under your
  name in the sidebar).
- **Anyone's tokens (admins):** **Admin → Tokens**, or
  `dogubako tokens create --user <name> --name <label>` on the host.
- **Expiry:** optional, in days (`--expires-days 90`).
- **Scope:** a token can be narrowed further than its account's policy, for
  example a token for one script that may only run `pdf.merge`:

  ```sh
  dogubako tokens create --user alice --name merge-script --allow pdf.merge --data-dir ./data
  ```

  A token can never do more than its account. Scoped tokens never carry admin
  rights, and tokens can't create tokens.
- **Revoke** in the app, or with `dogubako tokens revoke <id>`.

The server stores only a hash of each token. A lost token can't be recovered,
only revoked and replaced.

## Managing accounts on the host

The `users` and `tokens` commands work directly on the server's database, so
they work even when the server is stopped. Point them at the data directory
with `--data-dir` or `DOGUBAKO_DATA_DIR`.

```sh
dogubako users list
dogubako users set-password bob
dogubako users disable bob
dogubako users enable bob
dogubako users delete bob
dogubako tokens list
```

Inside Docker, run them in the container:

```sh
docker compose exec dogubako dogubako users list
```

## Audit log

Sign-ins (including failures and rate limiting), setup, token and policy
changes, uploads, model downloads, every tool run and every denied request
are recorded. Admins see them in **Admin → Audit log**, or at
`GET /api/v1/admin/audit`.
