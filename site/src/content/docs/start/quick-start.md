---
title: Quick start
description: Run your first Dogubako tool in the app, on the command line, over HTTP and from an AI agent.
sidebar:
  order: 3
---

This page runs the same job, compressing a photo, in each interface. Pick the
section you need.

## In the app

1. Open Dogubako (the desktop app, or your server in a browser).
2. Drag `photo.jpg` anywhere onto the window, or click **Choose files**.
3. Dogubako asks **What should happen to it?** and numbers the likeliest
   tools. Press <kbd>1</kbd> for the top one, or type a word such as
   `smaller` and press <kbd>Enter</kbd>.
4. On the tool page, adjust the settings if you like and run the tool. The
   original and the result appear side by side with the size change.
5. Download or save the result. It also lands **on your bench** on the Home
   page, ready for the next tool.

Every tool is also one search away: press <kbd>⌘</kbd> <kbd>K</kbd> (or
<kbd>Ctrl</kbd> <kbd>K</kbd>) and type its name. The
[app tour](/app/tour/) explains every screen.

## On the command line

```sh
dogubako image compress photo.jpg            # writes photo-compressed.jpg in the current folder
dogubako image compress photo.jpg -o small.jpg
dogubako image compress --help               # every option, generated from the tool
```

Find tools by keyword and inspect one:

```sh
dogubako tools search compress
dogubako tools info image.compress
```

More in the [CLI overview](/cli/overview/) and [recipes](/cli/recipes/).

## Over HTTP

Against a [self-hosted server](/self-hosting/overview/), with an
[API token](/self-hosting/accounts-and-agents/#api-tokens):

```sh
H="Authorization: Bearer $DOGUBAKO_TOKEN"
HOST=http://localhost:8420

# 1. upload the file and keep its id
ID=$(curl -s -H "$H" -T photo.jpg "$HOST/api/v1/files/raw?name=photo.jpg" | jq -r .id)

# 2. run the tool and wait up to 60 seconds for the result
curl -s -H "$H" -H 'Content-Type: application/json' \
  -d "{\"params\":{\"input\":[\"$ID\"]},\"wait_seconds\":60}" \
  "$HOST/api/v1/tools/image.compress/run" | jq '.result.artifacts'

# 3. download the output by its id
curl -H "$H" -o small.jpg "$HOST/api/v1/files/<artifact id>/content"
```

The [HTTP API reference](/reference/http-api/) covers jobs, progress events
and errors.

## From an AI agent

For an agent on the same machine, register the stdio MCP server, here with
Claude Code:

```sh
claude mcp add dogubako -- dogubako mcp serve --discovery compact
```

Then ask the agent to "compress photo.jpg with Dogubako". For a shared server
with a token per agent, see [Remote agents](/agents/remote/).
