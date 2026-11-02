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

Installers for macOS (universal), Windows (`.msi` and `.exe`) and Linux
(`.deb`, `.rpm` and `.AppImage`) are attached to each
[GitHub release](https://github.com/mischa-dev/dogubako/releases). Until the
first release is published, [build the app yourself](/develop/building/#desktop-app-tauri-2).

1. Download the installer for your system and install it the usual way.
2. Open Dogubako. Native tools (about half the catalog) work immediately.
3. Open **Settings → Dependencies** to see which engines are missing and copy
   the install command for your system. See [Engines](#engines) below.

## CLI

The `dogubako` binary contains the whole engine, the web server and the MCP
server. Install it from a checkout with Cargo (Rust 1.95 or newer):

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
cd web && npm ci && npm run build && cd ..   # optional: embeds the web UI for `server start`
cargo install --path crates/cli
```

The release workflow also builds standalone binaries for Linux (x86_64),
macOS (Apple silicon) and Windows (x86_64); you can download them from the
workflow run of a release tag. Put the binary on your `PATH` and check it:

```sh
dogubako --version
dogubako tools list
```

Continue with the [CLI overview](/cli/overview/).

## Docker

The image bundles FFmpeg, ImageMagick, libvips, ExifTool, qpdf, Poppler,
Ghostscript, Tesseract (English), 7-Zip and bubblewrap, so every common tool
works out of the box.

```sh
git clone https://github.com/mischa-dev/dogubako && cd dogubako
docker compose up -d --build
docker compose logs dogubako | grep "Setup token"
```

Open <http://localhost:8420>, paste the setup token and create the owner
account. The full guide, including speech recognition and extra languages, is
in [Docker](/self-hosting/docker/).

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

Dogubako calls well-known open-source programs for the heavy work. They are
never bundled with the desktop app or CLI, so you install only the ones you
need. A tool whose engine is missing stays visible, is marked unavailable and
tells you what to install.

```sh
dogubako deps          # what is installed, what is missing, and how to install it
```

Common sets:

```sh
# macOS (Homebrew)
brew install ffmpeg qpdf poppler tesseract exiftool imagemagick vips ghostscript sevenzip

# Debian / Ubuntu
sudo apt install ffmpeg qpdf poppler-utils tesseract-ocr libimage-exiftool-perl \
  imagemagick libvips-tools ghostscript 7zip

# Windows (winget)
winget install Gyan.FFmpeg QPDF.QPDF
```

Speech recognition, face detection and text to speech also need small local
models, which you download from **Settings → Models** or with
`dogubako models download <id>`. See
[Engines and models](/app/engines-and-models/) for the full list.

## Updating

- **Desktop:** install the newer release over the old one. Downloaded models,
  extensions and outputs live in your user data folder and are kept.
- **CLI:** replace the binary, or run `cargo install --path crates/cli` again
  from an updated checkout.
- **Docker:** `git pull && docker compose up -d --build`. Your data lives in
  `./data` and survives rebuilds. See [Operations](/self-hosting/operations/#upgrading).
