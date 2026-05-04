import { describe, expect, it } from "vitest";
import { validateOrderEventSequence, type OrderFlowEvent } from "../../src/order/order-flow-validator.js";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";
import { validOrderCreated } from "../../src/conformance/fixtures.js";

const paymentIntent = {
  order_id: validOrderCreated.order_id,
  payment_id: "pay:customer.example:01JPAY",
  adapter: validOrderCreated.payment_adapter,
  amount: validOrderCreated.price.amount,
  currency: validOrderCreated.price.currency,
  capture_policy: "before_entitlement",
  provider_ref: "pi_123",
  confirmation: {
    method: "redirect",
    uri: "https://payments.example/confirm/pi_123"
  },
  expires_at: "2026-05-04T10:20:00Z"
};

const paymentCaptured = {
  order_id: validOrderCreated.order_id,
  payment_id: paymentIntent.payment_id,
  adapter: paymentIntent.adapter,
  amount: paymentIntent.amount,
  currency: paymentIntent.currency,
  provider_ref: "ch_123",
  evidence: {
    kind: "provider_receipt",
    uri: "https://payments.example/receipts/ch_123",
    sha256: "sha256:receipt"
  }
};

const entitlementGranted = {
  order_id: validOrderCreated.order_id,
  payment_id: paymentIntent.payment_id,
  entitlement_id: "ent:customer.example:01JENT",
  type: validOrderCreated.entitlement_type,
  external_ref: "booking:slot-123",
  evidence: {
    kind: "provider_receipt",
    uri: "https://entitlements.example/receipt/slot-123",
    sha256: "sha256:entitlement"
  }
};

function happyPath(overrides: Partial<Record<string, Record<string, unknown>>> = {}): OrderFlowEvent[] {
  return [
    { type: "io.marketplace.order.created", body: { ...validOrderCreated, ...overrides.created } },
    { type: "io.marketplace.order.accepted", body: { order_id: validOrderCreated.order_id, ...overrides.accepted } },
    { type: "io.marketplace.payment.intent.created", body: { ...paymentIntent, ...overrides.intent } },
    { type: "io.marketplace.payment.authorized", body: { order_id: validOrderCreated.order_id, payment_id: paymentIntent.payment_id, ...overrides.authorized } },
    { type: "io.marketplace.payment.captured", body: { ...paymentCaptured, ...overrides.captured } },
    { type: "io.marketplace.entitlement.granted", body: { ...entitlementGranted, ...overrides.entitlement } },
    { type: "io.marketplace.order.completed", body: { order_id: validOrderCreated.order_id, ...overrides.completed } }
  ];
}

describe("validateOrderEventSequence", () => {
  it("accepts a payload-consistent happy path", () => {
    expect(() => validateOrderEventSequence(happyPath())).not.toThrow();
  });

  it("rejects refund events before payment capture", () => {
    expect(() =>
      validateOrderEventSequence([
        { type: "io.marketplace.order.created", body: validOrderCreated },
        { type: "io.marketplace.order.accepted", body: { order_id: validOrderCreated.order_id } },
        { type: "io.marketplace.payment.intent.created", body: paymentIntent },
        { type: "io.marketplace.payment.authorized", body: { order_id: validOrderCreated.order_id, payment_id: paymentIntent.payment_id } },
        { type: "io.marketplace.payment.refund.requested", body: { order_id: validOrderCreated.order_id, payment_id: paymentIntent.payment_id } }
      ])
    ).toThrow(MarketplaceValidationError);
  });

  it("rejects entitlement before capture when capture_policy=before_entitlement", () => {
    expect(() =>
      validateOrderEventSequence([
        { type: "io.marketplace.order.created", body: validOrderCreated },
        { type: "io.marketplace.order.accepted", body: { order_id: validOrderCreated.order_id } },
        { type: "io.marketplace.payment.intent.created", body: paymentIntent },
        { type: "io.marketplace.payment.authorized", body: { order_id: validOrderCreated.order_id, payment_id: paymentIntent.payment_id } },
        { type: "io.marketplace.entitlement.granted", body: entitlementGranted }
      ])
    ).toThrow(/before_entitlement/);
  });

  it("rejects captured payment terms that differ from the payment intent", () => {
    expect(() => validateOrderEventSequence(happyPath({ captured: { amount: "101.00" } }))).toThrow(
      /amount does not match/
    );
  });

  it("rejects events for a different order id", () => {
    expect(() => validateOrderEventSequence(happyPath({ captured: { order_id: "ord:customer.example:OTHER" } }))).toThrow(
      /different order_id/
    );
  });

  it("rejects entitlement grants that reference a different payment id", () => {
    expect(() =>
      validateOrderEventSequence(happyPath({ entitlement: { payment_id: "pay:customer.example:OTHER" } }))
    ).toThrow(/different payment_id/);
  });
});
