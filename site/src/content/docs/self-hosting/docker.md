---
title: Docker
description: Run Dogubako with Docker Compose, with every common engine included and a hardened container.
sidebar:
  order: 2
---

Docker Compose is the recommended way to self-host. The image includes FFmpeg,
ImageMagick, libvips, ExifTool, qpdf, Poppler, Ghostscript, Tesseract
(English), 7-Zip and bubblewrap.

## Start it

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
docker compose up -d --build
docker compose logs dogubako | grep "Setup token"
```

Open <http://localhost:8420>, enter the one-time setup token from the log, and
create the **owner** account. The setup token only works once; after that,
people sign in with their accounts.

Data lives in `./data` next to the compose file: the database, uploads and
outputs, models, extensions and the optional config file.

## What the compose file does

The bundled `docker-compose.yml` is safe by default:

- It publishes the port **only on `127.0.0.1`**, so nothing outside the host
  can reach it until you choose to (see [Networking](/self-hosting/networking/)).
- The container runs as an unprivileged user with a **read-only root
  filesystem**, a 2 GB `tmpfs` for `/tmp`, **all Linux capabilities dropped**
  and `no-new-privileges`.
- It limits the container to 4 CPUs, 8 GB of memory and 512 processes.
- A health check runs `dogubako server health` every 30 seconds.

Change settings with environment variables in the compose file. The common
ones are already there, commented. The full list is in
[Configuration](/self-hosting/configuration/).

## Build options

| Build argument | Effect |
|---|---|
| `WITH_WHISPER=1` | Also compiles whisper.cpp for offline speech recognition |
| `WHISPER_REF=v1.7.6` | The whisper.cpp version to build |
| `EXTRA_PACKAGES="…"` | Extra Debian packages, such as OCR languages or document converters |
| `RUST_VERSION`, `NODE_VERSION` | Toolchain versions used to build |

For example, speech recognition plus German and French OCR plus document
conversion:

```sh
docker build \
  --build-arg WITH_WHISPER=1 \
  --build-arg EXTRA_PACKAGES="tesseract-ocr-deu tesseract-ocr-fra pandoc libreoffice-core" \
  -t dogubako:full .
```

Set `image: dogubako:full` in the compose file (and remove `build: .`), or put
the same values under `build.args` in the compose file.

### Speech recognition models

After building with `WITH_WHISPER=1`, download a model, either in **Settings
→ Models** as an admin, or from the host:

```sh
docker compose exec dogubako dogubako models download whisper-base
```

Models are stored in `/data/models` inside the container, which is
`./data/models` on the host.

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
