# dogubako.app

The project website. The documentation is a separate Astro site at
docs.dogubako.app, in `site/`. The design concepts this site was picked from
(and the first static draft) are kept in `design/website-concepts/`.

Static pages with no build step, no framework and no third-party requests.
Fonts are self-hosted (SIL OFL, in `assets/fonts/`), and the colours and type
mirror `web/src/styles/tokens.css` so the site and the app look like one thing.
Every page follows the system's light or dark mode and has a toggle (stored in
`localStorage` as `dgb-theme`). Pages use relative links, so the site also works
from a sub-folder.

## Pages

| Path | What it is |
|---|---|
| `index.html` | Home: the tilted app window that flattens and plays as you scroll, the trailer, "each tool shows its work", four ways in, a tool finder, privacy, and the alpha card |
| `tools/` | All tools, searchable, by category, with links to each tool in the docs |
| `self-hosting/` | Running one server for a household or team |
| `agents/` | MCP, the HTTP API and the CLI for AI agents and scripts |
| `download/` | Release status, "Use the app" and "Host it on a server", and the copy-prompt for an AI agent |
| `agent-install.md` | Step-by-step server install written for AI agents (the download page's prompt points here) |
| `llms.txt` | The short index for language models |

## Files

| File | What it is |
|---|---|
| `assets/css/site.css` | Shared: tokens, type, buttons, nav, footer, sections, reveals |
| `assets/css/home.css`, `app.css` | Home only; `app.css` draws the app window in the demo |
| `assets/css/pages.css` | The four sub-pages |
| `assets/js/site.js` | Shared: theme toggle, phone menu, reveals |
| `assets/js/timeline.js` | The small animation engine every demo uses (`render(t)`, pauses offscreen and with reduced motion) |
| `assets/js/demo.js`, `home.js`, `bento.js`, `finder.js` | The home page's demo, sections, tool tiles and finder |
| `assets/js/pages/*.js` | The sub-pages' demos, the tools search and the copy buttons |
| `assets/js/tools-data.js` | Every tool's id, name, description and keywords (generated, see below) |
| `assets/img/`, `og.png`, `favicon.svg`, `apple-touch-icon.png` | Images and icons |
| `screenshots/` | WebP copies of `docs/screenshots`, made by `scripts/screenshots.py` |
| `scripts/` | Tool list generators, screenshot capture, the trailer copy |
| `CNAME` | The domain, for GitHub Pages |

## Preview

```sh
python3 -m http.server --directory website 8000
# open http://localhost:8000
```

## When tools change

`assets/js/tools-data.js` feeds the home finder and the tools page search, and
the tools page also has a pre-rendered list for visitors without JavaScript.
Regenerate both from the real catalog:

```sh
dogubako tools list --json > /tmp/tools.json     # or the docs site's public/tools.json
node website/scripts/tools-data.mjs /tmp/tools.json
node website/scripts/build-tools-page.mjs
```

Tool links go to the tool's heading on its docs category page
(`https://docs.dogubako.app/tools/<category>/#<heading>`), so they follow the
docs' heading slugs.

## Trailer

The trailer lives once in `trailer/dist` (about 24 MB), so it isn't committed
here. `scripts/trailer.sh` copies the video and poster into `website/trailer/`
(git-ignored), and only if `trailer/dist/review-status.json` says the cut is
reviewed and its checksums match. The home page checks for
`trailer/dogubako-alpha-trailer.mp4` with a HEAD request: if it's there, the
player takes the trailer frame and the hero shows "Watch the trailer"; if not,
the frame shows a "trailer is on its way" card.

## Release day

* **Downloads**: done for 0.1.0-alpha.1. For the next release, update the macOS
  card's DMG link on `download/` (or run `packaging/release/website_links.py
  --release <tag>`), and the version in the page text and in `llms.txt`.
* **Server image**: when a public image is published, name it in step 1 of the
  download page's server steps and in `agent-install.md` (both say there is no
  prebuilt image yet).

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

## Deploying

Any static host works: run `website/scripts/trailer.sh`, then serve the
`website/` folder as the web root. For GitHub Pages, publish `website/` (for
example with `actions/upload-pages-artifact` and `actions/deploy-pages`) and
keep `CNAME` so the domain stays `dogubako.app`. `scripts/` and this README
don't need to be published.
