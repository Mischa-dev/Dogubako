---
title: Plain words
description: Say what you want done instead of picking a tool. Set up a model, see what it does, and what it sends where.
---

Plain words lets you type what you want done, such as "convert this PNG to
lossless WebP" or "strip the location from these photos", where you would
normally pick a tool. A language model you choose reads the request and runs or
opens the right Dogubako tools for you.

It is **off by default** and sends nothing anywhere until you set it up.

## Turn it on

1. Open **Settings → Plain words** and switch on **Do things in plain words**.
2. Pick a **Model provider**:

   | Provider | Base URL | Key |
   |---|---|---|
   | OpenAI | `https://api.openai.com/v1` | Required |
   | Anthropic | `https://api.anthropic.com/v1` (its OpenAI-compatible API) | Required |
   | OpenRouter | `https://openrouter.ai/api/v1` | Required |
   | Ollama | `http://localhost:11434/v1` | Not needed |
   | LM Studio | `http://localhost:1234/v1` | Not needed |
   | Custom | Any base URL | Depends on the server |
   | This server | Set by the administrator (self-hosted only) | Kept on the server |

3. Enter your API key if the provider needs one, then choose a **Model**.
   Each preset suggests models, and you can type any model id, including your
   own fine-tunes. **Load models** fetches the endpoint's own list.
4. Click **Test connection**. It sends a one-word request and shows the reply
   or a readable error.

Any endpoint that speaks the OpenAI chat-completions API **with tool calling**
works. Pick a model that supports tool calling, or the requests will fail.

## Use it

Type into the search (<kbd>⌘</kbd> <kbd>K</kbd>), the box on Home, or the
**What should happen to it?** screen after you drop files. Short queries still
search tools as before. When you type a sentence, **Do it** comes first in the
list, and <kbd>⌘</kbd> <kbd>Enter</kbd> (<kbd>Ctrl</kbd> <kbd>Enter</kbd>)
runs it directly.

What happens next depends on the request:

| You type | Dogubako |
|---|---|
| A clear request: "convert this PNG to lossless WebP" | Runs the tools in place. Each step is listed and can be expanded to show its result, and the outputs appear underneath |
| A tool name, or a request that leaves a choice to you: "convert this file" | Opens that tool with your files and every setting it is sure of filled in, plus a one-line note on what is left to pick |
| A question a tool can answer: "what DNS records does example.com have" | Runs the tool and answers in a sentence |

A model switcher sits next to running requests, so you can retry with a
different model without going back to Settings.

### Undo

**Undo** removes what a request made (its output files and History entries)
and takes you back. Your original files are never changed by an ordinary run,
and every step is also recorded in History.

If a step did change files on disk, such as a rename, a delete or a write to a
host folder (always after your OK), the button reads **Remove results**
instead. It removes only the new result files, tells you that the other
changes stay, and keeps those steps in History.

## Confirmation prompts

Steps that use the internet, write to host folders, read the list of
processes or change files always stop and wait for **Run** or **Skip**, with
every value shown in full. Under **Ask before running** in Settings you can
choose:

- **Risky steps** (the default): only those steps wait for you.
- **Every step**: you approve every tool run.

Opening a tool on your behalf never pre-fills a tool that would need your OK,
because some tool pages run as soon as they open.

## What is sent where

- **To the model provider:** your request, the names of the files you gave
  it, and tool results the model needs to answer (such as extracted text).
- **Not sent:** file contents. Dogubako processes the files itself, and the
  model only sees results. With a local model (Ollama, LM Studio), nothing
  leaves your machine.
- **API keys** are saved in the app or browser on your device only. On a
  self-hosted server they travel with each relayed request and are never
  stored; signing out forgets them.
- The model can only pass the files you gave it, or outputs of its own runs,
  to tools. Any other file id or path is refused, so instructions hidden inside
  a document can't make it read other files. Tool results are marked as data,
  not instructions, in the prompt.
- Tools run through the normal run API, so the same validation, permissions,
  quotas and audit log apply as for a manual run.
- Finished requests are kept for the browser tab only when History is on, with
  passwords and keys masked.

## On a self-hosted server

The browser can't reach most model endpoints directly (the server's CSP allows
only same-origin requests, and local runtimes send no CORS headers), so the
server relays model requests at `/api/v1/assistant/*`. The desktop app relays
them itself.

The relay:

- follows the network policy. It reaches public addresses only, unless
  `DOGUBAKO_ALLOW_PRIVATE_NETWORK` or `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE` is
  set. Set the second one to use Ollama or LM Studio on the same host or LAN.
- is for people only. Agent accounts get `403`; they bring their own model
  through [MCP](/agents/overview/).
- needs the CSRF token, allows at most 4 requests at once per user, and writes
  an audit entry with the model id and endpoint host only.

An administrator can offer one shared model to everyone as **This server**:

```sh
DOGUBAKO_ASSISTANT_BASE_URL=https://api.openai.com/v1
DOGUBAKO_ASSISTANT_MODEL=gpt-6-luna
DOGUBAKO_ASSISTANT_API_KEY=sk-…        # stays on the server, never sent to browsers
```

When a model is set, every request through **This server** uses it, so people
can't spend the administrator's key on other models. Set
`DOGUBAKO_ASSISTANT_ENABLED=false` to turn plain words off for the whole
server. The [Docker setup](/self-hosting/docker/#plain-words-and-a-local-model)
includes an optional Ollama service for a fully local model.

See [Configuration reference](/reference/configuration/#plain-words) for every
setting.
