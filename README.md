# Federated Marketplace Protocol

Rust protocol implementation, conformance suite, and server skeleton for `io.marketplace` v0.1, a strict federated digital marketplace protocol over Matrix.

## Current Scope

This workspace validates protocol events, catalog sync, order-room replay, federation policy, and conformance vectors. It also includes the first runnable server milestone: protocol/core crates, in-memory event store, Synapse-compatible Application Service transaction endpoint, CLI config validation, migrations, and local Docker Compose scaffolding.

For order-room replay validation, use `morpheus_core::validate_order_room_timeline` for Matrix envelopes and room authority, or `morpheus_core::validate_order_sequence` for payload-aware lifecycle replay. They enforce `customer.bound` before `order.created`, locked order terms, payment intent/capture/refund references, entitlement references, and dispute references. `OrderTransitionGraph` is intentionally only the capture-policy-agnostic transition graph and is not sufficient by itself for strict protocol acceptance.

Matrix `event_id` is the homeserver-assigned immutable event id. Marketplace content uses an independent `content.protocol_event_id` with `evt:<instance_id>:<local_id>` grammar; validators do not require it to match Matrix `event_id`.

Retention, security, indexing, compatibility, and privacy validators are exported policy validators. They are advisory unless a caller wires them into a strict validation context.

## Strict API Entry Points

- `morpheus_protocol::validate_marketplace_event(event, context)` validates Matrix event envelope semantics, room profile routing, unknown event handling, redaction rejection, sender/issuer binding, and known event bodies.
- `morpheus_core::validate_catalog_snapshot(snapshot, expected_hash)` validates canonical snapshot hashes.
- `morpheus_core::replay_catalog_timeline(instance_id, snapshot, events)` applies snapshot plus delta events with dedupe and revision protection.
- `morpheus_core::validate_order_room_timeline(events, context)` validates required members, event authorities, Matrix event envelopes, and payload-aware order replay.
- `morpheus_core::validate_allowlist_policy(policy, now_epoch_ms)` validates local trust policy metadata.
- `morpheus_conformance::ConformanceRunner` runs the 24 required v0.1 conformance vectors with stable results.

Low-level schemas, ID parsers, `OrderStateMachine`, room-profile helpers, and individual policy validators remain exported as building blocks.

## Coverage Policy

Protocol confidence is measured by contract coverage first, line coverage second.

- Every required conformance vector and every migrated TypeScript parity scenario must have a Rust test with stable accept/reject status and error code.
- Behavioral coverage must exercise the protocol surface: envelope, catalog, order lifecycle, payments, entitlements, disputes, arbitration, privacy/security, and Application Service ingest.
- Line coverage is enforced for protocol/core/matrix/conformance crates with a practical `98%` gate. Do not chase formatting-only or mechanically unreachable lines when behavioral and conformance contracts are already covered.
- Do not weaken or remove conformance vectors to satisfy line coverage. If protocol behavior changes, update the spec note, conformance vector, and Rust behavioral test together.

## Commands

```bash
brew install rust
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo nextest run --workspace
LLVM_COV=/opt/homebrew/opt/llvm/bin/llvm-cov LLVM_PROFDATA=/opt/homebrew/opt/llvm/bin/llvm-profdata PATH="$HOME/.cargo/bin:$PATH" cargo llvm-cov --workspace --exclude morpheus-cli --exclude morpheus-server --exclude morpheus-store --fail-under-lines 98
make check
```

Local server bootstrap:

```bash
cargo run -p morpheus-cli -- config validate --config config/local.toml
cargo run -p morpheus-cli -- synapse registration --config config/local.toml --out .local/synapse/morpheus-registration.yaml
docker compose up -d postgres synapse
```

## Documents

- Spec: `docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md`
- Plan: `docs/superpowers/plans/2026-05-04-federated-marketplace-reference-validator.md`
