# Rust Implementation

The Morpheus Rust workspace is the production implementation surface for `io.marketplace` v0.1. It contains the validators, conformance vectors, Matrix Application Service ingest, projections, and operator tooling used by the project.

## Workspace Layout

```text
crates/
  morpheus-protocol      Wire protocol, IDs, envelope validation, schemas, policies
  morpheus-core          Pure catalog/order/payment/entitlement/dispute/arbitration logic
  morpheus-matrix        Matrix AS transaction helpers and Synapse registration generation
  morpheus-store         EventStore trait, in-memory store, SQLite store, Postgres store, SQL migrations
  morpheus-server        Axum runtime routes and projection pipeline
  morpheus-cli           Operator CLI
  morpheus-conformance   Required protocol vectors and runner
```

The core boundary is intentional: `morpheus-core` contains pure validation and state transition code. It does not depend on Axum, SQLx, Tokio networking, Matrix SDKs, or Postgres. Matrix parsing, HTTP, persistence, and deployment concerns stay in outer crates.

## Crates

### `morpheus-protocol`

Responsibilities:

- protocol constants and event type allowlists;
- marketplace object ID parsing and instance binding;
- Matrix event envelope validation;
- `protocol_event_id` validation independent from Matrix `event_id`;
- UTC timestamp validation;
- sender/issuer binding;
- room profile checks for catalog and order rooms;
- canonical JSON and `sha256:<hex>` hash checks;
- known schema validation for catalog and order event bodies;
- privacy, security, retention, compatibility, and Application Service policy helpers;
- stable `ValidationCode` values and retryable/terminal disposition.

Primary APIs:

```rust
morpheus_protocol::validate_event_envelope(raw_event)
morpheus_protocol::validate_marketplace_event(raw_event, context)
morpheus_protocol::canonical_json(value)
morpheus_protocol::assert_sha256_matches(value, expected)
```

### `morpheus-core`

Responsibilities:

- catalog snapshots and delta replay;
- seller/product/offer index rules;
- allowlist decisions and allowlist metadata validation;
- order creation validation against catalog and customer binding;
- order state machine and payload-aware order sequence validation;
- sender authority for seller, customer, payment AS, and arbiter users;
- payment capture, refund, chargeback, entitlement, dispute, and arbitration rules.

Primary APIs:

```rust
morpheus_core::validate_catalog_snapshot(document, expected_hash)
morpheus_core::replay_catalog_timeline(instance_id, snapshot, deltas)
morpheus_core::validate_order_created(order, catalog, allowlist, customer)
morpheus_core::validate_order_sequence(events)
morpheus_core::validate_order_room_timeline(events, context)
morpheus_core::assert_event_authority(event_type, sender, authorities)
```

### `morpheus-matrix`

Responsibilities:

- Synapse Application Service transaction payload shape;
- transaction event ID extraction and validation;
- Application Service sender namespace checks;
- Synapse registration YAML data model generation.

Primary APIs:

```rust
morpheus_matrix::AppServiceTransaction
morpheus_matrix::validate_transaction_event_ids(transaction)
morpheus_matrix::validate_application_service_sender(sender, context)
morpheus_matrix::generate_synapse_registration(...)
```

### `morpheus-store`

Responsibilities:

- async `EventStore` trait for runtime persistence;
- raw Matrix event retention;
- accepted marketplace event retention;
- projection error retention;
- catalog, order, payment, entitlement, dispute, and arbitration projections;
- AppService transaction idempotency;
- in-memory implementation for unit and server tests;
- SQLite implementation for local/conformance usage;
- Postgres implementation for runtime containers;
- SQL migration text for SQLite and Postgres.

Idempotency policy:

- same `txn_id` with same Matrix event IDs is accepted;
- same `txn_id` with different Matrix event IDs is rejected;
- raw events are retained with accepted/rejected validation state.

### `morpheus-server`

Responsibilities:

- Axum router construction through `build_router(config, store)`;
- `PUT /_matrix/app/v1/transactions/{txn_id}`;
- homeserver token validation on the Matrix AS endpoint;
- Matrix transaction event ID validation;
- protocol envelope validation;
- context validation against previously accepted order events;
- raw event recording before or alongside projection;
- accepted marketplace event projection;
- projection error recording;
- health, readiness, metrics, and admin routes.

HTTP routes:

```text
GET  /healthz
GET  /readyz
GET  /metrics
PUT  /_matrix/app/v1/transactions/{txn_id}
GET  /admin/config
GET  /admin/allowlist
POST /admin/catalog/rebuild
POST /admin/orders/{order_id}/replay
```

Admin routes require `Authorization: Bearer <admin-token>`.

The server crate exposes both a reusable router and a standalone binary. The binary runs as:

```bash
MORPHEUS_ADMIN_TOKEN=admin-token cargo run -p morpheus-server -- --config config/local.toml
```

It loads TOML config, reads the admin bearer token from the configured env var, runs Postgres migrations, constructs `PostgresEventStore`, and binds the configured address.

### `morpheus-cli`

Commands:

```text
morpheus config validate
morpheus synapse registration
morpheus conformance run
morpheus snapshot verify
morpheus db migrate
morpheus catalog rebuild
```

Examples:

```bash
cargo run -p morpheus-cli -- config validate --config config/local.toml
cargo run -p morpheus-cli -- synapse registration --config config/local.toml --out .local/synapse/morpheus-registration.yaml
cargo run -p morpheus-cli -- conformance run
cargo run -p morpheus-cli -- db migrate --database-url sqlite:.local/morpheus.db --database-kind sqlite
```

### `morpheus-conformance`

Responsibilities:

- required v0.1 vector definitions;
- stable expected accept/reject status;
- stable expected error codes;
- JSON-like runner results consumed by CLI and tests.

Run:

```bash
cargo test -p morpheus-conformance
cargo run -p morpheus-cli -- conformance run
```

## Runtime Flow

The Application Service ingest path is:

1. Synapse sends `PUT /_matrix/app/v1/transactions/{txn_id}?access_token=<hs-token>`.
2. `morpheus-server` validates the homeserver token.
3. `morpheus-matrix` validates that every event has a Matrix `event_id`.
4. `morpheus-store` records the AppService transaction idempotency key.
5. For each Matrix event:
   - `morpheus-protocol` validates the Morpheus envelope and body;
   - invalid events are stored as raw rejected events with a `ValidationCode`;
   - accepted events are checked against order context where needed;
   - accepted events are stored as raw events and marketplace events;
   - projection code updates catalog/order/payment/entitlement/dispute/arbitration views.
6. Duplicate valid transactions are accepted when their event IDs match the first submission.
7. Conflicting transaction replays return conflict.

## Storage Model

The store layer keeps replayability ahead of convenience projections.

Important record families:

- `appservice_transactions`
- `raw_matrix_events`
- `marketplace_events`
- `projection_errors`
- catalog sellers/products/offers/tombstones
- orders and order events
- payments
- entitlements
- disputes
- arbitration rulings

Raw Matrix events are retained even when invalid. Accepted marketplace events are projection input. Projections are rebuildable from accepted events and projection errors capture events that could not be applied.

## Configuration

Local bootstrap config lives in `config/local.toml`.

Main sections:

```toml
[instance]
[appservice]
[database]
[admin]
[[allowlist.instances]]
```

Validate it with:

```bash
cargo run -p morpheus-cli -- config validate --config config/local.toml
```

Generate Synapse registration:

```bash
cargo run -p morpheus-cli -- synapse registration --config config/local.toml --out .local/synapse/morpheus-registration.yaml
```

## Local Infrastructure

Start Postgres and pgweb:

```bash
docker compose up -d postgres pgweb
```

Synapse needs a homeserver config before the Compose service can boot on a fresh checkout. One local bootstrap path is:

```bash
mkdir -p .local/synapse
docker run --rm -v "$PWD/.local/synapse:/data" -e SYNAPSE_SERVER_NAME=localhost -e SYNAPSE_REPORT_STATS=no matrixdotorg/synapse:latest generate
cargo run -p morpheus-cli -- synapse registration --config config/local.toml --out .local/synapse/morpheus-registration.yaml
```

Then add the container-visible registration path to `app_service_config_files` in `.local/synapse/homeserver.yaml`:

```yaml
app_service_config_files:
  - /data/morpheus-registration.yaml
```

Start Synapse:

```bash
docker compose up -d synapse
```

Run a Postgres migration:

```bash
cargo run -p morpheus-cli -- db migrate --database-url postgres://morpheus:morpheus@localhost:5432/morpheus --database-kind postgres
```

Run the three-instance E2E stack:

```bash
make e2e-three-synapse
make e2e-three-synapse-down
```

The E2E stack uses `config/e2e/books.toml`, `config/e2e/cases.toml`, and `config/e2e/fashion.toml`, plus `docker-compose.e2e.yml`. It starts three Synapse homeservers, three Postgres databases, three Morpheus containers, seeds demo catalogs, and verifies catalog/order/payment/entitlement projections.

Run a SQLite migration:

```bash
cargo run -p morpheus-cli -- db migrate --database-url sqlite:.local/morpheus.db --database-kind sqlite
```

## Testing And Coverage

Primary gate:

```bash
make check
```

This runs:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo llvm-cov --workspace --exclude morpheus-cli --exclude morpheus-server --exclude morpheus-store --fail-under-lines 98
```

Additional useful checks:

```bash
cargo nextest run --workspace
cargo test -p morpheus-protocol
cargo test -p morpheus-core
cargo test -p morpheus-matrix
cargo test -p morpheus-server
cargo test -p morpheus-store
cargo test -p morpheus-conformance
```

Coverage policy:

- contract and conformance coverage are more important than artificial line coverage;
- all required vectors and migrated parity scenarios must have Rust tests;
- behavioral tests must cover protocol lifecycle scenarios end to end;
- line coverage is held at a practical `98%` gate for protocol/core/matrix/conformance.

## Current Scope

Implemented now:

- protocol/core/matrix/conformance Rust parity surface;
- AppService transaction route and admin/ops routes as an Axum router and standalone binary;
- raw event retention and projection behavior;
- in-memory, SQLite, and Postgres store implementations;
- SQL migration text for SQLite and Postgres;
- CLI operator tools;
- Rust-only tests and coverage gate.

Not yet production-hardening:

- Synapse federation in E2E is local/dev only and not TLS-hardened;
- real Stripe/bank/payment provider integrations are intentionally out of v0.1 runtime scope;
- entitlement delivery providers are external and not implemented as secret/file/license delivery through Matrix.
