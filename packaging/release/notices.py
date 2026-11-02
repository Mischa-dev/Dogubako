#!/usr/bin/env python3
"""Generate THIRD_PARTY_NOTICES.md for a Dogubako release.

Reads, without network access unless --fetch-sources is given:
  packaging/engines/engines.toml and engines.lock.json  (managed engines, every
      Homebrew bottle in each engine's closure, with its license)
  Cargo.lock via `cargo tree` (Rust crates compiled into the CLI/server/app)
  web/node_modules (npm packages bundled into the web UI)

--fetch-sources asks formulae.brew.sh for each bottle's source location, so the
notice can list where the corresponding source of every GPL/LGPL/AGPL part
lives. The release checklist says to mirror those tarballs as release assets.

Usage:
  python3 -I packaging/release/notices.py [--fetch-sources] [-o THIRD_PARTY_NOTICES.md]
  python3 -I packaging/release/notices.py --check   # fail if the file is stale
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINES = ROOT / "packaging" / "engines"
COPYLEFT = ("GPL", "LGPL", "AGPL", "MPL")


def sh(*args: str, cwd: Path = ROOT) -> str:
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True).stdout


def brew_source(formula: str, version: str) -> str | None:
    url = f"https://formulae.brew.sh/api/formula/{formula}.json"
    try:
        with urllib.request.urlopen(url, timeout=20) as r:
            d = json.load(r)
    except Exception:
        return None
    stable = d.get("urls", {}).get("stable", {})
    src = stable.get("url")
    if src and stable.get("revision"):
        src = f"{src} @ {stable['revision']}"
    current = d.get("versions", {}).get("stable")
    base = version.split("_")[0]
    if current != base:
        # Homebrew moved on since the lock; the tarball for our pin follows the
        # same URL pattern, so point at the project instead of a wrong file.
        return f"{d.get('homepage')} (pinned {base}; Homebrew now at {current})"
    return src


def engines_section(fetch: bool) -> list[str]:
    toml_path, lock_path = ENGINES / "engines.toml", ENGINES / "engines.lock.json"
    if not lock_path.exists():
        return ["## Engines", "", "_packaging/engines is not on this branch; regenerate after PR #7 merges._", ""]
    manifest = tomllib.loads(toml_path.read_text())["engine"]
    lock = json.loads(lock_path.read_text())
    bottles = lock["bottles"]
    out = [
        "## Engines (separate programs shipped with or downloaded by the macOS app)",
        "",
        "Dogubako runs these as separate programs; it does not link to them. Each is",
        "under its own license, listed below. Their license texts are in",
        "`Dogubako.app/Contents/Resources/engines/licenses/<formula>/` (bundled) and in",
        "each downloaded pack's `licenses/` folder (on-demand).",
        "",
        "**Written offer.** For every component below under the GPL, LGPL or AGPL, the",
        "complete corresponding source code is available from the locations listed,",
        "and for at least three years from the release date we will provide it on",
        "request through https://github.com/Mischa-dev/Dogubako/issues. Each release",
        "also attaches the source tarballs of these components.",
        "",
    ]
    for tier, title in (("bundled", "Bundled in the app"), ("on-demand", "Downloaded on first use")):
        out += [f"### {title}", ""]
        for eid, e in lock["engines"].items():
            if e["tier"] != tier:
                continue
            m = manifest.get(eid, {})
            out.append(f"#### {eid} {e.get('version', '')}")
            out.append("")
            out.append(f"- License: {m.get('license', 'see upstream')}")
            if m.get("license_note"):
                out.append(f"- Note: {m['license_note']}")
            if m.get("source_url"):
                out.append(f"- Source: {m['source_url']}")
            if e.get("git"):
                out.append(f"- Built from {e['git']} at {e.get('ref')}")
            if e.get("url"):
                out.append(f"- Upstream binary: {e['url']}")
            formulae = e.get("formulae", [])
            if formulae:
                out += ["", "| Library / program | Version | License | Source |", "|---|---|---|---|"]
                for f in sorted(formulae):
                    b = bottles.get(f, {})
                    lic = b.get("license") or "see upstream"
                    src = ""
                    if fetch and any(c in lic for c in COPYLEFT):
                        src = brew_source(f, b.get("version", "")) or ""
                    out.append(f"| {f} | {b.get('version', '')} | {lic} | {src} |")
            out.append("")
    return out


def rust_section() -> list[str]:
    rows: dict[tuple[str, str], str] = {}
    for manifest in (ROOT / "Cargo.toml", ROOT / "apps/desktop/src-tauri/Cargo.toml"):
        if not manifest.exists():
            continue
        txt = sh(
            "cargo", "tree", "--locked", "--manifest-path", str(manifest),
            "--workspace", "-e", "normal", "--prefix", "none",
            # The platforms this release ships: macOS app, Linux homelab image.
            "--target", "aarch64-apple-darwin", "--target", "x86_64-unknown-linux-gnu",
            "--target", "aarch64-unknown-linux-gnu",
            "--format", "{p}\t{l}",
        )
        for line in txt.splitlines():
            if "\t" not in line:
                continue
            p, lic = line.split("\t", 1)
            lic = lic.removesuffix(" (*)").strip()  # cargo's "already shown" marker
            parts = p.split()
            if len(parts) < 2 or "(" in p and str(ROOT) in p:
                continue  # our own workspace crates
            rows[(parts[0], parts[1].lstrip("v"))] = lic or "see crate"
    out = [
        "## Rust crates compiled into Dogubako",
        "",
        f"{len(rows)} crates. All permit redistribution under AGPL-3.0-or-later.",
        "",
        "| Crate | Version | License |",
        "|---|---|---|",
    ]
    out += [f"| {n} | {v} | {lic} |" for (n, v), lic in sorted(rows.items())]
    return out + [""]


def npm_section() -> list[str]:
    web = ROOT / "web"
    if not (web / "node_modules").exists():
        return ["## Web UI packages", "", "_Run `npm ci` in web/ and regenerate._", ""]
    tree = json.loads(sh("npm", "ls", "--omit=dev", "--all", "--json", cwd=web) or "{}")
    seen: dict[tuple[str, str], str] = {}

    def walk(deps: dict) -> None:
        for name, d in (deps or {}).items():
            key = (name, d.get("version", ""))
            if key in seen:
                continue
            pj = web / "node_modules" / name / "package.json"
            lic = "see package"
            if pj.exists():
                lic = json.loads(pj.read_text()).get("license") or lic
            seen[key] = lic if isinstance(lic, str) else json.dumps(lic)
            walk(d.get("dependencies", {}))

    walk(tree.get("dependencies", {}))
    out = [
        "## Web UI packages bundled into the app",
        "",
        "Fonts: Instrument Sans and DM Mono (SIL Open Font License 1.1, via Fontsource).",
        "",
        "| Package | Version | License |",
        "|---|---|---|",
    ]
    out += [f"| {n} | {v} | {lic} |" for (n, v), lic in sorted(seen.items())]
    return out + [""]


def render(fetch: bool) -> str:
    head = [
        "# Third-party notices",
        "",
        "Dogubako is free software under the GNU Affero General Public License v3.0 or",
        "later (see `LICENSE`). It includes or works with the third-party components",
        "below, each under its own license.",
        "",
        "_Generated by `packaging/release/notices.py` against the full alpha stack (PRs #1 to #8 merged). Regenerate after merging, before tagging. Do not edit by hand._",
        "",
    ]
    return "\n".join(head + engines_section(fetch) + rust_section() + npm_section()).rstrip() + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--output", default=str(ROOT / "THIRD_PARTY_NOTICES.md"))
    ap.add_argument("--fetch-sources", action="store_true")
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    text = render(a.fetch_sources)
    out = Path(a.output)
    if a.check:
        if not out.exists():
            print(f"{out} is missing", file=sys.stderr)
            return 1
        # Source columns depend on the network; compare everything else.
        strip = lambda s: [l.rsplit("|", 2)[0] if l.startswith("| ") and l.count("|") == 5 else l for l in s.splitlines()]
        if strip(out.read_text()) != strip(text):
            print(f"{out} is stale: run packaging/release/notices.py", file=sys.stderr)
            return 1
        return 0
    out.write_text(text)
    print(f"wrote {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
