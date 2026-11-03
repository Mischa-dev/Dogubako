---
title: Docker
description: Run Dogubako with Docker Compose, with every engine baked in at pinned versions, settings in .env, and Tailscale access.
sidebar:
  order: 2
---

Docker Compose is the recommended way to self-host ([Podman](/self-hosting/podman/)
works too, with the same compose file). The homelab image bakes
in **every engine** at exact versions from a fixed Debian 13 (trixie)
snapshot: FFmpeg, ImageMagick, ExifTool, qpdf, Poppler, Ghostscript,
Tesseract (English), Pandoc, 7-Zip and bubblewrap, plus whisper.cpp and Piper.
LibreOffice is optional. The image tag is the engine set, so a rebuild comes
back identical and the server works offline.

## Start it

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
cp .env.example .env              # optional, see below
docker compose up -d --build      # the first build takes a while; the image is about 1.5 GB
docker compose logs dogubako | grep "Setup token"
```

Open <http://localhost:8420>, enter the one-time setup token from the log, and
create the **owner** account. The setup token only works once; after that,
people sign in with their accounts.

Data lives in `./data` next to the compose file: the database, uploads and
outputs, models, extensions and the optional config file.

:::tip[Prebuilt image]
Releases publish the image as `ghcr.io/mischa-dev/dogubako:<version>`
(linux/amd64), starting with `0.1.0-alpha.1`. The package is private during the
alpha, so build it as above for now. Once it's public, set
`image: ghcr.io/mischa-dev/dogubako:0.1.0-alpha.1` in the compose file and
start with `docker compose up -d --no-build`. To load an image tarball from
a test build instead, see [Try the alpha](/start/alpha/#test-builds-from-ci).
:::

## Settings in `.env`

The compose file reads per-install settings from a `.env` file next to it.
Everything is optional; copy `.env.example` to start.

| Variable | Default | Effect |
|---|---|---|
| `DOGUBAKO_WITH_OFFICE` | `0` | `1` adds LibreOffice for Office → PDF conversion (about 500 MB). Rebuild after changing |
| `DOGUBAKO_WITH_WHISPER` | `1` | The speech recognition engine. Models are downloaded in the app. Rebuild after changing |
| `DOGUBAKO_WITH_PIPER` | `1` | The text to speech engine. Voices are downloaded in the app. Rebuild after changing |
| `DOGUBAKO_WITH_REMBG` | `0` | `1` adds rembg for background removal (about 600 MB). Models are downloaded in the app. Rebuild after changing |
| `DOGUBAKO_ASSISTANT_ENABLED` | `true` | Allow [plain words](/app/plain-words/) on this server |
| `DOGUBAKO_ASSISTANT_BASE_URL` | — | A shared model for everyone ("This server"), for example `https://api.openai.com/v1` or `http://ollama:11434/v1` |
| `DOGUBAKO_ASSISTANT_MODEL` | — | Its model id, for example `gpt-6-luna` |
| `DOGUBAKO_ASSISTANT_API_KEY` | — | Its key. Kept on the server and never sent to browsers |
| `DOGUBAKO_ASSISTANT_ALLOW_PRIVATE` | `false` | `true` when the model runs on this machine or your LAN (Ollama, LM Studio) |

Apply changes with `docker compose up -d` (add `--build` after changing an
engine option). Other server settings, such as upload limits and quotas, are
in the `environment:` block of `docker-compose.yml`; the full list is in
[Configuration](/self-hosting/configuration/).

## What the compose file does

The bundled `docker-compose.yml` is safe by default:

- It publishes the port **only on `127.0.0.1`**, so nothing outside the host
  can reach it until you choose to. For other devices, we recommend
  [Tailscale](#reach-it-from-your-devices-with-tailscale).
- The container runs as an unprivileged user with a **read-only root
  filesystem**, a 2 GB `tmpfs` for `/tmp`, **all Linux capabilities dropped**
  and `no-new-privileges`.
- It limits the container to 4 CPUs, 8 GB of memory and 512 processes, and
  runs at most 4 jobs at once (2 per account).
- A health check runs `dogubako server health` every 30 seconds.
- `./data` is mounted with `:z`, so it also works with SELinux enforcing.

Because the container is read-only, it never downloads and runs engines the
way the macOS app does. Everything it runs was in the image when it was built.

## Reach it from your devices with Tailscale

[Tailscale](https://tailscale.com) is the easiest safe way to use the server
from your phone and laptops: nothing is exposed to the internet, and you get
HTTPS with a real certificate. The repository includes a sidecar overlay:

```sh
TS_AUTHKEY=tskey-… docker compose -f docker-compose.yml -f deploy/docker-compose.tailscale.yml up -d
```

Then open `https://dogubako.<your-tailnet>.ts.net`. The overlay turns on
secure cookies for you. Sign-in still applies: being on your tailnet is
network access, not authorization. [Networking](/self-hosting/networking/)
covers Tailscale on the host, LAN access and reverse proxies.

## Plain words and a local model

People can bring their own model in **Settings → Plain words**. To offer one
model to everyone, fill in the `DOGUBAKO_ASSISTANT_*` values in `.env`.

For a fully local model, the compose file has an optional Ollama service:

```sh
docker compose --profile local-model up -d
docker compose exec ollama ollama pull <model>
```

Then set these in `.env` and run `docker compose up -d` again:

```sh
DOGUBAKO_ASSISTANT_BASE_URL=http://ollama:11434/v1
DOGUBAKO_ASSISTANT_MODEL=<the model you pulled>
DOGUBAKO_ASSISTANT_ALLOW_PRIVATE=true
```

Pick a model that supports tool calling. See [Plain words](/app/plain-words/).

## Speech and voice models

whisper.cpp and Piper are in the image by default; they need models. Download
them in **Settings → Models** as an admin, or from the host:

```sh
docker compose exec dogubako dogubako models download whisper-base
```

Models are stored in `/data/models` inside the container, which is
`./data/models` on the host.

With `DOGUBAKO_WITH_REMBG=1`, the background removal models are in the same
place (group **rembg**). A model downloaded there is used offline; one that
isn't is downloaded again by rembg each time the tool runs.

## Build options

`docker compose build` passes the `.env` engine options as build arguments.
With plain `docker build` you can set them directly:

| Build argument | Effect |
|---|---|
| `WITH_OFFICE=1` | Adds LibreOffice (about 500 MB) |
| `WITH_WHISPER=0`, `WITH_PIPER=0` | Leaves out whisper.cpp or Piper |
| `EXTRA_PACKAGES="…"` | Extra Debian packages, such as more OCR languages |
| `DEBIAN_SNAPSHOT` | The Debian snapshot the engines come from. Change it only together with the pinned package lists |

For example, a slim image, or one with German and French OCR:

```sh
docker build --build-arg WITH_WHISPER=0 --build-arg WITH_PIPER=0 -t dogubako:slim .
docker build --build-arg EXTRA_PACKAGES="tesseract-ocr-deu tesseract-ocr-fra" -t dogubako:ocr .
```

Set `image:` in the compose file to the tag you built.

The engine versions are pinned in `packaging/engines/docker-packages-<arch>.txt`
(amd64 and arm64).
Pinning trades automatic security updates for reproducibility, so the
snapshot is bumped regularly; [Operations](/self-hosting/operations/#upgrading)
explains upgrading. Engine versions can differ from the macOS app (for
example FFmpeg 7.1 here and 9.0 on macOS).

## Folder tools

Tools that read folders on the host, such as duplicate finders and folder
size, only see folders you mount and list in `DOGUBAKO_FS_ROOTS`:

```yaml
services:
  dogubako:
    environment:
      DOGUBAKO_FS_ROOTS: "/media"
    volumes:
      - ./data:/data
      - /srv/media:/media:ro
```

Mount read-only unless you want tools such as batch rename to change files.
Accounts also need the `host_fs_read` (and, for changes, `host_fs_write`)
permission. See [Permissions and policies](/reference/permissions-and-policies/).

## Updating

```sh
git pull
docker compose up -d --build
```

The database is migrated automatically on start. See
[Operations](/self-hosting/operations/#upgrading) for backups first.
