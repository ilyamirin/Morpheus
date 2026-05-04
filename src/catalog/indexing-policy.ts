import { AllowlistPolicy } from "../protocol/allowlist.js";

export interface CatalogIndexingEvent {
  type: string;
  body: Record<string, unknown>;
}

export function shouldIndexCatalogRoom(instanceId: string, allowlist: AllowlistPolicy, now = new Date()): boolean {
  return allowlist.can(instanceId, "catalog", now) && allowlist.can(instanceId, "indexing", now);
}

export class LocalSearchIndex {
  private readonly offers = new Map<string, { sellerId: string; revision: number; body: Record<string, unknown> }>();

  apply(event: CatalogIndexingEvent): void {
    if (event.type === "io.marketplace.offer.upserted") {
      const offerId = requireId(event.body, "offer_id", "offerId");
      const sellerId = requireId(event.body, "seller_id", "sellerId");
      const revision = typeof event.body.revision === "number" ? event.body.revision : 0;
      const status = event.body.status;
      if (status === "withdrawn") {
        this.offers.delete(offerId);
        return;
      }
      this.offers.set(offerId, { sellerId, revision, body: event.body });
      return;
    }
    if (event.type === "io.marketplace.offer.withdrawn") {
      this.offers.delete(requireId(event.body, "offer_id", "offerId"));
      return;
    }
    if (event.type === "io.marketplace.actor.seller.suspended") {
      const sellerId = requireId(event.body, "seller_id", "sellerId");
      for (const [offerId, offer] of this.offers.entries()) {
        if (offer.sellerId === sellerId) {
          this.offers.delete(offerId);
        }
      }
    }
  }

  hasOffer(offerId: string): boolean {
    return this.offers.has(offerId);
  }
}

function requireId(body: Record<string, unknown>, snake: string, camel: string): string {
  const value = body[snake] ?? body[camel];
  if (typeof value !== "string") {
    throw new Error(`Missing ${snake}`);
  }
  return value;
}
