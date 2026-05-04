import {
  CatalogIndex,
  type OfferRecord,
  type ProductRecord,
  type SellerRecord,
  type SnapshotRecord
} from "../catalog/catalog-index.js";
import type { CustomerBinding, OrderCreatedBody } from "../order/order-validator.js";

const snapshot: SnapshotRecord = {
  snapshotId: "snap_01J",
  sequence: 1,
  sha256: "abc",
  coversEventsUntil: "$snap"
};

const seller: SellerRecord = {
  sellerId: "seller:shop.example:01JSELLER",
  status: "active"
};

const product: ProductRecord = {
  productId: "prod:shop.example:01JPROD",
  sellerId: seller.sellerId,
  revision: 1
};

const offer: OfferRecord = {
  offerId: "offer:shop.example:01JOFFER",
  productId: product.productId,
  sellerId: seller.sellerId,
  revision: 3,
  price: { amount: "100.00", currency: "USD" },
  entitlementType: "booking_slot"
};

export const validCatalog = {
  snapshot,
  seller,
  product,
  offer,
  build(): CatalogIndex {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot(snapshot);
    catalog.upsertSeller(seller);
    catalog.upsertProduct(product);
    catalog.upsertOffer(offer);
    return catalog;
  }
};

export const validOrderCreated: OrderCreatedBody = {
  order_id: "ord:customer.example:01JORDER",
  room_id: "!order:customer.example",
  customer_id: "customer:customer.example:01JCUST",
  seller_id: seller.sellerId,
  offer_id: offer.offerId,
  offer_revision: offer.revision,
  catalog_snapshot_id: snapshot.snapshotId,
  quantity: 1,
  price: offer.price,
  payment_adapter: "stripe",
  entitlement_type: offer.entitlementType,
  arbiter_instance: "arbiter.example",
  arbiter_actor: "arbiter:arbiter.example:default",
  arbitration_policy_id: "standard-digital-v1",
  arbitration_window: "P14D",
  expires_at: "2026-05-04T10:30:00Z"
};

export const validCustomerBinding: CustomerBinding = {
  customer_id: validOrderCreated.customer_id,
  status: "active",
  accepted_payment_adapters: [validOrderCreated.payment_adapter],
  accepted_arbitration_policies: [validOrderCreated.arbitration_policy_id]
};
