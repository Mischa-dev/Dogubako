---
title: FAQ
description: Short answers to common questions about Dogubako.
---

### Is Dogubako free?

Yes. It is free and open-source software under the AGPL-3.0-or-later. There
are no paid tiers, accounts for desktop use, ads or telemetry.

### Do my files leave my computer?

Not with the desktop app or the CLI: everything runs on your machine. With a
self-hosted server, files go to that server and nowhere else. The only
outbound traffic comes from tools whose job is the network (such as DNS
lookups) and from model downloads you start.

### Why do some tools say they need extra software?

About half the tools are built in. The rest use mature programs such as FFmpeg
or Tesseract, which Dogubako calls but doesn't bundle, so you install only what
you need. See [Engines and models](/app/engines-and-models/).

### Does it work offline?

Yes, apart from network tools. Speech recognition, OCR, face detection and
text to speech all run locally once their engine and model are installed.

### Which operating systems are supported?

The desktop app and CLI run on macOS, Windows and Linux. The server runs
anywhere the CLI does, and in Docker on Linux. The web app works in current
browsers, including on phones.

### What does "Dogubako" mean?

道具箱 (dōgubako) is Japanese for "toolbox".

### Can I use it from scripts or other programs?

Yes: the [CLI](/cli/overview/) for shell scripts, the
[HTTP API](/reference/http-api/) with an OpenAPI document for everything else,
and [MCP](/agents/overview/) for AI agents.

### Is it safe to give an AI agent access?

That is what agent accounts are for. An agent starts with no tools and no
permissions, you grant exactly what it needs, and every call is checked by the
engine whatever interface it uses. There is no shell tool. See
[Permissions and policies](/reference/permissions-and-policies/).

### Can I add my own tools?

Yes. Wrap any command-line program with a TOML [extension](/develop/extensions/),
or [write a Rust tool](/develop/writing-tools/).

### Why the AGPL?

So that a hosted, multi-user Dogubako stays free software for its users:
anyone who offers a modified Dogubako over a network must share its source.
It is also compatible with the GPL-licensed engines Dogubako works with. See
[Licensing](/project/licensing/).

### Where do I report a bug or a security issue?

Bugs go to [GitHub issues](https://github.com/mischa-dev/dogubako/issues).
Security issues go to
[GitHub security advisories](https://github.com/mischa-dev/dogubako/security/advisories/new),
privately.
