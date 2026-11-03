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
import importlib.util
import json
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "packaging" / "engines" / "engines.lock.json"
COPYLEFT = ("GPL", "LGPL", "AGPL", "MPL")
INPUTS = ROOT / "packaging" / "engines" / "source-inputs.json"


def validate_source(src: dict) -> None:
    """Reject floating git refs and unverified downloads before doing any I/O."""
    if src.get("url") and not src.get("git"):
        if urlsplit(src["url"]).scheme != "https" or not re.fullmatch(r"[0-9a-f]{64}", src.get("sha256", "")):
            raise ValueError("source requires HTTPS and a full SHA-256")
    elif src.get("git") and not src.get("url"):
        if urlsplit(src["git"]).scheme != "https" or not re.fullmatch(r"[0-9a-f]{40}", src.get("revision", "")):
            raise ValueError("git source requires HTTPS and a full commit ID")
    else:
        raise ValueError("source must pin either a URL or a git commit")


def verify_recipe(name: str, bottle: dict, review: dict, data: bytes) -> None:
    if hashlib.sha256(data).hexdigest() != review["recipe_sha256"]:
        raise RuntimeError(f"{name}: bottle recipe does not match the reviewed inputs")
    src = bottle["source"]
    pin = src.get("sha256") or src["revision"]
    if pin.encode() not in data:
        raise RuntimeError(f"{name}: pinned upstream source is absent from the bottle recipe")


def fetch_recipe(name: str, bottle: dict, review: dict, stage: Path) -> None:
    """Keep the recipe from the exact content-addressed bottle, never master."""
    spec = importlib.util.spec_from_file_location("engine_fetch", ROOT / "packaging/engines/fetch.py")
    fetch = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fetch)
    archive = fetch.download(bottle["url"], bottle["sha256"], name)
    with tarfile.open(archive) as t:
        (member,) = [m for m in t.getmembers() if m.name.endswith(f"/.brew/{name}.rb")]
        data = t.extractfile(member).read()
    verify_recipe(name, bottle, review, data)
    recipes = stage / "recipes"
    recipes.mkdir(exist_ok=True)
    (recipes / f"{name}.rb").write_bytes(data)


def get(url: str, timeout: int = 60) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "dogubako-release"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def fetch_source(name: str, version: str, src: dict, stage: Path) -> tuple[str, str, str]:
    """Download the source pinned in the lock. Returns (file, sha256, note)."""
    validate_source(src)
    if src.get("url"):
        url = src["url"]
        data = get(url, timeout=300)
        digest = hashlib.sha256(data).hexdigest()
        if digest != src["sha256"]:
            raise RuntimeError(f"checksum mismatch for {url}: got {digest}, lock has {src['sha256']}")
        fname = f"{name}-{version}-" + Path(urlsplit(url).path).name
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
    ap.add_argument("--lock", type=Path, default=LOCK, help="exact binary lock to collect sources for")
    ap.add_argument("--inputs", type=Path, default=INPUTS, help="reviewed recipe and supplemental source pins")
    ap.add_argument("--check", action="store_true", help="validate pins and recipe reviews without downloading")
    a = ap.parse_args()

    lock = json.loads(a.lock.read_text())
    reviews = json.loads(a.inputs.read_text())
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

    for name, b in wanted.items():
        validate_source(b.get("source") or {})
        review = reviews.get(name, {})
        if (review.get("bottle_sha256") != b["sha256"]
                or review.get("source") != b["source"]
                or not re.fullmatch(r"[0-9a-f]{64}", review.get("recipe_sha256", ""))):
            raise ValueError(f"{name}: review source inputs from the exact pinned bottle first")
        for src in review["inputs"]:
            validate_source(src)
    if a.check:
        print(f"validated {len(wanted)} source pins and recipe reviews")
        return 0

    out = Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    # A fresh stage prevents old files surviving --only or a failed retry.
    work = tempfile.TemporaryDirectory(prefix="engine-sources-", dir=out)
    stage = Path(work.name)
    index = [
        f"# Engine sources for Dogubako {a.version}",
        "",
        "Upstream source of every GPL, LGPL, AGPL or MPL component in the macOS",
        "engines, at the version shipped. Exact bottle recipes are in recipes/.",
        "The binary lock and reviewed patch/resource pins are included alongside this index.",
        "",
        "| Component | Version | License | Used by | File | SHA-256 | Note |",
        "|---|---|---|---|---|---|---|",
    ]
    missing = []
    collected = []
    for f, b in sorted(wanted.items()):
        fname, digest, note = "", "", ""
        src = b.get("source")
        if not src:
            note = "no source pinned in engines.lock.json (re-run packaging/engines/lock.py)"
        else:
            try:
                fname, digest, note = fetch_source(f, b["version"], src, stage)
                fetch_recipe(f, b, reviews[f], stage)
                collected.extend([stage / fname, stage / "recipes" / f"{f}.rb"])
                for number, extra in enumerate(reviews[f]["inputs"]):
                    extra_file, extra_digest, _ = fetch_source(f"{f}-input-{number}", b["version"], extra, stage)
                    collected.append(stage / extra_file)
                    index.append(f"| {f} input {number} | {b['version']} | see recipe | {f} | {extra_file} | {extra_digest} | {extra['url']} |")
            except Exception as exc:  # recorded and reported, never silently dropped
                fname = ""
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
    (stage / "engines.lock.json").write_text(a.lock.read_text())
    (stage / "source-inputs.json").write_text(a.inputs.read_text())
    tar = out / f"engine-sources-{a.version}.tar"
    if missing:
        print("missing sources (fetch by hand before publishing): " + ", ".join(missing), file=sys.stderr)
        work.cleanup()
        return 1
    # Do not create or overwrite a release-shaped archive on failure.
    pending = stage / "complete.tar"
    with tarfile.open(pending, "w") as t:
        # Only index explicitly verified outputs. Filesystem helpers may create
        # transient files even in a fresh directory; never recurse over those.
        for p in sorted(collected + [stage / n for n in ("SOURCES.md", "engines.lock.json", "source-inputs.json")]):
            t.add(p, arcname=f"engine-sources-{a.version}/{p.relative_to(stage)}", recursive=False)
    pending.replace(tar)
    work.cleanup()
    print(f"wrote {tar}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
