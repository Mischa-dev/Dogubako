#!/usr/bin/env python3
"""Collect the corresponding source of every copyleft engine component.

The macOS app ships (or downloads) Homebrew-built engines. For the GPL, LGPL,
AGPL and MPL parts, a release offering the binaries for download must offer
the source from the same place. This script downloads the upstream source of
each such component exactly as pinned in engines.lock.json (tarball URL and
SHA-256, or git commit, recorded by lock.py) and writes one tar to attach to the
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
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "packaging" / "engines" / "engines.lock.json"
COPYLEFT = ("GPL", "LGPL", "AGPL", "MPL")


def get(url: str, timeout: int = 60) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "dogubako-release"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def fetch_source(name: str, version: str, src: dict, stage: Path) -> tuple[str, str, str]:
    """Download the source pinned in the lock. Returns (file, sha256, note)."""
    if src.get("url"):
        url = src["url"]
        data = get(url, timeout=300)
        digest = hashlib.sha256(data).hexdigest()
        if digest != src["sha256"]:
            raise RuntimeError(f"checksum mismatch for {url}: got {digest}, lock has {src['sha256']}")
        fname = f"{name}-{version}-" + url.rsplit("/", 1)[-1]
        (stage / fname).write_bytes(data)
        return fname, digest, ""
    # Git sources (e.g. x264): the commit hash is the pin. Fetch exactly that
    # commit and archive it, so the content is verified by git itself.
    repo, rev = src["git"], src["revision"]
    fname = f"{name}-{version}-{rev[:12]}.tar.gz"
    with tempfile.TemporaryDirectory() as tmp:
        git = ["git", "-C", tmp]
        subprocess.run([*git, "init", "-q"], check=True)
        if subprocess.run([*git, "fetch", "-q", "--depth", "1", repo, rev]).returncode != 0:
            # Servers that refuse fetching by commit: fetch everything, then pick it.
            subprocess.run([*git, "fetch", "-q", repo, "+refs/*:refs/remotes/src/*"], check=True)
        subprocess.run(
            [*git, "archive", "--format=tar.gz", f"--prefix={name}-{rev[:12]}/", "-o", str(stage / fname), rev],
            check=True,
        )
    digest = hashlib.sha256((stage / fname).read_bytes()).hexdigest()
    return fname, digest, f"git {repo} @ {rev}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True, help="Dogubako version, for the file name")
    ap.add_argument("--out", default=str(ROOT / "build" / "release"))
    ap.add_argument("--only", nargs="*", help="only these formulae (for testing)")
    a = ap.parse_args()

    lock = json.loads(LOCK.read_text())
    wanted: dict[str, dict] = {}
    for eid, e in lock["engines"].items():
        if e["tier"] not in ("bundled", "on-demand"):
            continue
        for f in e.get("formulae", []):
            b = lock["bottles"].get(f, {})
            if a.only and f not in a.only:
                continue
            if any(c in (b.get("license") or "") for c in COPYLEFT):
                wanted.setdefault(f, {**b, "engines": []})["engines"].append(eid)

    # Absolute: git archive runs in a temporary repository and would resolve a
    # relative output path there.
    out = Path(a.out).resolve()
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
        fname, digest, note = "", "", ""
        src = b.get("source")
        if not src:
            note = "no source pinned in engines.lock.json (re-run packaging/engines/lock.py)"
        else:
            try:
                fname, digest, note = fetch_source(f, b["version"], src, stage)
            except Exception as exc:  # recorded and reported, never silently dropped
                note = f"download failed ({exc})"
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
