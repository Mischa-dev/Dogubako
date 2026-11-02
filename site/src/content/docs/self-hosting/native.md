---
title: Native install
description: Run the Dogubako server as a systemd service on Linux, or directly on macOS and Windows.
sidebar:
  order: 3
---

The server is the same `dogubako` binary as the CLI. A native install uses
the engines installed on the host.

## Linux with systemd

Build the web UI and the binary (see [Building](/develop/building/)), then:

```sh
sudo install -m755 target/release/dogubako /usr/local/bin/
sudo useradd --system --home /var/lib/dogubako dogubako
sudo cp deploy/dogubako.service /etc/systemd/system/
sudo systemctl enable --now dogubako
journalctl -u dogubako | grep "Setup token"
```

The bundled unit:

- runs as the `dogubako` user with its data in `/var/lib/dogubako`,
- listens on `127.0.0.1:8420`,
- reads extra settings from `/etc/dogubako.env` if it exists (one
  `DOGUBAKO_…=value` per line),
- is hardened with `NoNewPrivileges`, `PrivateTmp`, `ProtectSystem=strict`,
  read-only home directories and kernel protections, and caps memory at 8 GB
  and tasks at 512.

Install the engines you want with your package manager. `dogubako deps`
prints the exact commands:

```sh
sudo apt install ffmpeg qpdf poppler-utils ghostscript tesseract-ocr \
  libimage-exiftool-perl imagemagick 7zip bubblewrap
sudo systemctl restart dogubako
```

To sandbox external programs with bubblewrap, set `DOGUBAKO_SANDBOX=bwrap` in
`/etc/dogubako.env`. It needs unprivileged user namespaces, which most modern
distributions enable. See [Security](/project/security/#executing-external-programs).

## macOS and Windows

Run the server directly:

```sh
dogubako server start --data-dir ~/dogubako-data
```

The first start prints the setup token. To keep it running, use a launchd
agent on macOS or a scheduled task at logon on Windows. Resource limits per
external program are enforced on macOS; on Windows, timeouts and cancellation
work but per-process limits and bubblewrap are not available.

## Server commands

```sh
dogubako server start [--bind ADDR] [--data-dir DIR] [--config FILE] [--public-url URL] [--no-auth]
dogubako server config      # print the effective configuration as TOML
dogubako server health      # exit 0 if a server answers on DOGUBAKO_BIND (for health checks)
```

The release build embeds the web UI. A debug build serves `web/dist` from
disk, which is handy when you work on the UI: rebuild it and reload.

## Trying it locally without accounts

For a quick look on your own machine only:

```sh
dogubako server start --no-auth
```

`--no-auth` is accepted only on a loopback address. The server refuses to
start without authentication on any other address.
