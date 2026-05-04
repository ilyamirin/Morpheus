import { MarketplaceValidationError } from "../protocol/errors.js";

export type OrderState =
  | "draft"
  | "created"
  | "accepted"
  | "payment_intent_created"
  | "payment_authorized"
  | "payment_captured"
  | "entitlement_granted"
  | "completed"
  | "cancelled"
  | "rejected"
  | "refunded"
  | "dispute_opened"
  | "ruling_issued"
  | "dispute_resolved"
  | "expired";

const transitions: Record<OrderState, Partial<Record<string, OrderState>>> = {
  draft: {
    "io.marketplace.order.created": "created"
  },
  created: {
    "io.marketplace.order.accepted": "accepted",
    "io.marketplace.order.rejected": "rejected",
    "io.marketplace.order.cancelled": "cancelled"
  },
  accepted: {
    "io.marketplace.payment.intent.created": "payment_intent_created",
    "io.marketplace.dispute.opened": "dispute_opened",
    "io.marketplace.order.cancelled": "cancelled"
  },
  payment_intent_created: {
    "io.marketplace.payment.authorized": "payment_authorized",
    "io.marketplace.payment.failed": "cancelled",
    "io.marketplace.payment.cancelled": "cancelled"
  },
  payment_authorized: {
    "io.marketplace.payment.captured": "payment_captured",
    "io.marketplace.payment.failed": "cancelled"
  },
  payment_captured: {
    "io.marketplace.entitlement.granted": "entitlement_granted",
    "io.marketplace.dispute.opened": "dispute_opened",
    "io.marketplace.payment.refunded": "refunded"
  },
  entitlement_granted: {
    "io.marketplace.order.completed": "completed",
    "io.marketplace.dispute.opened": "dispute_opened",
    "io.marketplace.entitlement.revoked": "cancelled"
  },
  dispute_opened: {
    "io.marketplace.dispute.ruling.issued": "ruling_issued"
  },
  ruling_issued: {
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.entitlement.granted": "entitlement_granted",
    "io.marketplace.dispute.closed": "dispute_resolved"
  },
  completed: {},
  cancelled: {},
  rejected: {},
  refunded: {},
  dispute_resolved: {},
  expired: {}
};

export class OrderStateMachine {
  public state: OrderState = "draft";

  apply(eventType: string): void {
    const next = transitions[this.state][eventType];
    if (!next) {
      throw new MarketplaceValidationError(
        "INVALID_STATE_TRANSITION",
        `Invalid transition from ${this.state} using ${eventType}`,
        { state: this.state, eventType }
      );
    }
    this.state = next;
  }
}
