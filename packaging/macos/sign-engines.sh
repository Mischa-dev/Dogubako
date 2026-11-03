#!/usr/bin/env bash
# Sign every Mach-O file in the app-managed engine folders with the Developer ID
# and the hardened runtime, so the notarization service accepts the DMG.
#
#   packaging/macos/sign-engines.sh <engine-dir>...
#
# Arguments are engine folders, signed in place. packaging/engines/assemble.py
# calls this through DOGUBAKO_ENGINE_SIGN_CMD once per engine folder, before the
# on-demand packs are archived and checksummed, so packs.json stays correct.
#
# Environment (the same names tauri-action uses, so one set of secrets serves both):
#   APPLE_SIGNING_IDENTITY      e.g. "Developer ID Application: Name (TEAMID)". Unset: the
#                               script prints a notice and exits 0, so unsigned test builds work.
#   APPLE_CERTIFICATE           optional base64 .p12; imported into a temporary keychain
#   APPLE_CERTIFICATE_PASSWORD  its password
#
# Run it after the engines are assembled and before `cargo tauri build`.
set -euo pipefail

if [ "$(uname -s)" != Darwin ]; then
  echo "sign-engines: macOS only" >&2; exit 1
fi
if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "::notice::APPLE_SIGNING_IDENTITY is not set; engines stay unsigned (test build)."
  exit 0
fi
[ $# -ge 1 ] || { echo "usage: $0 <engine-dir>..." >&2; exit 2; }

KEYCHAIN=""
cleanup() { [ -n "$KEYCHAIN" ] && security delete-keychain "$KEYCHAIN" 2>/dev/null || true; }
trap cleanup EXIT

# Import the certificate only if the identity isn't usable yet (a local Mac
# with the certificate in the login keychain needs nothing imported).
if [ -n "${APPLE_CERTIFICATE:-}" ] && ! security find-identity -v -p codesigning | grep -qF "$APPLE_SIGNING_IDENTITY"; then
  KEYCHAIN="$(mktemp -d)/dogubako-engines.keychain-db"
  KC_PASS="$(uuidgen)"
  P12="$(mktemp)"
  printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$P12"
  security create-keychain -p "$KC_PASS" "$KEYCHAIN"
  security set-keychain-settings -lut 3600 "$KEYCHAIN"
  security unlock-keychain -p "$KC_PASS" "$KEYCHAIN"
  security import "$P12" -k "$KEYCHAIN" -P "${APPLE_CERTIFICATE_PASSWORD:-}" -T /usr/bin/codesign
  security set-key-partition-list -S apple-tool:,apple: -s -k "$KC_PASS" "$KEYCHAIN" >/dev/null
  security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | tr -d '"')
  rm -f "$P12"
fi

sign_tree() {
  local dir="$1" n=0
  # Libraries first, then executables, so nothing is signed before what it loads.
  while IFS= read -r -d '' f; do
    if file -b "$f" | grep -q 'Mach-O'; then
      codesign --force --timestamp --options runtime --sign "$APPLE_SIGNING_IDENTITY" "$f"
      n=$((n + 1))
    fi
  done < <(find "$dir" -type f \( -name '*.dylib' -o -name '*.so' \) -print0; find "$dir" -type f ! -name '*.dylib' ! -name '*.so' -perm -u+x -print0)
  # Verify everything we touched.
  while IFS= read -r -d '' f; do
    if file -b "$f" | grep -q 'Mach-O'; then codesign --verify --strict "$f"; fi
  done < <(find "$dir" -type f -print0)
  echo "signed $n Mach-O files in $dir"
}

for target in "$@"; do sign_tree "$target"; done
