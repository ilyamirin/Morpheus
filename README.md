# Federated Marketplace Protocol

Reference validator and conformance suite for `io.marketplace` v0.1, a strict federated digital marketplace protocol over Matrix.

## Current Scope

This package validates protocol events, catalog sync, order-room replay, federation policy, and conformance vectors. It does not run a Matrix Application Service, homeserver, database, HTTP server, or federated search service.

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

## Documents

- Spec: `docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md`
- Plan: `docs/superpowers/plans/2026-05-04-federated-marketplace-reference-validator.md`
