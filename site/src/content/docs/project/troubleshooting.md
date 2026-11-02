---
title: Troubleshooting
description: Fixes for the most common Dogubako problems, from missing engines to sign-in and upload issues.
---

## macOS says the app can't be opened

The alpha isn't signed with an Apple Developer ID yet. Click **Done**, then
**System Settings → Privacy & Security → Open Anyway**. See
[Try the alpha](/start/alpha/#open-the-unsigned-app). The app needs macOS 15
or newer on Apple silicon.

## A tool says it needs software that isn't installed

The tool's engine wasn't found.

- **macOS app:** if the notice has a **Download** button, click it (or use
  **Settings → Dependencies**). LibreOffice, Piper and rembg aren't managed
  by the app yet; install them yourself and click refresh.
- **CLI or native server:** the notice (and `dogubako deps`) shows the install
  command for your system. After installing, click refresh on
  **Settings → Dependencies**, or restart the server.
- **Installed somewhere unusual:** Dogubako checks your `PATH`, its own `bin`
  folder and, on macOS, the Homebrew and MacPorts folders. Anywhere else, set
  `DOGUBAKO_<ID>_PATH` or link the program into the `bin` folder; see
  [Desktop app](/app/desktop/#how-it-finds-engines).
- **Docker:** every engine is in the image except LibreOffice, which needs
  `DOGUBAKO_WITH_OFFICE=1` in `.env` and a rebuild. Extra OCR languages need
  [`EXTRA_PACKAGES`](/self-hosting/docker/#build-options).

## An engine download fails

On-demand engines come from the project's GitHub release and are checked
against a pinned SHA-256. A failed or interrupted download is discarded; try
again from **Settings → Dependencies**. If you're offline or behind a proxy
that blocks GitHub, the download can't complete.

## Plain words doesn't respond or picks nothing

- Click **Test connection** in **Settings → Plain words**. It shows the
  provider's own error, such as a wrong key or model id.
- Use a model that supports **tool calling**.
- On a server, a model on the same host or LAN needs
  `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE=true`. If the toggle is missing, the
  administrator set `DOGUBAKO_ASSISTANT_ENABLED=false`.
- Agent accounts can't use plain words; they use [MCP](/agents/overview/).

## Transcription says a model is missing

Speech tools need both the whisper.cpp program and a model. Download a model
in **Settings → Models** or with `dogubako models download whisper-base`. On a
server, only admins can download models.

## A tool fails with `unsupported`

The tool or option isn't possible with this engine build or platform. A common
case is Homebrew's FFmpeg (which the macOS app also uses), which lacks libass,
drawtext, libvorbis and animated WebP. On a CLI or server, install an FFmpeg
build that includes them and point to it with `DOGUBAKO_FFMPEG_PATH`; the
Docker image's FFmpeg has them. See
[Known engine issues](/app/engines-and-models/#known-engine-issues).

## A job times out or hits a limit

- `timeout`: the job ran longer than the tool's timeout or the server's
  `max_job_seconds`. On a server, raise `DOGUBAKO_MAX_JOB_SECONDS` (and
  `DOGUBAKO_EXEC_CPU_SECS` for long encodes).
- `limit_exceeded`: an input hit a safety limit, such as an archive that
  expands too much or an image with too many pixels. These limits protect
  the machine from decompression bombs.
- An external program killed by the memory limit usually shows up as an
  `external_failure`. Raise `DOGUBAKO_EXEC_MEMORY_MB`, and keep the container
  memory above `EXEC_MEMORY_MB × MAX_CONCURRENT_JOBS`.

## I lost the setup token

The token is printed each time the server starts while no owner exists:
restart it and check the log (`docker compose logs dogubako | grep "Setup token"`).
Or create the owner directly on the host:

```sh
dogubako users create alice --role owner --data-dir ./data
```

## I forgot my password

An admin can reset it in **Admin → Users & agents**. On the host:

```sh
dogubako users set-password alice --data-dir ./data
```

## Sign-in works but every action fails with `forbidden`

The browser isn't sending the CSRF token, which usually means a proxy or
extension is stripping headers, or the page was cached across a sign-in.
Reload the page. If you serve over HTTPS, make sure
`DOGUBAKO_SECURE_COOKIES=true`; if you serve over plain HTTP, it must be
`false`, or the browser drops the session cookie.

## Uploads fail for large files

- Raise `DOGUBAKO_MAX_UPLOAD_MB` on the server.
- Raise the reverse proxy's body size limit (`client_max_body_size` in nginx,
  `request_body max_size` in Caddy).
- Check the account's storage quota on the **Files** page.

## Progress doesn't update, or MCP calls hang behind a proxy

Server-sent events are being buffered. Turn off response buffering for
`/api/v1/jobs/*/events` and `/mcp` (`proxy_buffering off;` in nginx). See
[Reverse proxy](/self-hosting/networking/#reverse-proxy).

## A network tool refuses an address

On a server, network tools refuse private, loopback and link-local addresses
by default, so they can't be used to probe your network. Set
`DOGUBAKO_ALLOW_PRIVATE_NETWORK=true` only if every account may reach your
internal network.

## An agent can't see or run a tool

- Agents start with no tools. Check its policy in **Admin → Users & agents**.
- A scoped token can narrow the agent's policy further.
- The tool may need a permission, such as `network`, that the agent hasn't
  been granted.
- With compact discovery, tools aren't listed; the agent has to use
  `dogubako_search_tools`.

## Getting more detail

Run with `DOGUBAKO_LOG=debug` (or `-v` on the CLI) and look at the server log.
The error object always carries its code and, for external programs, the end
of the program's own error output in `stderr`.
