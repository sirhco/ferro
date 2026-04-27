# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the Ferro CLI.
#
# Stage 1 (builder): pulls the pinned nightly toolchain via rust-toolchain.toml,
# warms a cargo registry cache, and produces a release binary.
# Stage 2 (runtime): debian-slim with just the libs Ferro needs (libssl,
# ca-certificates) plus the compiled binary.

ARG RUST_VERSION=1.86

FROM rust:${RUST_VERSION}-slim-bookworm AS builder

WORKDIR /usr/src/ferro

# Build deps for surrealdb-rocksdb + sqlx + aws-sdk-s3.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        clang \
        libclang-dev \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache the dependency graph in its own layer.
COPY rust-toolchain.toml ./
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY examples ./examples

RUN cargo build --release --locked -p ferro-cli --bin ferro

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 ferro

COPY --from=builder /usr/src/ferro/target/release/ferro /usr/local/bin/ferro

USER ferro
WORKDIR /home/ferro

# Default config + data + media + plugin dirs land here when the operator
# mounts a volume. `ferro init` writes ferro.toml on first boot.
VOLUME ["/home/ferro/data", "/home/ferro/media-store", "/home/ferro/plugins"]

EXPOSE 8080

ENV FERRO_BIND=0.0.0.0:8080 \
    RUST_LOG=info,ferro=info

ENTRYPOINT ["ferro"]
CMD ["serve", "--bind", "0.0.0.0:8080"]
