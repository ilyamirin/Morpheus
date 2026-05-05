# syntax=docker/dockerfile:1.7

FROM rust:1.95-slim AS chef

WORKDIR /app
RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
  && rm -rf /var/lib/apt/lists/*
RUN --mount=type=cache,id=morpheus-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=morpheus-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    cargo install cargo-chef --locked

FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher

COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=morpheus-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=morpheus-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=morpheus-cargo-target,target=/app/target,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json

FROM chef AS builder

COPY . .
RUN --mount=type=cache,id=morpheus-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=morpheus-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=morpheus-cargo-target,target=/app/target,sharing=locked \
    cargo build --release -p morpheus-server -p morpheus-cli \
    && mkdir -p /out \
    && cp /app/target/release/morpheus-server /out/morpheus-server \
    && cp /app/target/release/morpheus /out/morpheus

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/morpheus-server /usr/local/bin/morpheus-server
COPY --from=builder /out/morpheus /usr/local/bin/morpheus

EXPOSE 8080
ENTRYPOINT ["morpheus-server"]
