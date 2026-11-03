# bench-3d: the bench, in 3D

## The idea

The page opens on a soft-lit workbench with the Dogubako toolbox open on it and a few everyday files lying around: a photo print, a stack of PDFs, a film strip, a waveform card, a spreadsheet and a JSON note. You can hover, run and drop files right there, then scroll into the box and past all twelve drawers to see every tool before the page turns back into plain, readable HTML.

## What moves, and how it's built

- **Hero (WebGL, three.js).** The ink-lacquer box (clearcoat physical material, vermilion latch, hinged lid, 12 drawers with their real labels and counts) is built from extruded rounded slabs, not a model file. Paper objects get canvas-painted textures and a grain bump map. Lighting is a key light with soft VSM shadows, a hemisphere fill and a PMREM environment baked from a small "studio" scene. Each object also gets a contact-shadow decal. The scene renders into a 4x MSAA half-float target. A single post pass then adds a cheap golden-angle focus blur around the hero, Neutral tone mapping, a vignette and dither.
- **Tumble-in.** A small hand-written settle simulation (gravity, bounce, a righting spring, a spring back to each rest spot) drops the files onto the bench while the lid swings open. Pointer parallax moves the camera slightly, and the lens is shifted so the box stays clear of the headline.
- **Hover and click.** A raycast lifts the object under the pointer, and a glass HTML label shows the three tools Dogubako would suggest (real tool names from the catalogue). A click plays that tool as an example: the photo shrinks with a "4.2 MB → 1.1 MB" tag, the three PDFs merge into one, the film strip gets shorter, the waveform turns into subtitle lines, the duplicate spreadsheet rows collapse and the JSON gets pretty-printed. The same flow works from the chip row under the hero for keyboard and touch. A one-off demo plays on the photo after the intro.
- **Your own files.** Drop a file anywhere, or use "Add your own file", and it lands on the bench as the right kind of object. Photos show the actual image through an object URL, videos get a frame and audio gets its waveform. The real work runs in the tab: images are re-encoded to WebP on a canvas, JSON is formatted, text gets a word count, CSV gets stats, and anything else gets a SHA-256 through `crypto.subtle`. Nothing is uploaded.
- **Scroll tour.** A Catmull-Rom camera path runs from the hero to the box front, past all 12 drawers and back out to the top. Each drawer slides out in 3D with its tools printed on cards inside, and a crisp HTML panel lists every tool from `cats.json` (249 total). The rail on the right jumps straight to any drawer.
- **Rest of the page.** Plain HTML with restrained reveal motion. It covers one tool in four ways (app form, CLI, HTTP API, MCP, all generated from one description), local-first with a "network receipt", self-hosting with scoped agent keys and a terminal that types once, the alpha download state and the footer.
- **Performance.** The loop renders on demand and sleeps after about 20 quiet frames. It pauses when the tab is hidden and stops when the canvas fades out after the tour. DPR is capped (1.75 on desktop, 1.6 on phones) and drops automatically if frames run long. Phones get a simpler layout with four objects and their own camera path.
- **Fallback.** Without WebGL, or with `prefers-reduced-motion`, the page shows pre-rendered stills of this same scene (`assets/still-*.jpg`, captured with Playwright via `?still`). The drawers appear as a static grid with every tool listed. All decorative motion is off.

Debug URLs: `?static` forces the still version. `?manual` steps the scene from `window.__bench.advance(sec)`. `?still[&t=2&sx=..&sy=..]` renders a single frame for capture.

## Libraries

- three.js r170 (`vendor/three/three.module.min.js`), MIT, license in `vendor/three/LICENSE`. Loaded through an import map, with no CDN.
- Instrument Sans and DM Mono, SIL OFL 1.1, licenses in `assets/fonts/`.
- Nothing else, no GSAP. There are no runtime requests outside the folder.

## Known rough edges

- Performance was only measured in headless SwiftShader, where it runs at a few fps. The 60 fps target on a real laptop GPU, and the adaptive-DPR thresholds, still need a check on real hardware.
- The sample results are illustrative and say so ("· example"). Only your own files actually run.
- The tour is long: about 8 viewports of scroll for 12 drawers. The rail helps, but some visitors will want it shorter.
- 道具箱 on the lid lining is painted on a canvas with the system CJK font, so its look varies by OS.
- The inside of each drawer's cavity is plain black. A dim interior or an AO texture would read better close up.
- When the hover label floats over the headline on narrow desktops (around 1024px), it covers some copy for as long as it is up.
- Own files are limited to two slots on desktop and three on phones. The oldest file is removed when you add more.
