import { describe, expect, it } from "vitest";
import { assertEventAuthority } from "../../src/order/authority.js";

describe("assertEventAuthority", () => {
  const authorities = {
    sellerAsUser: "@market:shop.example",
    customerAsUser: "@market:customer.example",
    arbiterAsUser: "@market:arbiter.example"
  };
  const paymentEventTypes = [
    "io.marketplace.payment.intent.created",
    "io.marketplace.payment.authorized",
    "io.marketplace.payment.captured",
    "io.marketplace.payment.failed",
    "io.marketplace.payment.cancelled",
    "io.marketplace.payment.refund.requested",
    "io.marketplace.payment.refunded",
    "io.marketplace.payment.chargeback.opened"
  ];

  it("allows seller AS to capture payment and grant entitlement", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:shop.example", authorities)
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.granted", "@market:shop.example", authorities)
    ).not.toThrow();
  });

  it("allows seller AS to activate and complete entitlements", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.activated", "@market:shop.example", authorities)
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.completed", "@market:shop.example", authorities)
    ).not.toThrow();
  });

  it("rejects customer AS for revoked and expired entitlements", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.revoked", "@market:customer.example", authorities)
    ).toThrow(/seller/);

    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.expired", "@market:customer.example", authorities)
    ).toThrow(/seller/);
  });

  it("rejects payment capture from customer AS", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:customer.example", authorities)
    ).toThrow(/seller/);
  });

  it.each(paymentEventTypes)("rejects customer AS for %s", (eventType) => {
    expect(() => assertEventAuthority(eventType, "@market:customer.example", authorities)).toThrow(/seller|payment/);
  });

  it("allows configured payment AS to capture payment", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:payments.example", {
        ...authorities,
        paymentAsUsers: ["@market:payments.example"]
      })
    ).not.toThrow();
  });

  it("allows only arbiter AS to issue rulings", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:arbiter.example", authorities)
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:shop.example", authorities)
    ).toThrow(/arbiter/);
  });

  it("rejects dispute opening and evidence from outsider AS", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.opened", "@market:outsider.example", authorities)
    ).toThrow(/seller|customer|arbiter/);

    expect(() =>
      assertEventAuthority("io.marketplace.dispute.evidence.submitted", "@market:outsider.example", authorities)
    ).toThrow(/seller|customer|arbiter/);
  });

  it("allows customer AS to open dispute", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.opened", "@market:customer.example", authorities)
    ).not.toThrow();
  });

  it("allows seller AS to submit dispute evidence", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.evidence.submitted", "@market:shop.example", authorities)
    ).not.toThrow();
  });

  it("allows only arbiter AS to close disputes", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.closed", "@market:shop.example", authorities)
    ).toThrow(/arbiter/);

    expect(() =>
      assertEventAuthority("io.marketplace.dispute.closed", "@market:arbiter.example", authorities)
    ).not.toThrow();
  });
});
