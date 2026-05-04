import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";
import { validateOrderCreated } from "../../src/order/order-validator.js";
import type { OrderCreatedBody } from "../../src/order/order-validator.js";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";

function catalog(): CatalogIndex {
  const index = new CatalogIndex("shop.example");
  index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
  index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
  index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 });
  index.upsertOffer({
    offerId: "offer:shop.example:01JOFFER",
    productId: "prod:shop.example:01JPROD",
    sellerId: "seller:shop.example:01JSELLER",
    revision: 3,
    price: { amount: "100.00", currency: "USD" },
    entitlementType: "booking_slot"
  });
  return index;
}

const body: OrderCreatedBody = {
  order_id: "ord:customer.example:01JORDER",
  room_id: "!order:customer.example",
  customer_id: "customer:customer.example:01JCUST",
  seller_id: "seller:shop.example:01JSELLER",
  offer_id: "offer:shop.example:01JOFFER",
  offer_revision: 3,
  catalog_snapshot_id: "snap_01J",
  quantity: 1,
  price: { amount: "100.00", currency: "USD" },
  payment_adapter: "stripe",
  entitlement_type: "booking_slot",
  arbiter_instance: "arbiter.example",
  arbiter_actor: "arbiter:arbiter.example:default",
  arbitration_policy_id: "standard-digital-v1",
  arbitration_window: "P14D",
  expires_at: "2026-05-04T10:30:00Z"
};

describe("validateOrderCreated", () => {
  it("accepts a matching trusted offer", () => {
    const allowlist = new AllowlistPolicy({
      "shop.example": ["catalog", "orders"],
      "arbiter.example": ["arbitration"]
    });
    expect(() => validateOrderCreated(body, catalog(), allowlist)).not.toThrow();
  });

  it("rejects stale offer revisions", () => {
    const allowlist = new AllowlistPolicy({
      "shop.example": ["catalog", "orders"],
      "arbiter.example": ["arbitration"]
    });
    expect(() => validateOrderCreated({ ...body, offer_revision: 2 }, catalog(), allowlist)).toThrow(/revision/);
  });

  it("rejects price substitution", () => {
    const allowlist = new AllowlistPolicy({
      "shop.example": ["catalog", "orders"],
      "arbiter.example": ["arbitration"]
    });
    expect(() =>
      validateOrderCreated({ ...body, price: { amount: "1.00", currency: "USD" } }, catalog(), allowlist)
    ).toThrow(/price/);
  });

  it("rejects non-allowlisted arbiters", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["catalog", "orders"] });
    expect(() => validateOrderCreated(body, catalog(), allowlist)).toThrow(/arbiter/);
  });
});
