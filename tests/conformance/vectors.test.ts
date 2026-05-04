import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";
import { LocalSearchIndex } from "../../src/catalog/indexing-policy.js";
import { validateCatalogSnapshot } from "../../src/catalog/catalog-replay.js";
import { assertEventAuthority } from "../../src/order/authority.js";
import { validateArbitrationFlow } from "../../src/order/arbitration.js";
import { validateOrderEventSequence } from "../../src/order/order-flow-validator.js";
import { OrderStateMachine } from "../../src/order/order-state.js";
import { validateOrderCreated } from "../../src/order/order-validator.js";
import { validCatalog, validCustomerBinding, validOrderCreated } from "../../src/conformance/fixtures.js";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";
import { validateMarketplaceEvent } from "../../src/protocol/marketplace-event-validator.js";
import { validateMarketplacePrivacy } from "../../src/protocol/privacy-policy.js";
import { validateAppserviceTransaction } from "../../src/protocol/appservice.js";
import { validateSecurityEnvelope } from "../../src/protocol/security.js";
import { validateRetentionPolicy } from "../../src/protocol/retention.js";
import { validateInstanceCompatibility } from "../../src/protocol/compatibility.js";
import { marketplaceEventSchema } from "../../src/protocol/schemas.js";

function orderAllowlist(): AllowlistPolicy {
  return new AllowlistPolicy({ "shop.example": ["orders"], "arbiter.example": ["arbitration"] });
}

describe("required conformance vectors", () => {
  it("1 accepts valid catalog snapshot", () => {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot(validCatalog.snapshot);
    expect(catalog).toBeDefined();
  });

  it("2 accepts valid product and offer deltas after snapshot", () => {
    const catalog = validCatalog.build();
    expect(catalog.getOffer("offer:shop.example:01JOFFER")?.revision).toBe(3);
  });

  it("3 rejects unknown instance catalog by allowlist policy", () => {
    const allowlist = new AllowlistPolicy({});
    expect(allowlist.can("unknown.example", "catalog")).toBe(false);
  });

  it("4 rejects later offer for suspended seller", () => {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot(validCatalog.snapshot);
    catalog.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "suspended" });
    expect(() => catalog.upsertOffer(validCatalog.offer)).toThrow();
  });

  it("5 rejects stale offer revision in order.created", () => {
    expect(() =>
      validateOrderCreated(
        { ...validOrderCreated, offer_revision: 1 },
        validCatalog.build(),
        orderAllowlist(),
        validCustomerBinding
      )
    ).toThrow();
  });

  it("6 rejects price mismatch in order.created", () => {
    expect(() =>
      validateOrderCreated(
        { ...validOrderCreated, price: { amount: "1.00", currency: "USD" } },
        validCatalog.build(),
        orderAllowlist(),
        validCustomerBinding
      )
    ).toThrow();
  });

  it("7 validates complete happy-path order lifecycle", () => {
    const machine = new OrderStateMachine();
    for (const eventType of [
      "io.marketplace.order.created",
      "io.marketplace.order.accepted",
      "io.marketplace.payment.intent.created",
      "io.marketplace.payment.authorized",
      "io.marketplace.payment.captured",
      "io.marketplace.entitlement.granted",
      "io.marketplace.order.completed"
    ]) {
      machine.apply(eventType);
    }
    expect(machine.state).toBe("completed");
  });

  it("8 rejects payment.captured from unauthorized sender", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:customer.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow();
  });

  it("9 rejects entitlement.granted before payment.captured when capture_policy=before_entitlement", () => {
    expect(() =>
      validateOrderEventSequence([
        {
          type: "io.marketplace.actor.customer.bound",
          body: {
            customer_id: validCustomerBinding.customer_id,
            status: validCustomerBinding.status,
            accepted_payment_adapters: validCustomerBinding.accepted_payment_adapters,
            accepted_arbitration_policies: validCustomerBinding.accepted_arbitration_policies
          }
        },
        { type: "io.marketplace.order.created", body: validOrderCreated },
        { type: "io.marketplace.order.accepted", body: { order_id: validOrderCreated.order_id } },
        {
          type: "io.marketplace.payment.intent.created",
          body: {
            order_id: validOrderCreated.order_id,
            payment_id: "pay:customer.example:01JPAY",
            adapter: validOrderCreated.payment_adapter,
            amount: validOrderCreated.price.amount,
            currency: validOrderCreated.price.currency,
            capture_policy: "before_entitlement",
            provider_ref: "pi_123",
            confirmation: { method: "redirect", uri: "https://payments.example/confirm/pi_123" },
            expires_at: "2026-05-04T10:20:00Z"
          }
        },
        {
          type: "io.marketplace.payment.authorized",
          body: { order_id: validOrderCreated.order_id, payment_id: "pay:customer.example:01JPAY" }
        },
        {
          type: "io.marketplace.entitlement.granted",
          body: {
            order_id: validOrderCreated.order_id,
            payment_id: "pay:customer.example:01JPAY",
            entitlement_id: "ent:customer.example:01JENT",
            type: validOrderCreated.entitlement_type,
            external_ref: "booking:slot-123"
          }
        }
      ])
    ).toThrow(/before_entitlement/);
  });

  it("10 rejects non-allowlisted arbiter", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["orders"] });
    expect(() =>
      validateOrderCreated(validOrderCreated, validCatalog.build(), allowlist, validCustomerBinding)
    ).toThrow();
  });

  it("11 rejects dispute ruling from non-arbiter", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow();
  });

  it("12 rejects unknown critical extension at schema layer", () => {
    expect(() =>
      marketplaceEventSchema.parse({
        type: "io.marketplace.order.created",
        room_id: validOrderCreated.room_id,
        event_id: "$order-created",
        sender: "@market:customer.example",
        origin_server_ts: 1_777_888_000_000,
        content: {
          protocol: "io.marketplace",
          protocol_version: "0.1",
          event_id: "evt:customer.example:01JORDER",
          created_at: "2026-05-04T10:00:00Z",
          issuer: {
            instance_id: "customer.example",
            actor_id: validOrderCreated.customer_id,
            matrix_user_id: "@market:customer.example"
          },
          critical: ["com.example.unknown"],
          body: validOrderCreated
        }
      })
    ).toThrow();
  });

  it("13 rejects order.created replayed into a different order room", () => {
    expect(() =>
      marketplaceEventSchema.parse({
        type: "io.marketplace.order.created",
        room_id: "!other-order:customer.example",
        event_id: "$order-created-replay",
        sender: "@market:customer.example",
        origin_server_ts: 1_777_888_000_000,
        content: {
          protocol: "io.marketplace",
          protocol_version: "0.1",
          event_id: "evt:customer.example:01JORDER",
          created_at: "2026-05-04T10:00:00Z",
          issuer: {
            instance_id: "customer.example",
            actor_id: validOrderCreated.customer_id,
            matrix_user_id: "@market:customer.example"
          },
          critical: [],
          body: validOrderCreated
        }
      })
    ).toThrow(/room.*mismatch/i);
  });

  it("14 rejects snapshot hash mismatch", () => {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot({ ...validCatalog.snapshot, sequence: 2, sha256: "abc" });
    try {
      catalog.applySnapshot({ ...validCatalog.snapshot, sequence: 2, sha256: "def" });
      throw new Error("Expected snapshot hash mismatch");
    } catch (error) {
      expect(error).toBeInstanceOf(MarketplaceValidationError);
      expect((error as MarketplaceValidationError).code).toBe("CATALOG_REFERENCE_MISMATCH");
      expect((error as MarketplaceValidationError).message).toBe("Snapshot hash mismatch");
    }
  });

  it("15 rejects revision rollback", () => {
    const catalog = validCatalog.build();
    expect(() => catalog.upsertOffer({ ...validCatalog.offer, revision: 2 })).toThrow();
  });

  it("16 rejects canonical catalog snapshot hash mismatch", () => {
    expect(() =>
      validateCatalogSnapshot(
        {
          snapshot_id: "snap_01JVALID",
          sequence: 1,
          covers_events_until: "$snapshot",
          sellers: [validCatalog.seller],
          products: [validCatalog.product],
          offers: [validCatalog.offer],
          tombstones: []
        },
        { expectedSha256: "sha256:" + "0".repeat(64) }
      )
    ).toThrow(/hash/i);
  });

  it("17 rejects redacted marketplace events", () => {
    expect(() =>
      validateMarketplaceEvent(
        {
          type: "io.marketplace.order.created",
          room_id: validOrderCreated.room_id,
          event_id: "$order-created",
          sender: "@market:customer.example",
          origin_server_ts: 1_777_888_000_000,
          unsigned: { redacted_because: { event_id: "$redaction" } },
          content: {
            protocol: "io.marketplace",
            protocol_version: "0.1",
            event_id: "$order-created",
            created_at: "2026-05-04T10:00:00Z",
            issuer: {
              instance_id: "customer.example",
              actor_id: validOrderCreated.customer_id,
              matrix_user_id: "@market:customer.example"
            },
            critical: [],
            body: validOrderCreated
          }
        },
        { roomProfile: "order" }
      )
    ).toThrow(/redacted/i);
  });

  it("18 rejects catalog privacy leakage", () => {
    expect(() =>
      validateMarketplacePrivacy(
        { type: "io.marketplace.offer.upserted", content: { body: { offer_id: validCatalog.offer.offerId, customer_id: validOrderCreated.customer_id } } },
        "catalog"
      )
    ).toThrow(/catalog/i);
  });

  it("19 rejects non-idempotent duplicate appservice transactions", () => {
    const seen = new Map<string, string[]>();
    validateAppserviceTransaction({ txnId: "txn1", eventIds: ["$a"] }, seen);
    expect(() => validateAppserviceTransaction({ txnId: "txn1", eventIds: ["$b"] }, seen)).toThrow(/idempotent/i);
  });

  it("20 rejects dispute evidence refs outside the order room timeline", () => {
    expect(() =>
      validateArbitrationFlow([
        { type: "io.marketplace.dispute.opened", event_id: "$disp", room_id: validOrderCreated.room_id, body: { order_id: validOrderCreated.order_id, dispute_id: "disp:arbiter.example:1" } },
        {
          type: "io.marketplace.dispute.ruling.issued",
          event_id: "$ruling",
          room_id: validOrderCreated.room_id,
          body: {
            order_id: validOrderCreated.order_id,
            dispute_id: "disp:arbiter.example:1",
            ruling: "refund_required",
            remedy: { type: "full_refund" },
            evidence_refs: ["$missing"],
            binding: true
          }
        }
      ])
    ).toThrow(/evidence/i);
  });

  it("21 removes withdrawn offers from local search index", () => {
    const index = new LocalSearchIndex();
    index.apply({ type: "io.marketplace.offer.upserted", body: { ...validCatalog.offer, status: "active" } });
    index.apply({ type: "io.marketplace.offer.withdrawn", body: { offer_id: validCatalog.offer.offerId, revision: 4 } });
    expect(index.hasOffer(validCatalog.offer.offerId)).toBe(false);
  });

  it("22 rejects protocol downgrade attempts", () => {
    expect(() =>
      validateSecurityEnvelope({ protocol_version: "0.1", min_consumer_version: "0.2" }, { supportedVersion: "0.1" })
    ).toThrow(/downgrade/i);
  });

  it("23 rejects zero-day retention policy", () => {
    expect(() =>
      validateRetentionPolicy({
        catalogTombstoneDays: 0,
        orderArchiveDays: 365,
        completedEntitlementDays: 365,
        suspendedActorDays: 90
      })
    ).toThrow(/retention/i);
  });

  it("24 rejects compatibility profiles from non-allowlisted instances", () => {
    expect(() =>
      validateInstanceCompatibility(
        {
          instance_id: "shop.example",
          catalog_room_id: "!catalog:shop.example",
          protocol_versions: ["0.1"],
          matrix_room_version: "10",
          payment_adapters: ["stripe"],
          arbitration_policies: ["standard-digital-v1"]
        },
        {
          allowlist: new AllowlistPolicy({}),
          minimumRoomVersion: "9",
          requiredProtocolVersion: "0.1"
        }
      )
    ).toThrow(/allowlist/i);
  });
});
