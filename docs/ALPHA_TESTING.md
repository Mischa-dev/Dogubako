# Trying the alpha

Test builds come from the **Alpha test builds** workflow
(`.github/workflows/alpha-test.yml`). Open the latest run under
**Actions → Alpha test builds** and download the artifacts at the bottom of the
page. Nothing there is signed or published.

## Mac (Apple silicon, macOS 15 or newer)

The test DMG has every engine inside, so no tool needs a download.

1. Download **Dogubako-alpha-macos-arm64-dmg**. GitHub wraps it in a zip:
   double-click the zip, then the `.dmg`.
2. Drag **Dogubako** into **Applications** and eject the disk image.
3. Open Dogubako from Applications. The app isn't signed yet, so macOS says it
   "can't be opened" or "can't verify the developer". Click **Done** (not
   Move to Trash).
4. Open **System Settings → Privacy & Security**, scroll down to the message
   about Dogubako, and click **Open Anyway**. Confirm with your password or
   Touch ID. From then on it opens normally.

If macOS says Dogubako **"is damaged and can't be opened"** instead, there is
no Open Anyway button. The app isn't damaged: macOS says this about unsigned
downloads (builds before November 2, 2026 weren't signed at all). Click
**Cancel**, then run this in Terminal and open it again:

```sh
xattr -dr com.apple.quarantine /Applications/Dogubako.app
```

That also works instead of step 4.

To check the engines: **Settings → Dependencies** shows each one as
**Built in**.

Intel Macs aren't supported by this build (docs/ENGINES.md explains why).

## Homelab (Docker, x86-64)

1. Download **dogubako-alpha-homelab-image-amd64** and unzip it.
2. Load and tag the image, then start it with this repo's compose file:

   ```sh
   gunzip -c dogubako-alpha-amd64.tar.gz | docker load
   docker tag dogubako:alpha dogubako:latest
   cp .env.example .env        # optional: plain-words model, API key
   docker compose up -d --no-build
   docker compose logs dogubako   # shows the one-time setup token
   ```

3. Open http://127.0.0.1:8420 and finish setup with the token.

To build the image yourself instead: `docker compose build` (about 1.5 GB;
add `DOGUBAKO_WITH_OFFICE=1` to `.env` for LibreOffice).
