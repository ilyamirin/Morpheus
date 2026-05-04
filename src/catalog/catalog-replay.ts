import { CatalogIndex, type OfferRecord, type ProductRecord, type SellerRecord, type SnapshotRecord } from "./catalog-index.js";
import { assertSha256Matches } from "../protocol/canonical-json.js";
import { MarketplaceValidationError } from "../protocol/errors.js";

export interface CatalogSnapshotDocument {
  snapshot_id: string;
  sequence: number;
  covers_events_until: string;
  sellers: SellerRecord[];
  products: ProductRecord[];
  offers: OfferRecord[];
  tombstones: Array<{ object_id: string; revision: number; reason: string }>;
}

export interface CatalogDeltaEvent {
  type: string;
  event_id: string;
  body: Record<string, unknown>;
}

export function validateCatalogSnapshot(
  snapshot: CatalogSnapshotDocument,
  context: { expectedSha256?: string } = {}
): void {
  if (context.expectedSha256) {
    assertSha256Matches(snapshot, context.expectedSha256);
  }
}

export function replayCatalogTimeline(
  events: CatalogDeltaEvent[],
  snapshot: CatalogSnapshotDocument,
  context: { instanceId: string }
): CatalogIndex {
  const catalog = new CatalogIndex(context.instanceId);
  const baseSnapshot: SnapshotRecord = {
    snapshotId: snapshot.snapshot_id,
    sequence: snapshot.sequence,
    sha256: "",
    coversEventsUntil: snapshot.covers_events_until
  };
  catalog.applySnapshot(baseSnapshot);
  for (const seller of snapshot.sellers) {
    catalog.upsertSeller(seller);
  }
  for (const product of snapshot.products) {
    catalog.upsertProduct(product);
  }
  for (const offer of snapshot.offers) {
    catalog.upsertOffer(offer);
  }
  for (const tombstone of snapshot.tombstones) {
    catalog.removeObject(tombstone.object_id);
  }

  const seen = new Set<string>();
  for (const event of events) {
    if (seen.has(event.event_id)) {
      continue;
    }
    seen.add(event.event_id);
    applyDelta(catalog, event);
  }
  return catalog;
}

function applyDelta(catalog: CatalogIndex, event: CatalogDeltaEvent): void {
  switch (event.type) {
    case "io.marketplace.actor.seller.announced":
      catalog.upsertSeller(event.body as unknown as SellerRecord);
      return;
    case "io.marketplace.actor.seller.suspended":
      catalog.upsertSeller(event.body as unknown as SellerRecord);
      return;
    case "io.marketplace.product.upserted":
      catalog.upsertProduct(event.body as unknown as ProductRecord);
      return;
    case "io.marketplace.offer.upserted":
      catalog.upsertOffer(event.body as unknown as OfferRecord);
      return;
    case "io.marketplace.product.withdrawn":
    case "io.marketplace.offer.withdrawn":
      catalog.removeObject((event.body as { product_id?: string; offer_id?: string }).product_id ?? (event.body as { offer_id?: string }).offer_id ?? "");
      return;
    default:
      throw new MarketplaceValidationError("ROOM_PROFILE_VIOLATION", `Unsupported catalog delta ${event.type}`, {
        eventType: event.type
      });
  }
}
