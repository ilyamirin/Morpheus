# Federated Digital Marketplace over Matrix v0.1

## Status

Design approved for specification draft.

This document defines a strict federated protocol for a digital-only marketplace built on top of Matrix. It connects marketplace instances, sellers, customers, payment adapters, entitlement providers, and arbiters through Matrix rooms and typed Matrix events.

The protocol namespace is:

```text
io.marketplace.*
```

## Goals

- Enable many marketplace instances to interoperate over Matrix federation.
- Support digital products, services, bookings, subscriptions, licenses, and external entitlements.
- Use a local allowlist trust model rather than open global discovery.
- Define implementable room profiles, event schemas, state machines, and validation rules.
- Keep search local to each instance.
- Keep payments and digital artifacts outside Matrix while recording verifiable protocol state in Matrix.
- Support contractual arbitration in each order room.

## Non-Goals for v0.1

- Physical fulfillment.
- Federated search API.
- Reviews and reputation.
- Global web-of-trust or trust recommendations.
- Trustless escrow as a protocol requirement.
- Storing digital goods directly in Matrix.
- Making Matrix users directly equal to sellers or customers.

## Architecture

The protocol runs on Matrix federation. A production marketplace instance MUST run as a Matrix Application Service.

Each marketplace instance has:

```text
instance_id
matrix_server_name
application_service_id
catalog_room_id
allowlist
supported_protocol_versions
supported_payment_adapters
supported_entitlement_types
local_policy
```

There is no global discovery mechanism in v0.1. Each instance keeps a local allowlist and uses it to decide:

- which catalog rooms to index;
- which instances may create or accept order rooms;
- which instances may issue valid marketplace events;
- which arbiters may be accepted for orders;
- which payment adapters and entitlement types are acceptable.

The main entities are:

```text
Instance
SellerActor
CustomerActor
Product
Offer
Order
PaymentIntent
Entitlement
Dispute
ArbitrationPolicy
```

Matrix users and Application Service users act on behalf of actors. They are not sellers or customers by themselves.

## Room Profiles

### Catalog Room

Each marketplace instance has exactly one federated catalog room.

Recommended Matrix settings:

```text
alias: #marketplace-catalog:<server>
join_rule: public or restricted
history_visibility: world_readable or shared
encryption: off
canonical owner: marketplace Application Service
```

The catalog room MUST NOT contain order data or personal data. It contains only indexable marketplace events.

Required state events:

```text
io.marketplace.instance.profile
io.marketplace.catalog.profile
```

Allowed timeline events:

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

Catalog synchronization uses mandatory snapshots plus mandatory delta events. Wire-format catalog records use `snake_case`; reference implementations MAY map them into local camelCase/index records internally, but the wire format remains normative.

`io.marketplace.inventory.updated` is non-binding advisory catalog metadata. Canonical purchase terms remain in `offer.upserted`; booking-slot holds, provider inventory locks, and reschedules are outside Matrix in v0.1.

An indexer MUST:

1. find the latest valid snapshot;
2. verify its hash, schema, and issuer;
3. apply delta events after the snapshot;
4. apply delta events in contiguous `catalog_sequence` order;
5. reject events from invalid actors or unsupported protocol versions;
6. rebuild from a later valid snapshot after mismatch, missing delta, or corruption.

### Order Room

Each order has exactly one private Matrix room.

Recommended Matrix settings:

```text
join_rule: invite
history_visibility: invited or shared
encryption: optional for non-protocol messages
```

Marketplace protocol events are plaintext to all participating marketplace AS users in the order room so each instance can validate and replay the order. Secrets, artifacts, bearer URLs, and private credentials MUST remain outside Matrix marketplace protocol events. If participants use encrypted attachments or encrypted free-form Matrix messages, those messages are outside `io.marketplace.*` validation.

Required members:

```text
customer marketplace AS user
seller marketplace AS user
customer actor representatives
seller actor representatives
arbiter AS user or operator
```

An order room becomes protocol-valid only after a valid `io.marketplace.actor.customer.bound` event appears before a valid `io.marketplace.order.created` event in timeline order.

Structurally required order-room protocol events:

```text
io.marketplace.actor.customer.bound
io.marketplace.order.created
```

Happy-path order lifecycle events:

```text
io.marketplace.order.created
io.marketplace.order.accepted
io.marketplace.payment.intent.created
io.marketplace.payment.authorized
io.marketplace.payment.captured
io.marketplace.entitlement.granted
io.marketplace.order.completed
```

Optional order lifecycle events:

```text
io.marketplace.order.cancelled
io.marketplace.order.rejected
io.marketplace.payment.failed
io.marketplace.payment.cancelled
io.marketplace.payment.refund.requested
io.marketplace.payment.refunded
io.marketplace.payment.chargeback.opened
io.marketplace.entitlement.activated
io.marketplace.entitlement.completed
io.marketplace.entitlement.revoked
io.marketplace.entitlement.expired
io.marketplace.dispute.opened
io.marketplace.dispute.evidence.submitted
io.marketplace.dispute.ruling.issued
io.marketplace.dispute.closed
```

An order room MUST NOT be reused for another order. `order_id` MUST be globally unique and bound to the Matrix `room_id`.

### Actor Control Room

Actor control is a required local mechanism. Federation of actor control rooms is optional in v0.1.

External instances do not need access to internal actor-control permissions. They validate actors through catalog and order events:

```text
io.marketplace.actor.seller.announced
io.marketplace.actor.customer.bound
```

Seller actors are announced in the catalog room. Customer actors are disclosed only inside order rooms.

## Event Envelope

Every marketplace Matrix event stores the protocol payload in Matrix `content`.

Outer Matrix event fields are authoritative for Matrix routing and replay:

```json
{
  "type": "io.marketplace.order.created",
  "room_id": "!orderroom:customer.example",
  "event_id": "$matrix_event_id",
  "sender": "@market:customer.example",
  "origin_server_ts": 1777888000000,
  "content": {}
}
```

The protocol envelope inside `content` carries an independent protocol id:

```json
{
  "protocol": "io.marketplace",
  "protocol_version": "0.1",
  "protocol_event_id": "evt:shop.example:01JABC",
  "created_at": "2026-05-04T10:00:00Z",
  "issuer": {
    "instance_id": "shop.example",
    "actor_id": "seller:shop.example:01JABC...",
    "matrix_user_id": "@market:shop.example"
  },
  "critical": [],
  "body": {}
}
```

Rules:

- Matrix `event_id` and `content.protocol_event_id` are independent and MUST NOT be required to match.
- `protocol_event_id` MUST follow `evt:<instance_id>:<local_id>` and be generated before the event is sent.
- `created_at` MUST be ISO-8601 UTC.
- `issuer.instance_id` MUST match a trusted marketplace instance for the action.
- `issuer.actor_id` is required for actor-bound events.
- `critical` lists fields or extensions that MUST NOT be ignored.
- `body` contains the typed payload for the event type.

Canonical marketplace object ids use:

```text
<kind>:<instance_id>:<local_id>
```

`instance_id` is DNS-like and must contain at least one dot. `local_id` is ULID/UUIDv7-like, uppercase ASCII letters, digits, `_`, or `-`. Standard kinds in v0.1 include `seller`, `customer`, `arbiter`, `prod`, `offer`, `ord`, `pay`, `ent`, `disp`, `refund`, `snap`, and `evt`.

## Versioning and Compatibility

Protocol versioning is strict.

```json
{
  "protocol": "io.marketplace",
  "protocol_version": "0.1",
  "min_consumer_version": "0.1",
  "extensions": []
}
```

Compatibility rules:

- unsupported `protocol_version`: reject;
- unknown event type in catalog or order room: ignore only when `critical` is empty;
- unknown non-critical field: preserve when relaying, ignore for validation;
- known event type with a critical extension: accept only if the extension is registered in local validation context;
- unknown critical field or extension: reject;
- extension events MUST use reverse-DNS names outside `io.marketplace.*` unless standardized.

## Core Event Types

### `io.marketplace.instance.profile`

State event in catalog room.

```text
state_key: <instance_id>
```

```json
{
  "body": {
    "instance_id": "shop.example",
    "matrix_server_name": "shop.example",
    "application_service_id": "io.marketplace.shop",
    "catalog_room_id": "!abc:shop.example",
    "protocol_versions": ["0.1"],
    "payment_adapters": ["stripe", "bank_transfer"],
    "entitlement_types": [
      "download_access",
      "license_key",
      "account_access",
      "service_delivery",
      "booking_slot",
      "subscription_access",
      "external_entitlement"
    ],
    "arbitration_policies": ["standard-digital-v1"]
  }
}
```

### `io.marketplace.actor.seller.announced`

Timeline event in catalog room.

```json
{
  "body": {
    "seller_id": "seller:shop.example:01JSELLER",
    "status": "active",
    "display_name": "Acme Digital",
    "legal_profile_ref": "https://shop.example/sellers/acme/legal.json",
    "terms_ref": "https://shop.example/sellers/acme/terms",
    "terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    "supported_payment_adapters": ["stripe"],
    "supported_entitlement_types": ["license_key", "booking_slot"]
  }
}
```

### `io.marketplace.catalog.snapshot.published`

Timeline event in catalog room.

```json
{
  "body": {
    "snapshot_id": "snap:shop.example:01JSNAP",
    "sequence": 42,
    "format": "application/json+io.marketplace.catalog.v0",
    "uri": "mxc://shop.example/...",
    "sha256": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "covers_events_until": "$matrix_event_id",
    "product_count": 1200,
    "offer_count": 3400,
    "created_at": "2026-05-04T10:00:00Z"
  }
}
```

Snapshot records:

```text
sellers[]
products[]
offers[]
tombstones[]
sequence
covers_events_until
```

Snapshot JSON is hashed with canonical JSON. `sha256` values use `sha256:<64 lowercase hex>`. `snapshot_id` uses `snap:<instance_id>:<local_id>`. Snapshot replay applies tombstones before later deltas, and product/offer withdrawal deltas remove the withdrawn object from the local catalog view. Deltas MUST carry a contiguous `catalog_sequence`; missing sequences require recovery from a later snapshot.

### `io.marketplace.product.upserted`

Timeline event in catalog room.

```json
{
  "body": {
    "product_id": "prod:shop.example:01JPROD",
    "seller_id": "seller:shop.example:01JSELLER",
    "revision": 7,
    "status": "active",
    "kind": "digital_service",
    "title": "Architecture consultation",
    "description": "One-hour remote architecture review.",
    "categories": ["software", "consulting"],
    "tags": ["architecture", "backend"],
    "media": [
      {
        "uri": "https://shop.example/media/prod.png",
        "sha256": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
      }
    ],
    "terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
  }
}
```

Allowed product kinds:

```text
digital_file
license
account_access
digital_service
booking
subscription
external_entitlement
```

### `io.marketplace.offer.upserted`

Timeline event in catalog room.

```json
{
  "body": {
    "offer_id": "offer:shop.example:01JOFFER",
    "product_id": "prod:shop.example:01JPROD",
    "seller_id": "seller:shop.example:01JSELLER",
    "revision": 3,
    "status": "active",
    "price": {
      "amount": "100.00",
      "currency": "USD"
    },
    "payment_terms": {
      "capture_policy": "before_entitlement",
      "adapter_policy": "seller_supported"
    },
    "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    "entitlement": {
      "type": "booking_slot",
      "duration": "PT1H",
      "delivery": "external"
    },
    "availability": {
      "mode": "limited",
      "quantity": 10,
      "valid_until": "2026-06-01T00:00:00Z"
    }
  }
}
```

### `io.marketplace.order.created`

Timeline event in order room.

Before this event, the order room MUST contain a valid `io.marketplace.actor.customer.bound` event for the customer actor. `customer.bound` after `order.created` is invalid.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "room_id": "!orderroom:customer.example",
    "customer_id": "customer:customer.example:01JCUST",
    "seller_id": "seller:shop.example:01JSELLER",
    "offer_id": "offer:shop.example:01JOFFER",
    "offer_revision": 3,
    "catalog_snapshot_id": "snap:shop.example:01JSNAP",
    "quantity": 1,
    "price": {
      "amount": "100.00",
      "currency": "USD"
    },
    "payment_adapter": "stripe",
    "payment_capture_policy": "before_entitlement",
    "entitlement_type": "booking_slot",
    "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    "arbiter_instance": "arbiter.example",
    "arbiter_actor": "arbiter:arbiter.example:DEFAULT",
    "arbitration_policy_id": "standard-digital-v1",
    "arbitration_policy_version": "1",
    "arbitration_window": "P14D",
    "expires_at": "2026-05-04T10:30:00Z"
  }
}
```

In v0.1, `quantity` MUST be `1`. Multi-quantity carts, bundles, and quantity-priced offers are out of scope until the protocol defines separate `unit_price` and `total_price` semantics.

### `io.marketplace.order.accepted`

Timeline event in order room, issued by the seller AS.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "offer_revision": 3,
    "seller_terms_hash": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    "offer_terms_hash": "sha256:2222222222222222222222222222222222222222222222222222222222222222",
    "payment_capture_policy": "before_entitlement",
    "arbitration_policy_version": "1"
  }
}
```

`order.accepted` confirms the seller is accepting the exact terms locked by `order.created`; mismatched revision, terms hashes, capture policy, or arbitration policy version are invalid.

### `io.marketplace.actor.customer.bound`

Timeline event in order room.

This event discloses the customer actor for a specific order. It is intentionally scoped to the order room and is not published to the catalog room.

```json
{
  "body": {
    "customer_id": "customer:customer.example:01JCUST",
    "status": "active",
    "display_name": "Acme Procurement",
    "instance_id": "customer.example",
    "authorized_representatives": [
      "@buyer:customer.example"
    ],
    "accepted_payment_adapters": ["stripe"],
    "accepted_arbitration_policies": ["standard-digital-v1"]
  }
}
```

Every `authorized_representatives[]` Matrix user disclosed in `customer.bound` MUST be joined to the order room. Seller representatives follow the same room-profile rule when disclosed by seller-side policy.

### `io.marketplace.payment.intent.created`

Timeline event in order room.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "payment_id": "pay:shop.example:01JPAY",
    "adapter": "stripe",
    "amount": "100.00",
    "currency": "USD",
    "capture_policy": "before_entitlement",
    "idempotency_key": "pay-intent-01JPAY",
    "provider_ref": "pi_...",
    "confirmation": {
      "method": "redirect",
      "uri": "https://pay.shop.example/confirm/pi_..."
    },
    "expires_at": "2026-05-04T10:30:00Z"
  }
}
```

The `capture_policy` in `payment.intent.created` MUST match `payment_capture_policy` locked in `order.created`.

### `io.marketplace.payment.captured`

Timeline event in order room.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "payment_id": "pay:shop.example:01JPAY",
    "adapter": "stripe",
    "amount": "100.00",
    "currency": "USD",
    "provider_ref": "ch_...",
    "evidence": {
      "kind": "provider_receipt",
      "uri": "https://shop.example/payments/ch_...",
      "sha256": "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    }
  }
}
```

### `io.marketplace.payment.refund.requested` and `io.marketplace.payment.refunded`

Timeline events in order room.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "payment_id": "pay:shop.example:01JPAY",
    "refund_id": "refund:shop.example:01JREFUND",
    "adapter": "stripe",
    "amount": "100.00",
    "currency": "USD",
    "provider_ref": "re_...",
    "evidence": {
      "kind": "provider_receipt",
      "uri": "https://shop.example/payments/refunds/re_...",
      "sha256": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    }
  }
}
```

Refund events MUST reference a captured `payment_id`, carry a stable `refund_id`, and include external evidence. Without a dispute ruling, a refund amount is validated against the captured amount. A full-refund ruling constrains the refund amount to the captured amount. A partial-refund ruling constrains the refund amount and currency to the ruling remedy.

### `io.marketplace.entitlement.granted`

Timeline event in order room.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "payment_id": "pay:shop.example:01JPAY",
    "entitlement_id": "ent:shop.example:01JENT",
    "type": "booking_slot",
    "external_ref": "bk_92381",
    "valid_from": "2026-05-10T12:00:00Z",
    "valid_until": "2026-05-10T13:00:00Z",
    "evidence": {
      "kind": "provider_receipt",
      "uri": "https://shop.example/receipts/bk_92381",
      "sha256": "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    }
  }
}
```

### `io.marketplace.dispute.ruling.issued`

Timeline event in order room.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "dispute_id": "disp:arbiter.example:01JDISP",
    "ruling": "refund_required",
    "reason_code": "entitlement_not_delivered",
    "remedy": {
      "type": "full_refund",
      "amount": "100.00",
      "currency": "USD"
    },
    "evidence_refs": ["$evidence_1", "$evidence_2"],
    "binding": true
  }
}
```

### `io.marketplace.dispute.evidence.submitted`

Timeline event in order room.

```json
{
  "body": {
    "order_id": "ord:customer.example:01JORDER",
    "dispute_id": "disp:arbiter.example:01JDISP",
    "evidence": {
      "kind": "customer_statement",
      "uri": "mxc://customer.example/evidence",
      "sha256": "sha256:4444444444444444444444444444444444444444444444444444444444444444"
    }
  }
}
```

Ruling `evidence_refs` MUST reference Matrix event ids from the same order-room timeline, not protocol-local evidence ids.

Allowed rulings:

```text
refund_required
partial_refund_required
entitlement_confirmed
entitlement_reissue_required
service_completion_required
no_fault
```

## Product and Offer Model

Product and Offer are separate.

`Product` describes what is sold:

```text
digital file
license
account access
digital service
booking
subscription
external entitlement
```

`Offer` describes how the product is purchased:

```text
price
currency
payment terms
availability
entitlement type
access duration
delivery mode
seller policy
```

One product can have multiple offers.

`seller_terms_hash` and `offer_terms_hash` in an offer are the hashes that `order.created` and `order.accepted` later lock. The seller hash should correspond to the seller terms current for that offer; the offer hash covers offer-specific purchase terms.

## Order Transition Graph

The low-level transition helper is a transition graph only. It is useful for shape checks, but strict order acceptance MUST use timeline replay (`validateOrderRoomTimeline` or payload-aware `validateOrderEventSequence`) because capture policy, terms, refund amounts, room binding, and actor authority are payload-dependent.

The preferred public helper name is `OrderTransitionGraph`. `OrderStateMachine` is retained as a compatibility alias and should not be used as the strict protocol acceptance API.

Nominal lifecycle:

```text
draft
  -> created
  -> accepted
  -> payment_intent_created
  -> payment_authorized
  -> payment_captured
  -> entitlement_granted
  -> completed
```

Terminal states:

```text
completed
cancelled
rejected
refunded
dispute_resolved
expired
```

Dispute branch:

```text
accepted/payment_captured/entitlement_granted
  -> dispute_opened
  -> ruling_issued
  -> refund_required | partial_refund_required | entitlement_confirmed | entitlement_reissue_required | service_completion_required
  -> dispute_resolved
```

## Payment Adapter Contract

The protocol does not move money. It standardizes payment state and evidence.

Required payment events:

```text
io.marketplace.payment.intent.created
io.marketplace.payment.authorized
io.marketplace.payment.captured
```

Optional payment events:

```text
io.marketplace.payment.failed
io.marketplace.payment.cancelled
io.marketplace.payment.refund.requested
io.marketplace.payment.refunded
io.marketplace.payment.chargeback.opened
```

Rules:

- The adapter MUST be announced in the seller instance profile.
- The customer instance MUST accept the adapter in `order.created`.
- Payment events MAY be issued only by the seller marketplace AS or seller-instance virtual payment AS users on the seller Matrix server.
- External payment provider homeservers are not normative event issuers in v0.1.
- Payment intent and refund events MUST carry idempotency/reference fields sufficient for adapter-level dedupe.
- Payment secrets MUST NOT be transmitted through Matrix marketplace events.
- Payment evidence MAY be an external URI plus hash.
- Refund events MUST reference a captured `payment_id`.

## Entitlement Lifecycle

Digital fulfillment is represented as entitlement state. Matrix records entitlement metadata and evidence, not the artifact or secret itself.

Supported entitlement types:

```text
download_access
license_key
account_access
service_delivery
booking_slot
subscription_access
external_entitlement
```

Lifecycle:

```text
pending
  -> granted
  -> active
  -> completed
```

Additional transitions:

```text
granted/active -> revoked
granted/active -> expired
granted/active -> disputed
```

Rules:

- `entitlement.granted` MUST be issued by the seller AS.
- `external_ref` MUST NOT contain a secret access credential.
- Sensitive access tokens MUST be delivered outside Matrix marketplace events or as encrypted attachments.
- Entitlements MUST reference `order_id`.
- Entitlements MUST reference `payment_id` when the offer requires payment before delivery.
- `valid_from` and `valid_until` are required for bookings and subscriptions.
- Evidence is required for `service_delivery` and `external_entitlement`.
- Booking hold, live inventory reservation, provider calendar state, reschedule, and cancellation-slot mechanics remain outside Matrix in v0.1.
- For `booking_slot`, Matrix records final entitlement proof via `entitlement.granted`; there are no booking hold events in v0.1.

## Arbitration and Disputes

Each order fixes the arbiter and arbitration policy in `order.created`:

```text
arbiter_instance
arbiter_actor
arbitration_policy_id
arbitration_policy_version
arbitration_window
```

The validating instance MUST locally allowlist the arbiter for `arbitration`. v0.1 does not require a global or mutual allowlist proof; each participant validates the order against its own local allowlist before accepting or continuing the room.

Dispute events:

```text
io.marketplace.dispute.opened
io.marketplace.dispute.evidence.submitted
io.marketplace.dispute.ruling.issued
io.marketplace.dispute.closed
```

Rules:

- `dispute.opened` MAY be issued by the customer, seller, or arbiter AS.
- `evidence.submitted` MAY be issued by any order party.
- `ruling.issued` MAY be issued only by the arbiter AS.
- Evidence references in a ruling MUST point to Matrix events in the same order-room timeline.
- A binding ruling MUST be executed if the arbitration policy was accepted in `order.created`.
- If a payment adapter cannot automate a refund, `refund_required` remains a protocol obligation and execution is confirmed by a later refund event.

## Privacy Model

Order rooms use a plaintext-protocol privacy profile.

Structured marketplace protocol events are readable by participating marketplace AS users:

```text
order_id
actor ids
offer_id
offer_revision
price
payment status
entitlement status
dispute status
timestamps
non-secret evidence hashes
```

Data that MUST NOT appear in open marketplace events:

```text
payment secrets
access tokens
private download URLs with bearer credentials
personal documents
unnecessary personal data
free-form private conversation
```

Sensitive data MAY be transmitted through:

```text
out-of-band seller provider flow
encrypted Matrix attachments
E2EE room messages when clients support them
short-TTL links, provided the link itself is not a bearer secret in an indexable event
```

## Validation Rules

Every marketplace event MUST be validated against:

```text
Matrix sender
room profile
issuer.instance_id
issuer.actor_id
actor status
local allowlist permission
event type authority
transition graph state
referenced object revision/hash
critical fields
protocol_event_id replay
```

An instance MUST reject an event when:

- `protocol_version` is unsupported;
- event type is not allowed for the room profile;
- sender is outside the expected Matrix server or AS namespace;
- `issuer.instance_id` is not allowlisted for the attempted action;
- `actor_id` is unknown, inactive, or suspended;
- object revision goes backwards;
- required fields are missing;
- unknown critical field or extension is present;
- the same `protocol_event_id` appears on a different Matrix event or with a different canonical body hash;
- order transition violates the transition graph or payload-aware timeline rules;
- price, currency, or offer revision differs from trusted catalog state;
- payment, entitlement, or dispute event is issued by an unauthorized party.

Cross-room references MUST include enough data to prevent replay and substitution:

```json
{
  "ref": {
    "room_id": "!catalog:shop.example",
    "event_id": "$abc...",
    "object_id": "offer:shop.example:01JOFFER",
    "revision": 3,
    "sha256": "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
  }
}
```

## Threat Model

v0.1 protects against:

```text
catalog spam from unknown instances
forged seller/customer actors
stale offer replay
price substitution
unauthorized order state transition
fake payment captured event
fake entitlement granted event
arbiter impersonation
snapshot/delta mismatch
revision rollback
unknown critical extension acceptance
order room reuse
cross-room event replay
```

v0.1 does not solve:

```text
global reputation
review fraud
Web-of-trust attacks
trust recommendation attacks
trustless escrow
fraud by a locally allowlisted instance
legal enforcement outside payment adapter and arbitration policy
```

## Required Test Vectors

The protocol conformance suite includes:

1. valid catalog snapshot accepted;
2. valid product and offer delta after snapshot accepted;
3. unknown instance catalog rejected;
4. seller suspended, later offer rejected;
5. stale offer revision in `order.created` rejected;
6. price mismatch in `order.created` rejected;
7. valid order lifecycle reaches completed;
8. `payment.captured` from unauthorized sender rejected;
9. `entitlement.granted` before `payment.captured` rejected when `capture_policy=before_entitlement`;
10. arbiter not allowlisted by both sides, order rejected;
11. `dispute.ruling.issued` from non-arbiter rejected;
12. unknown critical extension rejected by validator context;
13. order event replayed into different room rejected;
14. snapshot hash mismatch rejected;
15. revision rollback rejected;
16. canonical catalog snapshot hash mismatch rejected;
17. redacted marketplace event rejected;
18. catalog privacy leakage rejected;
19. non-idempotent duplicate appservice transaction rejected;
20. dispute evidence reference outside order-room timeline rejected;
21. withdrawn offer removed from local search index;
22. protocol downgrade attempt rejected;
23. zero-day retention policy rejected;
24. compatibility profile from non-allowlisted instance rejected.

## Remaining v0.1 Gaps

The current Spec+Validator surface intentionally leaves production integration out of scope. Remaining implementation work is Matrix Application Service runtime behavior, persistent storage, HTTP APIs, homeserver deployment profiles, and operator tooling. These are implementation milestones, not protocol semantics.

## v0.1 Completion Clarifications

The Rust protocol implementation is the normative Spec+Validator package. The server milestone intentionally keeps production Matrix Application Service behavior, persistent projections, HTTP API expansion, and federated search as separate implementation milestones.

Normative validation entrypoints:

```text
morpheus_protocol::validate_marketplace_event(event, context)
morpheus_core::validate_catalog_snapshot(snapshot, expected_hash)
morpheus_core::replay_catalog_timeline(instance_id, snapshot, events)
morpheus_core::validate_order_room_timeline(events, context)
morpheus_core::validate_allowlist_policy(policy, now_epoch_ms)
morpheus_conformance::ConformanceRunner
```

Low-level exports remain building blocks and MUST NOT be treated as complete protocol acceptance on their own. In particular, `OrderTransitionGraph`/`OrderStateMachine` validate only transition shape; strict order acceptance requires order-room timeline validation.

Retention, security, compatibility, indexing, and privacy validators are policy validators. They are advisory unless an implementation invokes them from its strict validation context.

### Canonical Event and Hash Rules

- Matrix events MUST include `type`, `room_id`, `event_id`, `sender`, `origin_server_ts`, and `content`.
- `content.protocol_event_id` MUST follow `evt:<instance_id>:<local_id>` and is independent from Matrix `event_id`.
- Redacted marketplace events are not protocol-valid.
- Known `io.marketplace.*` events MUST pass the registered body schema and room-profile rules.
- Unknown non-critical marketplace events MAY be ignored by routing code.
- Known critical extensions MUST be present in the local supported-critical registry.
- Unknown critical events or extensions MUST be rejected.
- Canonical JSON uses sorted object keys and UTF-8 JSON serialization.
- Hashes use `sha256:<64 lowercase hex>`.

### Local Trust and Operational Rules

- Allowlist entries support `catalog`, `orders`, `payments`, `arbitration`, and `indexing` capabilities.
- Revoked or expired entries MUST NOT authorize new orders or indexing.
- Existing order-room replay remains possible after revoke for audit and dispute resolution.
- Appservice transactions MUST be idempotent by transaction id and event id list.
- Backfill pages MUST NOT contain duplicate Matrix event ids.
- Snapshot cache entries MUST NOT change hash for the same sequence.

### Federation Policy Rules

- Catalog events MUST NOT contain order, customer, payment, entitlement, dispute, or unnecessary PII fields.
- Order events MUST NOT contain bearer tokens, payment secrets, artifact secrets, passwords, or private credentials.
- Local search indexes only allowlisted catalog rooms with both `catalog` and `indexing` capabilities.
- Withdrawn offers and suspended seller content MUST be removed from local search results.
- Extension names outside the standard protocol MUST use non-`io.marketplace.*` reverse-DNS namespaces.
- Instance compatibility is discovered only from allowlisted catalog rooms and requires the negotiated protocol version and minimum Matrix room version.

### Arbitration, Retention, and Security Rules

- Arbitration policies include policy id, version, arbitration window, accepted remedies, and binding flag.
- Dispute ruling evidence refs MUST point to events in the same order-room timeline.
- A binding refund ruling creates a protocol obligation to emit a later refund event.
- Retention windows for catalog tombstones, archived orders, completed entitlements, and suspended actors MUST be positive.
- Downgrade attempts below `min_consumer_version` MUST be rejected.
- Sender server and issuer instance mismatches MUST be rejected as confused-deputy risks.

## Implementation Status

The current implementation milestone is Rust-only for protocol validation and conformance. It covers event schemas, canonical hashes, Matrix event routing, local allowlist checks, catalog snapshot/delta rules, order state transitions, payment/entitlement/dispute authority checks, privacy/indexing policy, appservice idempotency, security/retention/compatibility validators, and 24 required v0.1 conformance vectors.
