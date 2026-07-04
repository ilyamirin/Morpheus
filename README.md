# Morpheus

![Morpheus protocol illustration](docs/assets/morpheus-hero.jpeg)

Morpheus is a Rust implementation of `io.marketplace`, a draft Matrix-based protocol for federated digital marketplaces.

The goal is to let independent marketplace instances trade with each other without a central registry. Each instance owns its catalog, allowlist, policies, storage, and API. Matrix/Synapse carries verifiable protocol events; Morpheus validates them, persists raw Matrix events, builds projections, and exposes admin, seller, and buyer APIs.

Sensitive data stays outside marketplace events: payment secrets, bearer URLs, credentials, files, license keys, and delivery artifacts are not transmitted through `io.marketplace.*`.

Full documentation: <https://ilyamirin.github.io/Morpheus>

## Current State

Morpheus is now Rust-only. The old TypeScript/npm validator was removed; Rust conformance and behavioral tests are the project oracle.

Implemented today:

- `io.marketplace` v0.1 protocol validation: envelopes, IDs, schemas, room profiles, canonical JSON, versioning, privacy/security policy, authority, and stable error codes.
- Core state machines for catalog, orders, payments, entitlements, disputes, arbitration, allowlists, and sender authority.
- Synapse-compatible Application Service ingest: `PUT /_matrix/app/v1/transactions/{txn_id}`.
- Standalone `morpheus-server --config <path>` runtime backed by Postgres.
- In-memory, SQLite, and Postgres storage implementations.
- Public HTTP APIs for admins, sellers, and buyers.
- Static admin, seller, and buyer UIs served by `morpheus-server`: admin auto-refresh, seller storefront with Quick Add and per-order lifecycle actions, buyer gallery with checkout sheet and pending projection state.
- Rust CLI for config, Synapse registration, conformance, DB migration, admin operations, seller publishing, and buyer catalog/order actions.
- Real local publish loop in server runtime: `Morpheus API -> Synapse -> Morpheus AS ingest -> Postgres`.
- Buyer resilience flows for stale offers, pending order projections, and temporarily unavailable trusted remote catalogs.
- Seller product image upload in the dev UI: images are compressed in-browser and published as product media metadata, with category images as fallback.
- Three-instance Docker E2E stack: books, smartphone cases, and fashion marketplaces, each with its own Morpheus server, Synapse homeserver, and Postgres database.
- EVM escrow adapter MVP: Vyper ERC-20 escrow, embedded finalized-log watcher, viem wallet actions for buyer deposit, seller release, and arbiter refund, plus local Anvil E2E wiring.

Important current limitation:

- Trusted remote catalog visibility is implemented by a trusted Morpheus catalog indexer over peer Morpheus catalog APIs. Local writes do round-trip through Synapse; remote catalog indexing does not yet read remote Matrix room history directly.
- When a trusted peer is temporarily unavailable, the indexer reports `cached` with `REMOTE_CATALOG_UNAVAILABLE` and keeps the last accepted catalog projections visible instead of failing buyer discovery.

User-facing edge cases now covered:

- Withdrawn offers are removed from buyer discovery and direct order creation returns `409 OFFER_WITHDRAWN`.
- After `Create order`, the buyer UI shows a pending projection state until Synapse AS ingest catches up, then resolves to the projected order timeline.
- If projection does not catch up inside the polling window, the UI surfaces `projection_timeout` and keeps the submitted room/event context in Advanced output.

## Documents

- [Full HTML Documentation](https://ilyamirin.github.io/Morpheus) is the broad GitHub Pages documentation for principles, installation, configuration, operation, and federated E2E.
- [Protocol](docs/protocol.md) describes the Morpheus wire protocol, event model, lifecycles, authority rules, and conformance expectations.
- [Payment Design Manifesto](docs/payment-design-manifesto.md) explains how Morpheus designs payments for federated marketplaces, escrow, developing-market constraints, privacy, and operational safety.
- [EVM Escrow Payment Adapter](docs/protocol-evm-escrow.md) describes the Vyper-based ERC-20 escrow adapter, local Anvil execution, and watcher verification model.
- [EVM Escrow Production Runbook](docs/evm-escrow-production-runbook.md) covers deployment options, readiness checks, replay, monitoring, and launch gates.
- [EVM Escrow and Crypto Marketplace Research](docs/evm-escrow-research.md) summarizes arXiv papers on smart-contract escrow, crypto marketplace payments, arbitration, privacy, and contract security.
- [Rust Implementation](docs/rust-implementation.md) describes the workspace architecture, crates, runtime flow, storage, config, tests, and operational scope.
- [TODO](TODO.md) tracks near-term product and implementation follow-ups.

## Stack

- Runtime: [Rust](https://www.rust-lang.org/), [Tokio](https://tokio.rs/), [Axum](https://docs.rs/axum/latest/axum/), and [SQLx](https://github.com/launchbadge/sqlx).
- Federation: [Matrix](https://matrix.org/) Application Service events on [Element Synapse](https://github.com/element-hq/synapse).
- Storage: [PostgreSQL](https://www.postgresql.org/) for server deployments; [SQLite](https://www.sqlite.org/) and in-memory stores for local/dev/test flows.
- Local infrastructure: [Docker](https://www.docker.com/) Compose, [nginx](https://nginx.org/) TLS federation proxies, and generated local CA certificates for E2E.
- UI: static HTML, vanilla JavaScript, committed CSS, and a small Vite/viem build that emits the committed `app.bundle.js`. There is no React or Tailwind runtime.
- Dev assets: product images were generated through [Replicate](https://replicate.com/) and checked in as compressed static PNG/JPEG assets.

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
npm run test:ui-wallet
npm run build:ui
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

Run local OIDC SSO with Keycloak:

```bash
docker compose up -d keycloak
MORPHEUS_OIDC_CLIENT_SECRET=morpheus-local-secret \
MORPHEUS_SESSION_SECRET=dev-session-secret \
cargo run -p morpheus-server -- --config config/local-oidc.toml
```

The local realm is imported from `config/oidc/keycloak/morpheus-realm.json`.
Keycloak listens on `http://127.0.0.1:18090`; the dev user is
`morpheus-admin` / `morpheus-password` and has `admin`, `seller`, and `buyer`
roles plus local seller/customer actor claims.

## CLI

The CLI is JSON-first and role-token based. Browser UI deployments can use native OIDC SSO sessions instead of browser-entered role tokens; CLI/dev automation can continue to use scoped role bearer tokens.

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

OIDC mode is configured under `[auth]` with `mode = "oidc"` plus issuer, authorization endpoint, token endpoint, client id, client secret env, redirect URL, and session secret env. In OIDC mode the UI uses an HttpOnly `morpheus_session` cookie; browser bearer-token fields are hidden. Matrix Application Service `homeserver_token` and `appservice_token` remain service secrets and are not replaced by user SSO.

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

Local UI URLs after the stack is running:

```text
books.example    http://127.0.0.1:18081/ui/admin   /ui/seller   /ui/buyer
cases.example    http://127.0.0.1:18082/ui/admin   /ui/seller   /ui/buyer
fashion.example  http://127.0.0.1:18083/ui/admin   /ui/seller   /ui/buyer
```

Demo tokens are `admin-token`, `seller-token`, and `buyer-token`.

The seller UI publishes a listing with one `Publish listing` action. It activates the seller, saves the product, publishes the offer, and then waits for projection catch-up. Product image upload is optional; uploaded covers are compressed locally before being included in product metadata. Seller order lifecycle actions are shown only inside each order card, so the visible next action always belongs to that specific order.

Stop the stack:

```bash
make e2e-three-synapse-down
```

## EVM Escrow E2E

Run the local escrow flow:

```bash
make e2e-evm-escrow
```

Required tools:

- Foundry: `anvil`, `cast`
- Moccasin: `mox`
- Docker Compose
- Node/npm for the viem UI bundle

The runner starts Anvil, tests/deploys the Vyper contracts, starts local Postgres,
launches `morpheus-server` with EVM escrow enabled, submits the Morpheus order and
payment intent flow, sends token/escrow transactions with Cast, and waits for the
embedded watcher to project authorized and captured payment states.

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

## License

Morpheus is licensed under the [MIT License](LICENSE).
