# Plain words (natural-language mode)

Type what you want done where you'd normally pick a tool, in the search
(⌘K / the home input) or on the "What should happen to it?" screen after
dropping files, and a model decides what to do:

* **Clear request** ("convert this PNG to lossless WebP", "strip the location
  from these photos") → it runs the tools, shows each step, and puts the
  results underneath. **Undo** removes what it made and goes back; the
  original files are never changed, and every step is also in History.
* **A tool name, or a request missing a choice only you can make**
  ("convert this file", "resize") → it opens that tool with your files and
  any settings it's sure of filled in.
* **A question a tool can answer** ("what DNS records does example.com have")
  → it runs the tool and answers in a sentence.

Short queries still search tools as before; a sentence puts **Do it** first.
<kbd>⌘/Ctrl</kbd> <kbd>↵</kbd> runs the words directly.

It is off until you turn it on in **Settings → Plain words**.

## Models

Any endpoint that speaks the OpenAI chat-completions API with tool calling
works. Presets: OpenAI (`gpt-6-luna`), Anthropic (its OpenAI-compatible API),
OpenRouter, Ollama, LM Studio, and Custom (any base URL). Every preset accepts
any model id; **Load models** asks the endpoint for its list, and **Test
connection** sends a one-word request. A server administrator can also offer a
shared model ("This server") with `DOGUBAKO_ASSISTANT_BASE_URL`,
`DOGUBAKO_ASSISTANT_API_KEY` and `DOGUBAKO_ASSISTANT_MODEL`. When a model is set,
every request through "This server" uses it, so users can't spend the
administrator's key on other models.

## How it works

* The tool-calling loop runs in the UI (`web/src/lib/assistant/agent.ts`). The
  model gets a one-line catalog of the tools you may use plus four functions:
  `tool_details`, `run_tool`, `open_tool` and `find_tools`. This keeps the
  request small and works with endpoints that cap the number of tools.
* Tools run through the normal run API, so validation, permissions, quotas and
  the audit log apply exactly as for a manual run.
* The host only relays the model requests (`crates/core/src/llm.rs`): the
  server at `/api/v1/assistant/*`, the desktop app through Tauri commands. This
  avoids browser CORS limits (local runtimes rarely send CORS headers, and the
  server's CSP only allows same-origin requests).

## Privacy and safety

* Your request, file names and tool results (such as extracted text) are sent
  to the model provider you chose. File contents are processed by Dogubako and
  are not uploaded to the model. With a local model, nothing leaves your machine.
* API keys are saved in the browser/app on your device only. On a server they
  travel with each relayed request and are never stored, and signing out
  forgets them; the server's own key (if configured) is never sent to browsers.
* The model can only pass the files you gave it, or outputs of its own runs,
  to tools. Any other file id or path is refused, so instructions hidden in a
  document can't make it read other files. Tool results are marked as data,
  not instructions, in the prompt, and settings that name host paths always
  wait for your OK, with every value shown in full.
* Undo removes what the request made (output files and History entries) and
  goes back; finished requests aren't re-run when you return with Back. If a
  step changed your files (renamed, deleted or wrote to a folder, always after
  your OK), Undo becomes "Remove results": it removes only the new result
  files, says that the changes stay, and keeps those steps in History.
* Finished requests are kept for the browser tab only when History is on,
  with passwords and keys masked.
* Opening a tool on your behalf never prefills a tool that would need your OK
  to run, since some tool pages run as soon as they open.
* Steps that use the internet, write to host folders, read processes or change
  files wait for your OK; choose "Every step" to approve each run.
* On a server the relay follows the network policy: it reaches only public
  addresses unless `DOGUBAKO_ALLOW_PRIVATE_NETWORK` or
  `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE` is set. Agent accounts can't use it (they
  bring their own model through MCP). Requests are rate limited per user (at most
  4 at once) and audit-logged with the model id and endpoint host.
