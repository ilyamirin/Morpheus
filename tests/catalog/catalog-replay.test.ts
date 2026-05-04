import { describe, expect, it } from "vitest";
import { replayCatalogTimeline, validateCatalogSnapshot } from "../../src/catalog/catalog-replay.js";
import { sha256Canonical } from "../../src/protocol/canonical-json.js";
import { validCatalog } from "../../src/conformance/fixtures.js";

const snapshot = {
  snapshot_id: "snap:shop.example:01JVALID",
  sequence: 1,
  covers_events_until: "$snapshot",
  sellers: [{ ...validCatalog.seller }],
  products: [{ ...validCatalog.product }],
  offers: [{ ...validCatalog.offer }],
  tombstones: []
};

describe("catalog snapshot and replay", () => {
  it("validates canonical snapshot hashes", () => {
    expect(() => validateCatalogSnapshot(snapshot, { expectedSha256: sha256Canonical(snapshot) })).not.toThrow();
  });

  it("rejects snapshot hash mismatches", () => {
    expect(() => validateCatalogSnapshot(snapshot, { expectedSha256: "sha256:" + "0".repeat(64) })).toThrow(/hash/i);
  });

  it("deduplicates Matrix deltas by event id while replaying catalog state", () => {
    const delta = {
      type: "io.marketplace.offer.upserted",
      event_id: "$delta",
      catalog_sequence: 2,
      body: { ...validCatalog.offer, revision: 4 }
    };
    const catalog = replayCatalogTimeline([delta, delta], snapshot, { instanceId: "shop.example" });
    expect(catalog.getOffer(validCatalog.offer.offerId)?.revision).toBe(4);
  });

  it("maps wire-format snake_case catalog records into local index records", () => {
    const wireSnapshot = {
      snapshot_id: "snap:shop.example:01JWIRE",
      sequence: 1,
      covers_events_until: "$snapshot",
      sellers: [{ seller_id: "seller:shop.example:01JSELLER", status: "active" }],
      products: [
        {
          product_id: "prod:shop.example:01JPROD",
          seller_id: "seller:shop.example:01JSELLER",
          revision: 1,
          terms_hash: "sha256:" + "3".repeat(64)
        }
      ],
      offers: [
        {
          offer_id: "offer:shop.example:01JOFFER",
          product_id: "prod:shop.example:01JPROD",
          seller_id: "seller:shop.example:01JSELLER",
          revision: 3,
          price: { amount: "100.00", currency: "USD" },
          entitlement_type: "booking_slot",
          payment_capture_policy: "before_entitlement",
          seller_terms_hash: "sha256:" + "1".repeat(64),
          offer_terms_hash: "sha256:" + "2".repeat(64)
        }
      ],
      tombstones: []
    };

    const catalog = replayCatalogTimeline([], wireSnapshot, { instanceId: "shop.example" });

    expect(catalog.getOffer("offer:shop.example:01JOFFER")?.offerTermsHash).toBe("sha256:" + "2".repeat(64));
  });

  it("rejects malformed snapshot documents before replay", () => {
    expect(() => validateCatalogSnapshot({ ...snapshot, snapshot_id: "snap_legacy" })).toThrow(/snapshot/i);
  });

  it("rejects missing catalog delta sequence gaps", () => {
    expect(() =>
      replayCatalogTimeline(
        [
          {
            type: "io.marketplace.offer.upserted",
            event_id: "$delta-3",
            catalog_sequence: 3,
            body: { ...validCatalog.offer, revision: 4 }
          }
        ],
        snapshot,
        { instanceId: "shop.example" }
      )
    ).toThrow(/sequence/i);
  });

  it("applies tombstones for withdrawn offers during catalog replay", () => {
    const catalog = replayCatalogTimeline([], {
      ...snapshot,
      tombstones: [{ object_id: validCatalog.offer.offerId, revision: 4, reason: "withdrawn" }]
    }, { instanceId: "shop.example" });
    expect(catalog.getOffer(validCatalog.offer.offerId)).toBeUndefined();
  });
});
