---
title: Install
description: Install the Dogubako desktop app, the CLI, the Docker image, or build from source, then add the engines you need.
sidebar:
  order: 2
---

Pick the form that fits how you work. You can install more than one: they
share nothing but the catalog, so the desktop app and a home server can live
side by side.

| You want… | Install |
|---|---|
| An app on your own computer | [Desktop app](#desktop-app) |
| Tools in your terminal and scripts | [CLI](#cli) |
| One server for your household, team or agents | [Docker](#docker) |
| To hack on Dogubako itself | [From source](#from-source) |

## Desktop app

The current release is **0.1.0-alpha.1**, a test build. The desktop app is
available for **macOS 15 (Sequoia) or newer on Apple silicon**. Installers for
Windows, Linux and Intel Macs aren't part of the alpha; you can
[build those yourself](/develop/building/#desktop-app-tauri-2).

1. Download `Dogubako_0.1.0-alpha.1_aarch64.dmg` from the
   [GitHub release](https://github.com/mischa-dev/dogubako/releases), open it
   and drag **Dogubako** into **Applications**.
2. Open Dogubako. If the build isn't signed yet, macOS refuses the first time;
   [Try the alpha](/start/alpha/#open-the-unsigned-app) shows the two clicks
   that let it open.
3. That's it: the common engines (FFmpeg, qpdf, Poppler, ExifTool and 7-Zip)
   are inside the app, and the bigger ones download with one click the first
   time a tool needs them. See [Engines](#engines) below.

## CLI

The `dogubako` binary contains the whole engine, the web server and the MCP
server. Install it from a checkout with Cargo (Rust 1.95 or newer):

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
cd web && npm ci && npm run build && cd ..   # optional: embeds the web UI for `server start`
cargo install --path crates/cli
```

Each release also has standalone CLI binaries for Linux (x86-64), macOS
(Apple silicon) and Windows (x86-64), plus `SHA256SUMS.txt`. Put the binary on
your `PATH` and check it:

```sh
dogubako --version
dogubako tools list
```

Continue with the [CLI overview](/cli/overview/).

## Docker

The homelab image bakes in every engine at pinned versions, so every tool
works out of the box. Build it from a checkout:

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
cp .env.example .env            # optional: extra engines, a shared plain-words model
docker compose up -d --build
docker compose logs dogubako | grep "Setup token"
```

Open <http://localhost:8420>, paste the setup token and create the owner
account. Each release also publishes the image as
`ghcr.io/mischa-dev/dogubako:<version>`; for the alpha the package is still
private, so build it yourself for now. The full guide, including Tailscale,
speech recognition and a local model, is in [Docker](/self-hosting/docker/).

## From source

You need Rust 1.95 or newer, a C compiler, and Node.js 20 or newer with npm.

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
cd web && npm ci && npm run build && cd ..
cargo build --release -p dogubako-cli
./target/release/dogubako server start      # http://127.0.0.1:8420
```

[Building](/develop/building/) covers the desktop app, feature flags for
smaller binaries and running the tests.

## Engines

Dogubako calls well-known open-source programs for the heavy work. How you
get them depends on what you installed:

| You run | Engines |
|---|---|
| The macOS app | Managed by the app: five are built in, the rest download into the app's folder on first use. Nothing to install |
| The Docker image | All baked into the image |
| The CLI, or a native server | Whatever is installed on the system |

A tool whose engine is missing stays visible, is marked unavailable and tells
you what to do. For the CLI and native servers:

```sh
dogubako deps          # what is installed, what is missing, and how to install it
```

Common sets:

```sh
# macOS (Homebrew)
brew install ffmpeg qpdf poppler tesseract exiftool imagemagick ghostscript sevenzip

# Debian / Ubuntu
sudo apt install ffmpeg qpdf poppler-utils tesseract-ocr libimage-exiftool-perl \
  imagemagick ghostscript 7zip

# Windows (winget)
winget install Gyan.FFmpeg QPDF.QPDF
```

Speech recognition, face detection and text to speech also need small local
models, which you download from **Settings → Models** or with
`dogubako models download <id>`. See
[Engines and models](/app/engines-and-models/) for the full list.

## Updating

- **Desktop:** install the newer release over the old one. Downloaded models,
  extensions and outputs live in your user data folder and are kept. When an
  update pins a newer engine version, the app downloads it the next time it's
  needed and deletes the old one.
- **CLI:** replace the binary, or run `cargo install --path crates/cli` again
  from an updated checkout.
- **Docker:** `git pull && docker compose up -d --build`. Your data lives in
  `./data` and survives rebuilds. See [Operations](/self-hosting/operations/#upgrading).
