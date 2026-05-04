import { describe, expect, it } from "vitest";
import { replayCatalogTimeline, validateCatalogSnapshot } from "../../src/catalog/catalog-replay.js";
import { sha256Canonical } from "../../src/protocol/canonical-json.js";
import { validCatalog } from "../../src/conformance/fixtures.js";

const snapshot = {
  snapshot_id: "snap_01JVALID",
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
      body: { ...validCatalog.offer, revision: 4 }
    };
    const catalog = replayCatalogTimeline([delta, delta], snapshot, { instanceId: "shop.example" });
    expect(catalog.getOffer(validCatalog.offer.offerId)?.revision).toBe(4);
  });
});
