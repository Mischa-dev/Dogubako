#!/bin/sh
# Copy the trailer (built in trailer/, see trailer/README) next to the page.
# The video is ~24 MB, so it lives once in trailer/dist and is copied at deploy
# time instead of being committed twice. Run from the repository root.
set -eu
# A stale release-advertising MP4 must never be copied just because it exists.
python3 - <<'CHECK'
import hashlib, json
from pathlib import Path
root = Path('trailer')
manifest = root / 'dist/review-status.json'
if not manifest.exists():
    raise SystemExit('Trailer has no prerelease review manifest. Re-render it before copying.')
info = json.loads(manifest.read_text())
if info.get('status') != 'in-development':
    raise SystemExit('Trailer is not marked in-development. Release approval is required.')
for relative, expected in info['sha256'].items():
    if hashlib.sha256((root / relative).read_bytes()).hexdigest() != expected:
        raise SystemExit('Trailer review manifest is stale: ' + relative)
CHECK
mkdir -p website/trailer
cp trailer/dist/dogubako-alpha-trailer.mp4 trailer/dist/poster.jpg website/trailer/
echo "trailer copied to website/trailer/"
