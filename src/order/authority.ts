import { MarketplaceValidationError } from "../protocol/errors.js";

export interface OrderAuthorities {
  sellerAsUser: string;
  customerAsUser: string;
  arbiterAsUser: string;
}

export function assertEventAuthority(eventType: string, sender: string, authorities: OrderAuthorities): void {
  if (
    eventType === "io.marketplace.payment.intent.created" ||
    eventType === "io.marketplace.payment.authorized" ||
    eventType === "io.marketplace.payment.captured" ||
    eventType === "io.marketplace.entitlement.granted"
  ) {
    assertSender(sender, authorities.sellerAsUser, "seller");
    return;
  }

  if (eventType === "io.marketplace.dispute.ruling.issued") {
    assertSender(sender, authorities.arbiterAsUser, "arbiter");
  }
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
