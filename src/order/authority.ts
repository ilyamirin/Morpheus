import { MarketplaceValidationError } from "../protocol/errors.js";

export interface OrderAuthorities {
  sellerAsUser: string;
  customerAsUser: string;
  arbiterAsUser: string;
  paymentAsUsers?: string[];
}

const paymentEventTypes = new Set([
  "io.marketplace.payment.intent.created",
  "io.marketplace.payment.authorized",
  "io.marketplace.payment.captured",
  "io.marketplace.payment.failed",
  "io.marketplace.payment.cancelled",
  "io.marketplace.payment.refund.requested",
  "io.marketplace.payment.refunded",
  "io.marketplace.payment.chargeback.opened"
]);

const entitlementEventTypes = new Set([
  "io.marketplace.entitlement.granted",
  "io.marketplace.entitlement.activated",
  "io.marketplace.entitlement.completed",
  "io.marketplace.entitlement.revoked",
  "io.marketplace.entitlement.expired"
]);

export function assertEventAuthority(eventType: string, sender: string, authorities: OrderAuthorities): void {
  if (paymentEventTypes.has(eventType)) {
    assertPaymentSender(sender, authorities);
    return;
  }

  if (entitlementEventTypes.has(eventType)) {
    assertSender(sender, authorities.sellerAsUser, "seller");
    return;
  }

  if (eventType === "io.marketplace.dispute.ruling.issued" || eventType === "io.marketplace.dispute.closed") {
    assertSender(sender, authorities.arbiterAsUser, "arbiter");
    return;
  }

  if (
    eventType === "io.marketplace.dispute.opened" ||
    eventType === "io.marketplace.dispute.evidence.submitted"
  ) {
    assertSenderIn(sender, [
      { userId: authorities.sellerAsUser, role: "seller" },
      { userId: authorities.customerAsUser, role: "customer" },
      { userId: authorities.arbiterAsUser, role: "arbiter" }
    ]);
  }
}

function assertPaymentSender(sender: string, authorities: OrderAuthorities): void {
  assertSenderIn(sender, [
    { userId: authorities.sellerAsUser, role: "seller" },
    ...(authorities.paymentAsUsers ?? []).map((userId) => ({ userId, role: "payment" }))
  ]);
}

function assertSender(sender: string, expected: string, role: string): void {
  if (sender !== expected) {
    throw new MarketplaceValidationError(
      "UNAUTHORIZED_SENDER",
      `Expected ${role} authority ${expected}, got ${sender}`,
      { sender, expected, role }
    );
  }
}

function assertSenderIn(sender: string, allowed: Array<{ userId: string; role: string }>): void {
  if (!allowed.some(({ userId }) => sender === userId)) {
    throw new MarketplaceValidationError(
      "UNAUTHORIZED_SENDER",
      `Expected ${allowed.map(({ role }) => role).join("/")} authority, got ${sender}`,
      { sender, allowed }
    );
  }
}
