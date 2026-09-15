# syntax=docker/dockerfile:1.7

# ---- Build ------------------------------------------------------------------
FROM rust:1-bookworm AS build

RUN apt-get update \
 && apt-get install -y --no-install-recommends clang libclang-dev \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p agenttx-protocol \
 && cp target/release/agenttx /usr/local/bin/agenttx

# ---- Runtime ----------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin agenttx \
 && mkdir -p /data \
 && chown agenttx:agenttx /data

COPY --from=build /usr/local/bin/agenttx /usr/local/bin/agenttx

ENV AGENTTX_LISTEN_ADDR=0.0.0.0:50051 \
    AGENTTX_DATA_DIR=/data/rocksdb \
    AGENTTX_FS_ROOT=/data/sandbox \
    AGENTTX_OUTBOX_PATH=/data/outbox.jsonl \
    AGENTTX_LOG_FORMAT=json

USER agenttx
VOLUME ["/data"]
EXPOSE 50051

ENTRYPOINT ["/usr/local/bin/agenttx"]
