import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";
import { validateOrderCreated } from "../../src/order/order-validator.js";
import type { OrderCreatedBody } from "../../src/order/order-validator.js";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";
import type { ValidationCode } from "../../src/protocol/errors.js";

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

function allowlist(): AllowlistPolicy {
  return new AllowlistPolicy({
    "shop.example": ["catalog", "orders"],
    "arbiter.example": ["arbitration"]
  });
}

function expectValidationCode(fn: () => void, code: ValidationCode): void {
  expect(fn).toThrow(MarketplaceValidationError);
  try {
    fn();
  } catch (error) {
    expect(error).toBeInstanceOf(MarketplaceValidationError);
    expect((error as MarketplaceValidationError).code).toBe(code);
  }
}

describe("validateOrderCreated", () => {
  it("accepts a matching trusted offer", () => {
    expect(() => validateOrderCreated(body, catalog(), allowlist())).not.toThrow();
  });

  it("rejects stale offer revisions", () => {
    expect(() => validateOrderCreated({ ...body, offer_revision: 2 }, catalog(), allowlist())).toThrow(/revision/);
  });

  it("rejects price substitution", () => {
    expect(() =>
      validateOrderCreated({ ...body, price: { amount: "1.00", currency: "USD" } }, catalog(), allowlist())
    ).toThrow(/price/);
  });

  it("rejects seller/offer mismatch", () => {
    expectValidationCode(
      () =>
        validateOrderCreated(
          { ...body, seller_id: "seller:shop.example:01JOTHER" },
          catalog(),
          allowlist()
        ),
      "CATALOG_REFERENCE_MISMATCH"
    );
  });

  it("rejects arbiter_actor/arbiter_instance mismatch", () => {
    expectValidationCode(
      () =>
        validateOrderCreated(
          { ...body, arbiter_instance: "arbiter.example", arbiter_actor: "arbiter:other-arbiter.example:default" },
          catalog(),
          allowlist()
        ),
      "CATALOG_REFERENCE_MISMATCH"
    );
  });

  it("rejects currency mismatch", () => {
    expectValidationCode(
      () => validateOrderCreated({ ...body, price: { amount: "100.00", currency: "EUR" } }, catalog(), allowlist()),
      "PAYMENT_TERMS_MISMATCH"
    );
  });

  it("rejects entitlement mismatch", () => {
    expectValidationCode(
      () => validateOrderCreated({ ...body, entitlement_type: "download_access" }, catalog(), allowlist()),
      "CATALOG_REFERENCE_MISMATCH"
    );
  });

  it("rejects non-allowlisted arbiters", () => {
    const policy = new AllowlistPolicy({ "shop.example": ["catalog", "orders"] });
    expect(() => validateOrderCreated(body, catalog(), policy)).toThrow(/arbiter/);
  });
});
