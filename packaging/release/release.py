#!/usr/bin/env python3
"""Prepare and check a Dogubako release.

  python3 -I packaging/release/release.py current            # version on this checkout
  python3 -I packaging/release/release.py bump 0.1.0-alpha.3 # version fields, changelog, notes scaffold
  python3 -I packaging/release/release.py regen [--check]    # generated docs from a built CLI
  python3 -I packaging/release/release.py check [--version V]
  python3 -I packaging/release/release.py site [--version V]   # websites agree with the release
  python3 -I packaging/release/release.py notes-body V       # GitHub release body

`bump` rewrites every version field (both Cargo workspaces and lockfiles,
tauri.conf.json, the web UI's package files), turns `[Unreleased]` in
CHANGELOG.md into the new version's section and scaffolds
docs/releases/v<version>.md. `regen` rebuilds the docs that are generated from
the tool registry, the website's tool list, and every tool count written in
the README, website and docs pages. `check` is what the Release workflow runs before it builds
anything: it fails while any version field, changelog entry, release note,
generated doc or page that names the current release is behind. `site` is the
part of `check` about dogubako.app and docs.dogubako.app (tool counts, the
website's tool list, and a download for every file the Release workflow
builds), which CI also runs on every pull request.

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
# Also matches tags (v0.1.0) and file names (Dogubako_0.1.0_aarch64.dmg).
VERSION_IN_TEXT = r"(?<![0-9A-Za-z.-])v?{}(?![0-9A-Za-z-]|\.\d)"
# A line that names an older release as history ("starting with 0.1.0-alpha.1")
# is not stale.
HISTORY = re.compile(r"\b(starting with|since|introduced in)\b", re.I)

CHANGELOG = ROOT / "CHANGELOG.md"
NOTES_DIR = ROOT / "docs" / "releases"
CAPABILITIES = ROOT / "docs" / "CAPABILITIES.md"
TOOLS_JSON = ROOT / "site" / "public" / "tools.json"
TOOL_PAGES = ROOT / "site" / "src" / "content" / "docs" / "tools"
WEBSITE_TOOLS = ["website/assets/js/tools-data.js", "website/tools/index.html"]

# Hand-written pages that state how many tools there are. `regen` rewrites the
# number from site/public/tools.json, so adding a tool can't leave them behind.
TOOL_COUNT_PAGES = [
    "README.md",
    "docs/PERFORMANCE.md",
    "website/index.html",
    "website/tools/index.html",
    "website/agents/index.html",
    "website/download/index.html",
    "website/404.html",
    "website/llms.txt",
    "website/agent-install.md",
    "site/src/content/docs/index.mdx",
    "site/src/content/docs/start/introduction.md",
    "site/src/content/docs/agents/overview.md",
    "site/src/content/docs/agents/discovery-and-results.md",
    "site/src/content/docs/project/performance.md",
]
# "249 tools", "249 free, open-source file tools", "<!-- tools:count -->249".
TOOL_COUNT = re.compile(
    r"(?<![\w.,])\d+(?=(?:&nbsp;|\s)(?:free, open-source (?:file )?|everyday |Dogubako )?tools\b)"
    r"|(?<=<!-- tools:count -->)\d+",
    re.I,
)
# "200 tools per page" is not the catalog's size.
PER_PAGE = re.compile(r"(?:&nbsp;|\s)\S*\s*tools per\b")
# Category rows in the introduction's table: "| [Speech & Voice](/tools/speech/) | 9 |".
CATEGORY_COUNT = re.compile(r"(\|\s*\[[^\]]+\]\(/tools/([a-z]+)/\)\s*\|\s*)\d+(?=\s*\|)")
# The README's table names categories without links: "| Speech | 9 |".
README_CATEGORIES = {
    "Images": "image", "Video": "video", "Audio": "audio", "Speech": "speech", "PDF & documents": "pdf",
    "Files": "files", "Text": "text", "Developer": "dev", "Data": "data", "Network": "network",
    "Design": "design", "System": "system",
}
README_CATEGORY_COUNT = re.compile(
    r"^(\|\s*(" + "|".join(re.escape(k) for k in README_CATEGORIES) + r")\s*\|\s*)\d+(?=\s*\|)", re.M
)
SITE_DATA = ROOT / "site" / "src" / "data"
WEBSITE = ROOT / "website"
TOOLS_DATA = WEBSITE / "assets" / "js" / "tools-data.js"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
DOWNLOAD_PAGE = "website/download/index.html"
ALPHA_PAGE = "site/src/content/docs/start/alpha.md"


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


# ------------------------------------------------------------------ artifacts

# What the Release workflow builds, per Rust target. A new target in
# release.yml fails `check` until it is described here, so the notes and the
# websites can offer it.
APP_BUNDLES = {
    "aarch64-apple-darwin": ("macOS, Apple silicon", "Dogubako_{v}_aarch64.dmg", "macOS 15 (Sequoia) or newer"),
}
CLI_TARGETS = {
    "x86_64-unknown-linux-gnu": ("Linux x86-64", "glibc 2.28 or newer (Debian 10+, Ubuntu 20.04+, RHEL 8+); uses engines installed on the system"),
    "aarch64-apple-darwin": ("macOS Apple silicon", "Uses engines installed on the system"),
    "x86_64-pc-windows-msvc": ("Windows x86-64", "Uses engines installed on the system"),
}


class Asset:
    """One thing a release offers. `download` ones are files people install."""

    def __init__(self, label: str, file: str, note: str = "", platform: str = "", download: bool = False):
        self.label, self.file, self.note, self.platform, self.download = label, file, note, platform, download


def workflow_jobs(text: str) -> dict[str, str]:
    body = text.split("\njobs:\n", 1)[-1]
    parts = re.split(r"^  ([A-Za-z][\w-]*):[ \t]*$", body, flags=re.M)
    return dict(zip(parts[1::2], parts[2::2]))


def matrix_include(job: str) -> list[dict[str, str]]:
    """A job's `strategy.matrix.include` rows, flow (`- { a: b }`) or block style."""
    inc = re.search(r"^([ \t]*)include:[ \t]*(?:#.*)?\n((?:\1[ \t]+\S.*\n?|[ \t]*\n)*)", job, re.M)
    if not inc:
        return []
    rows = re.split(r"^[ \t]*-[ \t]+", inc.group(2), flags=re.M)[1:]
    return [{k: v.strip().strip("'\"") for k, v in re.findall(r"(\w+):[ \t]*([^,}\n]+)", row.split("#")[0])}
            for row in rows]


def release_assets(v: str) -> tuple[list[Asset], list[str]]:
    """Every file the Release workflow puts in the draft, read from release.yml."""
    rel = RELEASE_WORKFLOW.relative_to(ROOT)
    assets: list[Asset] = []
    errors: list[str] = []
    jobs = workflow_jobs(RELEASE_WORKFLOW.read_text(encoding="utf-8"))
    for name, job in jobs.items():
        if "tauri-apps/tauri-action" in job:
            targets = re.findall(r"--target\s+([\w-]+)", job)
            if not targets:
                errors.append(f"{rel}: job {name} builds an app bundle without --target")
            for t in targets:
                if t not in APP_BUNDLES:
                    errors.append(f"{rel}: job {name} builds an app for {t}; describe it in APP_BUNDLES in release.py")
                    continue
                label, file, note = APP_BUNDLES[t]
                assets.append(Asset(label, file.format(v=v), note, label.replace(",", ""), download=True))
        if "-p dogubako-cli" in job and "gh release upload" in job:
            for fields in matrix_include(job):
                t = fields.get("target")
                if not t:
                    continue
                if t not in CLI_TARGETS:
                    errors.append(f"{rel}: job {name} builds the CLI for {t}; describe it in CLI_TARGETS in release.py")
                    continue
                platform, note = CLI_TARGETS[t]
                assets.append(Asset(f"CLI, {platform}", f"dogubako-{t}{fields.get('ext', '')}", note, platform, download=True))
        if "docker/build-push-action" in job:
            image = re.search(r"IMAGE:\s*(\S+?):\$\{\{", job)
            platforms = re.search(r"platforms:\s*(\S+)", job)
            if not image or not platforms:
                errors.append(f"{rel}: job {name} builds an image, but its IMAGE or platforms are not where release.py looks")
                continue
            assets.append(Asset("Homelab / server", f"{image.group(1)}:{v}",
                                f"Docker or Podman, {platforms.group(1)}; every engine included"))
    text = RELEASE_WORKFLOW.read_text(encoding="utf-8")
    if "engine_sources.py" in text:
        assets.append(Asset("Engine sources", f"engine-sources-{v}.tar", "Corresponding source for the GPL/LGPL/AGPL/MPL engines"))
    if "SHA256SUMS.txt" in text:
        assets.append(Asset("Checksums", "SHA256SUMS.txt"))
    order = {"Homelab / server": 1, "Checksums": 3, "Engine sources": 4}
    assets.sort(key=lambda a: order.get(a.label, 2 if a.label.startswith("CLI") else 0))
    if not any(a.download for a in assets):
        errors.append(f"{rel}: found no app or CLI downloads; release.py can't read the workflow any more")
    return assets, errors


def downloads_table(v: str) -> str:
    assets, _ = release_assets(v)
    rows = ["| | File | Notes |", "|---|---|---|"]
    rows += [f"| {a.label} | `{a.file}` | {a.note} |".replace("|  |", "| |") for a in assets]
    return "\n".join(rows)


# ---------------------------------------------------------------- tool counts


def release_tools() -> list[dict]:
    return json.loads(read(TOOLS_JSON))["tools"]


def count_pages() -> list[Path]:
    """Hand-written pages that may say how many tools there are."""
    pages = [ROOT / "README.md", *sorted((ROOT / "docs").glob("*.md"))]
    pages += [p for p in sorted(WEBSITE.rglob("*"))
              if p.suffix in (".html", ".md", ".txt", ".js") and "scripts" not in p.relative_to(WEBSITE).parts]
    pages += [p for p in sorted((ROOT / "site" / "src").rglob("*"))
              if p.suffix in (".md", ".mdx", ".astro") and not p.is_relative_to(TOOL_PAGES) and not p.is_relative_to(SITE_DATA)]
    return [p for p in pages if p.is_file()]


def site_errors(v: str) -> list[str]:
    """Where dogubako.app or docs.dogubako.app disagree with release v."""
    errors: list[str] = []
    tools = release_tools()
    n = len(tools)
    for page in count_pages():
        rel = page.relative_to(ROOT)
        for i, line in enumerate(page.read_text(encoding="utf-8").splitlines(), 1):
            for m in TOOL_COUNT.finditer(line):
                if int(m.group(0)) != n and not PER_PAGE.match(line, m.end()):
                    fix = "run `release.py regen`" if str(rel) in TOOL_COUNT_PAGES else "add the page to TOOL_COUNT_PAGES in release.py and run `release.py regen`"
                    errors.append(f"{rel}:{i}: says {m.group(0)} tools, the release has {n} ({fix})")

    m = re.search(r"^window\.DGB_TOOLS=(.*);$", TOOLS_DATA.read_text(encoding="utf-8"), re.M)
    listed = {t[2]: t[1] for t in json.loads(m.group(1))} if m else {}
    expected = {t["id"]: t["name"] for t in tools}
    if listed != expected:
        rel = TOOLS_DATA.relative_to(ROOT)
        missing = sorted(set(expected) - set(listed))
        extra = sorted(set(listed) - set(expected))
        renamed = sorted(k for k in set(expected) & set(listed) if expected[k] != listed[k])
        detail = "; ".join(f"{what}: {', '.join(ids[:5])}{' …' if len(ids) > 5 else ''}"
                           for what, ids in (("missing", missing), ("not in the release", extra), ("renamed", renamed)) if ids)
        errors.append(f"{rel}: the website's tool list is not the release's ({detail}) (run `release.py regen`)")

    assets, asset_errors = release_assets(v)
    errors += asset_errors
    files = {a.file for a in assets}

    notes = notes_path(v)
    if notes.exists():
        section = re.search(r"^## Downloads\n(.*?)(?=^## |\Z)", notes_body(v), re.M | re.S)
        listed_files = set(re.findall(r"^\|[^|\n]*\|\s*`([^`]+)`", section.group(1), re.M)) if section else set()
        rel_notes = notes.relative_to(ROOT)
        for a in assets:
            if a.file not in listed_files:
                errors.append(f"{rel_notes}: Downloads doesn't list {a.label} (`{a.file}`), which Release builds")
        for f in sorted(listed_files - files):
            errors.append(f"{rel_notes}: Downloads lists `{f}`, which Release doesn't build")

    page = ROOT / DOWNLOAD_PAGE
    if page.exists():
        text = re.sub(r"<!--.*?-->", "", page.read_text(encoding="utf-8"), flags=re.S)
        for a in assets:
            if (a.download or a.file == "SHA256SUMS.txt") and f"{REPO}/releases/download/v{v}/{a.file}" not in text:
                errors.append(f"{DOWNLOAD_PAGE}: no download link for {a.label} ({a.file})")
        for tag, f in re.findall(re.escape(REPO) + r"/releases/download/([^/\"']+)/([^\"'#?<\s]+)", text):
            if tag != f"v{v}" or f not in files:
                errors.append(f"{DOWNLOAD_PAGE}: links {tag}/{f}, which release v{v} doesn't have")

    page = ROOT / ALPHA_PAGE
    if page.exists():
        text = page.read_text(encoding="utf-8")
        for a in assets:
            if a.file not in text and not (a.label.startswith("CLI") and a.platform in text):
                errors.append(f"{ALPHA_PAGE}: doesn't mention {a.label} ({a.file}), which Release builds")
    return errors


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

{downloads_table(version)}

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


def sync_tool_counts() -> None:
    tools = json.loads(read(TOOLS_JSON))["tools"]
    per_category: dict[str, int] = {}
    for t in tools:
        per_category[t["category"]] = per_category.get(t["category"], 0) + 1
    for rel in TOOL_COUNT_PAGES:
        text = read(rel)
        new = TOOL_COUNT.sub(str(len(tools)), text)
        new = CATEGORY_COUNT.sub(lambda m: m.group(1) + str(per_category[m.group(2)]), new)
        if rel == "README.md":
            new, rows = README_CATEGORY_COUNT.subn(
                lambda m: m.group(1) + str(per_category[README_CATEGORIES[m.group(2)]]), new)
            if rows != len(README_CATEGORIES):
                raise SystemExit(f"README.md: found {rows} of {len(README_CATEGORIES)} category rows in the tools table")
        if not TOOL_COUNT.search(text):
            raise SystemExit(f"{rel}: no tool count found; drop it from TOOL_COUNT_PAGES")
        if new != text:
            write(rel, new)


def regen(check: bool) -> int:
    binary = find_bin()
    env = {**os.environ, "DOGUBAKO_BIN": binary, "DOGUBAKO_LOG": "error"}
    # Some defaults follow the CPU count (Whisper threads): pin to 4 CPUs, as
    # on the CI runner, so the pages don't change with the machine.
    pin = ["taskset", "-c", "0-3"] if shutil.which("taskset") and (os.cpu_count() or 0) >= 4 else []
    if not pin and os.cpu_count() != 4:
        print(f"warning: can't pin to 4 CPUs here ({os.cpu_count()} CPUs): CPU-dependent defaults will differ "
              "from CI. Regenerate on Linux, or take the diff from the Release workflow's generated-docs job.",
              file=sys.stderr)
    matrix = subprocess.run([*pin, binary, "tools", "matrix"], check=True, capture_output=True, text=True, env=env).stdout
    CAPABILITIES.write_text(matrix if matrix.endswith("\n") else matrix + "\n", encoding="utf-8")
    subprocess.run([*pin, "npm", "run", "--silent", "gen:tools"], cwd=ROOT / "site", check=True, env=env)
    subprocess.run(["node", "website/scripts/tools-data.mjs", str(TOOLS_JSON)], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["node", "website/scripts/build-tools-page.mjs"], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    sync_tool_counts()
    if not check:
        return 0
    paths = ["docs/CAPABILITIES.md", "site/src/content/docs/tools", "site/src/data", "site/public/tools.json",
             *WEBSITE_TOOLS, *TOOL_COUNT_PAGES]
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

    errors += site_errors(v)

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
    print(f"release {v}: version fields, changelog, notes, generated docs, current-release pages and websites agree")
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
    s = sub.add_parser("site", help="fail unless the websites agree with the release (tool counts, downloads)")
    s.add_argument("--version", help="the release being made (default: the checkout's version)")
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
    if a.cmd == "site":
        v = a.version.removeprefix("v") if a.version else current()
        errors = site_errors(v)
        if errors:
            print(f"The websites don't match release {v}:", file=sys.stderr)
            for e in errors:
                print(f"  - {e}", file=sys.stderr)
            return 1
        print(f"release {v}: tool counts, the website's tool list and every download agree")
        return 0
    if a.cmd == "notes-body":
        body = notes_body(a.version.removeprefix("v"))
        if not body:
            raise SystemExit(f"{notes_path(a.version.removeprefix('v')).relative_to(ROOT)}: no body below a `---` line")
        sys.stdout.write(body)
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
