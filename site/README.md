# dogubako.app

The project website: one static page, no build step, no framework, no
third-party requests. Fonts are self-hosted (SIL OFL, see `fonts/`), and the
colours and type mirror `web/src/styles/tokens.css` so the site and the app look
like one thing.

| File | What it is |
|---|---|
| `index.html` | The page |
| `styles.css` | Styles, light and dark (follows the system, with a toggle) |
| `main.js` | Theme toggle, the drop demo, the screenshot tour and tabs |
| `screenshots/` | WebP copies of `docs/screenshots`, made by `scripts/screenshots.py` |
| `CNAME` | The domain, for GitHub Pages |

## Preview

```sh
python3 -m http.server --directory site 8000
# open http://localhost:8000
```

## Refresh the screenshots

The README and the site share one set of screenshots. Update `docs/screenshots`
first, then:

```sh
pip install pillow
python3 site/scripts/screenshots.py
```

## Placeholders

These are marked with `PLACEHOLDER` comments in `index.html`:

* **Trailer**: the 16:9 frame under the hero. Swap the placeholder block for the
  video embed once the trailer exists.
* **Downloads**: the three platform cards link to GitHub Releases. Point them at
  the installer assets once the alpha is published, and drop the
  "Coming with the alpha" labels.

## Deploying

Any static host works: serve the `site/` folder as the web root. For GitHub
Pages, publish `site/` (for example with `actions/upload-pages-artifact` and
`actions/deploy-pages`) and keep `CNAME` so the domain stays `dogubako.app`.
