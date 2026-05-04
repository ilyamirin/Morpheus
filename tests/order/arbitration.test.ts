import { describe, expect, it } from "vitest";
import { validateArbitrationPolicy, validateArbitrationFlow } from "../../src/order/arbitration.js";
import { validOrderCreated } from "../../src/conformance/fixtures.js";

describe("arbitration policy and dispute flow", () => {
  it("accepts a complete arbitration policy", () => {
    expect(() =>
      validateArbitrationPolicy({
        policy_id: "standard-digital-v1",
        version: "1",
        arbitration_window: "P14D",
        accepted_remedies: ["full_refund", "partial_refund", "entitlement_reissue"],
        binding: true
      })
    ).not.toThrow();
  });

  it("rejects dispute rulings with evidence refs outside the order room timeline", () => {
    expect(() =>
      validateArbitrationFlow([
        { type: "io.marketplace.dispute.opened", event_id: "$disp", room_id: validOrderCreated.room_id, body: { order_id: validOrderCreated.order_id, dispute_id: "disp:arbiter.example:01JDISP" } },
        {
          type: "io.marketplace.dispute.ruling.issued",
          event_id: "$ruling",
          room_id: validOrderCreated.room_id,
          body: {
            order_id: validOrderCreated.order_id,
            dispute_id: "disp:arbiter.example:01JDISP",
            ruling: "refund_required",
            remedy: { type: "full_refund" },
            evidence_refs: ["$missing"],
            binding: true
          }
        }
      ])
    ).toThrow(/evidence/i);
  });

  it("requires refund execution after a binding refund ruling", () => {
    expect(() =>
      validateArbitrationFlow([
        { type: "io.marketplace.dispute.opened", event_id: "$disp", room_id: validOrderCreated.room_id, body: { order_id: validOrderCreated.order_id, dispute_id: "disp:arbiter.example:01JDISP" } },
        { type: "io.marketplace.dispute.evidence.submitted", event_id: "$ev", room_id: validOrderCreated.room_id, body: { order_id: validOrderCreated.order_id, dispute_id: "disp:arbiter.example:01JDISP" } },
        {
          type: "io.marketplace.dispute.ruling.issued",
          event_id: "$ruling",
          room_id: validOrderCreated.room_id,
          body: {
            order_id: validOrderCreated.order_id,
            dispute_id: "disp:arbiter.example:01JDISP",
            ruling: "refund_required",
            remedy: { type: "full_refund" },
            evidence_refs: ["$ev"],
            binding: true
          }
        }
      ])
    ).toThrow(/refund/i);
  });
});
