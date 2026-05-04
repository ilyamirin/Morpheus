import { describe, expect, it } from "vitest";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";
import { LocalSearchIndex, shouldIndexCatalogRoom } from "../../src/catalog/indexing-policy.js";
import { validCatalog } from "../../src/conformance/fixtures.js";

describe("local indexing policy", () => {
  it("indexes only catalog rooms from instances with catalog and indexing capabilities", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["catalog", "indexing"] });
    expect(shouldIndexCatalogRoom("shop.example", allowlist)).toBe(true);
    expect(shouldIndexCatalogRoom("other.example", allowlist)).toBe(false);
  });

  it("removes withdrawn offers and suspended seller content from the local search index", () => {
    const index = new LocalSearchIndex();
    index.apply({
      type: "io.marketplace.offer.upserted",
      body: { ...validCatalog.offer, status: "active", title: "Slot" }
    });
    expect(index.hasOffer(validCatalog.offer.offerId)).toBe(true);

    index.apply({ type: "io.marketplace.offer.withdrawn", body: { offer_id: validCatalog.offer.offerId, revision: 4 } });
    expect(index.hasOffer(validCatalog.offer.offerId)).toBe(false);

    index.apply({
      type: "io.marketplace.offer.upserted",
      body: { ...validCatalog.offer, offerId: validCatalog.offer.offerId, status: "active", revision: 5 }
    });
    index.apply({ type: "io.marketplace.actor.seller.suspended", body: { seller_id: validCatalog.seller.sellerId, status: "suspended" } });
    expect(index.hasOffer(validCatalog.offer.offerId)).toBe(false);
  });
});
