---
title: Self-hosting overview
description: Why and how to run a Dogubako server for your household, team or AI agents.
sidebar:
  order: 1
---

A Dogubako server runs the same tools as the desktop app, but on a machine
you control: a home server, a NAS, a VPS or a spare laptop. Everyone you let
in uses it from a browser, scripts use the HTTP API, and AI agents connect
over MCP.

## When a server makes sense

- **Install the heavy engines once.** FFmpeg, Tesseract, Ghostscript,
  whisper.cpp and friends live on the server, so phones, tablets and work
  laptops get every tool without installing anything.
- **Share it.** Family or teammates get their own accounts, files and
  history.
- **Give agents tools without giving them a shell.** Each agent gets a
  scoped token for exactly the tools it needs, and nothing else.
- **Offload the heavy work.** Transcode video or transcribe audio on a
  machine with more cores.

## What you get

- The full web app at `/`, including on phones.
- The [HTTP API](/reference/http-api/) under `/api/v1`, with an OpenAPI
  document at `/api/v1/openapi.json`.
- The [MCP endpoint](/agents/remote/) at `/mcp`.
- Optional [plain words](/app/plain-words/), with a shared model you
  configure or each person's own.
- Accounts with roles, per-account tool policies, API tokens, quotas, file
  retention and an audit log.

## Choosing an install

| Option | Best for | Guide |
|---|---|---|
| Docker Compose | Most people. Every engine baked in at pinned versions, hardened by default | [Docker](/self-hosting/docker/) |
| Native binary with systemd | Machines without Docker, or when you want the host's own engine builds | [Native install](/self-hosting/native/) |
| `dogubako server start` | Trying it out, or macOS and Windows hosts | [Native install](/self-hosting/native/#macos-and-windows) |

## Then

1. **Make it reachable**: only from the machine, on your LAN, on your
   tailnet, or behind a reverse proxy with HTTPS. For a home server we
   recommend [Tailscale](/self-hosting/docker/#reach-it-from-your-devices-with-tailscale):
   HTTPS on every device and nothing exposed to the internet. See
   [Networking](/self-hosting/networking/).
2. **Create accounts and agents** and decide what each may do. See
   [Accounts and agents](/self-hosting/accounts-and-agents/).
3. **Tune it**: upload limits, quotas, retention, concurrency, sandboxing. See
   [Configuration](/self-hosting/configuration/).
4. **Keep it healthy**: backups, upgrades, monitoring. See
   [Operations](/self-hosting/operations/).

:::note[Authentication is always on]
Being on your LAN or tailnet is network access, not authorization. Every
request to a server is authenticated unless you bind to loopback and pass
`--no-auth` for local testing. See [Security](/project/security/).
:::
