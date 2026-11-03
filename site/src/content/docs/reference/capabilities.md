---
title: Capability matrix
description: Which Dogubako tools run natively, which need an engine, model or network, and on which platforms.
---

The [tool catalog](/tools/) is the readable version: one page per category,
every tool with its engines, permissions, platforms and options. The overview
table there summarizes how many tools in each category are native or need an
engine, model or network access.

Two machine-generated versions are also available:

- **[`/tools.json`](/tools.json)** on this site: every tool's full spec
  (parameters, output kind, engines, permissions, platforms), as used to build
  the catalog.
- **`dogubako tools matrix`** prints a Markdown matrix of every tool with its
  engine, requirements, platforms and permissions. The repository keeps a
  copy in
  [`docs/CAPABILITIES.md`](https://github.com/mischa-dev/dogubako/blob/main/docs/CAPABILITIES.md).

On a machine with Dogubako installed, `dogubako tools list --available` shows
only the tools that can run right now, and `dogubako deps` shows what is
missing.

## Legend

| Engine | Meaning |
|---|---|
| native | Built into Dogubako. Nothing else to install |
| external | Calls the listed program(s) |
| model | Needs a downloadable local model |
| network | Makes outbound requests |

Every built-in tool currently supports Linux, macOS and Windows. A tool
limited to some platforms would carry a "… only" tag in the catalog.
