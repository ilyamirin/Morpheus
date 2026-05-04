import { describe, expect, it } from "vitest";
import { assertEventAuthority } from "../../src/order/authority.js";

describe("assertEventAuthority", () => {
  it("allows seller AS to capture payment and grant entitlement", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.granted", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).not.toThrow();
  });

  it("rejects payment capture from customer AS", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:customer.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow(/seller/);
  });

  it("allows only arbiter AS to issue rulings", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:arbiter.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow(/arbiter/);
  });
});
