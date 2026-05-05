# Morpheus

![Morpheus protocol illustration](docs/assets/morpheus-hero.jpeg)

Morpheus is a Rust implementation of `io.marketplace`, a draft Matrix-based protocol for federated digital marketplaces.

The goal is to let independent marketplace instances trade with each other without a central registry. Each instance owns its catalog, allowlist, policies, storage, and API. Matrix/Synapse carries verifiable protocol events; Morpheus validates them, persists raw Matrix events, builds projections, and exposes admin, seller, and buyer APIs.

Sensitive data stays outside marketplace events: payment secrets, bearer URLs, credentials, files, license keys, and delivery artifacts are not transmitted through `io.marketplace.*`.

## Current State

Morpheus is now Rust-only. The old TypeScript/npm validator was removed; Rust conformance and behavioral tests are the project oracle.

Implemented today:

- `io.marketplace` v0.1 protocol validation: envelopes, IDs, schemas, room profiles, canonical JSON, versioning, privacy/security policy, authority, and stable error codes.
- Core state machines for catalog, orders, payments, entitlements, disputes, arbitration, allowlists, and sender authority.
- Synapse-compatible Application Service ingest: `PUT /_matrix/app/v1/transactions/{txn_id}`.
- Standalone `morpheus-server --config <path>` runtime backed by Postgres.
- In-memory, SQLite, and Postgres storage implementations.
- Public HTTP APIs for admins, sellers, and buyers.
- Rust CLI for config, Synapse registration, conformance, DB migration, admin operations, seller publishing, and buyer catalog/order actions.
- Real local publish loop in server runtime: `Morpheus API -> Synapse -> Morpheus AS ingest -> Postgres`.
- Three-instance Docker E2E stack: books, smartphone cases, and fashion marketplaces, each with its own Morpheus server, Synapse homeserver, and Postgres database.

Important current limitation:

- Trusted remote catalog visibility is implemented by a trusted Morpheus catalog indexer over peer Morpheus catalog APIs. Local writes do round-trip through Synapse; remote catalog indexing does not yet read remote Matrix room history directly.

## Documents

- [Protocol](docs/protocol.md) describes the Morpheus wire protocol, event model, lifecycles, authority rules, and conformance expectations.
- [Rust Implementation](docs/rust-implementation.md) describes the workspace architecture, crates, runtime flow, storage, config, tests, and operational scope.

## Quick Start

Install Rust and baseline tools:

```bash
brew install rustup-init
rustup-init -y
source "$HOME/.cargo/env"
rustup component add rustfmt clippy llvm-tools-preview
cargo install cargo-nextest cargo-llvm-cov
```

Run the main Rust gate:

```bash
make check
```

Run individual checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo nextest run --workspace
cargo run -p morpheus-cli -- conformance run
```

Validate config and generate a Synapse Application Service registration:

```bash
cargo run -p morpheus-cli -- config validate --config config/local.toml
cargo run -p morpheus-cli -- synapse registration --config config/local.toml --out .local/synapse/morpheus-registration.yaml
```

Run local Postgres and migrate it:

```bash
docker compose up -d postgres pgweb
cargo run -p morpheus-cli -- db migrate \
  --database-url postgres://morpheus:morpheus@localhost:5432/morpheus \
  --database-kind postgres
```

Start the standalone server:

```bash
MORPHEUS_ADMIN_TOKEN=admin-token \
MORPHEUS_SELLER_TOKEN=seller-token \
MORPHEUS_BUYER_TOKEN=buyer-token \
cargo run -p morpheus-server -- --config config/local.toml
```

## CLI

The CLI is JSON-first and role-token based.

```bash
cargo run -p morpheus-cli -- admin health --server-url http://127.0.0.1:8080 --token admin-token
cargo run -p morpheus-cli -- admin rooms bootstrap --server-url http://127.0.0.1:8080 --token admin-token
cargo run -p morpheus-cli -- buyer catalog offers --server-url http://127.0.0.1:8080 --token buyer-token
```

Default token env vars:

```bash
MORPHEUS_ADMIN_TOKEN=admin-token
MORPHEUS_SELLER_TOKEN=seller-token
MORPHEUS_BUYER_TOKEN=buyer-token
```

## Three-Instance E2E

Run the full local network:

```bash
make e2e-three-synapse
```

This starts:

- `books.example`: books marketplace;
- `cases.example`: smartphone cases marketplace;
- `fashion.example`: shoes and clothing marketplace;
- one Synapse homeserver per instance;
- one Morpheus server per instance;
- one Postgres database per instance.

The E2E seeds 4-5 sellers per instance, publishes products/offers through the seller CLI, verifies Synapse AS ingest and Postgres projections, checks trusted remote catalog visibility, and exercises idempotent/conflicting transaction behavior.

Stop the stack:

```bash
make e2e-three-synapse-down
```

## Main Crates

- `morpheus-protocol`: wire constants, IDs, envelope validation, canonical JSON, room profile checks, versioning, and policy helpers.
- `morpheus-core`: pure catalog/order/payment/entitlement/dispute/arbitration state machines and validators.
- `morpheus-config`: shared TOML config loading and validation.
- `morpheus-api`: shared HTTP DTOs for CLI/server.
- `morpheus-matrix`: Matrix Application Service transaction types and Synapse registration generation.
- `morpheus-store`: storage trait plus in-memory, SQLite, and Postgres implementations.
- `morpheus-server`: Axum server, Synapse publisher, AS ingest, projections, health, admin, seller, and buyer APIs.
- `morpheus-cli`: local operator, seller, buyer, and demo tools.
- `morpheus-conformance`: required v0.1 vectors and stable conformance runner.

## Quality Policy

Protocol confidence is measured by contract coverage first, line coverage second.

- Every required conformance vector and migrated parity scenario must have a Rust test with stable accept/reject status and error code.
- Behavioral coverage must exercise envelope validation, catalog replay, order lifecycle, payments, entitlements, disputes, arbitration, privacy/security, and Application Service ingest.
- Line coverage is enforced for protocol/core/matrix/conformance crates with a practical `98%` gate.
- Protocol behavior changes require a spec note, conformance vector, and Rust behavioral test.
