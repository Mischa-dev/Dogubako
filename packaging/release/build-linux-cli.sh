#!/bin/sh
# Build the Linux x86-64 CLI inside a manylinux_2_28 container (AlmaLinux 8,
# glibc 2.28). A binary links against the glibc of the machine that built it,
# so building on a current runner makes it refuse to start on older distros.
#
#   packaging/release/build-linux-cli.sh            # release build
#   packaging/release/build-linux-cli.sh --debug    # faster, for CI checks
#
# Needs Docker and web/dist (the server embeds it). Prints the binary's path.
set -eu

profile=release
case "${1:-}" in
  "") ;;
  --debug) profile=debug ;;
  *) echo "usage: $0 [--debug]" >&2; exit 2 ;;
esac
flag=""; [ "$profile" = release ] && flag=--release

image="${DOGUBAKO_LINUX_BUILD_IMAGE:-quay.io/pypa/manylinux_2_28_x86_64}"
toolchain="${RUST_TOOLCHAIN:-stable}"
target=x86_64-unknown-linux-gnu
root="$(cd "$(dirname "$0")/../.." && pwd)"

[ -f "$root/web/dist/index.html" ] || {
  echo "web/dist is missing: run 'npm ci && npm run build' in web/ first" >&2
  exit 1
}

docker run --rm --user "$(id -u):$(id -g)" \
  -v "$root:/src" -w /src -e HOME=/tmp/home -e CARGO_TERM_COLOR \
  "$image" sh -euc "
    curl -sSf https://sh.rustup.rs | sh -s -- -y --quiet --profile minimal --default-toolchain $toolchain
    . /tmp/home/.cargo/env
    cargo build --locked -p dogubako-cli --target $target $flag
  " >&2

echo "target/$target/$profile/dogubako"
