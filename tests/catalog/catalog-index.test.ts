import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";

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

  it("does not return offers after their seller is suspended", () => {
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

    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "suspended" });

    expect(index.getOffer("offer:shop.example:01JOFFER")).toBeUndefined();
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

  it("rejects a same-sequence snapshot with a different hash", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });

    let error: unknown;
    try {
      index.applySnapshot({ snapshotId: "snap_01J_ALT", sequence: 1, sha256: "def", coversEventsUntil: "$snap" });
    } catch (caught) {
      error = caught;
    }

    expect(error).toBeInstanceOf(MarketplaceValidationError);
    expect((error as MarketplaceValidationError).code).toBe("CATALOG_REFERENCE_MISMATCH");
    expect((error as MarketplaceValidationError).message).toBe("Snapshot hash mismatch");
  });

  it("treats a same-sequence snapshot with the same hash as idempotent", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });

    expect(() =>
      index.applySnapshot({ snapshotId: "snap_01J_REPLAY", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" })
    ).not.toThrow();
  });

  it("rejects a seller from a different catalog instance", () => {
    const index = new CatalogIndex("shop.example");

    expect(() => index.upsertSeller({ sellerId: "seller:evil.example:01JSELLER", status: "active" })).toThrow(
      /Catalog reference mismatch/
    );
  });

  it("rejects a product from a different catalog instance", () => {
    const index = new CatalogIndex("shop.example");
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });

    expect(() =>
      index.upsertProduct({ productId: "prod:evil.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 })
    ).toThrow(/Catalog reference mismatch/);
  });

  it("rejects an offer from a different catalog instance", () => {
    const index = new CatalogIndex("shop.example");
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
    index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 });

    expect(() =>
      index.upsertOffer({
        offerId: "offer:evil.example:01JOFFER",
        productId: "prod:shop.example:01JPROD",
        sellerId: "seller:shop.example:01JSELLER",
        revision: 1,
        price: { amount: "100.00", currency: "USD" },
        entitlementType: "booking_slot"
      })
    ).toThrow(/Catalog reference mismatch/);
  });

  it("rejects offers for unknown products", () => {
    const index = new CatalogIndex("shop.example");
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });

    expect(() =>
      index.upsertOffer({
        offerId: "offer:shop.example:01JOFFER",
        productId: "prod:shop.example:01JPROD",
        sellerId: "seller:shop.example:01JSELLER",
        revision: 1,
        price: { amount: "100.00", currency: "USD" },
        entitlementType: "booking_slot"
      })
    ).toThrow(/Unknown product/);
  });

  it("rejects offers whose seller does not match the product seller", () => {
    const index = new CatalogIndex("shop.example");
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JOTHER", status: "active" });
    index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 });

    expect(() =>
      index.upsertOffer({
        offerId: "offer:shop.example:01JOFFER",
        productId: "prod:shop.example:01JPROD",
        sellerId: "seller:shop.example:01JOTHER",
        revision: 1,
        price: { amount: "100.00", currency: "USD" },
        entitlementType: "booking_slot"
      })
    ).toThrow(/Product seller mismatch/);
  });
});
