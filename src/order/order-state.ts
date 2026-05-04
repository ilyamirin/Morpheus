import { MarketplaceValidationError } from "../protocol/errors.js";

export type OrderState =
  | "draft"
  | "created"
  | "accepted"
  | "payment_intent_created"
  | "payment_authorized"
  | "payment_captured"
  | "refund_requested"
  | "entitlement_granted_before_capture"
  | "entitlement_granted"
  | "entitlement_activated"
  | "entitlement_completed"
  | "completed"
  | "cancelled"
  | "rejected"
  | "refunded"
  | "chargeback_opened"
  | "dispute_opened_pre_payment"
  | "dispute_opened_after_capture"
  | "dispute_opened_after_entitlement"
  | "ruling_issued_pre_payment"
  | "ruling_issued_after_capture"
  | "ruling_issued_after_entitlement"
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
    "io.marketplace.dispute.opened": "dispute_opened_pre_payment",
    "io.marketplace.order.cancelled": "cancelled"
  },
  payment_intent_created: {
    "io.marketplace.payment.authorized": "payment_authorized",
    "io.marketplace.payment.failed": "cancelled",
    "io.marketplace.payment.cancelled": "cancelled"
  },
  payment_authorized: {
    "io.marketplace.payment.captured": "payment_captured",
    "io.marketplace.entitlement.granted": "entitlement_granted_before_capture",
    "io.marketplace.payment.failed": "cancelled"
  },
  payment_captured: {
    "io.marketplace.entitlement.granted": "entitlement_granted",
    "io.marketplace.dispute.opened": "dispute_opened_after_capture",
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened"
  },
  refund_requested: {
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened"
  },
  entitlement_granted_before_capture: {
    "io.marketplace.payment.captured": "entitlement_granted",
    "io.marketplace.dispute.opened": "dispute_opened_after_entitlement",
    "io.marketplace.payment.failed": "cancelled",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened",
    "io.marketplace.entitlement.revoked": "cancelled"
  },
  entitlement_granted: {
    "io.marketplace.order.completed": "completed",
    "io.marketplace.dispute.opened": "dispute_opened_after_entitlement",
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened",
    "io.marketplace.entitlement.activated": "entitlement_activated",
    "io.marketplace.entitlement.completed": "entitlement_completed",
    "io.marketplace.entitlement.expired": "expired",
    "io.marketplace.entitlement.revoked": "cancelled"
  },
  entitlement_activated: {
    "io.marketplace.order.completed": "completed",
    "io.marketplace.dispute.opened": "dispute_opened_after_entitlement",
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened",
    "io.marketplace.entitlement.completed": "entitlement_completed",
    "io.marketplace.entitlement.expired": "expired",
    "io.marketplace.entitlement.revoked": "cancelled"
  },
  entitlement_completed: {
    "io.marketplace.order.completed": "completed",
    "io.marketplace.dispute.opened": "dispute_opened_after_entitlement",
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened",
    "io.marketplace.entitlement.expired": "expired",
    "io.marketplace.entitlement.revoked": "cancelled"
  },
  dispute_opened_pre_payment: {
    "io.marketplace.dispute.evidence.submitted": "dispute_opened_pre_payment",
    "io.marketplace.dispute.ruling.issued": "ruling_issued_pre_payment"
  },
  dispute_opened_after_capture: {
    "io.marketplace.dispute.evidence.submitted": "dispute_opened_after_capture",
    "io.marketplace.dispute.ruling.issued": "ruling_issued_after_capture"
  },
  dispute_opened_after_entitlement: {
    "io.marketplace.dispute.evidence.submitted": "dispute_opened_after_entitlement",
    "io.marketplace.dispute.ruling.issued": "ruling_issued_after_entitlement"
  },
  ruling_issued_pre_payment: {
    "io.marketplace.dispute.evidence.submitted": "ruling_issued_pre_payment",
    "io.marketplace.dispute.closed": "dispute_resolved"
  },
  ruling_issued_after_capture: {
    "io.marketplace.dispute.evidence.submitted": "ruling_issued_after_capture",
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened",
    "io.marketplace.entitlement.granted": "entitlement_granted",
    "io.marketplace.dispute.closed": "dispute_resolved"
  },
  ruling_issued_after_entitlement: {
    "io.marketplace.dispute.evidence.submitted": "ruling_issued_after_entitlement",
    "io.marketplace.payment.refund.requested": "refund_requested",
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.payment.chargeback.opened": "chargeback_opened",
    "io.marketplace.dispute.closed": "dispute_resolved"
  },
  completed: {},
  cancelled: {},
  rejected: {},
  refunded: {
    "io.marketplace.payment.chargeback.opened": "chargeback_opened"
  },
  chargeback_opened: {},
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
