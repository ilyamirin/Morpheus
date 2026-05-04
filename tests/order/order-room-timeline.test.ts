import { describe, expect, it } from "vitest";
import { validateOrderRoomTimeline } from "../../src/order/order-room-timeline.js";
import { validCustomerBinding, validOrderCreated } from "../../src/conformance/fixtures.js";

function envelope(type: string, body: object, sender = "@market:customer.example") {
  return {
    type,
    room_id: validOrderCreated.room_id,
    event_id: `$${type}`,
    sender,
    origin_server_ts: 1777898400000,
    content: {
      protocol: "io.marketplace",
      protocol_version: "0.1",
      protocol_event_id: `evt:customer.example:${type.replaceAll(".", "").toUpperCase()}`,
      created_at: "2026-05-04T10:00:00Z",
      issuer: {
        instance_id: sender.split(":")[1],
        actor_id: type.includes("customer") || type.includes("order.created") ? validOrderCreated.customer_id : validOrderCreated.seller_id,
        matrix_user_id: sender
      },
      critical: [],
      body
    }
  };
}

describe("validateOrderRoomTimeline", () => {
  it("validates order room membership, authorities, and payload replay", () => {
    expect(() =>
      validateOrderRoomTimeline(
        [
          envelope("io.marketplace.actor.customer.bound", {
            customer_id: validCustomerBinding.customer_id,
            status: validCustomerBinding.status,
            display_name: "Acme Procurement",
            instance_id: "customer.example",
            authorized_representatives: ["@buyer:customer.example"],
            accepted_payment_adapters: validCustomerBinding.accepted_payment_adapters,
            accepted_arbitration_policies: validCustomerBinding.accepted_arbitration_policies
          }),
          envelope("io.marketplace.order.created", validOrderCreated),
          envelope("io.marketplace.order.accepted", { order_id: validOrderCreated.order_id }, "@market:shop.example")
        ],
        {
          roomId: validOrderCreated.room_id,
          sellerAsUser: "@market:shop.example",
          customerAsUser: "@market:customer.example",
          arbiterAsUser: "@market:arbiter.example",
          requiredMembers: ["@market:shop.example", "@market:customer.example", "@market:arbiter.example"],
          members: ["@market:shop.example", "@market:customer.example", "@market:arbiter.example"]
        }
      )
    ).not.toThrow();
  });

  it("rejects order rooms missing required parties", () => {
    expect(() =>
      validateOrderRoomTimeline([], {
        roomId: validOrderCreated.room_id,
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example",
        requiredMembers: ["@market:shop.example", "@market:customer.example", "@market:arbiter.example"],
        members: ["@market:shop.example"]
      })
    ).toThrow(/member/i);
  });
});
