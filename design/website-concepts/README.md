# Website concepts

Design explorations for dogubako.app, kept so parts of them can be reused.
They are not deployed: the live site is `website/`. Each folder is a
self-contained static page; open its `index.html` through a local server, e.g.

```sh
python3 -m http.server 8000 --directory design/website-concepts/bench-3d
```

Each has a `NOTES.md` (idea, how it's built, rough edges) and `review/` screenshots.

| Folder | Idea | Preview (private Artifact) |
|---|---|---|
| `live-product/` | The app rebuilt in code, playing a looped demo with a cursor; bento of live tool animations. **Chosen as the base for the real site (2026-11-02).** | https://claude.ai/artifact/SAmHzW9KWTyfs4SJHsUJQH |
| `bench-3d/` | Real-time three.js workbench: files tumble out of the toolbox, click to run, camera tours the 12 drawers. Its "One tool, four ways in" section was picked for the real site. | https://claude.ai/artifact/37QGxgJk8NpiXpQnpsBNGU |
| `open-the-box/` | Scroll-driven cinematic: the logo box opens in 3D, drawers rise, camera dives into the app. | https://claude.ai/artifact/QW1KK9HyqS1xNnfbAHAGBQ |
| `ink-and-paper/` | 道具箱 brushed stroke by stroke, a seal stamp, the product as an illustrated hand scroll. | https://claude.ai/artifact/4qhcvpM9rrfjta5igNGr3X |
| `classic/` | The first static draft (drop demo, screenshot tour, drawer cabinet). Screenshots and fonts copied in. | https://claude.ai/artifact/2tjHwxxRP3m42nKo7sHAWA |
