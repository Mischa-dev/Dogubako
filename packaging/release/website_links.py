#!/usr/bin/env python3
"""Point the built website at the live docs site and the published release.

While the docs site (PR #4) is undeployed and no release exists, the website
links to repository documents on a feature branch and to the bare releases
page. Run this on the folder being published, never on the sources, so the
in-repo pages keep their prerelease wording until a release is approved:

  python3 -I packaging/release/website_links.py _site --docs        # docs.dogubako.app is live
  python3 -I packaging/release/website_links.py _site --release v0.1.0-alpha.1
  python3 -I packaging/release/website_links.py _site --check       # report what still points at branches

--release points the macOS download at the DMG on that release. It leaves
the "prerelease" wording in place. Replacing that copy is an editorial step
on the chosen concept (see docs/releases/LAUNCH.md), and --check lists it.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = "https://github.com/Mischa-dev/Dogubako"
DOCS = "https://docs.dogubako.app"

# Repository document -> docs site page.
DOC_PAGES = {
    "README.md#documentation": "/",
    "README.md": "/",
    "docs/CLI.md": "/cli/overview/",
    "docs/MCP.md": "/agents/overview/",
    "docs/SELF_HOSTING.md": "/self-hosting/overview/",
    "docs/SECURITY.md": "/project/security/",
    "docs/DEPENDENCIES.md": "/project/licensing/",
    "docs/BUILD.md": "/develop/building/",
    "docs/API.md": "/reference/http-api/",
    "docs/CAPABILITIES.md": "/reference/capabilities/",
    "docs/ARCHITECTURE.md": "/develop/architecture/",
    "docs/TOOL_DEVELOPMENT.md": "/develop/writing-tools/",
    "docs/ENGINES.md": "/app/engines-and-models/",
    "docs/PERFORMANCE.md": "/project/performance/",
}
BLOB = re.compile(re.escape(REPO) + r"/blob/(?P<ref>(?:claude|review|website)/[^/\"'\s]+|[^/\"'\s]+)/(?P<path>[^\"'\s<)]+)")
CLONE_BRANCH = re.compile(r"git clone --branch \S+ (" + re.escape(REPO) + r")")
PRERELEASE_COPY = [
    "Not released yet",
    "no public alpha or installers yet",
    "Installers come with the first alpha",
    "arrive with the first alpha",
    "Watch for the alpha",
    "Follow the alpha",
    "main only contains a placeholder",
]


def rewrite(text: str, docs: bool, release: str | None) -> tuple[str, int]:
    n = 0

    def blob(m: re.Match) -> str:
        nonlocal n
        path = m.group("path")
        if docs and path in DOC_PAGES:
            n += 1
            return DOCS + DOC_PAGES[path]
        if release and m.group("ref") != release:
            # Anything else in the repo (LICENSE, notices): pin to the release tag.
            n += 1
            return f"{REPO}/blob/{release}/{path}"
        return m.group(0)

    text = BLOB.sub(blob, text)
    if release:
        text, k = CLONE_BRANCH.subn(r"git clone \1", text)
        n += k
        version = release.removeprefix("v")
        dmg = f"{REPO}/releases/download/{release}/Dogubako_{version}_aarch64.dmg"
        # Download cards for macOS: data-os="mac" (current site) or a card whose
        # label is macOS (concepts).
        text, k = re.subn(r'(<a\b[^>]*data-os="mac"[^>]*href=")[^"]*(")', r"\g<1>" + dmg + r"\2", text)
        n += k
        text, k = re.subn(
            r'(<a\b[^>]*href=")' + re.escape(REPO) + r'/releases("[^>]*>\s*<span class="dl-os">macOS<)',
            r"\g<1>" + dmg + r"\2",
            text,
        )
        n += k
        text, k = re.subn(re.escape(REPO) + r'/releases(?=["/]?")', f"{REPO}/releases/tag/{release}", text)
        n += k
    return text, n


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("site", type=Path, help="the folder being published (a copy, not website/)")
    ap.add_argument("--docs", action="store_true", help="docs.dogubako.app is live")
    ap.add_argument("--release", help="published release tag, e.g. v0.1.0-alpha.1")
    ap.add_argument("--check", action="store_true", help="only report")
    a = ap.parse_args()
    files = [p for p in a.site.rglob("*") if p.suffix in (".html", ".js") and p.is_file()]
    total = 0
    for p in files:
        src = p.read_text(encoding="utf-8")
        if not a.check:
            new, n = rewrite(src, a.docs, a.release)
            if n:
                p.write_text(new, encoding="utf-8")
                total += n
            src = new
        for m in BLOB.finditer(src):
            if m.group("ref") not in ("main", a.release or "main"):
                print(f"branch link  {p.relative_to(a.site)}: {m.group(0)}")
        for phrase in PRERELEASE_COPY:
            if phrase in src:
                print(f"prerelease   {p.relative_to(a.site)}: \"{phrase}\"")
    if not a.check:
        print(f"rewrote {total} links in {len(files)} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
