import { describe, expect, it } from "vitest";
import { validateOrderEventSequence, type OrderFlowEvent } from "../../src/order/order-flow-validator.js";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";
import { validCustomerBinding, validOrderCreated } from "../../src/conformance/fixtures.js";

const paymentIntent = {
  order_id: validOrderCreated.order_id,
  payment_id: "pay:customer.example:01JPAY",
  adapter: validOrderCreated.payment_adapter,
  amount: validOrderCreated.price.amount,
  currency: validOrderCreated.price.currency,
  capture_policy: "before_entitlement",
  idempotency_key: "idem-123",
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

const refundRequested = {
  order_id: validOrderCreated.order_id,
  payment_id: paymentIntent.payment_id,
  refund_id: "refund:customer.example:01JREFUND",
  amount: paymentIntent.amount,
  currency: paymentIntent.currency,
  provider_ref: "re_123",
  evidence: {
    kind: "provider_receipt",
    uri: "https://payments.example/refunds/re_123",
    sha256: "sha256:refund"
  }
};

const entitlementGranted = {
  order_id: validOrderCreated.order_id,
  payment_id: paymentIntent.payment_id,
  entitlement_id: "ent:customer.example:01JENT",
  type: validOrderCreated.entitlement_type,
  external_ref: "booking:slot-123",
  valid_from: "2026-05-04T11:00:00Z",
  valid_until: "2026-05-04T12:00:00Z",
  evidence: {
    kind: "provider_receipt",
    uri: "https://entitlements.example/receipt/slot-123",
    sha256: "sha256:entitlement"
  }
};

const customerBound = {
  customer_id: validCustomerBinding.customer_id,
  status: validCustomerBinding.status,
  accepted_payment_adapters: validCustomerBinding.accepted_payment_adapters,
  accepted_arbitration_policies: validCustomerBinding.accepted_arbitration_policies
};

const orderAccepted = {
  order_id: validOrderCreated.order_id,
  offer_revision: validOrderCreated.offer_revision,
  seller_terms_hash: validOrderCreated.seller_terms_hash,
  offer_terms_hash: validOrderCreated.offer_terms_hash,
  payment_capture_policy: validOrderCreated.payment_capture_policy,
  arbitration_policy_version: validOrderCreated.arbitration_policy_version
};

function happyPath(overrides: Partial<Record<string, Record<string, unknown>>> = {}): OrderFlowEvent[] {
  return [
    { type: "io.marketplace.actor.customer.bound", body: { ...customerBound, ...overrides.customer } },
    { type: "io.marketplace.order.created", body: { ...validOrderCreated, ...overrides.created } },
    { type: "io.marketplace.order.accepted", body: { ...orderAccepted, ...overrides.accepted } },
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

  it("accepts entitlement before capture when capture_policy=after_entitlement", () => {
    expect(() =>
      validateOrderEventSequence([
        { type: "io.marketplace.actor.customer.bound", body: customerBound },
        { type: "io.marketplace.order.created", body: { ...validOrderCreated, payment_capture_policy: "after_entitlement" } },
        { type: "io.marketplace.order.accepted", body: { ...orderAccepted, payment_capture_policy: "after_entitlement" } },
        {
          type: "io.marketplace.payment.intent.created",
          body: { ...paymentIntent, capture_policy: "after_entitlement" }
        },
        { type: "io.marketplace.payment.authorized", body: { order_id: validOrderCreated.order_id, payment_id: paymentIntent.payment_id } },
        { type: "io.marketplace.entitlement.granted", body: entitlementGranted },
        { type: "io.marketplace.payment.captured", body: paymentCaptured },
        { type: "io.marketplace.order.completed", body: { order_id: validOrderCreated.order_id } }
      ])
    ).not.toThrow();
  });

  it("rejects customer binding after order creation", () => {
    expect(() =>
      validateOrderEventSequence([
        { type: "io.marketplace.order.created", body: validOrderCreated },
        { type: "io.marketplace.actor.customer.bound", body: customerBound }
      ])
    ).toThrow(/customer.bound/);
  });

  it("rejects order creation without a preceding customer binding", () => {
    expect(() => validateOrderEventSequence(happyPath().slice(1))).toThrow(/customer.bound/);
  });

  it("rejects order terms that are not accepted by the customer binding", () => {
    expect(() => validateOrderEventSequence(happyPath({ customer: { accepted_payment_adapters: ["other"] } }))).toThrow(
      /payment adapter/
    );
  });

  it("rejects payment intents whose terms differ from order.created", () => {
    expect(() => validateOrderEventSequence(happyPath({ intent: { amount: "1.00" } }))).toThrow(
      /payment.intent.created amount/
    );
  });

  it("rejects seller acceptance that does not confirm locked order terms", () => {
    expect(() =>
      validateOrderEventSequence(
        happyPath({
          accepted: {
            offer_revision: 99,
            seller_terms_hash: validOrderCreated.seller_terms_hash,
            offer_terms_hash: validOrderCreated.offer_terms_hash,
            payment_capture_policy: validOrderCreated.payment_capture_policy,
            arbitration_policy_version: validOrderCreated.arbitration_policy_version
          }
        })
      )
    ).toThrow(/accepted.*terms/i);
  });

  it("rejects payment intent capture policy mismatch with order.created", () => {
    expect(() => validateOrderEventSequence(happyPath({ intent: { capture_policy: "after_entitlement" } }))).toThrow(
      /capture_policy/
    );
  });

  it("rejects capture before entitlement when capture_policy=after_entitlement", () => {
    expect(() =>
      validateOrderEventSequence(happyPath({
        created: { payment_capture_policy: "after_entitlement" },
        accepted: { payment_capture_policy: "after_entitlement" },
        intent: { capture_policy: "after_entitlement" }
      }).filter(
        (event) => event.type !== "io.marketplace.entitlement.granted"
      ))
    ).toThrow(/after_entitlement/);
  });

  it("rejects refund events before payment capture", () => {
    expect(() =>
      validateOrderEventSequence([
        { type: "io.marketplace.actor.customer.bound", body: customerBound },
        { type: "io.marketplace.order.created", body: validOrderCreated },
        { type: "io.marketplace.order.accepted", body: orderAccepted },
        { type: "io.marketplace.payment.intent.created", body: paymentIntent },
        { type: "io.marketplace.payment.authorized", body: { order_id: validOrderCreated.order_id, payment_id: paymentIntent.payment_id } },
        { type: "io.marketplace.payment.refund.requested", body: refundRequested }
      ])
    ).toThrow(MarketplaceValidationError);
  });

  it("rejects refund amount mismatches against captured payment", () => {
    expect(() =>
      validateOrderEventSequence([
        ...happyPath().slice(0, 5),
        { type: "io.marketplace.payment.captured", body: paymentCaptured },
        { type: "io.marketplace.payment.refund.requested", body: { ...refundRequested, amount: "101.00" } }
      ])
    ).toThrow(/refund.*amount/i);
  });

  it("accepts a partial refund when constrained by a partial-refund ruling", () => {
    expect(() =>
      validateOrderEventSequence([
        ...happyPath().slice(0, 7),
        {
          type: "io.marketplace.dispute.opened",
          body: { order_id: validOrderCreated.order_id, dispute_id: "disp:arbiter.example:01JDISP" }
        },
        {
          type: "io.marketplace.dispute.evidence.submitted",
          body: {
            order_id: validOrderCreated.order_id,
            dispute_id: "disp:arbiter.example:01JDISP",
            evidence: {
              kind: "customer_statement",
              uri: "mxc://customer.example/evidence",
              sha256: "sha256:" + "4".repeat(64)
            }
          }
        },
        {
          type: "io.marketplace.dispute.ruling.issued",
          body: {
            order_id: validOrderCreated.order_id,
            dispute_id: "disp:arbiter.example:01JDISP",
            ruling: "partial_refund_required",
            remedy: { type: "partial_refund", amount: "25.00", currency: "USD" },
            evidence_refs: ["$evidence"],
            binding: true
          }
        },
        {
          type: "io.marketplace.payment.refund.requested",
          body: { ...refundRequested, amount: "25.00" }
        }
      ])
    ).not.toThrow();
  });

  it("rejects full refund amounts after a partial-refund ruling", () => {
    expect(() =>
      validateOrderEventSequence([
        ...happyPath().slice(0, 7),
        {
          type: "io.marketplace.dispute.opened",
          body: { order_id: validOrderCreated.order_id, dispute_id: "disp:arbiter.example:01JDISP" }
        },
        {
          type: "io.marketplace.dispute.ruling.issued",
          body: {
            order_id: validOrderCreated.order_id,
            dispute_id: "disp:arbiter.example:01JDISP",
            ruling: "partial_refund_required",
            remedy: { type: "partial_refund", amount: "25.00", currency: "USD" },
            evidence_refs: ["$evidence"],
            binding: true
          }
        },
        {
          type: "io.marketplace.payment.refund.requested",
          body: refundRequested
        }
      ])
    ).toThrow(/refund.*amount/i);
  });

  it("rejects entitlement before capture when capture_policy=before_entitlement", () => {
    expect(() =>
      validateOrderEventSequence([
        { type: "io.marketplace.actor.customer.bound", body: customerBound },
        { type: "io.marketplace.order.created", body: validOrderCreated },
        { type: "io.marketplace.order.accepted", body: orderAccepted },
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
