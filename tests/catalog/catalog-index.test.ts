import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";

describe("CatalogIndex", () => {
  it("accepts snapshot then seller/product/offer deltas and retrieves offer revision", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
    index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 });
    index.upsertOffer({
      offerId: "offer:shop.example:01JOFFER",
      productId: "prod:shop.example:01JPROD",
      sellerId: "seller:shop.example:01JSELLER",
      revision: 1,
      price: { amount: "100.00", currency: "USD" },
      entitlementType: "booking_slot"
    });

    expect(index.getOffer("offer:shop.example:01JOFFER")?.revision).toBe(1);
  });

  it("rejects offers for suspended sellers", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "suspended" });

    expect(() =>
      index.upsertOffer({
        offerId: "offer:shop.example:01JOFFER",
        productId: "prod:shop.example:01JPROD",
        sellerId: "seller:shop.example:01JSELLER",
        revision: 1,
        price: { amount: "100.00", currency: "USD" },
        entitlementType: "booking_slot"
      })
    ).toThrow(/not active/);
  });

  it("rejects revision rollback on product", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
    index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 2 });
    expect(() =>
      index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 })
    ).toThrow(/rollback/);
  });
});
