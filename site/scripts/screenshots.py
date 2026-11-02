#!/usr/bin/env python3
"""Copy the app screenshots from docs/screenshots into the website as WebP.

The README and the website share one set of screenshots. Refresh docs/screenshots
first (see docs/screenshots), then run this from the repository root:

    python3 site/scripts/screenshots.py

Needs Pillow (`pip install pillow`).
"""
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "docs" / "screenshots"
OUT = ROOT / "site" / "screenshots"
MAX_WIDTH = 1600

OUT.mkdir(parents=True, exist_ok=True)
for png in sorted(SRC.glob("*.png")):
    img = Image.open(png)
    if img.width > MAX_WIDTH:
        img = img.resize((MAX_WIDTH, round(img.height * MAX_WIDTH / img.width)), Image.LANCZOS)
    dest = OUT / (png.stem + ".webp")
    img.save(dest, "WEBP", quality=84, method=6)
    print(f"{png.name} -> {dest.relative_to(ROOT)} ({dest.stat().st_size // 1024} KB, {img.width}x{img.height})")
