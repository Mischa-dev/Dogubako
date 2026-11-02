# Dogubako alpha trailer

A 72-second trailer for the first alpha, in the app's own "paper with frosted
panels" look. The finished film is in [`dist/`](dist):

* `dist/dogubako-alpha-trailer.mp4`: 1920×1080, 60 fps, H.264 + AAC, faststart (plays in every browser)
* `dist/poster.jpg`: the end card, for `<video poster>`

## What's in it

| Time | Bars | Scene |
|---|---|---|
| 0:00 | 1–2 | The mark draws itself; the vermilion latch clicks shut. 道具箱 · toolbox |
| 0:05 | 3–4 | "Every little tool you reach for…" — 30 real tools fly into one box |
| 0:10 | 5–6 | The bench: a photo, a PDF, a clip, a recording, JSON and a CSV land on it |
| 0:15 | 7–11 | The real app: drop a photo, type "smaller", Compress opens filled in, run, before/after, −53% |
| 0:27 | 12–14 | JSON formatter, Redact Regions, the ⌘K palette, System overview |
| 0:35 | 15–16 | 249 tools across 12 categories |
| 0:40 | 17–19 | One tool spec → desktop, self-hosted web, CLI, HTTP API, MCP |
| 0:47 | 20–22 | Local-first: nothing leaves your computer; the engines it's built on |
| 0:55 | 23–24 | Light → dark switch, then the phone layout |
| 1:00 | 25–29 | End card: Dogubako, alpha, dogubako.app |

Every UI screen is a capture of the real app (the combined frontend + backend),
not a mock-up. The music is synthesised from scratch by `tools/music.py`
(96 BPM, so every scene change lands on a bar line), so there's nothing to license.

## How it's made

* `scenes/index.html` + `scenes/timeline.js`: the whole film as one page. Nothing
  animates by itself: `window.render(t)` places everything for time `t`, so each
  frame is computed exactly and the video can't stutter or drop frames.
* `tools/capture.mjs`: drives a running Dogubako in headless Chromium and saves
  2× screenshots plus the positions of the controls the cursor clicks.
* `tools/make_samples.py`: paints the sample landscape photos (`assets/`).
* `tools/music.py`: the soundtrack.
* `tools/render.mjs`: renders every frame in headless Chromium (a few browsers in
  parallel) and encodes them with FFmpeg under the soundtrack.

## Rebuild it

Needs Node 20+, Playwright's Chromium, Python 3 with numpy and Pillow, and FFmpeg
(or `pip install imageio-ffmpeg`, which the renderer finds by itself).

```sh
# 1. the app, built from this branch
cd web && npm ci && npm run build && cd ..
cargo build -p dogubako-cli
target/debug/dogubako server start --no-auth --data-dir /tmp/dgb-trailer --bind 127.0.0.1:8420 &

# 2. sample photos, app captures, music
python3 -I trailer/tools/make_samples.py trailer/assets
node trailer/tools/capture.mjs          # use a fresh --data-dir so the bench starts empty
python3 trailer/tools/music.py trailer/build/music.wav

# 3. render (about 20 minutes on 4 cores)
node trailer/tools/render.mjs --out trailer/dist/dogubako-alpha-trailer.mp4
```

While editing, open `scenes/index.html?t=31.2` to see one moment, or
`?play` (or `?play=40`) to watch it in real time with the music. Stills:
`node trailer/tools/render.mjs --stills 3,18.5,61` writes PNGs to `build/stills/`.
