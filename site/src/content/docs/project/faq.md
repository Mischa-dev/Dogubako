---
title: FAQ
description: Short answers to common questions about Dogubako.
---

### Is Dogubako free?

Yes. It is free and open-source software under the AGPL-3.0-or-later. There
are no paid tiers, accounts for desktop use, ads or telemetry.

### Do my files leave my computer?

It depends on how you run Dogubako. The desktop app and the CLI process files
on your computer. With a hosted Dogubako, files go to that server and are
processed there, so use a server you trust. Other outbound traffic comes only
from tools whose job is the network (such as DNS lookups), from model and
engine downloads you start, and from [plain words](/app/plain-words/) if you
turn it on: that sends your request, file names and tool results (not the
files themselves) to the model provider you picked.

### Why do some tools say they need extra software?

About half the tools are native. The rest use mature programs such as FFmpeg
or Tesseract. The macOS app has the common ones built in and downloads the
bigger ones the first time you need them, and the Docker image includes them
all. With the CLI you install what you need. See
[Engines and models](/app/engines-and-models/).

### Does it work offline?

Yes, apart from network tools and plain words with a hosted model. Speech recognition, OCR, face detection and
text to speech all run locally once their engine and model are installed.

### Which operating systems are supported?

The released desktop app is for macOS 15 or newer on Apple silicon. It also
builds on Windows and Linux, but there are no installers for those yet. The CLI
runs on macOS, Windows and Linux, the server runs anywhere the CLI does and in
Docker on Linux (x86-64), and the web app works in current browsers, including
on phones.

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
