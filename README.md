# Federated Marketplace Protocol

Reference validator, conformance suite, and Rust server skeleton for `io.marketplace` v0.1, a strict federated digital marketplace protocol over Matrix.

## Current Scope

This package validates protocol events, catalog sync, order-room replay, federation policy, and conformance vectors. The Rust workspace adds the first runnable server milestone: protocol/core crates, in-memory event store, Synapse-compatible Application Service transaction endpoint, CLI config validation, migrations, and local Docker Compose scaffolding.

For order-room replay validation, use `validateOrderRoomTimeline` for Matrix envelopes and room authority, or `validateOrderEventSequence` for payload-aware lifecycle replay. They enforce `customer.bound` before `order.created`, locked order terms, payment intent/capture/refund references, entitlement references, and dispute references. `OrderStateMachine` is intentionally only the capture-policy-agnostic transition graph and is not sufficient by itself for strict protocol acceptance.

Matrix `event_id` is the homeserver-assigned immutable event id. Marketplace content uses an independent `content.protocol_event_id` with `evt:<instance_id>:<local_id>` grammar; validators do not require it to match Matrix `event_id`.

Retention, security, indexing, compatibility, and privacy validators are exported policy validators. They are advisory unless a caller wires them into a strict validation context.

## Strict API Entry Points

- `validateMarketplaceEvent(event, context)` validates Matrix event envelope semantics, room profile routing, unknown event handling, redaction rejection, sender/issuer binding, and known event bodies.
- `validateCatalogSnapshot(snapshot, context)` validates canonical snapshot hashes.
- `replayCatalogTimeline(events, snapshot, context)` applies snapshot plus delta events with dedupe and revision protection.
- `validateOrderRoomTimeline(events, context)` validates required members, event authorities, Matrix event envelopes, and payload-aware order replay.
- `validateAllowlistPolicy(policy, now)` validates local trust policy metadata.
- `ConformanceRunner` runs required and optional conformance vectors with stable results.

Low-level schemas, ID parsers, `OrderStateMachine`, room-profile helpers, and individual policy validators remain exported as building blocks.

## Commands

```bash
npm install
npm run check
```

Rust toolchain:

```bash
brew install rust
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
PATH="$HOME/.cargo/bin:$PATH" cargo nextest run --workspace
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
