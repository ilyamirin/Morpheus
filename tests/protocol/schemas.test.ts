import { describe, expect, it } from "vitest";
import { marketplaceEventSchema } from "../../src/protocol/schemas.js";

type MarketplaceEventFixture = {
  type: string;
  room_id: string;
  event_id: string;
  sender: string;
  origin_server_ts: number;
  content: {
    protocol: string;
    protocol_version: string;
    event_id: string;
    created_at: string;
    issuer: {
      instance_id: string;
      actor_id: string;
      matrix_user_id: string;
    };
    critical: string[];
    body: unknown;
  };
};

const baseEvent: MarketplaceEventFixture = {
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

  it("rejects non-UTC envelope timestamps", () => {
    const invalid = structuredClone(baseEvent);
    invalid.content.created_at = "2026-05-04T13:00:00+03:00";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow(/UTC/);
  });

  it("rejects envelopes whose sender differs from issuer matrix user", () => {
    const invalid = structuredClone(baseEvent);
    invalid.sender = "@attacker:customer.example";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow(/sender.*issuer/i);
  });

  it("rejects invalid money amounts", () => {
    const invalid = structuredClone(baseEvent);
    const body = invalid.content.body as { price: { amount: string } };
    body.price.amount = "free";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });

  it("rejects order.created when event room and body room mismatch", () => {
    const invalid = structuredClone(baseEvent);
    invalid.room_id = "!other-order:customer.example";

    expect(() => marketplaceEventSchema.parse(invalid)).toThrow(/room.*mismatch/i);
  });

  it("rejects unknown critical extensions", () => {
    const invalid = structuredClone(baseEvent);
    invalid.content.critical = ["com.example.unknown"];
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });

  it("rejects arbitrary bodies for known payment intent events", () => {
    const invalid = structuredClone(baseEvent);
    invalid.type = "io.marketplace.payment.intent.created";
    invalid.content.body = { arbitrary: true };
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });

  it("accepts valid payment intent created bodies", () => {
    const valid = structuredClone(baseEvent);
    valid.type = "io.marketplace.payment.intent.created";
    valid.content.body = {
      order_id: "ord:customer.example:01JORDER",
      payment_id: "pay:shop.example:01JPAY",
      adapter: "stripe",
      amount: "100.00",
      currency: "USD",
      capture_policy: "before_entitlement",
      provider_ref: "pi_01JPAY",
      confirmation: {
        method: "redirect",
        uri: "https://pay.shop.example/confirm/pi_01JPAY"
      },
      expires_at: "2026-05-04T10:30:00Z"
    };

    expect(marketplaceEventSchema.parse(valid).content.body).toEqual(valid.content.body);
  });
});
