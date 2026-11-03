#!/usr/bin/env python3
"""Paint the sample photos used in the trailer (and fed to the real app for
the screen captures): layered dusk landscapes, so nothing needs licensing.

    python3 -I trailer/tools/make_samples.py trailer/assets
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image

W, H = 1600, 1200


def ridge(x, seed, base, amp, freq):
    r = np.random.default_rng(seed)
    y = np.full_like(x, base, dtype=float)
    for k in range(1, 7):
        y += amp / k ** 1.25 * np.sin(x * freq * k * (1 + r.uniform(-.2, .2)) + r.uniform(0, 6.3))
    return y


def scene(sky_top, sky_bot, sun, layers, sun_pos=(0.55, 0.48)):
    yy, xx = np.mgrid[0:H, 0:W].astype(float)
    ty = yy / H
    img = np.empty((H, W, 3))
    for c in range(3):
        img[..., c] = sky_top[c] + (sky_bot[c] - sky_top[c]) * np.clip(ty / 0.62, 0, 1) ** 1.2
    sx, sy = sun_pos[0] * W, sun_pos[1] * H
    d = np.hypot(xx - sx, yy - sy)
    glow = np.exp(-(d / (W * 0.22)) ** 2) * 0.55 + np.exp(-(d / (W * 0.08)) ** 2) * 0.35
    img += glow[..., None] * np.array(sun) * 0.6
    disc = np.clip((W * 0.028 - d) / 2.0, 0, 1)
    img = img * (1 - disc[..., None]) + np.array([255, 246, 228]) * disc[..., None]
    x = np.arange(W) / W * 2 * np.pi
    for i, (col, base, amp, freq, haze) in enumerate(layers):
        top = ridge(x, 11 + i * 7, base * H, amp * H, freq)
        mask = np.clip((yy - top[None, :]) / 1.5, 0, 1)
        shade = col * (1 - haze * np.clip((yy - top[None, :]) / (H * 0.3), 0, 1))[..., None] + np.array(sky_bot) * haze * 0.0
        img = img * (1 - mask[..., None]) + shade * mask[..., None]
    grain = np.random.default_rng(1).normal(0, 1.6, img.shape)
    return Image.fromarray(np.clip(img + grain, 0, 255).astype(np.uint8))


def main(out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    dune = scene((112, 84, 120), (228, 168, 150), (255, 214, 190), [
        (np.array([214, 150, 140]), 0.58, 0.035, 1.3, 0.0),
        (np.array([176, 118, 120]), 0.68, 0.03, 1.8, 0.0),
        (np.array([122, 82, 96]), 0.78, 0.025, 2.4, 0.0),
        (np.array([52, 34, 44]), 0.9, 0.012, 3.0, 0.0),
    ])
    dune.save(out / 'dune.jpg', quality=92)
    dusk = scene((48, 66, 118), (172, 170, 196), (255, 236, 210), [
        (np.array([122, 116, 152]), 0.56, 0.05, 1.6, 0.0),
        (np.array([84, 80, 116]), 0.7, 0.03, 2.1, 0.0),
        (np.array([46, 44, 70]), 0.82, 0.02, 2.8, 0.0),
        (np.array([20, 20, 32]), 0.92, 0.01, 3.4, 0.0),
    ], sun_pos=(0.48, 0.42))
    dusk.save(out / 'dusk.png')
    meadow = scene((86, 128, 160), (236, 214, 176), (255, 226, 170), [
        (np.array([150, 160, 130]), 0.6, 0.04, 1.2, 0.0),
        (np.array([98, 124, 96]), 0.72, 0.03, 1.9, 0.0),
        (np.array([52, 78, 60]), 0.85, 0.02, 2.6, 0.0),
    ], sun_pos=(0.3, 0.5))
    meadow.save(out / 'meadow.jpg', quality=92)
    print('wrote', ', '.join(p.name for p in sorted(out.glob('*'))))


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'assets')
