import { z } from "zod";
import { MarketplaceValidationError } from "../protocol/errors.js";

export const arbitrationPolicySchema = z.object({
  policy_id: z.string().min(1),
  version: z.string().min(1),
  arbitration_window: z.string().min(1),
  accepted_remedies: z.array(z.enum(["full_refund", "partial_refund", "entitlement_reissue", "service_completion", "no_fault"])),
  binding: z.boolean()
});

export interface ArbitrationFlowEvent {
  type: string;
  event_id: string;
  room_id: string;
  body: Record<string, unknown>;
}

export function validateArbitrationPolicy(policy: unknown): void {
  arbitrationPolicySchema.parse(policy);
}

export function validateArbitrationFlow(events: ArbitrationFlowEvent[]): void {
  const eventIds = new Set(events.map((event) => event.event_id));
  let bindingRefundRequired = false;
  let refundExecuted = false;

  for (const event of events) {
    if (event.type === "io.marketplace.dispute.ruling.issued") {
      const evidenceRefs = event.body.evidence_refs;
      if (!Array.isArray(evidenceRefs) || evidenceRefs.some((ref) => typeof ref !== "string" || !eventIds.has(ref))) {
        throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Dispute ruling evidence_refs must point to order room events", {
          eventId: event.event_id,
          evidenceRefs
        });
      }
      if (event.body.binding === true && event.body.ruling === "refund_required") {
        bindingRefundRequired = true;
      }
    }
    if (event.type === "io.marketplace.payment.refund.requested" || event.type === "io.marketplace.payment.refunded") {
      refundExecuted = true;
    }
  }

  if (bindingRefundRequired && !refundExecuted) {
    throw new MarketplaceValidationError("POLICY_VIOLATION", "Binding refund ruling requires a refund event", {
      ruling: "refund_required"
    });
  }
}
