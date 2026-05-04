import { MarketplaceValidationError } from "../protocol/errors.js";
import { parseObjectInstance } from "../protocol/ids.js";
import type { EntitlementType, Money } from "../protocol/types.js";

export interface SnapshotRecord {
  snapshotId: string;
  sequence: number;
  sha256: string;
  coversEventsUntil: string;
}

export interface SellerRecord {
  sellerId: string;
  status: "active" | "suspended";
}

export interface ProductRecord {
  productId: string;
  sellerId: string;
  revision: number;
}

export interface OfferRecord {
  offerId: string;
  productId: string;
  sellerId: string;
  revision: number;
  price: Money;
  entitlementType: EntitlementType;
}

export class CatalogIndex {
  private snapshot?: SnapshotRecord;
  private readonly sellers = new Map<string, SellerRecord>();
  private readonly products = new Map<string, ProductRecord>();
  private readonly offers = new Map<string, OfferRecord>();

  constructor(public readonly instanceId: string) {}

  applySnapshot(snapshot: SnapshotRecord): void {
    if (this.snapshot) {
      if (snapshot.sequence === this.snapshot.sequence) {
        if (snapshot.sha256 !== this.snapshot.sha256) {
          throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Snapshot hash mismatch", {
            snapshot,
            currentSnapshot: this.snapshot
          });
        }
        return;
      }
      if (snapshot.sequence < this.snapshot.sequence) {
        throw new MarketplaceValidationError("REVISION_ROLLBACK", "Snapshot sequence rollback", { snapshot });
      }
    }
    this.snapshot = snapshot;
  }

  upsertSeller(seller: SellerRecord): void {
    this.assertCatalogInstance("sellerId", seller.sellerId);
    this.sellers.set(seller.sellerId, seller);
  }

  upsertProduct(product: ProductRecord): void {
    this.assertCatalogInstance("productId", product.productId);
    this.assertCatalogInstance("sellerId", product.sellerId);
    this.assertSellerActive(product.sellerId);
    const current = this.products.get(product.productId);
    if (current && product.revision <= current.revision) {
      throw new MarketplaceValidationError("REVISION_ROLLBACK", "Product revision rollback", { product });
    }
    this.products.set(product.productId, product);
  }

  upsertOffer(offer: OfferRecord): void {
    this.assertCatalogInstance("offerId", offer.offerId);
    this.assertCatalogInstance("productId", offer.productId);
    this.assertCatalogInstance("sellerId", offer.sellerId);
    this.assertSellerActive(offer.sellerId);
    const product = this.products.get(offer.productId);
    if (!product) {
      throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", `Unknown product ${offer.productId}`, {
        offer
      });
    }
    if (product.sellerId !== offer.sellerId) {
      throw new MarketplaceValidationError(
        "CATALOG_REFERENCE_MISMATCH",
        `Product seller mismatch for ${offer.productId}`,
        { offer, product }
      );
    }
    const current = this.offers.get(offer.offerId);
    if (current && offer.revision <= current.revision) {
      throw new MarketplaceValidationError("REVISION_ROLLBACK", "Offer revision rollback", { offer });
    }
    this.offers.set(offer.offerId, offer);
  }

  getOffer(offerId: string): OfferRecord | undefined {
    const offer = this.offers.get(offerId);
    if (!offer) {
      return undefined;
    }

    const seller = this.sellers.get(offer.sellerId);
    if (!seller || seller.status !== "active") {
      return undefined;
    }

    const product = this.products.get(offer.productId);
    if (!product || product.sellerId !== offer.sellerId) {
      return undefined;
    }

    return offer;
  }

  private assertSellerActive(sellerId: string): void {
    const seller = this.sellers.get(sellerId);
    if (!seller || seller.status !== "active") {
      throw new MarketplaceValidationError("ACTOR_NOT_ACTIVE", `Seller ${sellerId} is not active`, { sellerId });
    }
  }

  private assertCatalogInstance(field: "sellerId" | "productId" | "offerId", id: string): void {
    const actualInstanceId = parseObjectInstance(id);
    if (actualInstanceId !== this.instanceId) {
      throw new MarketplaceValidationError(
        "CATALOG_REFERENCE_MISMATCH",
        `Catalog reference mismatch for ${field}: expected ${this.instanceId}, got ${actualInstanceId}`,
        { field, id, expectedInstanceId: this.instanceId, actualInstanceId }
      );
    }
  }
}
