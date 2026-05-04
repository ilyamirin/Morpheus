import { describe, expect, it } from "vitest";
import { marketplaceEventSchema } from "../../src/protocol/schemas.js";

const baseEvent = {
  type: "io.marketplace.order.created",
  room_id: "!order:customer.example",
  event_id: "$matrix",
  sender: "@market:customer.example",
  origin_server_ts: 1777898400000,
  content: {
    protocol: "io.marketplace",
    protocol_version: "0.1",
    event_id: "evt_01JORDER",
    created_at: "2026-05-04T10:00:00Z",
    issuer: {
      instance_id: "customer.example",
      actor_id: "customer:customer.example:01JCUST",
      matrix_user_id: "@market:customer.example"
    },
    critical: [],
    body: {
      order_id: "ord:customer.example:01JORDER",
      room_id: "!order:customer.example",
      customer_id: "customer:customer.example:01JCUST",
      seller_id: "seller:shop.example:01JSELLER",
      offer_id: "offer:shop.example:01JOFFER",
      offer_revision: 3,
      catalog_snapshot_id: "snap_01J",
      quantity: 1,
      price: { amount: "100.00", currency: "USD" },
      payment_adapter: "stripe",
      entitlement_type: "booking_slot",
      arbiter_instance: "arbiter.example",
      arbiter_actor: "arbiter:arbiter.example:default",
      arbitration_policy_id: "standard-digital-v1",
      arbitration_window: "P14D",
      expires_at: "2026-05-04T10:30:00Z"
    }
  }
};

describe("marketplaceEventSchema", () => {
  it("accepts a valid marketplace envelope", () => {
    expect(marketplaceEventSchema.parse(baseEvent).content.protocol).toBe("io.marketplace");
  });

  it("rejects unsupported protocol versions", () => {
    const invalid = structuredClone(baseEvent);
    invalid.content.protocol_version = "0.2";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });

  it("rejects invalid money amounts", () => {
    const invalid = structuredClone(baseEvent);
    invalid.content.body.price.amount = "free";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });
});
