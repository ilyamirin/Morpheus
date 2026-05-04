import {
  CatalogIndex,
  type OfferRecord,
  type ProductRecord,
  type SellerRecord,
  type SnapshotRecord
} from "../catalog/catalog-index.js";
import type { CustomerBinding, OrderCreatedBody } from "../order/order-validator.js";

const snapshot: SnapshotRecord = {
  snapshotId: "snap:shop.example:01JSNAP",
  sequence: 1,
  sha256: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  coversEventsUntil: "$snap"
};

const seller: SellerRecord = {
  sellerId: "seller:shop.example:01JSELLER",
  status: "active"
};

const product: ProductRecord = {
  productId: "prod:shop.example:01JPROD",
  sellerId: seller.sellerId,
  revision: 1,
  termsHash: "sha256:3333333333333333333333333333333333333333333333333333333333333333"
};

const offer: OfferRecord = {
  offerId: "offer:shop.example:01JOFFER",
  productId: product.productId,
  sellerId: seller.sellerId,
  revision: 3,
  price: { amount: "100.00", currency: "USD" },
  entitlementType: "booking_slot",
  paymentCapturePolicy: "before_entitlement",
  offerTermsHash: "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  sellerTermsHash: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
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
  payment_capture_policy: "before_entitlement",
  entitlement_type: offer.entitlementType,
  seller_terms_hash: "sha256:1111111111111111111111111111111111111111111111111111111111111111",
  offer_terms_hash: "sha256:2222222222222222222222222222222222222222222222222222222222222222",
  arbiter_instance: "arbiter.example",
  arbiter_actor: "arbiter:arbiter.example:DEFAULT",
  arbitration_policy_id: "standard-digital-v1",
  arbitration_policy_version: "1",
  arbitration_window: "P14D",
  expires_at: "2026-05-04T10:30:00Z"
};

export const validCustomerBinding: CustomerBinding = {
  customer_id: validOrderCreated.customer_id,
  status: "active",
  accepted_payment_adapters: [validOrderCreated.payment_adapter],
  accepted_arbitration_policies: [validOrderCreated.arbitration_policy_id]
};
