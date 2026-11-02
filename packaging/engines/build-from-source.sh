#!/bin/sh
# Build engines that have no usable prebuilt macOS binary, into the same
# staging tree fetch.py uses (build/engines/stage/<id>/bin).
#
#   packaging/engines/build-from-source.sh whisper
#
# The git ref comes from engines.lock.json, so builds are pinned like the rest.
set -eu
root="$(cd "$(dirname "$0")/../.." && pwd)"
lock="$root/packaging/engines/engines.lock.json"
stage="$root/build/engines/stage"

field() { python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["engines"][sys.argv[2]][sys.argv[3]])' "$lock" "$1" "$2"; }

case "${1:-}" in
  whisper)
    git_url="$(field whisper git)"; ref="$(field whisper ref)"
    src="$root/build/engines/src/whisper.cpp"
    rm -rf "$src" && git clone --depth 1 --branch "$ref" "$git_url" "$src"
    # Static libraries and the Metal shaders embedded, so the result is one
    # self-contained binary.
    cmake -S "$src" -B "$src/build" -DCMAKE_BUILD_TYPE=Release \
      -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF \
      -DGGML_NATIVE=OFF -DGGML_METAL_EMBED_LIBRARY=ON -DCMAKE_OSX_DEPLOYMENT_TARGET=15.0
    cmake --build "$src/build" -j"$(sysctl -n hw.ncpu 2>/dev/null || nproc)" --target whisper-cli
    rm -rf "$stage/whisper" && mkdir -p "$stage/whisper/bin"
    cp "$src/build/bin/whisper-cli" "$stage/whisper/bin/"
    cp "$src/LICENSE" "$stage/whisper/"
    echo "staged whisper ($ref)"
    ;;
  *) echo "usage: $0 whisper" >&2; exit 2 ;;
esac
