FROM rust:1.95-slim AS builder

WORKDIR /app
RUN apt-get update \
  && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
  && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release -p morpheus-server -p morpheus-cli

FROM debian:bookworm-slim

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/morpheus-server /usr/local/bin/morpheus-server
COPY --from=builder /app/target/release/morpheus /usr/local/bin/morpheus

EXPOSE 8080
ENTRYPOINT ["morpheus-server"]
