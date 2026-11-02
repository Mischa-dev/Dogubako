# dogubako.app

The project website. The documentation is a separate Astro site at
docs.dogubako.app, in `site/`.

This is a single static page with no build step, no framework and no
third-party requests. Fonts are self-hosted (SIL OFL, see `fonts/`), and the
colours and type mirror `web/src/styles/tokens.css` so the site and the app look
like one thing.

| File | What it is |
|---|---|
| `index.html` | The page |
| `styles.css` | Styles, light and dark (follows the system, with a toggle) |
| `main.js` | Theme toggle, the working bench, the tool cabinet, the four-ways switcher and the screenshot tour |
| `screenshots/`, `og.png` | WebP copies of `docs/screenshots` and the 1200×630 link preview, made by `scripts/screenshots.py` |
| `scripts/` | Screenshot capture, sample files and the WebP conversion |
| `CNAME` | The domain, for GitHub Pages |

## What's interactive

* **The bench** (hero). Drop or choose a file and it's drawn on the bench with
  the app's suggestions. Tools in `RUN` in `main.js` really run in the tab
  (compress, convert or clean a photo, format JSON, count or sort text, view a
  CSV, hash, detect the type, hex view) and put their result on the bench.
  Nothing is uploaded. The rest show the CLI command instead.
* **The cabinet** (`#tools`). One `<details>` drawer per category, listing every
  tool by name, with a search box. The lists come from the tool registry; if
  tools are added or renamed, update them in `index.html`.
* **One tool, four ways in** (`#everywhere`). `WAYS` in `main.js` describes six
  tools once, and the app form, CLI, HTTP and MCP views are generated from it.
  The HTML holds a pre-rendered copy of the first tool for visitors without JS;
  if you change `WAYS['image.compress']`, copy the rendered markup back in.

## Preview

```sh
python3 -m http.server --directory website 8000
# open http://localhost:8000
```

## Refresh the screenshots

The README and the site share one set of screenshots in `docs/screenshots`.
`scripts/capture-app.mjs` re-captures all of them from a running server: it
seeds a few runs through the HTTP API, then walks the UI in Chromium.

```sh
cargo build -p dogubako-cli && (cd web && npm ci && npm run build)
python3 website/scripts/samples.py /tmp/dgb-samples          # sample photos and a CSV
target/debug/dogubako server start --no-auth --data-dir /tmp/dgb-data --bind 127.0.0.1:8420 &
node website/scripts/capture-app.mjs http://127.0.0.1:8420 /tmp/dgb-samples
python3 website/scripts/screenshots.py                        # WebP copies for the site
```

Use a fresh `--data-dir` each time so the bench and history look the same.
The scripts need Pillow (`pip install pillow`) and Playwright with Chromium
(`NODE_PATH=$(npm root -g)` if Playwright is installed globally).

## Trailer

The trailer lives once in `trailer/dist` (about 24 MB), so it isn't committed
here. Copy it next to the page before previewing or deploying:

```sh
website/scripts/trailer.sh      # copies the video and poster into website/trailer/ (git-ignored)
```

If the video isn't there, the page shows a "trailer is on its way" card
instead of a broken player.

## Placeholders

* **Downloads**: the three platform cards link to GitHub Releases. Point them at
  the installer assets once the alpha is published, and drop the
  "Coming with the alpha" labels. It's marked with a `PLACEHOLDER` comment in
  `index.html`.

## Deploying

Any static host works: run `website/scripts/trailer.sh`, then serve the `website/` folder as the web root. For GitHub
Pages, publish `website/` (for example with `actions/upload-pages-artifact` and
`actions/deploy-pages`) and keep `CNAME` so the domain stays `dogubako.app`.

## Prerelease review

The hero and platform cards identify this as a prerelease. No public GitHub release existed at review time on November 2, 2026. Keep the status and source-build wording until a release is approved and real installer assets exist. The docs links currently use repository documents while PR #4 remains undeployed.

The browser demo uses simpler implementations than the native app. Engines are installed separately; models download on request. Network tools make outbound requests. The Japanese name uses a self-hosted three-glyph Noto Sans JP subset, with its OFL in `fonts/noto-sans-jp-LICENSE.txt`.

Review screenshots and interaction checks used Chromium at 1440×900, 390×844 and 320×740 in light/dark themes. This does not establish Safari/iOS, native desktop packaging, touch, screen-reader or real-device behavior. PR #2's `web/` sources are untouched.
