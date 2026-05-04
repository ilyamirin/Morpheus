import { describe, expect, it } from "vitest";
import { OrderStateMachine } from "../../src/order/order-state.js";

describe("OrderStateMachine", () => {
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
});
