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
      protocol_event_id: "evt:customer.example:01JPROTO",
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

  it("allows Matrix event id to differ from protocol event id", () => {
    const valid = event({ event_id: "$matrix-from-homeserver" });
    expect(validateMarketplaceEvent(valid, { roomProfile: "order" }).status).toBe("accepted");
  });

  it("accepts registered critical extensions for known event types", () => {
    const known = event();
    (known.content.critical as string[]) = ["com.example.required"];
    expect(
      validateMarketplaceEvent(known, {
        roomProfile: "order",
        supportedCritical: ["com.example.required"]
      }).status
    ).toBe("accepted");
  });

  it("rejects unsupported critical extensions for known event types", () => {
    const known = event();
    (known.content.critical as string[]) = ["com.example.unsupported"];
    expect(() => validateMarketplaceEvent(known, { roomProfile: "order" })).toThrow(/critical/i);
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

  it("rejects protocol_event_id replay with a different Matrix event or body hash", () => {
    const seenProtocolEvents = new Map<string, { matrixEventId: string; bodyHash: string }>();
    validateMarketplaceEvent(event(), { roomProfile: "order", seenProtocolEvents });

    expect(() =>
      validateMarketplaceEvent(
        event({
          event_id: "$different-matrix-event",
          content: {
            ...event().content,
            body: { ...validOrderCreated, price: { amount: "1.00", currency: "USD" } }
          }
        }),
        { roomProfile: "order", seenProtocolEvents }
      )
    ).toThrow(/protocol_event_id/i);
  });
});
