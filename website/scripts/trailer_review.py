"""Check a reviewed trailer against the website's published release and asset bytes."""
import hashlib
import json
import re
import sys
from pathlib import Path

ASSETS = ('dist/dogubako-alpha-trailer.mp4', 'dist/poster.jpg')


def validate(repo):
    root = Path(repo)
    trailer = root / 'trailer'
    try:
        info = json.loads((trailer / 'dist/review-status.json').read_text())
        if not isinstance(info, dict) or info.get('status') != 'reviewed':
            raise ValueError('Trailer must be reviewed before copying.')
        page = (root / 'website/download/index.html').read_text()
        tags = set(re.findall(r'github\.com/Mischa-dev/Dogubako/releases/download/([^/\s\"]+)/', page))
        if len(tags) != 1 or not isinstance(info.get('release'), str) or info['release'] not in tags:
            raise ValueError("Trailer review must identify the website's published release.")
        hashes = info.get('sha256')
        if not isinstance(hashes, dict) or set(hashes) != set(ASSETS):
            raise ValueError('Trailer review must hash exactly the MP4 and poster.')
        for relative in ASSETS:
            expected = hashes[relative]
            if not isinstance(expected, str) or not re.fullmatch(r'[0-9a-f]{64}', expected):
                raise ValueError('Invalid SHA-256: ' + relative)
            if hashlib.sha256((trailer / relative).read_bytes()).hexdigest() != expected:
                raise ValueError('Trailer review manifest is stale: ' + relative)
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError('Trailer review or assets are missing/unreadable: ' + str(exc)) from exc


if __name__ == '__main__':
    try:
        validate(Path(__file__).resolve().parents[2])
    except ValueError as exc:
        sys.exit(str(exc))
