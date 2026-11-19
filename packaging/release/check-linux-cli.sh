#!/bin/sh
# Check that a Linux CLI binary starts on every distro we support: it needs
# no glibc symbol newer than the floor, and `--help` plus a small real job
# work on the oldest (AlmaLinux 8, glibc 2.28) and a current stable distro
# (Debian 13, glibc 2.41).
#
#   packaging/release/check-linux-cli.sh target/x86_64-unknown-linux-gnu/release/dogubako
#
# Needs objdump (binutils) and Docker. Exits non-zero on the first failure.
set -eu

fail() { echo "FAIL: $*" >&2; exit 1; }

[ $# -eq 1 ] || { echo "usage: $0 <binary>" >&2; exit 2; }
bin="$1"
floor="${MAX_GLIBC:-2.28}"
images="${SMOKE_IMAGES:-almalinux:8-minimal debian:13-slim}"
[ -x "$bin" ] || fail "$bin is not an executable file"

need="$(objdump -T "$bin" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -n1)"
[ -n "$need" ] || fail "$bin has no versioned glibc symbols"
newest="$(printf '%s\n%s\n' "$need" "$floor" | sort -V | tail -n1)"
[ "$newest" = "$floor" ] || fail "$bin needs GLIBC_$need, newer than the supported floor GLIBC_$floor"
echo "ok: needs at most GLIBC_$need (floor $floor)"

dir="$(cd "$(dirname "$bin")" && pwd)"
name="$(basename "$bin")"
for image in $images; do
  docker run --rm -v "$dir:/b:ro" "$image" "/b/$name" --help >/dev/null \
    || fail "'$name --help' does not start on $image"
  out="$(echo '{"b":2,"a":1}' | docker run --rm -i -v "$dir:/b:ro" "$image" "/b/$name" dev json)" \
    || fail "'$name dev json' failed on $image"
  echo "$out" | grep -q '"a"' || fail "'$name dev json' printed unexpected output on $image: $out"
  echo "ok: $image"
done
