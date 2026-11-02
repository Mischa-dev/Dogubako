#!/usr/bin/env python3
"""Download the pinned engine sources from engines.lock.json, verify every
SHA-256, and unpack them into a staging tree. Runs on any OS.

    python3 -I packaging/engines/fetch.py                 # all managed engines
    python3 -I packaging/engines/fetch.py ffmpeg qpdf     # just these
    python3 -I packaging/engines/fetch.py --pin-upstream  # record hashes of upstream archives

Downloads are cached in build/engines/cache (keyed by SHA-256) and unpacked
into build/engines/stage/<formula>/. A checksum mismatch aborts.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
LOCK = HERE / "engines.lock.json"
BUILD = ROOT / "build" / "engines"
CACHE = BUILD / "cache"
STAGE = BUILD / "stage"
UA = {"User-Agent": "dogubako-engines-fetch"}

_tokens: dict[str, str] = {}


def ghcr_headers(url: str) -> dict[str, str]:
    if "ghcr.io/v2/" not in url:
        return {}
    repo = url.split("/v2/", 1)[1].split("/blobs/", 1)[0]
    if repo not in _tokens:
        req = urllib.request.Request(f"https://ghcr.io/token?scope=repository:{repo}:pull", headers=UA)
        with urllib.request.urlopen(req, timeout=60) as r:
            _tokens[repo] = json.load(r)["token"]
    return {"Authorization": f"Bearer {_tokens[repo]}"}


def download(url: str, sha256: str | None, name: str) -> Path:
    """Download into the cache; verify `sha256` when given. Returns the path."""
    CACHE.mkdir(parents=True, exist_ok=True)
    if sha256:
        dest = CACHE / sha256
        if dest.exists() and hashlib.sha256(dest.read_bytes()).hexdigest() == sha256:
            return dest
    part = CACHE / f"{name}.part"
    req = urllib.request.Request(url, headers={**UA, **ghcr_headers(url)})
    h = hashlib.sha256()
    with urllib.request.urlopen(req, timeout=300) as r, open(part, "wb") as out:
        while chunk := r.read(1 << 20):
            h.update(chunk)
            out.write(chunk)
    got = h.hexdigest()
    if sha256 and got != sha256:
        part.unlink()
        raise SystemExit(f"checksum mismatch for {name}: expected {sha256}, got {got}")
    dest = CACHE / got
    part.replace(dest)
    return dest


def safe_extract(archive: Path, dest: Path) -> None:
    """Unpack, refusing absolute paths, `..` and links that escape `dest`."""
    dest.mkdir(parents=True, exist_ok=True)
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as z:
            for m in z.infolist():
                p = (dest / m.filename).resolve()
                if not p.is_relative_to(dest.resolve()):
                    raise SystemExit(f"{archive.name}: unsafe path {m.filename}")
            z.extractall(dest)
            # zipfile drops the executable bit
            for m in z.infolist():
                mode = (m.external_attr >> 16) & 0o777
                if mode:
                    (dest / m.filename).chmod(mode)
        return
    with tarfile.open(archive) as t:
        t.extractall(dest, filter="data")


def stage_bottle(name: str, b: dict) -> Path:
    out = STAGE / name
    marker = out / ".sha256"
    if marker.exists() and marker.read_text().strip() == b["sha256"]:
        return out
    archive = download(b["url"], b["sha256"], name)
    if out.exists():
        shutil.rmtree(out)
    tmp = STAGE / f".{name}.tmp"
    if tmp.exists():
        shutil.rmtree(tmp)
    safe_extract(archive, tmp)
    # Bottles unpack to <name>/<version>/...; flatten to stage/<name>/.
    (version_dir,) = [d for d in (tmp / name).iterdir() if d.is_dir()]
    version_dir.replace(out)
    shutil.rmtree(tmp)
    marker.write_text(b["sha256"] + "\n")
    return out


def stage_upstream(eid: str, e: dict) -> Path:
    if not e.get("sha256"):
        raise SystemExit(f"{eid}: upstream sha256 not pinned; run with --pin-upstream first")
    out = STAGE / eid
    marker = out / ".sha256"
    if marker.exists() and marker.read_text().strip() == e["sha256"]:
        return out
    archive = download(e["url"], e["sha256"], eid)
    if out.exists():
        shutil.rmtree(out)
    tmp = STAGE / f".{eid}.tmp"
    if tmp.exists():
        shutil.rmtree(tmp)
    safe_extract(archive, tmp)
    # Upstream archives usually wrap everything in one top-level folder.
    entries = list(tmp.iterdir())
    if len(entries) == 1 and entries[0].is_dir():
        entries[0].replace(out)
        tmp.rmdir()
    else:
        tmp.replace(out)
    marker.write_text(e["sha256"] + "\n")
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("engines", nargs="*", help="engine ids (default: all managed)")
    ap.add_argument("--pin-upstream", action="store_true", help="download upstream archives and record their SHA-256 in the lock")
    args = ap.parse_args()
    lock = json.loads(LOCK.read_text())
    wanted = args.engines or list(lock["engines"])
    unknown = [e for e in wanted if e not in lock["engines"]]
    if unknown:
        raise SystemExit(f"not in lock: {', '.join(unknown)}")

    if args.pin_upstream:
        for eid in wanted:
            e = lock["engines"][eid]
            if e["source"] != "upstream":
                continue
            path = download(e["url"], None, eid)
            e["sha256"] = path.name
            e["download_bytes"] = path.stat().st_size
            print(f"pinned {eid}: {e['sha256']} ({e['download_bytes']} bytes)", file=sys.stderr)
        LOCK.write_text(json.dumps(lock, indent=2) + "\n")
        return

    done: set[str] = set()
    for eid in wanted:
        e = lock["engines"][eid]
        if e["source"] == "homebrew":
            for name in e["formulae"]:
                if name not in done:
                    stage_bottle(name, lock["bottles"][name])
                    done.add(name)
        elif e["source"] == "upstream":
            stage_upstream(eid, e)
        else:
            print(f"{eid}: source '{e['source']}' is built by build-from-source.sh, skipping", file=sys.stderr)
        print(f"staged {eid}", file=sys.stderr)


if __name__ == "__main__":
    main()
