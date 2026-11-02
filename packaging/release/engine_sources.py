#!/usr/bin/env python3
"""Collect the corresponding source of every copyleft engine component.

The macOS app ships (or downloads) Homebrew-built engines. For the GPL, LGPL,
AGPL and MPL parts, a release offering the binaries for download must offer
the source from the same place. This script downloads the upstream source of
each such component at the pinned version and writes one tar to attach to the
GitHub release, with a SOURCES.md index inside.

  python3 -I packaging/release/engine_sources.py --version 0.1.0-alpha.1
  # -> build/release/engine-sources-0.1.0-alpha.1.tar

Homebrew's own build recipes (formula files) are public at
https://github.com/Homebrew/homebrew-core; the index links each one.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "packaging" / "engines" / "engines.lock.json"
COPYLEFT = ("GPL", "LGPL", "AGPL", "MPL")


def get(url: str, timeout: int = 60) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "dogubako-release"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def source_url(formula: str, pinned: str) -> tuple[str | None, str]:
    """Return (url, note) for the formula's source at the pinned version."""
    d = json.loads(get(f"https://formulae.brew.sh/api/formula/{formula}.json"))
    stable = d.get("urls", {}).get("stable", {})
    url, rev = stable.get("url"), stable.get("revision")
    current = d.get("versions", {}).get("stable", "")
    base = pinned.split("_")[0]
    if url and url.endswith(".git"):
        # Git sources (e.g. x264): GitLab and GitHub both serve archives by revision.
        repo = url.removesuffix(".git")
        name = repo.rsplit("/", 1)[-1]
        if current != base:
            return None, f"git {url}; pinned {base} but Homebrew is at {current}: fetch by hand"
        if "github.com" in repo:
            return f"{repo}/archive/{rev}.tar.gz", f"git {url} @ {rev}"
        return f"{repo}/-/archive/{rev}/{name}-{rev}.tar.gz", f"git {url} @ {rev}"
    if url and current != base:
        url = url.replace(current, base)
        return url, f"version adjusted from Homebrew's current {current}"
    return url, ""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True, help="Dogubako version, for the file name")
    ap.add_argument("--out", default=str(ROOT / "build" / "release"))
    a = ap.parse_args()

    lock = json.loads(LOCK.read_text())
    wanted: dict[str, dict] = {}
    for eid, e in lock["engines"].items():
        if e["tier"] not in ("bundled", "on-demand"):
            continue
        for f in e.get("formulae", []):
            b = lock["bottles"].get(f, {})
            if any(c in (b.get("license") or "") for c in COPYLEFT):
                wanted.setdefault(f, {**b, "engines": []})["engines"].append(eid)

    out = Path(a.out)
    stage = out / "engine-sources"
    stage.mkdir(parents=True, exist_ok=True)
    index = [
        f"# Engine sources for Dogubako {a.version}",
        "",
        "Upstream source of every GPL, LGPL, AGPL or MPL component in the macOS",
        "engines, at the version shipped. Build recipes:",
        "https://github.com/Homebrew/homebrew-core/tree/master/Formula",
        "",
        "| Component | Version | License | Used by | File | SHA-256 | Note |",
        "|---|---|---|---|---|---|---|",
    ]
    missing = []
    for f, b in sorted(wanted.items()):
        url, note = source_url(f, b["version"])
        fname, digest = "", ""
        if url:
            try:
                data = get(url, timeout=300)
                fname = f"{f}-{b['version']}-" + url.rsplit("/", 1)[-1]
                (stage / fname).write_bytes(data)
                digest = hashlib.sha256(data).hexdigest()
            except Exception as exc:  # recorded and reported, never silently dropped
                note = f"{note}; download failed ({exc}) from {url}".lstrip("; ")
        if not fname:
            missing.append(f)
        index.append(f"| {f} | {b['version']} | {b['license']} | {', '.join(b['engines'])} | {fname} | {digest} | {note} |")
        print(f"{'ok  ' if fname else 'MISS'} {f} {b['version']}")

    # whisper.cpp is MIT, Real-ESRGAN BSD: no source obligation, listed for completeness.
    for eid in ("whisper", "realesrgan"):
        e = lock["engines"].get(eid)
        if e:
            index.append(f"| {eid} | {e.get('version')} | permissive | {eid} | | | {e.get('git') or e.get('url') or ''} |")

    (stage / "SOURCES.md").write_text("\n".join(index) + "\n")
    tar = out / f"engine-sources-{a.version}.tar"
    with tarfile.open(tar, "w") as t:
        t.add(stage, arcname=f"engine-sources-{a.version}")
    print(f"wrote {tar}")
    if missing:
        print("missing sources (fetch by hand before publishing): " + ", ".join(missing), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
