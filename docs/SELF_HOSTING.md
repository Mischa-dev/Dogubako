# Self-hosting

Run Dogubako on a home server, NAS, VPS or any Linux/macOS/Windows machine and
use it from a browser, the HTTP API or MCP.

## Docker Compose (recommended)

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
docker compose up -d --build
docker compose logs dogubako | grep "Setup token"
```

Open <http://localhost:8420>, enter the setup token and create the owner account.

Podman works with the same compose file (`podman compose up -d --build`). Run every
`podman` command either with `sudo` (rootful) or without (rootless), never mixed: the two
keep separate images. Rootless, prepare `./data` with `podman unshare chown 10001:10001 data`. To start at boot,
enable `podman-restart.service`, or run it as a systemd service with the Quadlet unit
[`deploy/dogubako.container`](../deploy/dogubako.container).

The image includes FFmpeg, ImageMagick, ExifTool, qpdf, Poppler,
Ghostscript, Tesseract (English), 7-Zip and bubblewrap. Options:

* `docker build --build-arg WITH_WHISPER=1 .` also compiles whisper.cpp (offline speech recognition). Download a model afterwards in **Settings → Models** or `docker compose exec dogubako dogubako models download whisper-base` (with `DOGUBAKO_MODELS_DIR=/data/models`).
* `--build-arg EXTRA_PACKAGES="tesseract-ocr-deu tesseract-ocr-fra pandoc libreoffice-core"` adds languages and document converters.

The compose file publishes only to `127.0.0.1` and hardens the container
(read-only root FS, all capabilities dropped, `no-new-privileges`, CPU/memory/PID limits).

### LAN

Change the port mapping to `"8420:8420"` and browse to `http://<server-ip>:8420`.
Accounts are always required. Prefer HTTPS (reverse proxy) and set
`DOGUBAKO_SECURE_COOKIES=true` and `DOGUBAKO_PUBLIC_URL`.

### Tailscale

```sh
TS_AUTHKEY=tskey-auth-… docker compose -f docker-compose.yml -f deploy/docker-compose.tailscale.yml up -d
```

The sidecar joins your tailnet as `dogubako` and serves HTTPS via `tailscale serve`
(`https://dogubako.<tailnet>.ts.net`). Alternatively run Tailscale on the host
and `tailscale serve --bg 8420` (it takes port 443 and silently replaces whatever is
served there; check `tailscale serve status` first and use `--https=<port>` if 443
is taken). Tailscale ACLs are a good outer layer; Dogubako
accounts and agent policies still apply inside.

### Reverse proxy (Caddy example)

```
tools.example.com {
    reverse_proxy 127.0.0.1:8420
    request_body { max_size 2GB }
}
```

Set `DOGUBAKO_PUBLIC_URL=https://tools.example.com` and `DOGUBAKO_SECURE_COOKIES=true`.
Disable proxy response buffering for `/api/v1/jobs/*/events` and `/mcp` (SSE) if your proxy buffers.

## Native install

```sh
cargo build --release -p dogubako-cli        # after `npm ci && npm run build` in web/
sudo install -m755 target/release/dogubako /usr/local/bin/
sudo useradd --system --home /var/lib/dogubako dogubako
sudo cp deploy/dogubako.service /etc/systemd/system/
sudo systemctl enable --now dogubako
journalctl -u dogubako | grep "Setup token"
```

Install whichever engines you want (`dogubako deps` lists them with install
commands). On macOS/Windows run `dogubako server start` (e.g. as a launchd
agent / scheduled task).

## Configuration

Settings come from (lowest → highest priority) built-in defaults, a TOML file
(`<data_dir>/dogubako.toml` or `--config`/`DOGUBAKO_CONFIG`, see
[`deploy/dogubako.example.toml`](../deploy/dogubako.example.toml)),
environment variables and CLI flags. `dogubako server config` prints the effective values.

| Variable | Default | Meaning |
|---|---|---|
| `DOGUBAKO_BIND` | `127.0.0.1:8420` | Listen address |
| `DOGUBAKO_DATA_DIR` | `./data` | Database, files, models, extensions |
| `DOGUBAKO_PUBLIC_URL` | — | External base URL (absolute links for agents) |
| `DOGUBAKO_INSTANCE_NAME` | `Dogubako` | Shown in the UI and MCP |
| `DOGUBAKO_AUTH` | `required` | `none` only allowed on loopback |
| `DOGUBAKO_SETUP_TOKEN` | random | Fixed first-run setup token |
| `DOGUBAKO_SECURE_COOKIES` | `false` | `Secure` cookies (HTTPS) |
| `DOGUBAKO_SESSION_HOURS` | `336` | Session lifetime |
| `DOGUBAKO_MAX_UPLOAD_MB` | `2048` | Max upload size |
| `DOGUBAKO_USER_QUOTA_MB` | `10240` | Default storage quota per user/agent |
| `DOGUBAKO_FILE_RETENTION_HOURS` | `24` | Uploads/outputs are deleted after |
| `DOGUBAKO_JOB_RETENTION_DAYS` | `14` | Job history retention |
| `DOGUBAKO_MAX_CONCURRENT_JOBS` | CPU count | Global concurrency |
| `DOGUBAKO_PER_USER_CONCURRENCY` | `2` | Default per-principal concurrency |
| `DOGUBAKO_MAX_JOB_SECONDS` | `3600` | Hard per-job wall-clock limit |
| `DOGUBAKO_EXEC_MEMORY_MB` | `4096` | Address-space limit per external process (0 = off) |
| `DOGUBAKO_EXEC_CPU_SECS` | `3600` | CPU-time limit per external process |
| `DOGUBAKO_SANDBOX` | `none` | `bwrap` to sandbox external programs (Linux) |
| `DOGUBAKO_FS_ROOTS` | — | Comma-separated host folders available to folder tools |
| `DOGUBAKO_ALLOW_PRIVATE_NETWORK` | `false` | Let network tools reach private addresses |
| `DOGUBAKO_MCP_ENABLED` | `true` | Enable `/mcp` |
| `DOGUBAKO_ASSISTANT_ENABLED` | `true` | Allow plain-words mode (relays model requests users configure; see [ASSISTANT.md](ASSISTANT.md)) |
| `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE` | `false` | Let that relay reach loopback/LAN model servers (e.g. Ollama on this host) |
| `DOGUBAKO_ASSISTANT_BASE_URL` / `_API_KEY` / `_MODEL` | — | Optional shared model offered to every user as “This server”; the key is never sent to browsers |
| `DOGUBAKO_MCP_DISCOVERY` | `full` | `full` or `compact` |
| `DOGUBAKO_ALLOWED_ORIGINS` | — | Extra allowed Origins (browser MCP clients) |
| `DOGUBAKO_MODELS_DIR` | `<data>/models` | Model storage |
| `DOGUBAKO_EXTENSIONS_DIR` | `<data>/extensions` | Trusted TOML extensions |
| `DOGUBAKO_<DEP>_PATH` | — | Explicit path to a dependency, e.g. `DOGUBAKO_FFMPEG_PATH` |
| `DOGUBAKO_LOG` | `info` | Log filter (`debug`, `dogubako_server=debug`…) |

## Accounts and agents

* Owner (setup) → creates admins, users and **agents** in **Admin → Users & agents**.
* For each agent pick allowed categories/tools, denied tools, capabilities,
  concurrency, quota and MCP discovery mode, then create an API token.
* Offline administration on the host: `dogubako users …`, `dogubako tokens …`
  (operate directly on `<data_dir>/dogubako.db`).

## Backups

Stop the server (or use `sqlite3 data/dogubako.db .backup backup.db`) and back
up `data/dogubako.db`, `data/dogubako.toml`, `data/extensions/` and optionally
`data/models/`. `data/files/` holds short-lived uploads and outputs.

## Resource planning

Video transcoding and speech recognition are CPU heavy. Size
`MAX_CONCURRENT_JOBS` to your cores (FFmpeg uses several threads per job), keep
the container memory limit above `EXEC_MEMORY_MB × concurrency`, and put
`data/` on a disk with room for `quota × users`.
