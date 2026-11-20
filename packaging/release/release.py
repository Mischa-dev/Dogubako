#!/usr/bin/env python3
"""Prepare and check a Dogubako release.

  python3 -I packaging/release/release.py current            # version on this checkout
  python3 -I packaging/release/release.py bump 0.1.0-alpha.3 # version fields, changelog, notes scaffold
  python3 -I packaging/release/release.py regen [--check]    # generated docs from a built CLI
  python3 -I packaging/release/release.py check [--version V]
  python3 -I packaging/release/release.py notes-body V       # GitHub release body

`bump` rewrites every version field (both Cargo workspaces and lockfiles,
tauri.conf.json, the web UI's package files), turns `[Unreleased]` in
CHANGELOG.md into the new version's section and scaffolds
docs/releases/v<version>.md. `regen` rebuilds the docs that are generated from
the tool registry. `check` is what the Release workflow runs before it builds
anything: it fails while any version field, changelog entry, release note,
generated doc or page that names the current release is behind.

Nothing here tags, pushes or publishes.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REPO = "https://github.com/Mischa-dev/Dogubako"
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?$")
VERSION_IN_TEXT = r"(?<![\w.-]){}(?![\w-]|\.\d)"
# A line that names an older release as history ("starting with 0.1.0-alpha.1")
# is not stale.
HISTORY = re.compile(r"\b(starting with|since|introduced in)\b", re.I)

CHANGELOG = ROOT / "CHANGELOG.md"
NOTES_DIR = ROOT / "docs" / "releases"
CAPABILITIES = ROOT / "docs" / "CAPABILITIES.md"
TOOLS_JSON = ROOT / "site" / "public" / "tools.json"
TOOL_PAGES = ROOT / "site" / "src" / "content" / "docs" / "tools"

# Pages that describe the current release. Once a version is bumped, none of
# them may still name an older release: `check` lists every line that does.
CURRENT_RELEASE_PAGES = [
    "README.md",
    "website/index.html",
    "website/download/index.html",
    "website/llms.txt",
    "website/agent-install.md",
    "site/src/content/docs/start/install.md",
    "site/src/content/docs/start/alpha.md",
    "site/src/content/docs/self-hosting/docker.md",
]


def read(rel: str | Path) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def write(rel: str | Path, text: str) -> None:
    (ROOT / rel).write_text(text, encoding="utf-8")


# ------------------------------------------------------------------ versions


def toml_package_version(rel: str, table: str) -> str:
    m = re.search(r"^\[" + re.escape(table) + r"\]\n(?:[^\[].*\n)*?version = \"([^\"]+)\"", read(rel), re.M)
    if not m:
        raise SystemExit(f"{rel}: no version in [{table}]")
    return m.group(1)


def set_toml_package_version(rel: str, table: str, new: str) -> None:
    text = read(rel)
    pat = re.compile(r"(^\[" + re.escape(table) + r"\]\n(?:[^\[].*\n)*?version = \")[^\"]+(\")", re.M)
    text, n = pat.subn(r"\g<1>" + new + r"\g<2>", text, count=1)
    if n != 1:
        raise SystemExit(f"{rel}: no version in [{table}]")
    write(rel, text)


LOCK_ENTRY = re.compile(r'(\[\[package\]\]\nname = "(dogubako[\w-]*)"\nversion = ")([^"]+)(")')


def lock_versions(rel: str) -> dict[str, str]:
    return {m.group(2): m.group(3) for m in LOCK_ENTRY.finditer(read(rel))}


def set_lock_versions(rel: str, old: str, new: str) -> None:
    text = LOCK_ENTRY.sub(lambda m: m.group(1) + (new if m.group(3) == old else m.group(3)) + m.group(4), read(rel))
    write(rel, text)


def version_fields() -> dict[str, str]:
    """Every place the release version is recorded, as location -> value."""
    fields = {
        "Cargo.toml [workspace.package]": toml_package_version("Cargo.toml", "workspace.package"),
        "apps/desktop/src-tauri/Cargo.toml": toml_package_version("apps/desktop/src-tauri/Cargo.toml", "package"),
        "apps/desktop/src-tauri/tauri.conf.json": json.loads(read("apps/desktop/src-tauri/tauri.conf.json"))["version"],
        "web/package.json": json.loads(read("web/package.json"))["version"],
    }
    lock = json.loads(read("web/package-lock.json"))
    fields["web/package-lock.json"] = lock["version"]
    fields['web/package-lock.json packages[""]'] = lock["packages"][""]["version"]
    for rel in ("Cargo.lock", "apps/desktop/src-tauri/Cargo.lock"):
        for name, v in lock_versions(rel).items():
            fields[f"{rel} {name}"] = v
    return fields


def current() -> str:
    return toml_package_version("Cargo.toml", "workspace.package")


def set_json_version(rel: str, new: str, lockfile: bool = False) -> None:
    # Edit the text, not a re-dump, so formatting and key order stay as they are.
    text = read(rel)
    n = 2 if lockfile else 1
    text = re.sub(r'("version":\s*")[^"]+(")', r"\g<1>" + new + r"\g<2>", text, count=n)
    write(rel, text)


# ----------------------------------------------------------------- changelog


def changelog_section(version: str) -> str | None:
    text = read(CHANGELOG)
    m = re.search(r"^## \[" + re.escape(version) + r"\][^\n]*\n(.*?)(?=^## \[|^\[[^\]]+\]: |\Z)", text, re.M | re.S)
    return m.group(1).strip() if m else None


def changelog_versions() -> list[str]:
    return [v for v in re.findall(r"^## \[([^\]]+)\]", read(CHANGELOG), re.M) if v != "Unreleased"]


def roll_changelog(old: str, new: str) -> None:
    text = read(CHANGELOG)
    if changelog_section(new) is not None:
        return
    text = text.replace("## [Unreleased]\n", f"## [Unreleased]\n\n## [{new}] - unreleased\n", 1)
    text = re.sub(r"^\[Unreleased\]: .*$", f"[Unreleased]: {REPO}/compare/v{new}...HEAD", text, count=1, flags=re.M)
    link = f"[{new}]: {REPO}/releases/tag/v{new}"
    if link not in text:
        text = re.sub(r"^(\[Unreleased\]: .*)$", r"\1\n" + link.replace("\\", r"\\"), text, count=1, flags=re.M)
    write(CHANGELOG, text)


# --------------------------------------------------------------------- notes


def notes_path(version: str) -> Path:
    return NOTES_DIR / f"v{version}.md"


def notes_body(version: str) -> str:
    text = notes_path(version).read_text(encoding="utf-8")
    parts = re.split(r"^---\s*$", text, maxsplit=1, flags=re.M)
    return parts[1].strip() + "\n" if len(parts) == 2 else ""


def scaffold_notes(version: str) -> None:
    path = notes_path(version)
    if path.exists():
        return
    whats_new = changelog_section(version) or ""
    whats_new = re.sub(r"^The .*Full notes: .*\n?", "", whats_new, flags=re.M).strip()
    whats_new = re.sub(r"^### ", "### ", whats_new, flags=re.M)
    path.write_text(
        f"""# Dogubako {version}

<!-- This file is the body of the GitHub release: everything below the line is
     published as is. `release.py check` fails while any HTML comment is left
     below the line, so write the intro and Known issues, check What's new
     against the changelog, and delete every comment before releasing. -->

---

<!-- One or two sentences: what this release is for. -->

This is an alpha for testers. Things will break, and settings or file formats
may change before 1.0. Please [report problems]({REPO}/issues).

## Downloads

| | File | Notes |
|---|---|---|
| macOS, Apple silicon | `Dogubako_{version}_aarch64.dmg` | macOS 15 (Sequoia) or newer |
| Homelab / server | `ghcr.io/mischa-dev/dogubako:{version}` | Docker or Podman, linux/amd64; every engine included |
| CLI, Linux x86-64 | `dogubako-x86_64-unknown-linux-gnu` | glibc 2.28 or newer (Debian 10+, Ubuntu 20.04+, RHEL 8+); uses engines installed on the system |
| CLI, macOS Apple silicon | `dogubako-aarch64-apple-darwin` | Uses engines installed on the system |
| CLI, Windows x86-64 | `dogubako-x86_64-pc-windows-msvc.exe` | Uses engines installed on the system |
| Checksums | `SHA256SUMS.txt` | |
| Engine sources | `engine-sources-{version}.tar` | Corresponding source for the GPL/LGPL/AGPL/MPL engines |

## What's new

{whats_new}

## Known issues

<!-- Carry over what still applies from the previous release's notes. -->

---

Full changelog: [CHANGELOG.md]({REPO}/blob/v{version}/CHANGELOG.md) ·
Licenses: [THIRD_PARTY_NOTICES.md]({REPO}/blob/v{version}/THIRD_PARTY_NOTICES.md)
""",
        encoding="utf-8",
    )


# --------------------------------------------------------------------- regen


def find_bin() -> str:
    if os.environ.get("DOGUBAKO_BIN"):
        return os.environ["DOGUBAKO_BIN"]
    exe = "dogubako.exe" if os.name == "nt" else "dogubako"
    for profile in ("release", "debug"):
        p = ROOT / "target" / profile / exe
        if p.exists():
            return str(p)
    raise SystemExit("no dogubako binary: run `cargo build -p dogubako-cli` or set DOGUBAKO_BIN")


def regen(check: bool) -> int:
    binary = find_bin()
    env = {**os.environ, "DOGUBAKO_BIN": binary, "DOGUBAKO_LOG": "error"}
    # Some defaults follow the CPU count (Whisper threads): pin to 4 CPUs, as
    # on the CI runner, so the pages don't change with the machine.
    pin = ["taskset", "-c", "0-3"] if shutil.which("taskset") and (os.cpu_count() or 0) >= 4 else []
    matrix = subprocess.run([*pin, binary, "tools", "matrix"], check=True, capture_output=True, text=True, env=env).stdout
    CAPABILITIES.write_text(matrix if matrix.endswith("\n") else matrix + "\n", encoding="utf-8")
    subprocess.run([*pin, "npm", "run", "--silent", "gen:tools"], cwd=ROOT / "site", check=True, env=env)
    if not check:
        return 0
    paths = ["docs/CAPABILITIES.md", "site/src/content/docs/tools", "site/src/data", "site/public/tools.json"]
    diff = subprocess.run(["git", "status", "--porcelain", "--", *paths], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    if diff.strip():
        print("Generated docs are stale; run `release.py regen` and commit:\n" + diff, file=sys.stderr)
        subprocess.run(["git", "--no-pager", "diff", "--stat", "--", *paths], cwd=ROOT)
        return 1
    print("generated docs are up to date")
    return 0


# --------------------------------------------------------------------- check


def check(version: str | None) -> int:
    errors: list[str] = []
    v = version or current()
    if not SEMVER.match(v):
        errors.append(f"{v!r} is not a semantic version")

    for where, value in version_fields().items():
        if value != v:
            errors.append(f"version: {where} is {value}, expected {v}")

    section = changelog_section(v)
    if section is None:
        errors.append(f"CHANGELOG.md: no `## [{v}]` section")
    elif not re.search(r"^- ", section, re.M):
        errors.append(f"CHANGELOG.md: `## [{v}]` has no entries")
    if "## [Unreleased]" not in read(CHANGELOG):
        errors.append("CHANGELOG.md: no `## [Unreleased]` section")

    notes = notes_path(v)
    rel_notes = notes.relative_to(ROOT)
    if not notes.exists():
        errors.append(f"{rel_notes}: missing (`release.py bump {v}` scaffolds it)")
    else:
        body = notes_body(v)
        if not body.strip():
            errors.append(f"{rel_notes}: nothing below the `---` line")
        if "<!--" in body:
            errors.append(f"{rel_notes}: HTML comments left below the `---` line (editorial notes or unused variants)")
        if v not in body:
            errors.append(f"{rel_notes}: the body never names {v}")

    if f"(Dogubako {v})" not in read(CAPABILITIES):
        errors.append(f"docs/CAPABILITIES.md: not generated from {v} (run `release.py regen`)")
    if json.loads(read(TOOLS_JSON)).get("version") != v:
        errors.append(f"site/public/tools.json: not generated from {v} (run `release.py regen`)")
    for page in sorted(TOOL_PAGES.glob("*.md")):
        if f"from Dogubako {v}." not in page.read_text(encoding="utf-8"):
            errors.append(f"{page.relative_to(ROOT)}: not generated from {v} (run `release.py regen`)")

    older = [o for o in changelog_versions() if o != v]
    for rel in CURRENT_RELEASE_PAGES:
        if not (ROOT / rel).exists():
            continue
        for n, line in enumerate(read(rel).splitlines(), 1):
            if HISTORY.search(line):
                continue
            for o in older:
                if re.search(VERSION_IN_TEXT.format(re.escape(o)), line):
                    errors.append(f"{rel}:{n}: still names {o}: {line.strip()[:140]}")

    if errors:
        print(f"Release {v} is not ready:", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1
    print(f"release {v}: version fields, changelog, notes, generated docs and current-release pages agree")
    return 0


# ---------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("current", help="print the version on this checkout")
    b = sub.add_parser("bump", help="set a new version everywhere and scaffold its changelog entry and notes")
    b.add_argument("version")
    r = sub.add_parser("regen", help="regenerate docs from the tool registry (needs a built CLI)")
    r.add_argument("--check", action="store_true", help="fail if that changes anything")
    c = sub.add_parser("check", help="fail unless everything names the same release")
    c.add_argument("--version", help="the release being made (default: the checkout's version)")
    n = sub.add_parser("notes-body", help="print the GitHub release body")
    n.add_argument("version")
    a = ap.parse_args()

    if a.cmd == "current":
        print(current())
        return 0
    if a.cmd == "bump":
        new, old = a.version.removeprefix("v"), current()
        if not SEMVER.match(new):
            raise SystemExit(f"{new!r} is not a semantic version")
        set_toml_package_version("Cargo.toml", "workspace.package", new)
        set_toml_package_version("apps/desktop/src-tauri/Cargo.toml", "package", new)
        set_json_version("apps/desktop/src-tauri/tauri.conf.json", new)
        set_json_version("web/package.json", new)
        set_json_version("web/package-lock.json", new, lockfile=True)
        for lock in ("Cargo.lock", "apps/desktop/src-tauri/Cargo.lock"):
            set_lock_versions(lock, old, new)
        roll_changelog(old, new)
        scaffold_notes(new)
        print(f"{old} -> {new}. Next: `release.py regen`, write {notes_path(new).relative_to(ROOT)}, then `release.py check`.")
        return 0
    if a.cmd == "regen":
        return regen(a.check)
    if a.cmd == "check":
        return check(a.version.removeprefix("v") if a.version else None)
    if a.cmd == "notes-body":
        body = notes_body(a.version.removeprefix("v"))
        if not body:
            raise SystemExit(f"{notes_path(a.version.removeprefix('v')).relative_to(ROOT)}: no body below a `---` line")
        sys.stdout.write(body)
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
