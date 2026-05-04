import { describe, expect, it } from "vitest";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";
import { OrderStateMachine, OrderTransitionGraph } from "../../src/order/order-state.js";

describe("OrderStateMachine", () => {
  it("exposes OrderTransitionGraph as the preferred transition helper name", () => {
    const graph = new OrderTransitionGraph();
    graph.apply("io.marketplace.order.created");
    expect(graph.state).toBe("created");
  });

  it("accepts the happy path", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.entitlement.granted");
    machine.apply("io.marketplace.order.completed");
    expect(machine.state).toBe("completed");
  });

  it("rejects entitlement before captured payment", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    expect(() => machine.apply("io.marketplace.entitlement.granted")).toThrow(/Invalid transition/);
  });

  it("allows after-entitlement payment capture", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.entitlement.granted");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.order.completed");

    expect(machine.state).toBe("completed");
  });

  it("allows disputes after captured payment", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.dispute.opened");
    machine.apply("io.marketplace.dispute.ruling.issued");
    machine.apply("io.marketplace.dispute.closed");
    expect(machine.state).toBe("dispute_resolved");
  });

  it("rejects refund and entitlement after a pre-payment dispute ruling", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.dispute.opened");
    machine.apply("io.marketplace.dispute.ruling.issued");

    expect(() => machine.apply("io.marketplace.payment.refunded")).toThrow(MarketplaceValidationError);
    expect(() => machine.apply("io.marketplace.entitlement.granted")).toThrow(MarketplaceValidationError);
  });

  it("allows refund after a dispute ruling when the dispute opened after capture", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.dispute.opened");
    machine.apply("io.marketplace.dispute.ruling.issued");
    machine.apply("io.marketplace.payment.refunded");

    expect(machine.state).toBe("refunded");
  });

  it("accepts evidence submission while dispute is open and after ruling", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.dispute.opened");
    machine.apply("io.marketplace.dispute.evidence.submitted");
    machine.apply("io.marketplace.dispute.ruling.issued");
    machine.apply("io.marketplace.dispute.evidence.submitted");
    machine.apply("io.marketplace.dispute.closed");

    expect(machine.state).toBe("dispute_resolved");
  });

  it("allows activated and completed entitlements to complete the order", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.entitlement.granted");
    machine.apply("io.marketplace.entitlement.activated");
    machine.apply("io.marketplace.entitlement.completed");
    machine.apply("io.marketplace.order.completed");

    expect(machine.state).toBe("completed");
  });

  it("accepts optional refund, chargeback, and expiration lifecycle events from eligible states", () => {
    const refundRequestedAfterCapture = new OrderStateMachine();
    refundRequestedAfterCapture.apply("io.marketplace.order.created");
    refundRequestedAfterCapture.apply("io.marketplace.order.accepted");
    refundRequestedAfterCapture.apply("io.marketplace.payment.intent.created");
    refundRequestedAfterCapture.apply("io.marketplace.payment.authorized");
    refundRequestedAfterCapture.apply("io.marketplace.payment.captured");
    refundRequestedAfterCapture.apply("io.marketplace.payment.refund.requested");
    refundRequestedAfterCapture.apply("io.marketplace.payment.refunded");
    expect(refundRequestedAfterCapture.state).toBe("refunded");

    const refundRequestedAfterRuling = new OrderStateMachine();
    refundRequestedAfterRuling.apply("io.marketplace.order.created");
    refundRequestedAfterRuling.apply("io.marketplace.order.accepted");
    refundRequestedAfterRuling.apply("io.marketplace.payment.intent.created");
    refundRequestedAfterRuling.apply("io.marketplace.payment.authorized");
    refundRequestedAfterRuling.apply("io.marketplace.payment.captured");
    refundRequestedAfterRuling.apply("io.marketplace.entitlement.granted");
    refundRequestedAfterRuling.apply("io.marketplace.dispute.opened");
    refundRequestedAfterRuling.apply("io.marketplace.dispute.ruling.issued");
    refundRequestedAfterRuling.apply("io.marketplace.payment.refund.requested");
    refundRequestedAfterRuling.apply("io.marketplace.payment.refunded");
    expect(refundRequestedAfterRuling.state).toBe("refunded");

    const chargeback = new OrderStateMachine();
    chargeback.apply("io.marketplace.order.created");
    chargeback.apply("io.marketplace.order.accepted");
    chargeback.apply("io.marketplace.payment.intent.created");
    chargeback.apply("io.marketplace.payment.authorized");
    chargeback.apply("io.marketplace.payment.captured");
    chargeback.apply("io.marketplace.payment.chargeback.opened");
    expect(chargeback.state).toBe("chargeback_opened");

    const chargebackAfterRuling = new OrderStateMachine();
    chargebackAfterRuling.apply("io.marketplace.order.created");
    chargebackAfterRuling.apply("io.marketplace.order.accepted");
    chargebackAfterRuling.apply("io.marketplace.payment.intent.created");
    chargebackAfterRuling.apply("io.marketplace.payment.authorized");
    chargebackAfterRuling.apply("io.marketplace.payment.captured");
    chargebackAfterRuling.apply("io.marketplace.dispute.opened");
    chargebackAfterRuling.apply("io.marketplace.dispute.ruling.issued");
    chargebackAfterRuling.apply("io.marketplace.payment.chargeback.opened");
    expect(chargebackAfterRuling.state).toBe("chargeback_opened");

    const expired = new OrderStateMachine();
    expired.apply("io.marketplace.order.created");
    expired.apply("io.marketplace.order.accepted");
    expired.apply("io.marketplace.payment.intent.created");
    expired.apply("io.marketplace.payment.authorized");
    expired.apply("io.marketplace.payment.captured");
    expired.apply("io.marketplace.entitlement.granted");
    expired.apply("io.marketplace.entitlement.activated");
    expired.apply("io.marketplace.entitlement.expired");
    expect(expired.state).toBe("expired");
  });

  it("rejects further lifecycle events after order completion", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    machine.apply("io.marketplace.payment.intent.created");
    machine.apply("io.marketplace.payment.authorized");
    machine.apply("io.marketplace.payment.captured");
    machine.apply("io.marketplace.entitlement.granted");
    machine.apply("io.marketplace.order.completed");

    expect(() => machine.apply("io.marketplace.payment.refund.requested")).toThrow(MarketplaceValidationError);
    expect(() => machine.apply("io.marketplace.payment.chargeback.opened")).toThrow(MarketplaceValidationError);
    expect(() => machine.apply("io.marketplace.entitlement.expired")).toThrow(MarketplaceValidationError);
  });
});
