# Full Morpheus Server Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Rust server fully implement Morpheus `io.marketplace` v0.1 ingest, validation, storage, projection, replay, admin, and behavioral coverage.

**Architecture:** Keep `morpheus-protocol` and `morpheus-core` as pure validation/state-machine crates. Add a real projection layer in `morpheus-store`, have `morpheus-server` orchestrate Matrix AS transactions atomically, and drive all behavior through unit tests plus HTTP behavioral tests. Server/storage/CLI must wrap core logic rather than duplicating protocol rules.

**Tech Stack:** Rust 2024, Axum, SQLx Postgres/SQLite, Tokio, serde JSON/TOML, cargo-nextest, cargo-llvm-cov.

---

## Current Assessment

The Rust protocol/core parity layer is substantially implemented, but the server does not yet fully implement the protocol.

Current strengths:
- `morpheus-protocol` validates envelope, typed bodies, IDs, room profile allowlists, privacy/security helpers, compatibility helpers, and all known event types.
- `morpheus-core` validates catalog snapshots/deltas, allowlist policy, order state graph, payment/entitlement/dispute/arbitration flows, authority, and mock payment adapter behavior.
- `morpheus-conformance` exposes 24 required vectors.
- TypeScript/npm has been removed and Rust tests are the active contract.

Current gaps:
- `morpheus-server` only validates `validate_event_envelope(raw)` without room context, allowlist, authority, catalog/order replay, or projection.
- `morpheus-store` only has `InMemoryEventStore` with two write methods; no SQLx Postgres/SQLite implementation despite migrations existing.
- `marketplace_events`, catalog projections, order projections, payments, entitlements, disputes, arbitration rulings, config revisions, allowlist entries, and projection errors are not written by runtime code.
- AppService idempotency is implemented before per-event persistence, but the server does not guarantee one DB transaction for transaction + raw events + projections.
- Admin endpoints are placeholders.
- CLI `db migrate` and `catalog rebuild` are placeholders.
- Behavioral tests only check token auth and empty transaction acceptance; they do not cover catalog/order lifecycle, invalid events, duplicate transactions, replay, rebuild, or admin auth semantics.
- Coverage gate currently proves broad Rust parity but not full server behavior.

Definition of complete for this plan:
- Every accepted marketplace event is stored as raw Matrix event and normalized marketplace event.
- Every invalid marketplace event is retained with stable rejection code and no projection side effects.
- Catalog room events rebuild `CatalogIndex` and persist sellers/products/offers/snapshots/tombstones/inventory.
- Order room events rebuild order state and persist orders/order_events/payments/entitlements/disputes/rulings.
- Server validates protocol envelope plus room profile, allowlist, authority, order-room reuse, sender namespace, privacy/security, idempotency, and replay context.
- Postgres and SQLite stores implement the same trait behavior and pass the same behavioral suite.
- Admin endpoints return real config/allowlist state and trigger deterministic rebuild/replay.
- Tests cover all 24 protocol event types, all required vectors, all specified invalid classes, AS idempotency, and end-to-end server ingest behavior.

## File Structure

- Modify `crates/morpheus-store/src/lib.rs`
  - Split store responsibilities into records, traits, in-memory store, and SQLx store modules if the file becomes too large.
  - Own transactional persistence, raw events, normalized marketplace events, projections, replay reads, idempotency, and projection errors.
- Create `crates/morpheus-store/tests/store_behavior.rs`
  - Shared store behavior tests for in-memory and SQLite.
- Create `crates/morpheus-store/tests/sqlite_store.rs`
  - SQLite migration and SQLx implementation tests.
- Modify `crates/morpheus-server/src/lib.rs`
  - Replace placeholder transaction loop with protocol-aware ingest pipeline.
  - Add admin/readiness behavior backed by store.
- Create `crates/morpheus-server/tests/appservice_behavior.rs`
  - HTTP behavioral tests for AS ingest, duplicate transaction behavior, valid projections, invalid retention.
- Create `crates/morpheus-server/tests/admin_behavior.rs`
  - Admin auth/config/allowlist/rebuild/replay tests.
- Modify `crates/morpheus-matrix/src/lib.rs`
  - Add Matrix transaction parsing helpers, sender namespace checks, event id extraction, and registration validation helpers.
- Modify `crates/morpheus-cli/src/main.rs`
  - Implement real config load shared with server, `db migrate`, `catalog rebuild`, `conformance run`, and registration generation.
- Modify `migrations/postgres/0001_initial.sql` and `migrations/sqlite/0001_initial.sql`
  - Add missing fields needed by projection semantics and indexes needed by replay.
- Create `crates/morpheus-core/tests/scenario_matrix.rs`
  - Behavioral matrix for all catalog/order/payment/entitlement/dispute/arbitration scenarios.
- Create `crates/morpheus-protocol/tests/server_contract_vectors.rs`
  - Wire JSON fixtures consumed by server behavioral tests.
- Modify `Makefile`
  - Add server/store behavioral gates.

## Task 1: Store Contract Expansion

**Files:**
- Modify: `crates/morpheus-store/src/lib.rs`
- Test: `crates/morpheus-store/tests/store_behavior.rs`

- [ ] **Step 1: Write failing in-memory store behavior tests**

Add tests that require the store to expose raw event reads, accepted marketplace event writes, projection error writes, and idempotent AS behavior:

```rust
#[tokio::test]
async fn store_records_raw_marketplace_and_projection_error() {
    let store = morpheus_store::InMemoryEventStore::default();
    let raw = morpheus_store::RawMatrixEventRecord {
        event_id: "$e1".into(),
        room_id: "!catalog:shop.example".into(),
        sender: "@market:shop.example".into(),
        event_type: "io.marketplace.offer.upserted".into(),
        origin_server_ts: 1,
        raw_json: serde_json::json!({"event_id": "$e1"}),
        validation_status: "accepted".into(),
        validation_code: None,
    };
    store.record_raw_event(raw.clone()).await.unwrap();
    assert_eq!(store.raw_event("$e1").await.unwrap().unwrap().event_id, "$e1");

    store.record_projection_error(morpheus_store::ProjectionErrorRecord {
        matrix_event_id: Some("$e1".into()),
        code: morpheus_protocol::ValidationCode::CatalogReferenceMismatch,
        message: "bad catalog".into(),
        details: serde_json::json!({"field": "offer_id"}),
    }).await.unwrap();
    assert_eq!(store.projection_errors().await.unwrap().len(), 1);
}
```

Run:

```bash
cargo test -p morpheus-store --test store_behavior
```

Expected: fail because `raw_event`, `record_projection_error`, and `projection_errors` do not exist.

- [ ] **Step 2: Add store methods and records**

Add these public records and trait methods:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceEventRecord {
    pub marketplace_event_id: String,
    pub matrix_event_id: String,
    pub protocol_version: String,
    pub issuer_instance: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub body: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionErrorRecord {
    pub matrix_event_id: Option<String>,
    pub code: ValidationCode,
    pub message: String,
    pub details: Value,
}

#[async_trait]
pub trait EventStore: Clone + Send + Sync + 'static {
    async fn record_appservice_transaction(&self, transaction: AppServiceTransactionRecord) -> Result<(), ValidationError>;
    async fn record_raw_event(&self, event: RawMatrixEventRecord) -> Result<(), ValidationError>;
    async fn raw_event(&self, event_id: &str) -> Result<Option<RawMatrixEventRecord>, ValidationError>;
    async fn record_marketplace_event(&self, event: MarketplaceEventRecord) -> Result<(), ValidationError>;
    async fn marketplace_events_by_room(&self, room_id: &str) -> Result<Vec<MarketplaceEventRecord>, ValidationError>;
    async fn record_projection_error(&self, error: ProjectionErrorRecord) -> Result<(), ValidationError>;
    async fn projection_errors(&self) -> Result<Vec<ProjectionErrorRecord>, ValidationError>;
}
```

- [ ] **Step 3: Implement in-memory behavior**

Extend `InMemoryState` with:

```rust
marketplace_events: HashMap<String, MarketplaceEventRecord>,
projection_errors: Vec<ProjectionErrorRecord>,
raw_event_rooms: HashMap<String, String>,
```

Implement reads and writes without changing existing idempotency behavior.

- [ ] **Step 4: Verify**

Run:

```bash
cargo test -p morpheus-store
```

Expected: all store tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/morpheus-store
git commit -m "feat(store): expand event store contract"
```

## Task 2: Projection Write Contract

**Files:**
- Modify: `crates/morpheus-store/src/lib.rs`
- Test: `crates/morpheus-store/tests/store_behavior.rs`

- [ ] **Step 1: Write failing projection tests**

Add tests that require catalog/order projection records:

```rust
#[tokio::test]
async fn store_upserts_catalog_and_order_projection_records() {
    let store = morpheus_store::InMemoryEventStore::default();
    store.upsert_catalog_seller("seller:shop.example:01JSELLER", "shop.example", "active", serde_json::json!({"status": "active"})).await.unwrap();
    store.upsert_catalog_product("prod:shop.example:01JPROD", "seller:shop.example:01JSELLER", 1, serde_json::json!({"revision": 1})).await.unwrap();
    store.upsert_catalog_offer("offer:shop.example:01JOFFER", "prod:shop.example:01JPROD", "seller:shop.example:01JSELLER", 1, serde_json::json!({"amount": "100.00"}), "booking_slot", serde_json::json!({"revision": 1})).await.unwrap();
    assert_eq!(store.catalog_offers().await.unwrap().len(), 1);

    store.upsert_order("ord:customer.example:01JORDER", "!order:customer.example", "customer:customer.example:01JCUST", "seller:shop.example:01JSELLER", "offer:shop.example:01JOFFER", "created", serde_json::json!({"order_id": "ord:customer.example:01JORDER"})).await.unwrap();
    assert_eq!(store.order("ord:customer.example:01JORDER").await.unwrap().unwrap().status, "created");
}
```

Expected: fail until projection methods exist.

- [ ] **Step 2: Add projection records**

Add `CatalogSellerRecord`, `CatalogProductRecord`, `CatalogOfferProjectionRecord`, `OrderProjectionRecord`, `PaymentProjectionRecord`, `EntitlementProjectionRecord`, `DisputeProjectionRecord`, and `ArbitrationRulingProjectionRecord`.

- [ ] **Step 3: Add trait methods**

Add explicit upsert/list/get methods for catalog and order projections. Keep methods narrow and typed instead of accepting generic SQL strings.

- [ ] **Step 4: Implement in-memory projections**

Store projections in maps keyed by protocol ids.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p morpheus-store
git add crates/morpheus-store
git commit -m "feat(store): add projection contract"
```

## Task 3: Projection Engine

**Files:**
- Create: `crates/morpheus-server/src/projection.rs`
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/appservice_behavior.rs`

- [ ] **Step 1: Write failing behavioral tests**

Add one catalog projection test and one order lifecycle projection test through the HTTP AS endpoint:

```rust
#[tokio::test]
async fn valid_catalog_offer_transaction_persists_catalog_projection() {
    let store = morpheus_store::InMemoryEventStore::default();
    let app = morpheus_server::build_router(test_config(), store.clone());
    let response = send_transaction(&app, "txn-catalog-1", vec![
        fixture_seller_announced(),
        fixture_product_upserted(),
        fixture_offer_upserted(),
    ]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(store.catalog_offers().await.unwrap().len(), 1);
}

#[tokio::test]
async fn valid_order_lifecycle_transaction_persists_order_projection() {
    let store = morpheus_store::InMemoryEventStore::default();
    let app = morpheus_server::build_router(test_config(), store.clone());
    let response = send_transaction(&app, "txn-order-1", fixture_order_lifecycle()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(store.order("ord:customer.example:01JORDER").await.unwrap().unwrap().status, "completed");
}
```

Expected: fail because server does not project.

- [ ] **Step 2: Add projection engine API**

Create:

```rust
pub async fn project_validated_event<S: EventStore>(
    store: &S,
    validated: morpheus_protocol::ValidatedMarketplaceEvent,
    raw: &serde_json::Value,
    context: &ProjectionContext,
) -> Result<(), morpheus_protocol::ValidationError>
```

`ProjectionContext` must include instance config, allowlist policy, catalog room id, order authorities resolver, and sender namespace policy.

- [ ] **Step 3: Implement catalog event projection**

Map accepted catalog events into `CatalogIndex`-compatible records and store projection rows:
- `catalog.snapshot.published` -> `catalog_snapshots`
- `actor.seller.announced/suspended` -> `catalog_sellers`
- `product.upserted/withdrawn` -> `catalog_products` or tombstone
- `offer.upserted/withdrawn` -> `catalog_offers` or tombstone
- `inventory.updated` -> offer body metadata only; canonical terms remain from offer.

- [ ] **Step 4: Implement order event projection**

Map order events into:
- `order_rooms`
- `orders`
- `order_events`
- `payments`
- `entitlements`
- `disputes`
- `arbitration_rulings`

Derive order status from `morpheus_core::validate_order_sequence`.

- [ ] **Step 5: Store projection failures**

When projection fails after raw event persistence:
- keep `raw_matrix_events.validation_status = "rejected"` or add a projection status field;
- write `projection_errors`;
- do not mutate projection tables for that event.

- [ ] **Step 6: Verify and commit**

```bash
cargo test -p morpheus-server --test appservice_behavior
git add crates/morpheus-server crates/morpheus-store
git commit -m "feat(server): project accepted marketplace events"
```

## Task 4: Contextual Server Validation

**Files:**
- Modify: `crates/morpheus-server/src/lib.rs`
- Modify: `crates/morpheus-server/src/projection.rs`
- Test: `crates/morpheus-server/tests/appservice_behavior.rs`

- [ ] **Step 1: Write failing invalid-behavior tests**

Cover:
- catalog event in order room rejected with `RoomProfileViolation`;
- order event in catalog room rejected with `RoomProfileViolation`;
- non-allowlisted seller instance rejected;
- unauthorized payment capture sender rejected;
- order room reuse rejected;
- bearer URL entitlement rejected and retained raw.

Each test must assert:
- HTTP transaction returns `200 OK` for accepted AS delivery unless transaction id conflicts;
- raw event exists;
- invalid event has stable code;
- projection table did not change.

- [ ] **Step 2: Add room profile resolver**

Implement:

```rust
fn room_profile_for_event(config: &RuntimeConfig, raw: &Value) -> Option<RoomProfile>
```

Catalog room is `config.instance.catalog_room_id`; any room with order-bound events is `RoomProfile::Order`.

- [ ] **Step 3: Add authority resolver**

Resolve seller/customer/arbiter/payment AS users from event body and config. Initial deterministic mapping:

```text
@market:<instance_id>
@payment:<seller_instance>
@market:<arbiter_instance>
```

Use `morpheus_core::assert_event_authority`.

- [ ] **Step 4: Add allowlist checks**

Convert TOML allowlist into `morpheus_core::AllowlistPolicy` at startup and enforce:
- catalog indexing requires `catalog` + `indexing`;
- orders require seller `orders`;
- arbitration requires arbiter `arbitration`;
- payment adapter must be configured.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p morpheus-server --test appservice_behavior
git add crates/morpheus-server
git commit -m "feat(server): enforce contextual protocol validation"
```

## Task 5: SQLx Store Implementations

**Files:**
- Create: `crates/morpheus-store/src/sqlx_store.rs`
- Modify: `crates/morpheus-store/src/lib.rs`
- Modify: `migrations/postgres/0001_initial.sql`
- Modify: `migrations/sqlite/0001_initial.sql`
- Test: `crates/morpheus-store/tests/sqlite_store.rs`

- [ ] **Step 1: Write failing SQLite store tests**

Use `SqlitePool::connect("sqlite::memory:")`, run migrations, and reuse the same behavior checks as `InMemoryEventStore`.

- [ ] **Step 2: Implement `SqliteEventStore`**

Implement the expanded `EventStore` trait using SQLx queries. Store JSON as text in SQLite using `serde_json::to_string`.

- [ ] **Step 3: Implement `PostgresEventStore`**

Implement the same trait with JSONB and transaction support. Use runtime SQLx queries instead of compile-time `query!` unless `DATABASE_URL` is guaranteed in CI.

- [ ] **Step 4: Add store transaction wrapper**

Expose:

```rust
async fn record_appservice_batch(&self, batch: AppServiceBatch) -> Result<(), ValidationError>
```

For SQLx stores this must run in one DB transaction. For in-memory store it must lock once.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p morpheus-store
git add crates/morpheus-store migrations
git commit -m "feat(store): add sqlx event stores"
```

## Task 6: Admin API Completion

**Files:**
- Modify: `crates/morpheus-server/src/lib.rs`
- Test: `crates/morpheus-server/tests/admin_behavior.rs`

- [ ] **Step 1: Write failing admin tests**

Cover:
- `GET /admin/config` returns instance/appservice/database/admin summary without tokens;
- `GET /admin/allowlist` returns configured allowlist;
- `POST /admin/catalog/rebuild` rebuilds catalog projections from accepted raw events;
- `POST /admin/orders/{order_id}/replay` rebuilds one order projection from accepted raw events;
- all admin routes reject missing/wrong bearer token.

- [ ] **Step 2: Implement config response**

Return redacted tokens:

```json
{
  "instance": {"instance_id": "shop.example", "protocol_versions": ["0.1"]},
  "appservice": {"homeserver_url": "http://localhost:8008", "sender_localpart": "market"},
  "database": {"url": "redacted"},
  "admin": {"bind": "127.0.0.1:8080"}
}
```

- [ ] **Step 3: Implement rebuild/replay**

Use store reads of accepted raw events, run the same projection engine, clear affected projections, and write deterministic response:

```json
{"status":"rebuilt","accepted":N,"rejected":M}
```

- [ ] **Step 4: Verify and commit**

```bash
cargo test -p morpheus-server --test admin_behavior
git add crates/morpheus-server
git commit -m "feat(server): complete admin protocol operations"
```

## Task 7: CLI Runtime Completion

**Files:**
- Modify: `crates/morpheus-cli/src/main.rs`
- Test: `crates/morpheus-cli/tests/config_cli.rs`

- [ ] **Step 1: Write failing CLI tests**

Cover:
- config validation rejects invalid allowlist status/capabilities;
- registration file contains correct namespace regex;
- `conformance run` exits non-zero if any vector fails;
- `snapshot verify` succeeds/fails by hash;
- `db migrate --database-url sqlite::memory:` applies migrations;
- `catalog rebuild --database-url ...` calls store rebuild path.

- [ ] **Step 2: Extract shared config**

Move config structs into `morpheus-server` or a small config module so CLI and server use identical parsing.

- [ ] **Step 3: Implement DB migrate**

Support:

```bash
cargo run -p morpheus-cli -- db migrate --database-url sqlite://.local/morpheus.db
cargo run -p morpheus-cli -- db migrate --database-url postgres://...
```

- [ ] **Step 4: Implement catalog rebuild command**

Call the same rebuild function used by admin API.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p morpheus-cli
git add crates/morpheus-cli crates/morpheus-server
git commit -m "feat(cli): wire runtime operations"
```

## Task 8: Full Scenario Matrix

**Files:**
- Create: `crates/morpheus-core/tests/scenario_matrix.rs`
- Create: `crates/morpheus-server/tests/protocol_behavior_matrix.rs`
- Modify: `crates/morpheus-conformance/src/lib.rs`

- [ ] **Step 1: Build explicit scenario list**

The behavioral matrix must include all event types:
- instance/catalog profile;
- seller announced/suspended;
- snapshot published;
- product/offer upserted/withdrawn;
- inventory updated;
- customer bound;
- order created/accepted/rejected/cancelled/completed;
- payment intent/authorized/failed/cancelled/captured/refund requested/refunded/chargeback opened;
- entitlement granted/activated/completed/revoked/expired;
- dispute opened/evidence/ruling/closed.

- [ ] **Step 2: Add rejection scenario list**

The rejection matrix must include:
- unsupported version;
- invalid id;
- non-UTC timestamp;
- missing required field;
- unknown critical extension;
- room profile violation;
- unauthorized sender;
- allowlist rejection;
- revision rollback;
- snapshot hash mismatch;
- order room reuse;
- payment terms mismatch;
- entitlement secret/bearer URL;
- duplicate protocol event id with different body;
- appservice transaction id conflict.

- [ ] **Step 3: Implement core matrix tests**

Core tests assert final `ValidationCode` and state/projection decisions.

- [ ] **Step 4: Implement server matrix tests**

Server tests send HTTP AS transactions and assert raw retention + projection side effects.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p morpheus-core --test scenario_matrix
cargo test -p morpheus-server --test protocol_behavior_matrix
git add crates/morpheus-core crates/morpheus-server crates/morpheus-conformance
git commit -m "test: cover full Morpheus behavior matrix"
```

## Task 9: Coverage and Quality Gates

**Files:**
- Modify: `Makefile`
- Modify: `README.md`

- [ ] **Step 1: Decide coverage semantics**

Use two gates:
- scenario coverage: every protocol event and every rejection class appears in the matrix;
- line coverage: keep `cargo llvm-cov` enforced, with realistic excludes only for binaries and generated migrations.

The command must remain:

```bash
PATH=/Users/ilyagmirin/.cargo/bin:$PATH LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata cargo llvm-cov --workspace --exclude morpheus-cli --exclude morpheus-server --exclude morpheus-store --fail-under-lines 100
```

If literal 100% still reports uncovered formatting-only lines, do not hide them with broad source excludes; instead document exact residual lines or refactor functions until the gate passes.

- [ ] **Step 2: Add Make targets**

Add:

```make
behavioral:
	cargo test -p morpheus-core --test scenario_matrix
	cargo test -p morpheus-server --test protocol_behavior_matrix
	cargo test -p morpheus-server --test appservice_behavior
	cargo test -p morpheus-server --test admin_behavior
```

Make `check` depend on `rust-check behavioral coverage-protocol`.

- [ ] **Step 3: Final full gate**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo nextest run --workspace
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo deny check
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo audit
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo machete
make check
```

- [ ] **Step 4: Commit**

```bash
git add Makefile README.md
git commit -m "chore: enforce full protocol gates"
```

## Task 10: Local E2E Smoke

**Files:**
- Modify: `docker-compose.yml`
- Create: `scripts/smoke-morpheus.sh`
- Modify: `README.md`

- [ ] **Step 1: Add smoke script**

Script must:
- validate config;
- generate Synapse registration;
- start Postgres and Synapse;
- run migrations;
- start server;
- send catalog events;
- send order lifecycle events;
- verify DB projections;
- send invalid payment capture and verify raw rejection.

- [ ] **Step 2: Verify locally**

Run:

```bash
./scripts/smoke-morpheus.sh
```

Expected final line:

```text
morpheus smoke ok
```

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml scripts/smoke-morpheus.sh README.md
git commit -m "test: add local Morpheus smoke flow"
```

## Final Acceptance Gate

Complete means all of this is true:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo nextest run --workspace
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo deny check
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo audit
PATH=/Users/ilyagmirin/.cargo/bin:$PATH cargo machete
make check
find . -path './target' -prune -o \( -name '*.ts' -o -name 'package.json' -o -name 'package-lock.json' -o -name 'tsconfig.json' -o -name 'vitest.config.ts' \) -print
rg "TypeScript|typescript|Vitest|vitest|npm|package.json|tsconfig|\\.test\\.ts|Node" README.md docs Cargo.toml Makefile rust-toolchain.toml crates -n
```

Expected:
- all Rust gates pass;
- `find` prints nothing;
- `rg` prints nothing;
- server behavioral matrix covers every event type and rejection class;
- Postgres/SQLite stores pass behavior tests;
- AS transaction ingestion persists raw events and projections;
- invalid events are retained with stable error codes;
- admin rebuild/replay works;
- local smoke script ends with `morpheus smoke ok`.

## Suggested Execution Order

1. Store contract expansion.
2. Projection write contract.
3. Projection engine through in-memory store.
4. Contextual server validation.
5. SQLx stores.
6. Admin API.
7. CLI runtime.
8. Full scenario matrix.
9. Coverage/quality gates.
10. Local E2E smoke.

This order keeps tests runnable after every step and avoids mixing SQLx complexity into the first server behavior milestone.
