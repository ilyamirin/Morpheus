import { describe, expect, it } from "vitest";
import { validateMarketplaceEvent } from "../../src/protocol/marketplace-event-validator.js";
import { validOrderCreated } from "../../src/conformance/fixtures.js";

function event(overrides: Record<string, unknown> = {}) {
  return {
    type: "io.marketplace.order.created",
    room_id: validOrderCreated.room_id,
    event_id: "$matrix",
    sender: "@market:customer.example",
    origin_server_ts: 1777898400000,
    content: {
      protocol: "io.marketplace",
      protocol_version: "0.1",
      event_id: "$matrix",
      created_at: "2026-05-04T10:00:00Z",
      issuer: {
        instance_id: "customer.example",
        actor_id: validOrderCreated.customer_id,
        matrix_user_id: "@market:customer.example"
      },
      critical: [],
      body: validOrderCreated
    },
    ...overrides
  };
}

describe("validateMarketplaceEvent", () => {
  it("accepts known events and returns parsed status", () => {
    expect(validateMarketplaceEvent(event(), { roomProfile: "order" }).status).toBe("accepted");
  });

  it("rejects redacted marketplace events", () => {
    expect(() =>
      validateMarketplaceEvent(event({ unsigned: { redacted_because: { event_id: "$redaction" } } }), {
        roomProfile: "order"
      })
    ).toThrow(/redacted/i);
  });

  it("rejects mismatched Matrix event and envelope event ids", () => {
    const invalid = event();
    invalid.content.event_id = "$other";
    expect(() => validateMarketplaceEvent(invalid, { roomProfile: "order" })).toThrow(/event_id/i);
  });

  it("ignores unknown non-critical marketplace events", () => {
    expect(
      validateMarketplaceEvent(event({ type: "com.example.analytics.observed" }), { roomProfile: "order" })
    ).toEqual({ status: "ignored", reason: "unknown_event_type" });
  });

  it("rejects unknown events with critical extensions", () => {
    const unknown = event({ type: "com.example.analytics.observed" });
    (unknown.content.critical as string[]) = ["com.example.analytics.required"];
    expect(() => validateMarketplaceEvent(unknown, { roomProfile: "order" })).toThrow(/critical/i);
  });
});
