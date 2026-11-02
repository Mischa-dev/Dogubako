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
| `main.js` | Theme toggle, the drop demo, the screenshot tour and tabs |
| `screenshots/`, `og.png` | WebP copies of `docs/screenshots` and the 1200×630 link preview, made by `scripts/screenshots.py` |
| `scripts/` | Screenshot capture, sample files and the WebP conversion |
| `CNAME` | The domain, for GitHub Pages |

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
