"""Offline tests for `release.py site`. Run: python3 -I -m unittest discover -s packaging/release -p 'test_*.py'."""

import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


release = load("release", "packaging/release/release.py")

V = "1.2.3"
DL = f"https://github.com/Mischa-dev/Dogubako/releases/download/v{V}"
DOWNLOADS = [f"Dogubako_{V}_aarch64.dmg", "dogubako-x86_64-unknown-linux-gnu",
             "dogubako-aarch64-apple-darwin", "dogubako-x86_64-pc-windows-msvc.exe"]


class ToolCount(unittest.TestCase):
    def test_per_page_limits_are_not_counts(self):
        self.assertTrue(release.PER_PAGE.match("returns 200 tools per page", len("returns 200")))
        self.assertIsNone(release.PER_PAGE.match("Dogubako has 250 tools.", len("Dogubako has 250")))


class WorkflowAssets(unittest.TestCase):
    def test_reads_every_artifact_from_release_yml(self):
        assets, errors = release.release_assets(V)
        self.assertEqual(errors, [])
        files = [a.file for a in assets]
        for f in [*DOWNLOADS, f"ghcr.io/mischa-dev/dogubako:{V}", "SHA256SUMS.txt", f"engine-sources-{V}.tar"]:
            self.assertIn(f, files)
        self.assertEqual(sorted(a.file for a in assets if a.download), sorted(DOWNLOADS))

    def test_unknown_cli_target_is_an_error(self):
        text = release.RELEASE_WORKFLOW.read_text(encoding="utf-8").replace(
            "target: x86_64-pc-windows-msvc, ext: .exe", "target: aarch64-pc-windows-msvc, ext: .exe")
        with tempfile.TemporaryDirectory() as d:
            wf = Path(d) / "release.yml"
            wf.write_text(text, encoding="utf-8")
            with patch.object(release, "RELEASE_WORKFLOW", wf), patch.object(release, "ROOT", Path(d)):
                _, errors = release.release_assets(V)
        self.assertTrue(any("aarch64-pc-windows-msvc" in e for e in errors), errors)


    def test_block_style_matrix_rows(self):
        job = """    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
          - { os: windows-latest, target: x86_64-pc-windows-msvc, ext: .exe }  # flow row
    steps:
"""
        self.assertEqual(release.matrix_include(job), [
            {"os": "ubuntu-latest", "target": "x86_64-unknown-linux-gnu"},
            {"os": "windows-latest", "target": "x86_64-pc-windows-msvc", "ext": ".exe"},
        ])


class RollbackLinks(unittest.TestCase):
    def test_rollback_retargets_every_release_download(self):
        links = load("website_links", "packaging/release/website_links.py")
        new = "https://github.com/Mischa-dev/Dogubako/releases/download/v0.2.0"
        page = (f'<a class="plat" data-os="mac" href="{new}/Dogubako_0.2.0_aarch64.dmg">'
                f'<a href="{new}/dogubako-x86_64-unknown-linux-gnu"><a href="{new}/SHA256SUMS.txt">')
        out, _ = links.rewrite(page, docs=False, release="v0.1.0")
        self.assertNotIn("0.2.0", out)
        old = "https://github.com/Mischa-dev/Dogubako/releases/download/v0.1.0"
        for f in ("Dogubako_0.1.0_aarch64.dmg", "dogubako-x86_64-unknown-linux-gnu", "SHA256SUMS.txt"):
            self.assertIn(f"{old}/{f}", out)


class SiteErrors(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        root = self.tmp
        tools = [{"id": f"cat.t{i}", "name": f"Tool {i}", "category": "cat"} for i in range(120)]
        self.put("site/public/tools.json", json.dumps({"version": V, "tools": tools}))
        self.put("website/assets/js/tools-data.js",
                 'window.DGB_CATS=[];\nwindow.DGB_TOOLS=' + json.dumps([[0, t["name"], t["id"]] for t in tools], separators=(",", ":")) + ';\n')
        self.put("README.md", "Dogubako has 120 tools.\n")
        self.put("website/index.html", '<b><!-- tools:count -->120</b> tools in one app\n')
        self.put("site/src/content/docs/index.mdx", "All 120 Dogubako tools.\n")
        self.put("website/download/index.html", "\n".join(f'<a href="{DL}/{f}">' for f in [*DOWNLOADS, "SHA256SUMS.txt"]))
        self.put("site/src/content/docs/start/alpha.md",
                 f"Dogubako_{V}_aarch64.dmg; homelab / server; Linux x86-64, macOS Apple silicon, Windows x86-64; "
                 f"SHA256SUMS.txt; engine-sources-{V}.tar; ghcr.io/mischa-dev/dogubako:{V}\n")
        self.put(f"docs/releases/v{V}.md", f"---\ntitle: x\n---\n## Downloads\n\n{release.downloads_table(V)}\n\n## Notes\n")
        shutil.copy(release.RELEASE_WORKFLOW, root / "release.yml")
        for name, value in {"ROOT": root, "TOOLS_JSON": root / "site/public/tools.json",
                            "TOOL_PAGES": root / "site/src/content/docs/tools", "SITE_DATA": root / "site/src/data",
                            "WEBSITE": root / "website", "TOOLS_DATA": root / "website/assets/js/tools-data.js",
                            "RELEASE_WORKFLOW": root / "release.yml", "NOTES_DIR": root / "docs/releases"}.items():
            p = patch.object(release, name, value)
            p.start()
            self.addCleanup(p.stop)

    def put(self, rel, text):
        path = self.tmp / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def edit(self, rel, old, new):
        path = self.tmp / rel
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def test_in_sync_passes(self):
        self.assertEqual(release.site_errors(V), [])

    def test_stale_counts_on_both_sites(self):
        self.edit("website/index.html", "-->120", "-->119")
        self.edit("site/src/content/docs/index.mdx", "120", "119")
        errors = release.site_errors(V)
        self.assertTrue(any(e.startswith("website/index.html:1: says 119") for e in errors), errors)
        self.assertTrue(any(e.startswith("site/src/content/docs/index.mdx:1: says 119") for e in errors), errors)

    def test_website_tool_list_missing_a_tool(self):
        self.edit("website/assets/js/tools-data.js", ',[0,"Tool 119","cat.t119"]', "")
        errors = release.site_errors(V)
        self.assertTrue(any("missing: cat.t119" in e for e in errors), errors)

    def test_download_page_missing_a_platform(self):
        self.edit("website/download/index.html", f'<a href="{DL}/dogubako-x86_64-pc-windows-msvc.exe">', "")
        errors = release.site_errors(V)
        self.assertTrue(any("no download link for CLI, Windows x86-64" in e for e in errors), errors)

    def test_commented_out_or_missing_checksum_links_fail(self):
        self.edit("website/download/index.html", f'<a href="{DL}/dogubako-aarch64-apple-darwin">',
                  f'<!-- <a href="{DL}/dogubako-aarch64-apple-darwin"> -->')
        self.edit("website/download/index.html", f'<a href="{DL}/SHA256SUMS.txt">', "")
        errors = release.site_errors(V)
        self.assertTrue(any("no download link for CLI, macOS Apple silicon" in e for e in errors), errors)
        self.assertTrue(any("no download link for Checksums" in e for e in errors), errors)

    def test_docs_alpha_page_missing_the_app_despite_mac_cli(self):
        self.edit("site/src/content/docs/start/alpha.md", f"Dogubako_{V}_aarch64.dmg; ", "")
        errors = release.site_errors(V)
        self.assertTrue(any("alpha.md: doesn't mention macOS, Apple silicon" in e for e in errors), errors)

    def test_download_page_links_an_old_release(self):
        self.edit("website/download/index.html", f"v{V}/Dogubako_{V}", f"v1.2.2/Dogubako_1.2.2")
        errors = release.site_errors(V)
        self.assertTrue(any("links v1.2.2/Dogubako_1.2.2_aarch64.dmg" in e for e in errors), errors)

    def test_notes_missing_an_artifact(self):
        self.edit(f"docs/releases/v{V}.md", "`dogubako-aarch64-apple-darwin`", "`something-else`")
        errors = release.site_errors(V)
        self.assertTrue(any("doesn't list CLI, macOS Apple silicon" in e for e in errors), errors)
        self.assertTrue(any("lists `something-else`" in e for e in errors), errors)

    def test_docs_alpha_page_missing_a_variant(self):
        self.edit("site/src/content/docs/start/alpha.md", "Windows x86-64; ", "")
        errors = release.site_errors(V)
        self.assertTrue(any("alpha.md: doesn't mention CLI, Windows x86-64" in e for e in errors), errors)


if __name__ == "__main__":
    unittest.main()
