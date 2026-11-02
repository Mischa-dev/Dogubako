---
title: Try the alpha
description: Install Dogubako 0.1.0-alpha.1, open the unsigned macOS app, load the homelab image, and report what breaks.
---

**0.1.0-alpha.1** is the first public test build. Things will break, and
settings or file formats may change before 1.0. Please
[report problems](https://github.com/mischa-dev/dogubako/issues) as you find
them.

## What's in it

| | Download | Notes |
|---|---|---|
| macOS app | `Dogubako_0.1.0-alpha.1_aarch64.dmg` | Apple silicon, macOS 15 (Sequoia) or newer |
| Homelab / server | `ghcr.io/mischa-dev/dogubako:0.1.0-alpha.1` | Docker, linux/amd64, every engine included. Private for now: [build it yourself](/self-hosting/docker/) |
| CLI | `dogubako-<target>` for Linux x86-64, macOS Apple silicon and Windows x86-64 | Uses the engines installed on the system |
| Checksums | `SHA256SUMS.txt` | |
| Engine sources | `engine-sources-0.1.0-alpha.1.tar` | Corresponding source for the GPL, LGPL, AGPL and MPL engines |

Everything is on the
[GitHub release](https://github.com/mischa-dev/dogubako/releases). Not in
this alpha: installers for Windows, the Linux desktop and Intel Macs. You can
[build those from source](/develop/building/).

## Open the unsigned app

Until the app is signed with an Apple Developer ID, macOS blocks it the first
time you open it:

1. Open the DMG and drag **Dogubako** into **Applications**, then eject the
   disk image.
2. Open Dogubako from Applications. macOS says it "can't be opened" or "can't
   verify the developer". Click **Done**, not **Move to Trash**.
3. Open **System Settings → Privacy & Security**, scroll down to the message
   about Dogubako and click **Open Anyway**. Confirm with your password or
   Touch ID.

You only need to do this once. The same thing from Terminal:

```sh
xattr -dr com.apple.quarantine /Applications/Dogubako.app
```

To check the engines, open **Settings → Dependencies**. The built-in ones show
as **Built in**; the others show a **Download** button with their size. See
[Engines and models](/app/engines-and-models/).

## Test builds from CI

Builds between releases come from the **Alpha test builds** workflow. Open the
latest run under **Actions → Alpha test builds** and download the artifacts at
the bottom of the page. Nothing there is signed or published.

- **Dogubako-alpha-macos-arm64-dmg** is a DMG with *every* engine inside, so
  no tool needs a download. GitHub wraps it in a zip; open the zip, then the
  DMG, and continue with [the steps above](#open-the-unsigned-app).
- **dogubako-alpha-homelab-image-amd64** is the homelab image as a tarball.
  From a checkout of the repository:

  ```sh
  gunzip -c dogubako-alpha-amd64.tar.gz | docker load
  docker tag dogubako:alpha dogubako:latest
  cp .env.example .env              # optional
  docker compose up -d --no-build
  docker compose logs dogubako      # shows the one-time setup token
  ```

  Then open <http://127.0.0.1:8420> and finish setup with the token. See
  [Docker](/self-hosting/docker/) for the rest.

## Known issues

- **Apple silicon only.** The managed engines come from Homebrew's prebuilt
  bottles, which are no longer published for Intel Macs.
- **Some engines aren't managed yet:** LibreOffice (Office conversions),
  Piper (text to speech) and rembg (background removal). Install them
  yourself; **Settings → Dependencies** shows what's missing and how. Piper's
  upstream macOS build is currently broken.
- **ExifTool** uses the Perl that ships with macOS.
- The CLI binaries don't include engines. They use what's installed on the
  system, and `dogubako deps` lists what's missing.

## Privacy in the alpha

Where your files go depends on how you run Dogubako. With the desktop app,
they're processed on your Mac. With a hosted Dogubako, they go to that server
and are processed there, so use a server you trust. Network tools contact the
targets you give them. Model downloads, engine downloads and
[plain words](/app/plain-words/) need a connection, and only happen when you
ask. There is no telemetry.
