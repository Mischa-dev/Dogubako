# Ink and paper

**The idea.** Dogubako's name is written by hand before the product is introduced: 道 具 箱 appear stroke by stroke in sumi ink on warm paper, a vermilion seal stamps the page and shrinks into the latch of the logo. The rest of the page keeps that material honest: the product is told as an illustrated hand scroll (emakimono) in ink line-art, the 249 tools are set as moving columns of type, and the four ways in are pages of one folded book.

## What moves, and how it is built

- **Hero calligraphy** (`assets/js/brush.js`). All 35 strokes of 道具箱 (12 + 8 + 15) are hand-placed `[x, y, width]` control points in Japanese stroke order, run through a centripetal Catmull-Rom spline and turned into variable-width outlines. Helpers give every horizontal, vertical and turn the same brush entry (a 45° press) and ending (a small pressed button, where ink pools a little). Sweeps taper to a point and carry dry-brush streaks (kasure), cut into each stroke's mask. Each stroke is revealed by its own SVG mask: a butt-capped centerline whose `stroke-dashoffset` is driven by one rAF scheduler with per-stroke easing (stops settle, sweeps fly off) and pauses scaled to how far the brush travels between strokes. One shared SVG filter (`#ink`: turbulence displacement, a faint blurred halo, slight tonal variation) makes the edges read as ink soaking into paper rather than a font outline. Each glyph is its own `<svg>`, so only the one being written repaints. A dashed practice-paper cell sits under each glyph.
- **Seal → latch** (`main.js`). The seal is a jittered vermilion square with 道具箱 carved out of it (the same brush outlines, filled with the paper colour) and a stipple filter for an uneven impression. It drops with WAAPI, lands with a 1.5 px page thud, a paper dent (radial shade) and a few vermilion specks, holds, loses its carving, then scales exactly onto the logo's latch (18×14 in the 100-unit mark) while the box outline draws out from the latch in both directions. The real latch swaps in, the wordmark letters rise, the headline sets in. Scrolling, a wheel, touch or a key during the intro plays the rest at 5× speed.
- **Paper** is generated once per theme on a canvas (per-pixel grain plus wrapped fibres) and used as a fixed background tile.
- **The scroll** (`assets/js/scenes.js`). On desktop the section pins and scrolls sideways as you scroll down. Five scenes, each with the real tool name, the app's "what should happen to it" ranking (1 to 4) and the CLI line: a photo print re-inked smaller with its byte count ticking down (and a dimension line saying the pixels stay the same); three PDF stacks sliding together and folding into one with a brushed binding; a film strip re-inked frame by frame by an encoder "wash" with a shrinking size bar; a waveform read left to right while bars drop onto a subtitle sheet as SRT text types in; a tangle of `{"b":1,"a":[1,2]}` characters flying into the exact `--sort-keys` output from the tool's own test. Everything is scrubbed by scroll position (it runs backwards too). Scenes are separated by stippled kasumi mist bands drawn in an SVG that moves at 0.86× for depth. Phones and tablets get the same scenes stacked vertically, drawn as each one passes the middle of the screen.
- **249 tools.** Twelve drawers, each a vertical (tategaki-style, `writing-mode: vertical-rl`) condensed category heading with an upright count (`text-combine-upright`) and an ink-edge filter, next to a window of the real tool names drifting up or down at different speeds. Hover or focus eases a drawer to 10% speed and dims the others. All 249 names are in the HTML; the looping copies are `aria-hidden`.
- **Four ways in.** App, CLI, HTTP API and MCP are four leaves of an orihon (concertina book): exact 3D concertina maths unfolds them from 84° to flat as the section scrolls in, with fold shading. Each leaf's numeral (一 二 三 四) is brushed with the same engine when the book opens. Below 1180 px the leaves stack and hinge open one at a time.
- Small things: the self-hosting key tag draws itself and swings once; agent calls tick in; the alpha status dots breathe; buttons copy CLI lines.

Animation uses transforms, opacity, SVG attributes and canvas only. Scroll work happens in one rAF-throttled handler with cached geometry; the drawer marquee only runs while visible and pauses with the tab. In headless Chromium (software rendering) the hero holds 60 fps.

**Themes.** Paper is the default; "Sumi" (dark) follows `prefers-color-scheme` and the toggle in the nav (remembered in localStorage). In sumi the brush turns to gofun white on ink. Vermilion appears only in the seal (and its specks) and the logo latch.

**Reduced motion.** Strokes appear fully written, no seal, the logo and copy are in place, the scroll is a vertical page with every scene in its finished state, drawers are still lists you can scroll, the book is flat.

## Libraries

None. Vanilla JS, CSS, SVG, Canvas and the Web Animations API. Fonts: Instrument Sans (variable, wght + wdth, latin and latin-ext subsets) and DM Mono (latin and latin-ext), both SIL OFL 1.1, from Fontsource; licence texts in `assets/fonts/`. The latin-ext subsets were added so "dōgu" renders in the brand fonts.

## Known rough edges

- The intro takes about 3.5 s to reach the seal and about 4.5 s to finish; scrolling fast-forwards it. Worth tuning by eye on real hardware.
- The kanji are hand-placed by me, not taken from a calligrapher; they read correctly in school stroke order but a calligrapher would want a few proportions adjusted (the 辶 zigzag especially).
- The ink filter is applied while the strokes animate; it is cheap at this size but is the first thing to drop if a low-end device struggles.
- File sizes in the scenes (4.2 MB to 1.1 MB, 418 MB to 64 MB) and the SRT lines are illustrative, not measurements. The tool rankings shown as "1 to 4" are plausible examples, not the app's actual ranking for those files.
- The desktop scroll section is long (about five screens of vertical scrolling).
- No-JS visitors get all the content but empty glyph cells in the hero.
