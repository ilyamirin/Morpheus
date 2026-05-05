# Morpheus

Morpheus is a Rust implementation of `io.marketplace`, a draft Matrix protocol for federated digital marketplaces.

The project goal is to make marketplace instances interoperable without a central registry. Each instance publishes a catalog room, validates allowlisted peers, creates private order rooms, and records order/payment/entitlement/dispute state as typed Matrix events. Matrix carries verifiable protocol state; search, payments, credentials, files, license delivery, and other sensitive artifacts stay outside Matrix.

## What Is Included

- Protocol validators for `io.marketplace` v0.1 envelopes, IDs, room profiles, schema rules, canonical JSON, privacy/security policy, compatibility, and stable error codes.
- Core catalog and order logic: catalog snapshot replay, seller/product/offer projection, order lifecycle validation, payment capture rules, entitlements, disputes, arbitration, allowlist policy, and sender authority checks.
- A Synapse-compatible Matrix Application Service runtime as an Axum router: transaction ingest, raw event retention, validation, projection, health/readiness/metrics, and bearer-protected admin endpoints.
- Storage contracts plus in-memory and SQLite implementations, with SQL migration text for SQLite and Postgres. A Postgres `EventStore` implementation is not present yet.
- Rust conformance vectors and behavioral tests as the project oracle.
- CLI tools for config validation, Synapse registration generation, conformance runs, snapshot hash checks, DB migration, and catalog rebuild scheduling.

## Documents

- [Protocol](docs/protocol.md) describes the Morpheus wire protocol, event model, lifecycles, authority rules, and conformance expectations.
- [Rust Implementation](docs/rust-implementation.md) describes the workspace architecture, crates, runtime flow, storage, config, tests, and current operational scope.
- [Original design draft](docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md) is kept as the detailed specification source for v0.1.

## Quick Start

Install Rust and project tools:

```bash
brew install rustup-init
rustup-init -y
source "$HOME/.cargo/env"
rustup component add rustfmt clippy llvm-tools-preview
cargo install cargo-nextest cargo-llvm-cov
```

Run the full Rust gate:

```bash
make check
```

Run individual checks:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo nextest run --workspace
cargo run -p morpheus-cli -- conformance run
```

Validate local config and generate a Synapse Application Service registration:

```bash
cargo run -p morpheus-cli -- config validate --config config/local.toml
cargo run -p morpheus-cli -- synapse registration --config config/local.toml --out .local/synapse/morpheus-registration.yaml
```

Start the local infrastructure used by the implementation:

```bash
docker compose up -d postgres pgweb
cargo run -p morpheus-cli -- db migrate --database-url postgres://morpheus:morpheus@localhost:5432/morpheus --database-kind postgres
```

The Compose file also contains a Synapse service, but a fresh checkout must initialize `.local/synapse` and wire the generated Application Service registration into `homeserver.yaml` before starting it.

The HTTP server runtime currently lives in `morpheus-server` as `build_router(config, store)`. It is exercised by integration tests and can be embedded by a small binary or deployment wrapper. Docker Compose starts infrastructure only; it does not start a Morpheus server container yet. Until that wrapper lands, use the CLI, conformance runner, and server tests to run the implementation surface:

```bash
cargo test -p morpheus-server
cargo test -p morpheus-store
cargo test -p morpheus-conformance
```

## Main Crates

- `morpheus-protocol`: wire constants, IDs, envelope validation, canonical JSON, room profile checks, versioning, and policy helpers.
- `morpheus-core`: pure catalog/order/payment/entitlement/dispute/arbitration state machines and validators.
- `morpheus-matrix`: Matrix Application Service transaction types and Synapse registration generation.
- `morpheus-store`: storage trait, in-memory store, SQLite store, and SQL migrations.
- `morpheus-server`: Axum routes for Matrix AS ingest, projections, health, metrics, and admin APIs.
- `morpheus-cli`: local operator tools.
- `morpheus-conformance`: required v0.1 vectors and stable conformance runner.

## Quality Policy

Protocol confidence is measured by contract coverage first, line coverage second.

- Every required conformance vector and every migrated parity scenario must have a Rust test with stable accept/reject status and error code.
- Behavioral coverage must exercise envelope validation, catalog replay, order lifecycle, payments, entitlements, disputes, arbitration, privacy/security, and Application Service ingest.
- Line coverage is enforced for protocol/core/matrix/conformance crates with a practical `98%` gate.
- Conformance vectors must not be weakened to satisfy line coverage. Protocol behavior changes require a spec note, conformance vector, and Rust behavioral test.
