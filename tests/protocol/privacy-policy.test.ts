import { describe, expect, it } from "vitest";
import { validateMarketplacePrivacy } from "../../src/protocol/privacy-policy.js";

describe("validateMarketplacePrivacy", () => {
  it("rejects order or customer data in catalog events", () => {
    expect(() =>
      validateMarketplacePrivacy(
        { type: "io.marketplace.product.upserted", content: { body: { product_id: "prod:shop.example:1", customer_id: "customer:customer.example:1" } } },
        "catalog"
      )
    ).toThrow(/catalog.*customer/i);
  });

  it("rejects bearer secrets in order events", () => {
    expect(() =>
      validateMarketplacePrivacy(
        {
          type: "io.marketplace.entitlement.granted",
          content: { body: { external_ref: "https://files.example/download?token=secret-bearer-token" } }
        },
        "order"
      )
    ).toThrow(/secret/i);
  });
});
