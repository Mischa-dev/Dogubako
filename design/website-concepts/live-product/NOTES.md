# Live product

**The idea.** The hero is the app itself: a code-built recreation of the Dogubako desktop window (sidebar, big input, bench, recent runs, glass panels) that runs one job after another with a synthetic cursor, so the first thing a visitor sees is the product working. Below it, every section shows a real tool or a real way in actually moving, and none of it is a picture.

## What moves, and how it's built

- **Hero demo (33 s loop, six chapters).**
  1. A photo is dragged in, the drop overlay opens and the photo lands on the bench as a print.
  2. "What should happen to it?" ranks four tools, the cursor types "smaller", Compress Images moves to the top and gets picked.
  3. Run, progress, then the before/after with "4.2 MB → 1.1 MB, 74% smaller".
  4. Ctrl K opens the palette and "pdf" is typed into it.
  5. A terminal runs `dogubako pdf merge a.pdf b.pdf -o merged.pdf` and prints its output.
  6. An agent chat calls `dogubako_search_tools`, then `dogubako_run_tool` with `video.compress`, `mode: "size"` and `target_mb: 25`.

  It loops seamlessly. Under the window are a pause button and six chapter pips, each with a progress bar. Every pip is a keyboard-reachable button, and the caption under the pips is also screen-reader text. Below 640 px the same timeline is rebuilt for the app's phone layout, with a phone bezel, stacked screens, the terminal and agent as bottom sheets, and a touch cursor.
- **`assets/js/timeline.js`** is a small keyframe-track timeline I wrote for this page, about 230 lines.
  - Every track is a pure function of time. `render(t)` writes only `transform`, `opacity` and `visibility`, plus text, html and class "step" tracks.
  - That makes pause, seek, looping and the reduced-motion still all the same call: `render(t)`.
  - A `Player` drives it with `requestAnimationFrame`. It stops when the window is offscreen (IntersectionObserver) or the tab is hidden, and it clamps frame deltas.
  - Element positions are measured from layout, using an offset chain that ignores transforms, so the cursor path follows the real DOM at any scale.
  - The desktop window is laid out at 1200×760 and scaled with one `transform: scale()`.
- **Bento, seven live tiles.** Each tile is its own small timeline and runs only while it is in view:
  - `speech.transcribe`: the playhead moves over the waveform, words light up and SRT cues are written.
  - `dev.json`: minified JSON is pretty-printed line by line, then validated.
  - `pdf.merge`: two documents' pages fly into order, stack, and are renumbered 1–5.
  - `video.compress`: the size counts down from 182 MB to land under a 25 MB target marker.
  - `files.hash`: scrambled hex resolves into SHA-256 and BLAKE3 digests.
  - `image.redact`: the cursor drags a box over a shipping-label address, which turns to mosaic.
  - `data.clean`: whitespace is trimmed, N/A and NULL are blanked, dates go to ISO 8601 and empty rows are dropped.
- **"249 tools".** A wall of every tool name, all 249 from the real registry, sits behind a frosted palette.
  - The count runs up and the names ripple in once.
  - Until the visitor types, the palette searches by itself (compress, subtitles, pdf, hash…).
  - Matches light up on the wall, and the list shows the top six with their ids and the matching `dogubako <cat> <tool> --help` line.
  - Real ranking (name, then keywords, id and description), category chips, and arrow-key selection.
- **Ways in.** App, CLI, HTTP API and MCP tabs show the same `image.compress` job with quality 75 highlighted in each.
  - The tabs follow the ARIA tabs pattern with arrow, Home and End keys.
  - They auto-advance with a progress line until the visitor touches them.
- **Smaller pieces:**
  - The local-first "cabinet" of bundled engines slides in, drawer by drawer.
  - The self-host agent policy log ticks through allowed and denied calls.
  - The logo lid opens on the download card.
  - The nav turns to glass on scroll, and the window tilts slightly with scroll.
- **Theme and motion.**
  - The theme follows the system and has a toggle; the choice is saved in localStorage inside try/catch.
  - App tokens are mirrored from `web/src/styles/tokens.css`.
  - Vermilion `#e0533d` appears only in the logo latch.
  - With `prefers-reduced-motion`, the hero starts paused on the Result chapter. Pips still seek to composed stills, every tile renders its finished frame, and the finder shows a static "compress" search.

## Libraries and assets

- No runtime libraries. Everything is plain JS in `assets/js/`, with no network requests other than same-folder files.
- Icons are Lucide (ISC), inlined as an SVG sprite; the license is in `assets/LICENSE-lucide.txt`.
- Fonts are Instrument Sans and DM Mono (SIL OFL 1.1), self-hosted; the licenses are in `assets/fonts/`.
- `assets/js/tools-data.js` is generated from the Rust tool specs (names, ids, descriptions, keywords and categories for all 249 tools).
- The two phone screenshots in the self-host section are the repo's own app screenshots, converted to WebP.
- The whole folder is about 1.2 MB, including the review PNGs.

## Rough edges

- The hero window is a hand recreation. It matches the app's current tokens and layout but will drift as the app changes, and the photo is a drawn SVG, not a real JPEG.
- On phones the hero becomes the app's phone layout. The terminal chapter leaves a lot of empty sheet below its short output.
- The self-host phone screenshot says "239 tools on this server", because it is a real server screenshot, while the page says 249.
- The finder's self-typing stops for good once the visitor interacts with it. Nothing restarts it.
