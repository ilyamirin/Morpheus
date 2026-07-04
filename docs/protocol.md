# Morpheus Protocol

Morpheus `io.marketplace` v0.1 is a strict federated protocol for digital marketplaces over Matrix. It lets independent marketplace instances exchange catalog and order state without a central marketplace, global search index, or global trust graph.

Matrix provides authenticated federation, room history, and event transport. Morpheus defines the marketplace event namespace, identifiers, payload envelopes, room profiles, validation rules, authority rules, and conformance expectations.

## Goals

- Interoperate across independently operated marketplace instances.
- Support digital products, services, bookings, subscriptions, licenses, and external entitlements.
- Use a local allowlist trust model instead of open global discovery.
- Keep search local to each instance.
- Keep payment provider flows, digital artifacts, secrets, bearer URLs, credentials, and delivery payloads outside Matrix.
- Record enough protocol state in Matrix for replayable order validation and arbitration.

v0.1 does not define physical fulfillment, reviews, reputation, federated search, trust recommendations, trustless escrow, or storage of digital goods in Matrix.

## Namespace And Version

All standardized protocol events use the Matrix event namespace:

```text
io.marketplace.*
```

Every Morpheus event has an inner protocol envelope:

```json
{
  "protocol": "io.marketplace",
  "protocol_version": "0.1",
  "protocol_event_id": "evt:shop.example:01JABC",
  "created_at": "2026-05-04T10:00:00Z",
  "issuer": {
    "instance_id": "shop.example",
    "actor_id": "seller:shop.example:01JSELLER",
    "matrix_user_id": "@market:shop.example"
  },
  "critical": [],
  "body": {}
}
```

Strict rules:

- `protocol` must be `io.marketplace`.
- `protocol_version` must be supported by the validator.
- `created_at` must be UTC.
- `protocol_event_id` is independent from Matrix `event_id`.
- Unknown critical fields or extensions are rejected.
- Unknown non-critical fields may be preserved and ignored by validators.
- Unsupported protocol versions and downgrade attempts are rejected.

The Rust implementation exposes this layer through `morpheus_protocol::validate_event_envelope` and `morpheus_protocol::validate_marketplace_event`.

## Identifiers

Canonical protocol object IDs use:

```text
<kind>:<instance_id>:<local_id>
```

The current v0.1 kinds are:

```text
seller customer prod offer ord pay refund ent disp arbiter evt snap
```

`instance_id` is DNS-like and must contain at least one dot. `local_id` is uppercase ASCII letters, digits, `_`, or `-`, matching the draft's ULID/UUIDv7-like expectation.

Important binding rules:

- The object instance in IDs must match the issuing or referenced marketplace instance where the protocol requires it.
- `seller:*` actors are announced in catalog rooms.
- `customer:*` actors are bound only inside order rooms.
- `arbiter:*`, `pay:*`, `ent:*`, `disp:*`, and `refund:*` references must match the lifecycle state they extend.

## Room Profiles

Morpheus uses Matrix room profiles to separate public catalog data from private order state.

### Catalog Room

Each marketplace instance has one federated catalog room. It contains indexable marketplace events and must not contain order data or personal data.

Required state events:

```text
io.marketplace.instance.profile
io.marketplace.catalog.profile
```

Allowed catalog timeline events:

```text
io.marketplace.catalog.snapshot.published
io.marketplace.actor.seller.announced
io.marketplace.actor.seller.suspended
io.marketplace.product.upserted
io.marketplace.product.withdrawn
io.marketplace.offer.upserted
io.marketplace.offer.withdrawn
io.marketplace.inventory.updated
```

An indexer starts from the latest valid snapshot, verifies the canonical hash, then applies contiguous catalog deltas. Missing sequence numbers, corrupted snapshots, invalid actors, or unsupported versions require recovery from a later valid snapshot.

`io.marketplace.inventory.updated` is advisory metadata. Binding purchase terms remain in `offer.upserted`.

### Order Room

Each order has exactly one private Matrix room. A room becomes protocol-valid only after a valid `io.marketplace.actor.customer.bound` appears before a valid `io.marketplace.order.created`.

Structurally required order-room events:

```text
io.marketplace.actor.customer.bound
io.marketplace.order.created
```

Order rooms must not be reused for another `order_id`. Protocol events are plaintext to participating marketplace Application Service users so each instance can validate and replay the order. Encrypted attachments or free-form Matrix messages are outside `io.marketplace.*` validation.

## Catalog Lifecycle

Catalog state is a projection of snapshots and deltas:

1. `io.marketplace.instance.profile` declares instance metadata and supported protocol features.
2. `io.marketplace.catalog.profile` declares catalog policy.
3. `io.marketplace.catalog.snapshot.published` publishes snapshot metadata and canonical hash.
4. `io.marketplace.actor.seller.announced` activates a seller actor.
5. `io.marketplace.actor.seller.suspended` removes seller eligibility from the local catalog view.
6. `io.marketplace.product.upserted` adds or revises a product. Product media is metadata: v0.1 accepts image references in `media[]`, but delivery artifacts, license secrets, bearer URLs, private files, and credentials remain outside marketplace events.
7. `io.marketplace.product.withdrawn` removes a product and its offers.
8. `io.marketplace.offer.upserted` adds or revises an offer with price, terms, entitlement type, and payment capture policy.
9. `io.marketplace.offer.withdrawn` removes an offer.
10. `io.marketplace.inventory.updated` updates advisory availability metadata.

Validation requires active sellers, same-instance catalog references, monotonic product/offer revisions, valid product kinds, valid entitlement types, canonical snapshot hash checks, and contiguous delta replay.

Local/dev UI may publish small compressed product cover images as product media metadata. Production deployments should prefer external object storage with stable content hashes and safety policy; the protocol still forbids using marketplace events as a secret or artifact delivery channel.

Rust entry points:

- `morpheus_core::validate_catalog_snapshot`
- `morpheus_core::replay_catalog_timeline`
- `morpheus_core::CatalogIndex`

## Order Lifecycle

The happy path is:

```text
customer.bound
order.created
order.accepted
payment.intent.created
payment.authorized
payment.captured
entitlement.granted
order.completed
```

Optional paths include:

```text
order.cancelled
order.rejected
payment.failed
payment.cancelled
payment.refund.requested
payment.refunded
payment.chargeback.opened
entitlement.activated
entitlement.completed
entitlement.revoked
entitlement.expired
dispute.opened
dispute.evidence.submitted
dispute.ruling.issued
dispute.closed
```

`order.created` locks the order terms: customer, seller, offer, offer revision, catalog snapshot, quantity, price, payment adapter, capture policy, entitlement type, seller terms hash, offer terms hash, arbiter, arbitration policy, and expiration.

v0.1 only accepts `quantity = 1`. Multi-quantity carts and bundles require a later protocol version with explicit unit and total price semantics.

`order.accepted` must confirm the exact terms locked by `order.created`. Mismatched revision, terms hashes, capture policy, or arbitration version are invalid.

Rust entry points:

- `morpheus_core::validate_order_created`
- `morpheus_core::validate_order_sequence`
- `morpheus_core::validate_order_room_timeline`
- `morpheus_core::OrderStateMachine`
- `morpheus_core::OrderTransitionGraph`

`OrderTransitionGraph` is capture-policy-agnostic. Strict protocol acceptance should use `validate_order_sequence` or `validate_order_room_timeline`.

## Payments

Payment events record protocol state and external evidence. They do not execute real payments.

Payment lifecycle events:

```text
payment.intent.created
payment.authorized
payment.captured
payment.failed
payment.cancelled
payment.refund.requested
payment.refunded
payment.chargeback.opened
```

Rules:

- Payment intent must reference the order and locked payment adapter.
- Intent `capture_policy` must match `order.created`.
- Capture policy controls whether payment may be captured before or after entitlement.
- Refunds must carry a stable `refund_id`.
- Standard provider refunds must reference a captured payment.
- Escrow-style refunds may reference an authorized payment when funds are already in protocol custody.
- Refund amount and currency are constrained by captured amount, authorized escrow amount, and any dispute ruling.
- Payment provider references and receipts are evidence, not secrets.

The initial implementation is protocol-level only. Real payment provider adapters should sit behind a separate adapter trait and emit valid protocol events after provider verification.

## Entitlements

Entitlement events record delivery state without placing secrets in Matrix.

Events:

```text
entitlement.granted
entitlement.activated
entitlement.completed
entitlement.revoked
entitlement.expired
```

Rules:

- Entitlement type must match the locked order terms.
- Entitlement IDs and payment references must match the order lifecycle.
- Evidence may reference external receipts with hashes.
- Marketplace events must not contain bearer access URLs, license secrets, private credentials, or direct artifact payloads.

Delivery providers are external to v0.1. The protocol records state and evidence only.

## Disputes And Arbitration

Order rooms support contractual arbitration.

Events:

```text
dispute.opened
dispute.evidence.submitted
dispute.ruling.issued
dispute.closed
```

Supported ruling kinds include:

```text
refund_required
partial_refund_required
entitlement_confirmed
entitlement_reissue_required
service_completion_required
no_fault
```

Rules:

- Disputes reference an existing order.
- Evidence references external documents or receipts.
- Rulings are issued by the arbiter authority for the room.
- Refund rulings constrain later refund events.
- Dispute closure follows the ruling and required remedy state.

## Authority

Matrix sender authority is part of validation. Matrix users are representatives of marketplace actors; they are not the actors themselves.

Typical authority rules:

- Customer or seller AS may create or cancel an order, depending on the exact event.
- Seller AS accepts, rejects, completes orders, and issues entitlement state.
- Payment AS users emit payment events.
- Arbiter AS emits rulings and closes disputes.
- Customer-bound representatives must be disclosed in `customer.bound`.
- Sender Matrix user must match the issuer matrix user for the event envelope.

Rust entry point:

- `morpheus_core::assert_event_authority`

## Security, Privacy, And Retention

Protocol events must be replayable and safe to share among order participants. They must not carry high-risk delivery material.

Validators reject or flag:

- Payment secrets.
- Bearer access URLs.
- Confused-deputy issuer/sender mismatches.
- Redacted protocol events.
- Unsafe critical extensions.
- Unsupported versions and downgrade attempts.
- Invalid retention or privacy policy metadata when strict policy validators are used.

Policy validators are exported in `morpheus-protocol`. Callers decide where to wire advisory policies into strict validation contexts.

## Allowlist Trust Model

There is no global discovery in v0.1. Every instance keeps a local allowlist that decides:

- which catalog rooms to index;
- which instances may create or accept order rooms;
- which instances may issue valid events;
- which arbiters are accepted;
- which payment and entitlement capabilities are trusted.

Rust entry points:

- `morpheus_core::AllowlistPolicy`
- `morpheus_core::validate_allowlist_policy`
- `morpheus_core::should_index_catalog_room`

## Conformance

Conformance requires stable accept/reject outcomes and stable error codes for required vectors. The Rust conformance suite is the current oracle and contains 24 required v0.1 vectors.

Run:

```bash
cargo run -p morpheus-cli -- conformance run
cargo test -p morpheus-conformance
```

Protocol changes must land with:

- a spec update or note;
- a required or behavioral conformance vector;
- a Rust test covering accept/reject status and error code;
- a migration/projection note when persisted shape changes.

## Persistence And Ingest Expectations

The protocol expects a Synapse-compatible Application Service ingest path:

- transactions preserve Matrix event ID order;
- AppService transaction IDs are idempotent;
- raw Matrix events are retained even when invalid;
- accepted marketplace events are retained separately from raw events;
- rejected events keep stable validation status and error code;
- catalog, order, payment, entitlement, dispute, and arbitration projections are rebuildable from accepted marketplace events.

The Rust implementation models these expectations with `morpheus_matrix::AppServiceTransaction`, `morpheus_store::EventStore`, and `morpheus_server::build_router`.
