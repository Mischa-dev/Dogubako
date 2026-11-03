#!/bin/sh
# Smoke-test assembled engine folders on a Mac: every binary starts with a
# clean environment, links nothing outside its folder or macOS, and does a
# small real job.
#
#   packaging/engines/smoke.sh apps/desktop/src-tauri/engines build/engines/packs/*/
#
# Run after assemble.py. Exits non-zero on the first failure.
set -eu

fail() { echo "FAIL: $*" >&2; exit 1; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

check_links() {
  # Only @rpath (inside the folder), /usr/lib and /System are allowed.
  # Universal binaries print one "<file> (architecture x):" header per slice.
  bad="$(otool -L "$1" | grep -v ':$' | awk '{print $1}' | grep -vE '^(@rpath/|/usr/lib/|/System/)' || true)"
  [ -z "$bad" ] || fail "$1 links outside its folder: $bad"
}

run_in() {
  # $1 = engine folder, rest = command. Same minimal env the app uses, plus
  # the engine's own variables from manifest.json / engine.json.
  dir="$1"; shift
  envs="$(python3 -I - "$dir" <<'PY'
import json, sys, pathlib, shlex
d = pathlib.Path(sys.argv[1])
items = []
for f in ("engine.json", "manifest.json"):
    p = d / f
    if p.exists():
        j = json.loads(p.read_text())
        for e in j.get("bundled", [j]):
            for k, v in e.get("env", {}).items():
                items.append(f"{k}={':'.join(str(d / x) for x in v.split(':'))}")
print(" ".join(shlex.quote(i) for i in items))
PY
)"
  eval "env -i HOME=\"$work\" TMPDIR=\"$work\" PATH=\"$dir/bin:/usr/bin:/bin\" $envs \"\$@\""
}

for dir in "$@"; do
  dir="$(cd "$dir" && pwd)"
  echo "== $dir"
  find "$dir" -type f -perm -u+x | while read -r f; do
    if file "$f" | grep -q Mach-O; then check_links "$f"; fi
  done
  find "$dir/lib" -name '*.dylib' 2>/dev/null | while read -r f; do check_links "$f"; done
  b="$dir/bin"
  [ -x "$b/ffmpeg" ] && {
    run_in "$dir" ffmpeg -v error -f lavfi -i testsrc=d=1:s=160x120 -f lavfi -i sine=d=1 -c:v libx264 -c:a aac -y "$work/t.mp4" || fail ffmpeg
    run_in "$dir" ffprobe -v error -show_streams "$work/t.mp4" >/dev/null || fail ffprobe
  }
  [ -x "$b/qpdf" ] && { run_in "$dir" qpdf --version >/dev/null || fail qpdf; }
  [ -x "$b/pdftoppm" ] && { run_in "$dir" pdftoppm -v 2>/dev/null || run_in "$dir" pdftoppm -v 2>&1 | grep -q pdftoppm || fail poppler; }
  [ -x "$b/exiftool" ] && { run_in "$dir" exiftool -ver >/dev/null || fail exiftool; }
  [ -x "$b/7zz" ] && { run_in "$dir" 7zz i >/dev/null || fail 7zz; }
  [ -x "$b/magick" ] && {
    run_in "$dir" magick -list format | grep -q HEIC || fail "magick: no HEIC coder (module path?)"
    run_in "$dir" magick -size 32x32 xc:red "$work/t.png" || fail magick
  }
  [ -x "$b/gs" ] && { run_in "$dir" gs -q -dNOPAUSE -dBATCH -sDEVICE=pdfwrite -o "$work/t.pdf" -c showpage || fail ghostscript; }
  [ -x "$b/tesseract" ] && { run_in "$dir" tesseract --list-langs 2>&1 | grep -q '^eng' || fail "tesseract: eng missing (TESSDATA_PREFIX?)"; }
  [ -x "$b/pandoc" ] && { echo '# hi' | run_in "$dir" pandoc -f markdown -t html | grep -q '<h1' || fail pandoc; }
  [ -x "$b/realesrgan-ncnn-vulkan" ] && { [ -d "$b/models" ] || fail "realesrgan: models missing"; }
  [ -x "$b/whisper-cli" ] && { run_in "$dir" whisper-cli --help >/dev/null 2>&1 || fail whisper; }
  echo "ok"
done
