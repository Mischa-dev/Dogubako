#!/usr/bin/env python3
"""Pin every managed engine (and its whole runtime dependency closure) to an
exact version, download URL, SHA-256 and size.

    python3 -I packaging/engines/lock.py            # writes engines.lock.json
    python3 -I packaging/engines/lock.py --check    # fail if the lock is stale vs engines.toml

Homebrew bottles are content-addressed on ghcr.io by their SHA-256, so a
locked bottle stays downloadable after Homebrew moves on. Runs on any OS;
only `assemble.py` needs a Mac.
"""

from __future__ import annotations

import argparse
import json
import sys
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "engines.toml"
LOCK = HERE / "engines.lock.json"
API = "https://formulae.brew.sh/api/formula/{}.json"
UA = {"User-Agent": "dogubako-engines-lock"}

_tokens: dict[str, str] = {}


def get_json(url: str, headers: dict[str, str] | None = None) -> dict:
    req = urllib.request.Request(url, headers={**UA, **(headers or {})})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def ghcr_repo(url: str) -> str:
    # https://ghcr.io/v2/homebrew/core/openssl/4/blobs/sha256:... -> homebrew/core/openssl/4
    return url.split("/v2/", 1)[1].split("/blobs/", 1)[0]


def ghcr_token(repo: str) -> str:
    if repo not in _tokens:
        _tokens[repo] = get_json(f"https://ghcr.io/token?scope=repository:{repo}:pull")["token"]
    return _tokens[repo]


def blob_size(url: str) -> int | None:
    req = urllib.request.Request(
        url, method="HEAD", headers={**UA, "Authorization": f"Bearer {ghcr_token(ghcr_repo(url))}"}
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            n = r.headers.get("Content-Length")
            return int(n) if n else None
    except urllib.error.HTTPError:
        return None


def pick_bottle(formula: dict, tags: list[str]) -> tuple[str, dict]:
    files = formula.get("bottle", {}).get("stable", {}).get("files", {})
    for t in tags:
        if t in files:
            return t, files[t]
    if "all" in files:  # architecture-independent bottles (e.g. pure scripts)
        return "all", files["all"]
    raise SystemExit(f"{formula['name']}: no bottle for {tags} (has {sorted(files)})")


def resolve_closure(root: str, tags: list[str], cache: dict[str, dict]) -> list[str]:
    """Runtime dependency closure in dependency-first order."""
    order: list[str] = []
    seen: set[str] = set()

    def visit(name: str) -> None:
        if name in seen:
            return
        seen.add(name)
        if name not in cache:
            cache[name] = get_json(API.format(name))
        for dep in cache[name].get("dependencies", []):
            visit(dep)
        order.append(name)

    visit(root)
    return order


def build_lock(manifest: dict) -> dict:
    tags = manifest["macos"]["bottle_tags"]
    cache: dict[str, dict] = {}
    bottles: dict[str, dict] = {}
    engines: dict[str, dict] = {}
    for eid, e in manifest["engine"].items():
        if e["tier"] not in ("bundled", "on-demand"):
            continue
        source = e.get("source", "homebrew")
        if source == "homebrew":
            closure = resolve_closure(e["formula"], tags, cache)
            for name in closure:
                if name in bottles:
                    continue
                f = cache[name]
                tag, b = pick_bottle(f, tags)
                version = f["versions"]["stable"] + (f"_{f['revision']}" if f.get("revision") else "")
                bottles[name] = {
                    "version": version,
                    "tag": tag,
                    "license": f.get("license"),
                    "url": b["url"],
                    "sha256": b["sha256"],
                    "size": blob_size(b["url"]),
                }
                print(f"  {name:<22} {version:<14} {tag:<14} {bottles[name]['size'] or '?':>10}", file=sys.stderr)
            engines[eid] = {
                "tier": e["tier"],
                "source": "homebrew",
                "version": bottles[e["formula"]]["version"],
                "formulae": closure,
                "download_bytes": sum(bottles[n]["size"] or 0 for n in closure),
            }
        elif source == "upstream":
            engines[eid] = {
                "tier": e["tier"],
                "source": "upstream",
                "version": e["version"],
                "url": e["url"],
                # Filled in by `fetch.py --pin-upstream` (downloads once, records the hash).
                "sha256": None,
                "download_bytes": None,
            }
        elif source == "build":
            engines[eid] = {
                "tier": e["tier"],
                "source": "build",
                "version": e["ref"],
                "git": e["git"],
                "ref": e["ref"],
            }
        else:
            raise SystemExit(f"{eid}: unknown source {source!r}")
    min_tag = max(
        (b["tag"] for b in bottles.values() if b["tag"] != "all"),
        key=lambda t: tags.index(t),
        default=tags[0],
    )
    return {
        "schema": 1,
        "target": "darwin-arm64",
        "newest_bottle_tag": min_tag,
        "engines": engines,
        "bottles": dict(sorted(bottles.items())),
    }


def merge_upstream_pins(new: dict, old: dict) -> None:
    """Keep hashes already recorded for unchanged upstream URLs."""
    for eid, e in new["engines"].items():
        o = old.get("engines", {}).get(eid)
        if e["source"] == "upstream" and o and o.get("url") == e["url"]:
            e["sha256"], e["download_bytes"] = o.get("sha256"), o.get("download_bytes")


def stale(manifest: dict, lock: dict) -> list[str]:
    problems = []
    for eid, e in manifest["engine"].items():
        managed = e["tier"] in ("bundled", "on-demand")
        locked = lock["engines"].get(eid)
        if managed and not locked:
            problems.append(f"{eid}: in engines.toml but not locked")
        elif locked and not managed:
            problems.append(f"{eid}: locked but tier is now {e['tier']}")
        elif locked and locked["tier"] != e["tier"]:
            problems.append(f"{eid}: tier changed to {e['tier']}")
        elif locked and e.get("source") == "upstream" and locked.get("url") != e["url"]:
            problems.append(f"{eid}: upstream url changed")
        elif locked and e.get("source") == "upstream" and not locked.get("sha256"):
            problems.append(f"{eid}: upstream sha256 not pinned (run fetch.py --pin-upstream)")
    return problems


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="only verify the lock matches engines.toml")
    args = ap.parse_args()
    manifest = tomllib.loads(MANIFEST.read_text())
    old = json.loads(LOCK.read_text()) if LOCK.exists() else {}
    if args.check:
        problems = stale(manifest, old) if old else ["no engines.lock.json"]
        for p in problems:
            print(f"stale: {p}", file=sys.stderr)
        sys.exit(1 if problems else 0)
    new = build_lock(manifest)
    merge_upstream_pins(new, old)
    LOCK.write_text(json.dumps(new, indent=2) + "\n")
    print(f"wrote {LOCK.relative_to(Path.cwd()) if LOCK.is_relative_to(Path.cwd()) else LOCK}", file=sys.stderr)


if __name__ == "__main__":
    main()
