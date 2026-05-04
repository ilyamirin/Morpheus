import { CatalogIndex, type OfferRecord, type ProductRecord, type SellerRecord, type SnapshotRecord } from "./catalog-index.js";
import { assertSha256Matches } from "../protocol/canonical-json.js";
import { MarketplaceValidationError } from "../protocol/errors.js";
import { isProtocolObjectId } from "../protocol/ids.js";

type WireSellerRecord = { seller_id: string; status: string };
type WireProductRecord = { product_id: string; seller_id: string; revision: number; terms_hash?: string };
type WireOfferRecord = {
  offer_id: string;
  product_id: string;
  seller_id: string;
  revision: number;
  price: OfferRecord["price"];
  entitlement_type: string;
  payment_capture_policy?: string;
  seller_terms_hash?: string;
  offer_terms_hash?: string;
};

export interface CatalogSnapshotDocument {
  snapshot_id: string;
  sequence: number;
  covers_events_until: string;
  sellers: Array<SellerRecord | WireSellerRecord>;
  products: Array<ProductRecord | WireProductRecord>;
  offers: Array<OfferRecord | WireOfferRecord>;
  tombstones: Array<{ object_id: string; revision: number; reason: string }>;
}

export interface CatalogDeltaEvent {
  type: string;
  event_id: string;
  catalog_sequence: number;
  body: Record<string, unknown>;
}

export function validateCatalogSnapshot(
  snapshot: CatalogSnapshotDocument,
  context: { expectedSha256?: string } = {}
): void {
  if (!Array.isArray(snapshot.sellers) || !Array.isArray(snapshot.products) || !Array.isArray(snapshot.offers) || !Array.isArray(snapshot.tombstones)) {
    throw new MarketplaceValidationError("MISSING_REQUIRED_FIELD", "Catalog snapshot must include sellers, products, offers, and tombstones arrays", {
      snapshotId: snapshot.snapshot_id
    });
  }
  if (!isProtocolObjectId(snapshot.snapshot_id, "snap")) {
    throw new MarketplaceValidationError("MISSING_REQUIRED_FIELD", "Invalid catalog snapshot id", {
      snapshotId: snapshot.snapshot_id
    });
  }
  if (context.expectedSha256) {
    assertSha256Matches(snapshot, context.expectedSha256);
  }
}

export function replayCatalogTimeline(
  events: CatalogDeltaEvent[],
  snapshot: CatalogSnapshotDocument,
  context: { instanceId: string }
): CatalogIndex {
  validateCatalogSnapshot(snapshot);
  const catalog = new CatalogIndex(context.instanceId);
  const baseSnapshot: SnapshotRecord = {
    snapshotId: snapshot.snapshot_id,
    sequence: snapshot.sequence,
    sha256: "",
    coversEventsUntil: snapshot.covers_events_until
  };
  catalog.applySnapshot(baseSnapshot);
  for (const seller of snapshot.sellers) {
    catalog.upsertSeller(normalizeSeller(seller));
  }
  for (const product of snapshot.products) {
    catalog.upsertProduct(normalizeProduct(product));
  }
  for (const offer of snapshot.offers) {
    catalog.upsertOffer(normalizeOffer(offer));
  }
  for (const tombstone of snapshot.tombstones) {
    catalog.removeObject(tombstone.object_id);
  }

  const seen = new Set<string>();
  let expectedSequence = snapshot.sequence + 1;
  for (const event of events) {
    if (seen.has(event.event_id)) {
      continue;
    }
    seen.add(event.event_id);
    if (event.catalog_sequence !== expectedSequence) {
      throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Catalog delta sequence gap", {
        expectedSequence,
        actualSequence: event.catalog_sequence
      });
    }
    expectedSequence += 1;
    applyDelta(catalog, event);
  }
  return catalog;
}

function applyDelta(catalog: CatalogIndex, event: CatalogDeltaEvent): void {
  switch (event.type) {
    case "io.marketplace.actor.seller.announced":
      catalog.upsertSeller(normalizeSeller(event.body as unknown as SellerRecord | WireSellerRecord));
      return;
    case "io.marketplace.actor.seller.suspended":
      catalog.upsertSeller(normalizeSeller(event.body as unknown as SellerRecord | WireSellerRecord));
      return;
    case "io.marketplace.product.upserted":
      catalog.upsertProduct(normalizeProduct(event.body as unknown as ProductRecord | WireProductRecord));
      return;
    case "io.marketplace.offer.upserted":
      catalog.upsertOffer(normalizeOffer(event.body as unknown as OfferRecord | WireOfferRecord));
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

function normalizeSeller(record: SellerRecord | WireSellerRecord): SellerRecord {
  if ("sellerId" in record) {
    return record;
  }
  return {
    sellerId: record.seller_id,
    status: normalizeSellerStatus(record.status)
  };
}

function normalizeProduct(record: ProductRecord | WireProductRecord): ProductRecord {
  if ("productId" in record) {
    return record;
  }
  return {
    productId: record.product_id,
    sellerId: record.seller_id,
    revision: record.revision,
    ...(record.terms_hash ? { termsHash: record.terms_hash } : {})
  };
}

function normalizeOffer(record: OfferRecord | WireOfferRecord): OfferRecord {
  if ("offerId" in record) {
    return record;
  }
  return {
    offerId: record.offer_id,
    productId: record.product_id,
    sellerId: record.seller_id,
    revision: record.revision,
    price: record.price,
    entitlementType: record.entitlement_type as OfferRecord["entitlementType"],
    ...(record.payment_capture_policy ? { paymentCapturePolicy: normalizeCapturePolicy(record.payment_capture_policy) } : {}),
    ...(record.seller_terms_hash ? { sellerTermsHash: record.seller_terms_hash } : {}),
    ...(record.offer_terms_hash ? { offerTermsHash: record.offer_terms_hash } : {})
  };
}

function normalizeSellerStatus(status: string): SellerRecord["status"] {
  if (status !== "active" && status !== "suspended") {
    throw new MarketplaceValidationError("MISSING_REQUIRED_FIELD", "Invalid seller status in catalog record", { status });
  }
  return status;
}

function normalizeCapturePolicy(policy: string): "before_entitlement" | "after_entitlement" {
  if (policy !== "before_entitlement" && policy !== "after_entitlement") {
    throw new MarketplaceValidationError("MISSING_REQUIRED_FIELD", "Invalid payment capture policy in catalog record", {
      policy
    });
  }
  return policy;
}
