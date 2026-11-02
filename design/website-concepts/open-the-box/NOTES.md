# Open the box

**Idea.** The logo is the product: the Dogubako mark starts as a flat logo, turns into a real 3D box, and as you scroll the latch flips, the lid opens, and the twelve category drawers come out. The rest of the page is the same box at work, in scroll-scrubbed scenes. You follow one photo from drop to result, see the same job in all four ways in, watch the file fail to leave the machine, and at the end the box closes back into the logo.

## What moves, and how it's built

Everything is vanilla JS, CSS 3D transforms, the Web Animations API, and one small canvas. Nothing comes from a CDN.

- **Scroll engine** (`assets/js/main.js`, bottom). Each scene is a tall section with a `position: sticky` stage. The engine reads the section's scroll progress, smooths it with an exponential follow (frame-rate independent), and calls `scene.render(p)`, which is a pure function of progress. One rAF loop runs only while a scene is on screen or still settling. It stops when the tab is hidden and restarts on `visibilitychange`.
- **Hero box (CSS 3D)**. The box, lid, latch, inner well, floor shadow and 12 drawers are built in JS from DOM faces. The rounded profile is extruded as thin strips, and the caps carry the logo's paper inlays. Shading is baked per face through `color-mix()` from a light direction. The lid is hinged on the back seam and the latch on its top edge. A `project()` helper reproduces the CSS camera, so 2D overlays line up with 3D points. Those overlays are the light shaft, the glow pool, the vignette, the dust canvas, and the closing flood out of the Images drawer.
  - Scroll timeline: settle-in on load (flat logo → 3/4 view), idle bob, latch flip, lid opens with a jolt, room dims, light and dust spill out, drawers rise row by row into a tansu cabinet, fan into a curved labelled wall, then the Images drawer flies to camera and its front opens out into the page.
- **App scene**. The app window is built in HTML/CSS, not a screenshot (sidebar with the real 12 counts, home input, bench). `dune.jpg` is carried in and dropped. It lands on the bench as a print. The "What should happen to it?" panel types "smaller", and Compress Images lifts to #1. Run is pressed, a progress bar fills, and a before/after wipe runs while the result counts down from 4.2 MB to 1.1 MB (−74%).
- **Four ways in**. App form, CLI, HTTP and MCP cards slide in as stacked layers of one object, and each command types itself out. At the end the stack tilts and explodes into header plates over an ink "Description · image.compress" plate. Commands come from the docs.
- **Nothing leaves your machine**. A 2D SVG scene: the photo leaves the box, travels toward "the internet", hits the machine's wall, gets squashed and stopped ("stays here"), and slides back into the box. The boundary seals and the cloud reads "0 B uploaded".
- **All 249 tools**. Twelve drawer cards with every real tool name from `cats.json`. When a card comes into view, its names pour out of the drawer with WAAPI, staggered, and a live counter runs to 249. A filter field matches tool and category names.
- **Self-host / agents**. Pick image-bot, media-bot or owner and a small tansu shows which drawers that token can open. It cycles on its own until you click. Real `dogubako users/tokens create` and `claude mcp add` commands are shown.
- **Download**. Three plates ("Not released yet", all linking to GitHub releases), a trailer slot that takes `trailer/dogubako-alpha-trailer.mp4` via `data-src`, and a build-from-source block.
- **Close**. The cabinet sinks back into the box, the lid and latch close, and the camera returns front-on until the box is the logo again.
- **Reduced motion** (`prefers-reduced-motion: reduce`). No pinning and no scrubbing. Each scene is rendered once at its key moments into still, composed frames with the captions beside them: 3 hero frames, 4 app frames, 4 layer frames, 2 privacy frames. The tool list shows fully and the box ends closed.
- **Phones**. The same scenes run with narrow layouts: a 2×6 drawer wall, a 340px app window, and a vertical privacy scene. Captions sit above the stage instead of beside it, and scene lengths are shorter.
- **Theme**. Light by default, dark from `prefers-color-scheme`, and a toggle in the nav saves the choice. The box is ink lacquer in light mode and a paper shell in dark mode.

## Libraries and assets

- No JS or CSS libraries.
- Fonts: Instrument Sans (variable) and DM Mono, copied from `website/fonts/`, SIL Open Font License 1.1 (`assets/fonts/OFL-*.txt`).
- The dune photo is an inline SVG data URI drawn for this page. The favicon is the Dogubako mark.

## Known rough edges

- Tested only in headless Chromium (Playwright) at 1440×900 and 375×812, light, dark and reduced motion. Not tested in Safari, Firefox or on a real phone. Chrome's preserve-3d sorting of translucent faces can show a one-frame overlap while drawers fade during the fan and dive.
- The end of the hero dive is a 2D clip-path rectangle matched to the projected drawer front. Mid-transition it can read as a flat card, and in dark mode it passes through a grey midpoint.
- The page is long (about 25,000 px at desktop) because every scene is scrubbed. Scene lengths were tuned by eye.
- On phones the self-hosting and build code blocks scroll sideways (they are focusable).
- The app window, tool ranking and timings are a choreographed recreation, not the real app logic.
- The trailer is a placeholder until the file ships.
