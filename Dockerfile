# syntax=docker/dockerfile:1
# Dogubako self-hosted image: web UI + HTTP API + MCP, with every engine baked
# in at pinned versions (packaging/engines, docs/ENGINES.md).
#
#   docker build -t dogubako .
#   docker build --build-arg WITH_OFFICE=1 -t dogubako:office .   # + LibreOffice (~500 MB)
#   docker build --build-arg WITH_REMBG=1 -t dogubako:rembg .     # + rembg background removal (~600 MB)
#   docker build --build-arg WITH_WHISPER=0 --build-arg WITH_PIPER=0 -t dogubako:slim .
#
# Engines come from a fixed Debian snapshot with exact package versions
# (packaging/engines/docker-packages*-<arch>.txt), so the same Dockerfile always
# builds the same tested engine set. Bump with packaging/engines/docker-lock.py.

ARG RUST_VERSION=1.95
ARG NODE_VERSION=22
# Keep in sync with [docker] in packaging/engines/engines.toml.
ARG DEBIAN_BASE=debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4
ARG DEBIAN_SNAPSHOT=20261101T000000Z
ARG PIPER_VERSION=2023.11.14-2
ARG TARGETARCH

FROM node:${NODE_VERSION}-trixie-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json* ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:${RUST_VERSION}-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web /src/web/dist ./web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p dogubako-cli --locked && cp target/release/dogubako /dogubako

# A Debian base that can install the pinned versions: the regular mirrors
# first (fast, and they still carry most of them), then the snapshot for any
# version they've since replaced. Every package is pinned to an exact version
# (packaging/engines/docker-packages*.txt), so the result is the same either way.
FROM ${DEBIAN_BASE} AS pinned
ARG DEBIAN_SNAPSHOT
RUN printf '%s\n' \
      'Types: deb' \
      'URIs: http://deb.debian.org/debian' \
      'Suites: trixie trixie-updates' \
      'Components: main' \
      '' \
      'Types: deb' \
      'URIs: http://deb.debian.org/debian-security' \
      'Suites: trixie-security' \
      'Components: main' \
      '' \
      'Types: deb' \
      "URIs: http://snapshot.debian.org/archive/debian/${DEBIAN_SNAPSHOT}" \
      'Suites: trixie trixie-updates' \
      'Components: main' \
      'Check-Valid-Until: no' \
      '' \
      'Types: deb' \
      "URIs: http://snapshot.debian.org/archive/debian-security/${DEBIAN_SNAPSHOT}" \
      'Suites: trixie-security' \
      'Components: main' \
      'Check-Valid-Until: no' \
      > /etc/apt/sources.list.d/debian.sources \
    && printf 'Acquire::Retries "8";\n' > /etc/apt/apt.conf.d/80-retries

# whisper.cpp for offline speech recognition (MIT), built from a pinned tag.
# Its build tools come from the regular mirrors; only the static binary ships.
FROM ${DEBIAN_BASE} AS whisper
ARG WITH_WHISPER=1
ARG WHISPER_REF=v1.7.6
RUN mkdir -p /out && if [ "$WITH_WHISPER" = "1" ]; then \
      apt-get update && apt-get install -y --no-install-recommends git cmake build-essential ca-certificates && \
      git clone --depth 1 --branch "$WHISPER_REF" https://github.com/ggml-org/whisper.cpp /whisper && \
      cmake -S /whisper -B /whisper/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF -DGGML_NATIVE=OFF && \
      cmake --build /whisper/build -j"$(nproc)" --target whisper-cli && \
      cp /whisper/build/bin/whisper-cli /out/; \
    fi

# Piper text-to-speech (MIT engine, GPL-3.0 espeak-ng), last native release.
# BuildKit verifies each download against its pinned SHA-256.
FROM pinned AS piper-amd64
ARG PIPER_VERSION
ADD --checksum=sha256:a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992 \
    https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_x86_64.tar.gz /tmp/piper.tgz
FROM pinned AS piper-arm64
ARG PIPER_VERSION
ADD --checksum=sha256:fea0fd2d87c54dbc7078d0f878289f404bd4d6eea6e7444a77835d1537ab88eb \
    https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}/piper_linux_aarch64.tar.gz /tmp/piper.tgz
FROM piper-${TARGETARCH} AS piper
ARG WITH_PIPER=1
RUN mkdir -p /opt/piper && if [ "$WITH_PIPER" = "1" ]; then \
      tar -xzf /tmp/piper.tgz -C /opt/piper --strip-components=1 && /opt/piper/piper --version; \
    fi

FROM pinned
ARG WITH_OFFICE=0
ARG WITH_REMBG=0
ARG EXTRA_PACKAGES=""
# One list per architecture: the dependency closure and binNMU versions differ.
COPY packaging/engines/docker-packages-*.txt /tmp/engines/
RUN set -eu; \
    arch="$(dpkg --print-architecture)"; \
    if [ ! -f "/tmp/engines/docker-packages-$arch.txt" ]; then \
      echo "No pinned package list for $arch; run packaging/engines/docker-lock.py --arch $arch" >&2; exit 1; \
    fi; \
    pkgs="$(grep -v '^#' "/tmp/engines/docker-packages-$arch.txt")"; \
    if [ "$WITH_OFFICE" = "1" ]; then pkgs="$pkgs $(grep -v '^#' "/tmp/engines/docker-packages-office-$arch.txt")"; fi; \
    if [ "$WITH_REMBG" = "1" ]; then pkgs="$pkgs $(grep -v '^#' "/tmp/engines/docker-packages-rembg-$arch.txt")"; fi; \
    apt-get update && apt-get install -y --no-install-recommends $pkgs $EXTRA_PACKAGES \
    && rm -rf /var/lib/apt/lists/* /tmp/engines \
    && useradd --system --create-home --home-dir /home/dogubako --uid 10001 dogubako \
    && mkdir -p /data && chown dogubako:dogubako /data
COPY --from=build /dogubako /usr/local/bin/dogubako
COPY --from=whisper /out/ /usr/local/bin/
COPY --from=piper /opt/piper /opt/piper
RUN if [ -x /opt/piper/piper ]; then ln -s /opt/piper/piper /usr/local/bin/piper; else rmdir /opt/piper; fi
# rembg (background removal) from PyPI at pinned versions, prebuilt wheels only.
# Its models are downloaded in the app (Settings → Models).
COPY packaging/engines/rembg-requirements.txt packaging/engines/rembg-cli.py /tmp/rembg/
RUN if [ "$WITH_REMBG" = "1" ]; then \
      python3 -m venv /opt/rembg \
      && /opt/rembg/bin/pip install --no-cache-dir --no-deps --only-binary=:all: -r /tmp/rembg/rembg-requirements.txt \
      && install -m 755 /tmp/rembg/rembg-cli.py /usr/local/bin/rembg \
      && rembg --version; \
    fi; \
    rm -rf /tmp/rembg
USER dogubako
ENV DOGUBAKO_BIND=0.0.0.0:8420 \
    DOGUBAKO_DATA_DIR=/data \
    DOGUBAKO_SECURE_COOKIES=false \
    DOGUBAKO_ENGINES=system \
    DOGUBAKO_DOCKER_IMAGE=1
VOLUME ["/data"]
EXPOSE 8420
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["dogubako", "server", "health"]
ENTRYPOINT ["/usr/bin/tini", "--", "dogubako"]
CMD ["server", "start"]
