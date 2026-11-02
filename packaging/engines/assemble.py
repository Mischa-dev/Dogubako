#!/usr/bin/env python3
"""Turn the staged engine sources (fetch.py) into self-contained, relocatable
engine folders the app can run from anywhere.

    python3 -I packaging/engines/assemble.py               # bundled folder + on-demand packs
    python3 -I packaging/engines/assemble.py --only ffmpeg # one engine (debugging)

Outputs
  apps/desktop/src-tauri/engines/     bundled tier, shipped inside Dogubako.app
      bin/ lib/ share/ opt/            one shared lib/ for all bundled engines
      manifest.json                    what's bundled + the on-demand pack catalog
  build/engines/packs/<id>-<version>-darwin-arm64.tar.gz
                                       one self-contained pack per on-demand engine
  build/engines/packs/packs.json       pack name, size, sha256 (upload with the packs)

How relocation works: every Mach-O file gets its Homebrew install names
(@@HOMEBREW_PREFIX@@/opt/x/lib/liby.dylib) rewritten to @rpath/liby.dylib and
an @loader_path-relative rpath to the folder's lib/. Only dylibs that are
actually linked are copied. On macOS each changed file is then ad-hoc signed
(`codesign -s -`), which Apple silicon requires to run it at all; this is not
Developer ID signing. On Linux the same steps run with llvm-otool and
llvm-install-name-tool so CI can check the tree, but the result is unsigned
and must be signed on a Mac before it runs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
MANIFEST = HERE / "engines.toml"
LOCK = HERE / "engines.lock.json"
STAGE = ROOT / "build" / "engines" / "stage"
PACKS = ROOT / "build" / "engines" / "packs"
BUNDLED = ROOT / "apps" / "desktop" / "src-tauri" / "engines"
TARGET = "darwin-arm64"
# Release assets for on-demand packs. Overridable for forks and test servers.
PACK_BASE_URL = os.environ.get(
    "DOGUBAKO_ENGINE_PACK_BASE_URL",
    "https://github.com/Mischa-dev/Dogubako/releases/download/engines-{lock_id}",
)

MACHO_MAGICS = {
    b"\xcf\xfa\xed\xfe",  # 64-bit little endian
    b"\xce\xfa\xed\xfe",
    b"\xca\xfe\xba\xbe",  # fat
    b"\xbe\xba\xfe\xca",
}
SYSTEM_PREFIXES = ("/usr/lib/", "/System/")
PLACEHOLDER = re.compile(r"@@HOMEBREW_(PREFIX|CELLAR)@@/(?:opt/([^/]+)|([^/]+)/[^/]+)/(.+)")


# ------------------------------------------------------------------ tools ---


def tool(name: str) -> str:
    for cand in (name, f"llvm-{name}", f"llvm-{name}-18", f"llvm-{name}-19", f"llvm-{name}-20"):
        if shutil.which(cand):
            return cand
    raise SystemExit(f"need {name} (Xcode command line tools, or llvm on Linux)")


IS_MAC = platform.system() == "Darwin"
OTOOL = tool("otool")
INT = tool("install-name-tool") if not IS_MAC else "install_name_tool"


def run(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout


def is_macho(p: Path) -> bool:
    if not p.is_file() or p.is_symlink():
        return False
    with open(p, "rb") as f:
        return f.read(4) in MACHO_MAGICS


def linked(p: Path) -> list[str]:
    """Load commands (dependencies) of a Mach-O file, excluding its own id."""
    out = run(OTOOL, "-L", str(p)).splitlines()[1:]
    deps = [l.strip().split(" (compatibility")[0] for l in out if l.strip()]
    own = dylib_id(p)
    seen, result = set(), []
    for d in deps:  # fat files list each arch; keep unique, drop own id
        if d != own and d not in seen:
            seen.add(d)
            result.append(d)
    return result


def dylib_id(p: Path) -> str | None:
    out = run(OTOOL, "-D", str(p)).splitlines()[1:]
    return out[0].strip() if out else None


def rpaths(p: Path) -> list[str]:
    lines = run(OTOOL, "-l", str(p)).splitlines()
    found = []
    for i, l in enumerate(lines):
        if "cmd LC_RPATH" in l:
            for j in range(i + 1, min(i + 4, len(lines))):
                m = re.match(r"\s*path (.+) \(offset", lines[j])
                if m:
                    found.append(m.group(1))
    return found


# ------------------------------------------------------------- relocation ---


class Folder:
    """One output engine folder (the bundled one, or one pack)."""

    def __init__(self, root: Path, lock: dict):
        self.root = root
        self.lock = lock
        self.lib = root / "lib"
        self.copied: dict[str, Path] = {}  # dylib basename -> source path
        self.machos: list[Path] = []
        self.warnings: list[str] = []
        # Staged source folders whose files ended up here: their licenses ship too.
        self.sources: set[str] = set()

    def resolve_dep(self, dep: str, origin: Path, origin_formula: str) -> Path | None:
        """Map an install name to a staged file, or None for system libraries."""
        if dep.startswith(SYSTEM_PREFIXES):
            return None
        m = PLACEHOLDER.match(dep)
        if m:
            formula = m.group(2) or m.group(3)
            cand = STAGE / formula / m.group(4)
            if cand.exists():
                return cand
            raise SystemExit(f"{origin}: {dep} -> {cand} is not staged (missing from the lock closure?)")
        if dep.startswith(("@rpath/", "@loader_path/", "@executable_path/")):
            name = dep.split("/", 1)[1]
            for base in (origin.parent, STAGE / origin_formula / "lib", origin.parent / "lib"):
                cand = base / name
                if cand.exists():
                    return cand
            raise SystemExit(f"{origin}: cannot find {dep}")
        raise SystemExit(f"{origin}: unexpected absolute dependency {dep}")

    def add_dylib(self, src: Path, name: str) -> None:
        if name in self.copied:
            return
        real = src.resolve()
        formula = real.relative_to(STAGE).parts[0]
        self.lib.mkdir(parents=True, exist_ok=True)
        dst = self.lib / name
        shutil.copy2(real, dst)
        dst.chmod(dst.stat().st_mode | stat.S_IWUSR)
        self.copied[name] = real
        self.sources.add(formula)
        self.relocate(dst, formula, real)

    def relocate(self, dst: Path, formula: str, src: Path | None = None) -> None:
        """Rewrite one copied Mach-O so it only refers into this folder."""
        src = src or dst
        args: list[str] = []
        own = dylib_id(dst)
        if own and dst.parent == self.lib:
            args += ["-id", f"@rpath/{dst.name}"]
        for dep in linked(dst):
            path = self.resolve_dep(dep, src, formula)
            if path is None:
                continue
            name = Path(dep).name
            self.add_dylib(path, name)
            new = f"@rpath/{name}"
            if dep != new:
                args += ["-change", dep, new]
        rel = os.path.relpath(self.lib, dst.parent)
        want = "@loader_path" if rel == "." else f"@loader_path/{rel}"
        existing = rpaths(dst)
        for rp in existing:
            if rp != want:
                args += ["-delete_rpath", rp]
        if want not in existing:
            args += ["-add_rpath", want]
        if args:
            run(INT, *args, str(dst))
        self.machos.append(dst)

    def copy_tree(self, src: Path, dst: Path, formula: str) -> None:
        for p in sorted(src.rglob("*")):
            rel = p.relative_to(src)
            out = dst / rel
            if p.is_dir() and not p.is_symlink():
                out.mkdir(parents=True, exist_ok=True)
                continue
            # ImageMagick loads its coder modules through their libtool .la
            # files, so keep those; drop other build leftovers.
            in_modules = any(part.startswith("modules-") for part in rel.parts)
            if (
                p.suffix in (".a", ".h")
                or (p.suffix == ".la" and not in_modules)
                or p.name == ".packlist"
                or "pkgconfig" in rel.parts
                or "man" in rel.parts
            ):
                continue
            out.parent.mkdir(parents=True, exist_ok=True)
            if p.is_symlink():
                target = p.resolve()
                if not target.exists():
                    continue
                shutil.copy2(target, out)
            else:
                shutil.copy2(p, out)
            out.chmod(out.stat().st_mode | stat.S_IWUSR)
            if is_macho(out):
                self.relocate(out, formula, p)
                continue
            data = out.read_bytes() if out.stat().st_size < 4 << 20 else b""
            if data.startswith(b"#!@@HOMEBREW_PERL@@"):
                # Homebrew builds these against macOS's own perl (uses_from_macos).
                data = b"#!/usr/bin/perl" + data[len(b"#!@@HOMEBREW_PERL@@"):]
                out.write_bytes(data)
            # configure.xml and .la paths are overridden by the engine's env vars
            # (libtool only uses the .la's dlname, relative to the file).
            if b"@@HOMEBREW_" in data and out.name != "configure.xml" and out.suffix != ".la":
                self.warnings.append(f"{out.relative_to(self.root)} still contains a Homebrew placeholder")

    def check(self) -> None:
        for p in self.machos:
            for dep in linked(p):
                if "@@HOMEBREW" in dep:
                    raise SystemExit(f"{p}: unrelocated dependency {dep}")
                if dep.startswith("@rpath/") and not (self.lib / dep[7:]).exists():
                    raise SystemExit(f"{p}: {dep} not in {self.lib}")

    def sign(self) -> None:
        if not IS_MAC:
            self.warnings.append("not signed: run on macOS (ad-hoc signing is required on Apple silicon)")
            return
        for p in self.machos:
            subprocess.run(["codesign", "--force", "--sign", "-", "--timestamp=none", str(p)], check=True, capture_output=True)
        # Release builds: re-sign with the Developer ID (hardened runtime)
        # before anything is packed. The command gets the folder as its argument.
        hook = os.environ.get("DOGUBAKO_ENGINE_SIGN_CMD")
        if hook:
            subprocess.run([hook, str(self.root)], check=True)


# ---------------------------------------------------------------- engines ---


LICENSE_NAME = re.compile(r"^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE)", re.I)


def license_files(src: Path) -> list[Path]:
    """License texts at the top of a keg or archive, or in its share/doc."""
    cands = [p for p in src.iterdir() if p.is_file() and LICENSE_NAME.match(p.name)]
    doc = src / "share" / "doc"
    if not cands and doc.is_dir():
        cands = [p for p in doc.rglob("*") if p.is_file() and LICENSE_NAME.match(p.name)]
    return sorted(cands)


def write_licenses(folder: "Folder", lock: dict, manifest: dict) -> None:
    """Copy each shipped component's license texts to licenses/<name>/.

    Components without a license file in their bottle or archive get a
    LICENSE-INFO.txt naming the license and where its text and source live.
    """
    by_source = {eid: e for eid, e in manifest["engine"].items()}
    by_formula = {e.get("formula"): e for e in manifest["engine"].values() if e.get("formula")}
    for name in sorted(folder.sources):
        src = STAGE / name
        out = folder.root / "licenses" / name
        out.mkdir(parents=True, exist_ok=True)
        found = license_files(src) if src.is_dir() else []
        vendored = HERE / "licenses" / name
        if vendored.is_dir():
            found += sorted(vendored.iterdir())
        for f in found:
            shutil.copy2(f, out / f.name)
        if not found:
            bottle = lock["bottles"].get(name, {})
            e = by_formula.get(name) or by_source.get(name) or {}
            spdx = bottle.get("license") or e.get("license") or "see source"
            url = e.get("source_url") or f"https://formulae.brew.sh/formula/{name}"
            (out / "LICENSE-INFO.txt").write_text(
                f"{name}\nLicense: {spdx}\nThe full license text and source code are available from: {url}\n"
            )
            folder.warnings.append(f"{name}: no license file in its package; wrote LICENSE-INFO.txt")


def install_engine(folder: Folder, eid: str, e: dict, locked: dict) -> dict:
    """Copy one engine into `folder`; returns its engine.json entry."""
    source = locked["source"]
    if source == "homebrew":
        src = STAGE / e["formula"]
    else:  # upstream archives and build-from-source.sh both stage under the engine id
        src = STAGE / eid
    if not src.exists():
        raise SystemExit(f"{eid}: {src} not staged; run fetch.py {eid}")
    folder.sources.add(src.name)
    bindir = folder.root / "bin"
    bindir.mkdir(parents=True, exist_ok=True)
    binaries = list(e.get("binaries", []))
    for b in binaries:
        cand = [src / "bin" / b, src / b]
        found = next((c for c in cand if c.exists()), None)
        if not found:
            raise SystemExit(f"{eid}: binary {b} not found in {src}")
        out = bindir / b
        shutil.copy2(found.resolve(), out)
        out.chmod(0o755)
        if is_macho(out):
            folder.relocate(out, e.get("formula", eid), found)
    for name, spec in e.get("perl_scripts", {}).items():
        tree = folder.root / "opt" / eid
        folder.copy_tree(src / spec["tree"], tree, e["formula"])
        script = tree / Path(spec["script"]).relative_to(spec["tree"])
        perl_lib = tree / Path(spec["lib"]).relative_to(spec["tree"])
        wrapper = bindir / name
        wrapper.write_text(
            "#!/bin/sh\n"
            "# Generated by packaging/engines/assemble.py: run the bundled script with the system perl.\n"
            'here="$(cd "$(dirname "$0")/.." && pwd)"\n'
            f'PERL5LIB="$here/{perl_lib.relative_to(folder.root)}" exec /usr/bin/perl "$here/{script.relative_to(folder.root)}" "$@"\n'
        )
        wrapper.chmod(0o755)
        binaries.append(name)
    for share in e.get("share", []):
        dst = bindir / share if e.get("share_into_bin") else folder.root / share
        folder.copy_tree(src / share, dst, e.get("formula", eid))
    return {
        "id": eid,
        "deps": e["deps"],
        "version": locked["version"],
        "binaries": binaries,
        "env": e.get("env", {}),
        "license": e["license"],
        "source_url": e.get("source_url"),
    }


def dir_size(p: Path) -> int:
    return sum(f.stat().st_size for f in p.rglob("*") if f.is_file() and not f.is_symlink())


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return h.hexdigest()


def lock_id(lock_text: str) -> str:
    return hashlib.sha256(lock_text.encode()).hexdigest()[:12]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", nargs="*", help="engine ids to assemble (default: all managed)")
    ap.add_argument("--no-packs", action="store_true", help="skip the on-demand packs")
    ap.add_argument(
        "--bundle-all",
        action="store_true",
        help="test builds: put the on-demand engines in the bundled folder too (no downloads needed)",
    )
    args = ap.parse_args()
    manifest = tomllib.loads(MANIFEST.read_text())
    lock_text = LOCK.read_text()
    lock = json.loads(lock_text)
    lid = lock_id(lock_text)
    wanted = set(args.only or lock["engines"])
    warnings: list[str] = []
    report: list[tuple[str, str, int]] = []

    # Bundled tier: one folder, shared lib/.
    if BUNDLED.exists():
        shutil.rmtree(BUNDLED)
    bundled = Folder(BUNDLED, lock)
    bundled_entries = []
    for eid, locked in lock["engines"].items():
        if eid not in wanted or not (locked["tier"] == "bundled" or args.bundle_all):
            continue
        if args.bundle_all and locked["source"] == "build" and not (STAGE / eid).exists():
            warnings.append(f"{eid}: built from source; run build-from-source.sh {eid} first, skipped")
            continue
        bundled_entries.append(install_engine(bundled, eid, manifest["engine"][eid], locked))
    write_licenses(bundled, lock, manifest)
    bundled.check()
    bundled.sign()
    warnings += [f"bundled: {w}" for w in bundled.warnings]

    # On-demand tier: one self-contained pack each.
    packs = []
    if not (args.no_packs or args.bundle_all):
        PACKS.mkdir(parents=True, exist_ok=True)
        for eid, locked in lock["engines"].items():
            if locked["tier"] != "on-demand" or eid not in wanted:
                continue
            if locked["source"] == "build" and not (STAGE / eid).exists():
                warnings.append(f"{eid}: built from source; run build-from-source.sh {eid} on a Mac first, skipped")
                continue
            root = PACKS / eid
            if root.exists():
                shutil.rmtree(root)
            folder = Folder(root, lock)
            entry = install_engine(folder, eid, manifest["engine"][eid], locked)
            write_licenses(folder, lock, manifest)
            folder.check()
            folder.sign()
            (root / "engine.json").write_text(json.dumps(entry, indent=2) + "\n")
            warnings += [f"{eid}: {w}" for w in folder.warnings]
            name = f"{eid}-{locked['version']}-{TARGET}.tar.gz"
            archive = PACKS / name
            with tarfile.open(archive, "w:gz") as t:
                for p in sorted(root.rglob("*")):
                    t.add(p, arcname=str(p.relative_to(root)), recursive=False)
            installed = dir_size(root)
            packs.append(
                {
                    **{k: entry[k] for k in ("id", "deps", "version", "binaries", "license")},
                    "url": f"{PACK_BASE_URL.format(lock_id=lid)}/{name}",
                    "sha256": sha256_file(archive),
                    "download_bytes": archive.stat().st_size,
                    "installed_bytes": installed,
                }
            )
            report.append((eid, "on-demand", installed))
        (PACKS / "packs.json").write_text(json.dumps({"lock_id": lid, "target": TARGET, "packs": packs}, indent=2) + "\n")

    for e in bundled_entries:
        report.append((e["id"], "bundled", 0))
    (BUNDLED / "manifest.json").write_text(
        json.dumps(
            {
                "schema": 1,
                "target": TARGET,
                "lock_id": lid,
                "minimum_system_version": manifest["macos"]["minimum_system_version"],
                "bundled": bundled_entries,
                "on_demand": packs,
            },
            indent=2,
        )
        + "\n"
    )

    print(f"bundled folder: {BUNDLED.relative_to(ROOT)}  {dir_size(BUNDLED) / 1e6:.0f} MB", file=sys.stderr)
    for eid, tier, size in report:
        if tier == "on-demand":
            print(f"  pack {eid:<12} {size / 1e6:6.0f} MB installed", file=sys.stderr)
    for w in warnings:
        print(f"warning: {w}", file=sys.stderr)


if __name__ == "__main__":
    main()
