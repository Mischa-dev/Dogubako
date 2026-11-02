#!/usr/bin/env python3
"""Make the sample files the app screenshots use: two sunset photos and a CSV.

    python3 website/scripts/samples.py /tmp/dgb-samples

Needs Pillow. The output is deterministic, so screenshots stay comparable.
"""
import csv
import math
import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter


def scene(sky, hills, sun_y, w=1600, h=1200):
    im = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(im)
    for y in range(h):
        t = y / h
        a, b, u = (sky[0], sky[1], t / 0.55) if t < 0.55 else (sky[1], sky[2], min(1, (t - 0.55) / 0.25))
        d.line([(0, y), (w, y)], fill=tuple(int(a[i] + (b[i] - a[i]) * u) for i in range(3)))
    glow = Image.new("L", (w, h), 0)
    ImageDraw.Draw(glow).ellipse((w * 0.42 - 260, sun_y - 260, w * 0.42 + 260, sun_y + 260), fill=150)
    im = Image.composite(Image.new("RGB", (w, h), (255, 240, 220)), im, glow.filter(ImageFilter.GaussianBlur(120)))
    d = ImageDraw.Draw(im)
    d.ellipse((w * 0.42 - 55, sun_y - 55, w * 0.42 + 55, sun_y + 55), fill=(255, 246, 232))
    for k, (col, base, amp, f) in enumerate(hills):
        pts = [(0, h)] + [
            (x, base + amp * math.sin(x / w * f * math.pi + k * 1.7) + amp * 0.4 * math.sin(x / w * f * 3.1 + k))
            for x in range(0, w + 20, 20)
        ] + [(w, h)]
        d.polygon(pts, fill=col)
    return im.filter(ImageFilter.GaussianBlur(1.2))


out = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/dgb-samples")
out.mkdir(parents=True, exist_ok=True)
scene([(108, 80, 122), (186, 138, 156), (226, 180, 166)],
      [((196, 137, 138), 760, 40, 2), ((156, 104, 115), 860, 30, 3), ((94, 62, 79), 960, 26, 2.5), ((47, 32, 40), 1080, 14, 2)],
      720).save(out / "dune.jpg", quality=92)
scene([(58, 72, 120), (122, 128, 170), (196, 168, 180)],
      [((96, 90, 130), 720, 110, 3), ((70, 64, 96), 860, 40, 2.5), ((44, 40, 62), 980, 26, 2), ((22, 20, 32), 1100, 12, 2)],
      620).save(out / "dusk.png")
rng = random.Random(4)
with open(out / "orders.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["order_id", "date", "customer", "country", "items", "total_eur"])
    for i in range(400):
        w.writerow([10000 + i, f"2026-10-{1 + i % 30:02d}", rng.choice(["Aiko", "Ben", "Chloe", "Dmitri", "Eva", "Farah"]),
                    rng.choice(["JP", "DE", "FR", "US", "NL"]), rng.randint(1, 6), round(rng.uniform(9, 240), 2)])
print(f"samples in {out}")
