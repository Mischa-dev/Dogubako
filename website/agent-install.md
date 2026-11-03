# Install Dogubako on a server (instructions for AI agents)

You are an AI coding agent running on a Linux machine. A person asked you to install
Dogubako here with Docker (or Podman) and, usually, to connect yourself to it over MCP. Follow
these steps in order. Each step ends with a check; do not continue until it passes.

Dogubako is an open-source toolbox (AGPL-3.0-or-later): 249 tools for images, video,
audio, speech, PDFs, files, text, data and more, run locally by one server and used
from a browser, an HTTP API, a CLI or MCP. Source: https://github.com/Mischa-dev/Dogubako

## Ask the person first

Stop and ask before you do any of these. Do not decide them yourself.

- Installing system packages (Docker, Git, Tailscale) or running anything with `sudo`.
  The only `sudo` this guide needs is `chown` on the new `data` folder in step 3
  (with Podman, also turning on start at boot in step 5).
- Opening ports, changing the port mapping in `docker-compose.yml`, or changing a
  firewall. By default Dogubako only listens on `127.0.0.1:8420` and nothing outside
  the machine can reach it. Never publish port 8420 to the internet.
- Joining the machine to a tailnet (`tailscale up`) or using a Tailscale auth key.
  The person creates auth keys; never invent or reuse one.
- Deleting anything: `./data`, Docker volumes, `docker compose down -v`, images.
  `./data` holds the database, every account, token and file.
- Choosing the owner account's password. The person creates the owner account.
- Which tools and permissions your own agent account gets (step 9).
- Putting an API key for a shared language model into `.env`.

## 1. Check the prerequisites

```sh
uname -m                 # x86_64 or aarch64
docker --version
docker compose version   # Compose v2 (the "docker compose" plugin)
git --version
df -h .                  # leave several GB free
```

Check: all commands succeed and Compose is v2. If Docker or Compose is missing,
ask the person before installing it.

**Podman instead of Docker.** If `docker` is missing but `podman` is installed
(Fedora, RHEL, CentOS Stream), use Podman and don't install Docker. Check
`podman --version` and `podman compose version` (it needs `podman-compose` or
`docker-compose` installed). Then read every `docker` in this guide as `podman`:
`podman compose up -d --build`, `podman compose logs dogubako`,
`podman compose exec dogubako dogubako …`. Decide once whether it runs as root
(rootful: every `podman` command as root or with `sudo`) or as a normal user
(rootless: never with `sudo`), and don't mix them: the two keep separate images
and containers. Steps 3 and 5 differ.

Plan for an image of about 1.5 GB (about 2 GB with LibreOffice), plus room for
uploads. The first build compiles Dogubako from source and can take a long time on a
small machine; let it finish.

There is no public prebuilt image yet. Do not try to pull one. Build from source
with Compose as below. When a prebuilt image is published, this page will name it.

## 2. Get the code

```sh
git clone https://github.com/Mischa-dev/Dogubako dogubako
cd dogubako
```

Check: `ls docker-compose.yml .env.example Dockerfile` lists all three. Run every
later command from this folder.

## 3. Prepare the data folder

The server runs as user `10001` inside the container and stores everything in
`./data`. If Docker creates that folder itself it belongs to root and the server
cannot write to it.

```sh
mkdir -p data && sudo chown 10001 data
```

With rootless Podman, use this instead (no `sudo`; the container's user 10001 maps
to a different user on the host):

```sh
mkdir -p data && podman unshare chown 10001:10001 data
```

Check: `ls -ldn data` shows owner `10001` (rootless Podman: `podman unshare ls -ldn data`).

## 4. Optional settings (.env)

Everything here is optional. Skip this step unless the person asked for one of these.

```sh
cp .env.example .env
```

| Variable | Default | Meaning |
|---|---|---|
| `DOGUBAKO_WITH_OFFICE` | `0` | `1` adds LibreOffice for Office to PDF conversion (about 500 MB) |
| `DOGUBAKO_WITH_WHISPER` | `1` | whisper.cpp speech recognition (models are downloaded in the app) |
| `DOGUBAKO_WITH_PIPER` | `1` | Piper text-to-speech (voices are downloaded in the app) |
| `DOGUBAKO_ASSISTANT_ENABLED` | `true` | Allow plain-words mode |
| `DOGUBAKO_ASSISTANT_BASE_URL` | empty | Shared model for everyone, any OpenAI-compatible endpoint with tool calling |
| `DOGUBAKO_ASSISTANT_MODEL` | empty | Model id for that endpoint |
| `DOGUBAKO_ASSISTANT_API_KEY` | empty | Kept on the server, never sent to browsers |
| `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE` | `false` | `true` when the model runs on this machine or the LAN |

The `WITH_*` values are build arguments: after changing them, run
`docker compose up -d --build` again.

For a local model, `docker compose --profile local-model up -d` also starts Ollama.
Then set `DOGUBAKO_ASSISTANT_BASE_URL=http://ollama:11434/v1`,
`DOGUBAKO_ASSISTANT_MODEL=<a model pulled into Ollama>` and
`DOGUBAKO_ASSISTANT_ALLOW_PRIVATE=true`.

Other settings (`DOGUBAKO_PUBLIC_URL`, `DOGUBAKO_SECURE_COOKIES`, quotas, upload
limits, concurrency) are set in the `environment:` block of `docker-compose.yml`,
not in `.env`.

## 5. Build and start

```sh
docker compose up -d --build
```

Check, once the build has finished:

```sh
docker compose ps                                  # dogubako is "running", later "healthy"
curl -fsS http://127.0.0.1:8420/healthz            # prints: ok
curl -fsS http://127.0.0.1:8420/api/v1/status      # JSON with "setup_required": true
```

`docker compose exec dogubako dogubako deps` lists the engines the image found.

With Podman there is no daemon, so the container does not come back after a reboot
on its own. Ask the person, then turn on Podman's restart service. It starts the
containers whose restart policy says so, not ones that were stopped by hand:

```sh
sudo systemctl enable podman-restart.service                                    # rootful
systemctl --user enable podman-restart.service && loginctl enable-linger "$USER"  # rootless
```

## 6. Get the setup token

On first start the server prints a one-time setup token (32 hex characters). Each
restart before the owner account exists prints a new one, so use the newest.

```sh
docker compose logs dogubako | grep "Setup token" | tail -n 1
```

The line looks like `dogubako  |   │ Setup token: 4f1c9a0e7d2b6c85e3a1f09b72d4c6e8`.
The URL printed above it says `http://0.0.0.0:8420`; that is the bind address
inside the container, not an address to open.

## 7. Let the person create the owner account

Give the person the token and tell them to open `http://127.0.0.1:8420` in a browser
on this machine (or the Tailscale address from step 8), enter the token, and choose
the owner's username and a password of at least 10 characters.

If they would rather use the terminal, they can run this themselves; it asks for the
password twice:

```sh
docker compose exec dogubako dogubako users create <name> --role owner
```

Check: `curl -fsS http://127.0.0.1:8420/api/v1/status` now shows
`"setup_required": false`.

## 8. Make it reachable on the person's own devices (recommended: Tailscale)

Ask first (see the top of this page). The recommended way is Tailscale: the
person's own devices get HTTPS and no port is opened to the internet.

Tailscale on the host, if the machine is already on the person's tailnet:

```sh
tailscale serve --bg 8420
tailscale serve status        # shows the https://<machine>.<tailnet>.ts.net address
```

Or as a sidecar container. The person supplies the auth key:

```sh
TS_AUTHKEY=tskey-auth-… docker compose -f docker-compose.yml -f deploy/docker-compose.tailscale.yml up -d
```

The sidecar joins the tailnet as `dogubako` and serves `https://dogubako.<tailnet>.ts.net`.
It sets `DOGUBAKO_SECURE_COOKIES=true` and removes the host port, so
`127.0.0.1:8420` no longer answers on the host. Keep passing both `-f` files to every
later `docker compose` command.

Then tell the server its address. In `docker-compose.yml`, uncomment and fill in:

```yaml
      DOGUBAKO_PUBLIC_URL: "https://<machine>.<tailnet>.ts.net"
      DOGUBAKO_SECURE_COOKIES: "true"
```

and apply it with `docker compose up -d` (with both `-f` files for the sidecar).

On a LAN without Tailscale, the port mapping would change to `"8420:8420"`, ideally
behind a reverse proxy with HTTPS. Only do that if the person asks for it.

Check: the HTTPS address opens the Dogubako sign-in page from another of the
person's devices.

## 9. Create an agent account and token for yourself

Agent accounts start with no tools. Ask the person which tools you may use. Patterns
are `*`, a category such as `image.*` or `category:pdf`, or one tool id such as
`video.compress`. Network access and host folders are extra permissions
(`--permissions network,host_fs_read,host_fs_write,system_info,process_info`); agents
get none unless the person says so.

```sh
docker compose exec dogubako dogubako users create my-agent --role agent \
  --allow 'pdf.*' --allow 'image.*' --allow 'text.*' --discovery compact
docker compose exec dogubako dogubako tokens create --user my-agent --name this-machine
```

`tokens create` prints the token (`dgb_` followed by 64 hex characters) once. Store
it straight into your MCP client configuration in step 10. Do not echo it into chat
logs, commit it, or put it in a file the person did not ask for.

Other useful flags: `--deny <pattern>`, `--max-jobs <n>`, `--quota-mb <n>`, and
`--expires-days <n>` on `tokens create`. `users policy <name> …` replaces an
account's policy later. The person can do all of this in the web app too:
**Admin → Users & agents**.

`--discovery compact` lists only the meta-tools (`dogubako_search_tools`,
`dogubako_describe_tool`, `dogubako_run_tool`, …) instead of every tool you are
allowed, which keeps your context small.

## 10. Connect yourself over MCP

Use `http://127.0.0.1:8420/mcp` when you run on this machine and the host port is
mapped, or the HTTPS address from step 8 otherwise.

Claude Code:

```sh
claude mcp add --transport http dogubako http://127.0.0.1:8420/mcp \
  --header "Authorization: Bearer dgb_…"
```

Other clients with a JSON config:

```json
{
  "mcpServers": {
    "dogubako": {
      "type": "http",
      "url": "http://127.0.0.1:8420/mcp",
      "headers": { "Authorization": "Bearer dgb_…" }
    }
  }
}
```

Clients that only speak stdio can use a bridge:
`npx mcp-remote http://127.0.0.1:8420/mcp --header "Authorization: Bearer dgb_…"`.

Your client may need a restart to pick up the new server.

## 11. Verify

1. Call `dogubako_search_tools` with `{"query": "count words"}` (or anything in a
   category you are allowed). Check: it returns tools.
2. Run one harmless tool you are allowed, for example with `text.*` allowed:
   `dogubako_run_tool` with `{"tool": "text.count", "arguments": {"text": "Dogubako is running."}}`.
   Check: it succeeds.
3. Without MCP, the same token works on the HTTP API:
   `curl -fsS -H "Authorization: Bearer dgb_…" http://127.0.0.1:8420/api/v1/status`.
   The OpenAPI description is at `/api/v1/openapi.json`.

## 12. Report back to the person

Tell them, briefly:

- The address to open (`http://127.0.0.1:8420` and the HTTPS address, if any).
- That they own the owner account, and that people and agents are added in
  **Admin → Users & agents**.
- Your agent account's name, the tools and permissions it has, and where its token is
  stored (your MCP client config). Not the token itself.
- Anything you skipped or that needs them (Tailscale login, firewall, LibreOffice).
- Where the data lives: `./data` in the clone. Back up `data/dogubako.db`.

## Troubleshooting

| Symptom | Fix |
|---|---|
| Container keeps restarting; logs mention permission denied on `/data` | Step 3: `sudo chown 10001 data` (ask first; rootless Podman: `podman unshare chown 10001:10001 data`), then `docker compose up -d`. |
| `docker compose` is unknown | Compose v2 is missing. Ask the person before installing the Docker Compose plugin. |
| Build stops with exit code 137 | The machine ran out of memory while compiling. Ask the person; add swap or build on a larger machine. |
| Build fails downloading packages | Retry `docker compose up -d --build`. Check the machine's internet access and proxy. |
| Port 8420 already in use | Ask the person. They can change the host side of the mapping, e.g. `"127.0.0.1:8421:8420"`, and you use that port. |
| "invalid setup token" | Use the newest token: `docker compose logs dogubako \| grep "Setup token" \| tail -n 1`. |
| No setup token in the logs | The owner already exists (`"setup_required": false`), or the server has not started yet. |
| `401` / "invalid bearer token" | The token is wrong, expired or revoked. Create a new one with `tokens create`. |
| A tool is missing from search, or the run is denied | Your agent policy does not allow it. Ask the person; `users policy` or **Admin → Users & agents** changes it. |
| A URL tool says the host "resolves only to non-public addresses" | The private-network guard blocks LAN and loopback addresses on purpose. Leave it unless the person asks. |
| Signing in from another device over plain HTTP doesn't stick | `DOGUBAKO_SECURE_COOKIES` is `true`, so the browser only keeps the session over HTTPS. Use the HTTPS address. |

Logs: `docker compose logs -f dogubako`. Effective settings:
`docker compose exec dogubako dogubako server config`.

## Docs

- Docker setup: https://docs.dogubako.app/self-hosting/docker/
- Networking, Tailscale and reverse proxies: https://docs.dogubako.app/self-hosting/networking/
- Accounts and agents: https://docs.dogubako.app/self-hosting/accounts-and-agents/
- Configuration: https://docs.dogubako.app/self-hosting/configuration/
- Connecting a remote agent: https://docs.dogubako.app/agents/remote/
- MCP reference: https://docs.dogubako.app/reference/mcp/
- Troubleshooting: https://docs.dogubako.app/project/troubleshooting/
- Every tool as JSON: https://docs.dogubako.app/tools.json
