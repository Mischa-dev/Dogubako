# syntax=docker/dockerfile:1.7
# Dogubako self-hosted image: web UI + HTTP API + MCP, with common processing engines.
#
#   docker build -t dogubako .
#   docker build --build-arg WITH_WHISPER=1 -t dogubako:whisper .   # + whisper.cpp

ARG RUST_VERSION=1.95
ARG NODE_VERSION=22

FROM node:${NODE_VERSION}-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json* ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:${RUST_VERSION}-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web /src/web/dist ./web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p dogubako-cli --locked && cp target/release/dogubako /dogubako

# Optional: whisper.cpp for offline speech recognition (MIT).
FROM debian:bookworm-slim AS whisper
ARG WITH_WHISPER=0
ARG WHISPER_REF=v1.7.6
RUN mkdir -p /out && if [ "$WITH_WHISPER" = "1" ]; then \
      apt-get update && apt-get install -y --no-install-recommends git cmake build-essential ca-certificates && \
      git clone --depth 1 --branch "$WHISPER_REF" https://github.com/ggml-org/whisper.cpp /whisper && \
      cmake -S /whisper -B /whisper/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF && \
      cmake --build /whisper/build -j"$(nproc)" --target whisper-cli && \
      cp /whisper/build/bin/whisper-cli /out/; \
    fi

FROM debian:bookworm-slim
ARG EXTRA_PACKAGES=""
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates tini \
      ffmpeg imagemagick libvips-tools libimage-exiftool-perl \
      qpdf poppler-utils ghostscript tesseract-ocr tesseract-ocr-eng \
      p7zip-full bubblewrap fonts-dejavu-core \
      $EXTRA_PACKAGES \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /home/dogubako --uid 10001 dogubako \
    && mkdir -p /data && chown dogubako:dogubako /data
COPY --from=build /dogubako /usr/local/bin/dogubako
COPY --from=whisper /out/ /usr/local/bin/
USER dogubako
ENV DOGUBAKO_BIND=0.0.0.0:8420 \
    DOGUBAKO_DATA_DIR=/data \
    DOGUBAKO_SECURE_COOKIES=false
VOLUME ["/data"]
EXPOSE 8420
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s CMD ["dogubako", "server", "health"]
ENTRYPOINT ["/usr/bin/tini", "--", "dogubako"]
CMD ["server", "start"]
