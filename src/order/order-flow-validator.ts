import { Decimal } from "decimal.js";
import { MarketplaceValidationError, type ValidationCode } from "../protocol/errors.js";
import { OrderStateMachine } from "./order-state.js";

export interface OrderFlowEvent<TBody extends object = object> {
  type: string;
  body: TBody;
}

interface PaymentIntent {
  paymentId: string;
  adapter: string;
  amount: string;
  currency: string;
  capturePolicy: "before_entitlement" | "after_entitlement";
}

interface OrderFlowContext {
  orderId?: string;
  paymentIntent?: PaymentIntent;
  authorizedPaymentId?: string;
  capturedPaymentId?: string;
  entitlementId?: string;
  disputeId?: string;
}

export function validateOrderEventSequence(events: OrderFlowEvent[]): void {
  const machine = new OrderStateMachine();
  const context: OrderFlowContext = {};

  for (const event of events) {
    if (event.type === "io.marketplace.actor.customer.bound") {
      continue;
    }

    validateEventReferences(event, context);
    machine.apply(event.type);
  }
}

function validateEventReferences(event: OrderFlowEvent, context: OrderFlowContext): void {
  const orderId = getOptionalString(event.body, "order_id");

  if (event.type === "io.marketplace.order.created") {
    const createdOrderId = requireString(event, "order_id");
    if (context.orderId && context.orderId !== createdOrderId) {
      fail("CATALOG_REFERENCE_MISMATCH", "Order sequence contains multiple order ids", {
        expected: context.orderId,
        actual: createdOrderId
      });
    }
    context.orderId = createdOrderId;
    return;
  }

  if (!context.orderId) {
    fail("INVALID_STATE_TRANSITION", "Order sequence must start with order.created before order-bound events", {
      eventType: event.type
    });
  }
  if (!orderId) {
    fail("MISSING_REQUIRED_FIELD", `${event.type} must include order_id`, { eventType: event.type });
  }
  if (orderId !== context.orderId) {
    fail("CATALOG_REFERENCE_MISMATCH", "Order event references a different order_id", {
      expected: context.orderId,
      actual: orderId,
      eventType: event.type
    });
  }

  switch (event.type) {
    case "io.marketplace.payment.intent.created":
      validatePaymentIntent(event, context);
      break;
    case "io.marketplace.payment.authorized":
      context.authorizedPaymentId = requireIntentPaymentId(event, context);
      break;
    case "io.marketplace.payment.captured":
      validatePaymentCapture(event, context);
      break;
    case "io.marketplace.payment.failed":
    case "io.marketplace.payment.cancelled":
      requireIntentPaymentId(event, context);
      break;
    case "io.marketplace.payment.refund.requested":
    case "io.marketplace.payment.refunded":
    case "io.marketplace.payment.chargeback.opened":
      requireCapturedPaymentId(event, context);
      break;
    case "io.marketplace.entitlement.granted":
      validateEntitlementGrant(event, context);
      break;
    case "io.marketplace.entitlement.activated":
    case "io.marketplace.entitlement.completed":
    case "io.marketplace.entitlement.revoked":
    case "io.marketplace.entitlement.expired":
      validateEntitlementLifecycle(event, context);
      break;
    case "io.marketplace.dispute.opened":
      context.disputeId = requireString(event, "dispute_id");
      break;
    case "io.marketplace.dispute.evidence.submitted":
    case "io.marketplace.dispute.ruling.issued":
    case "io.marketplace.dispute.closed":
      validateDisputeLifecycle(event, context);
      break;
  }
}

function validatePaymentIntent(event: OrderFlowEvent, context: OrderFlowContext): void {
  if (context.paymentIntent) {
    fail("PAYMENT_TERMS_MISMATCH", "Order sequence contains multiple payment intents", {
      expected: context.paymentIntent.paymentId,
      actual: requireString(event, "payment_id")
    });
  }
  context.paymentIntent = {
    paymentId: requireString(event, "payment_id"),
    adapter: requireString(event, "adapter"),
    amount: requireString(event, "amount"),
    currency: requireString(event, "currency"),
    capturePolicy: requireCapturePolicy(event)
  };
}

function validatePaymentCapture(event: OrderFlowEvent, context: OrderFlowContext): void {
  const paymentId = requireIntentPaymentId(event, context);
  if (context.authorizedPaymentId !== paymentId) {
    fail("INVALID_STATE_TRANSITION", "payment.captured must reference an authorized payment", {
      expected: context.authorizedPaymentId,
      actual: paymentId
    });
  }

  const intent = requirePaymentIntent(context);
  const adapter = requireString(event, "adapter");
  const amount = requireString(event, "amount");
  const currency = requireString(event, "currency");

  if (adapter !== intent.adapter) {
    fail("PAYMENT_TERMS_MISMATCH", "payment.captured adapter does not match payment.intent.created", {
      expected: intent.adapter,
      actual: adapter
    });
  }
  assertMoneyEqual(intent.amount, intent.currency, amount, currency, "payment.captured amount does not match payment.intent.created");
  context.capturedPaymentId = paymentId;
}

function validateEntitlementGrant(event: OrderFlowEvent, context: OrderFlowContext): void {
  const intent = requirePaymentIntent(context);
  if (intent.capturePolicy === "before_entitlement" && !context.capturedPaymentId) {
    fail("INVALID_STATE_TRANSITION", "entitlement.granted requires captured payment when capture_policy=before_entitlement", {
      paymentId: intent.paymentId
    });
  }

  const paymentId = getOptionalString(event.body, "payment_id");
  if (paymentId) {
    const expectedPaymentId = context.capturedPaymentId ?? intent.paymentId;
    if (paymentId !== expectedPaymentId) {
      fail("PAYMENT_TERMS_MISMATCH", "entitlement.granted references a different payment_id", {
        expected: expectedPaymentId,
        actual: paymentId
      });
    }
  }
  context.entitlementId = requireString(event, "entitlement_id");
}

function validateEntitlementLifecycle(event: OrderFlowEvent, context: OrderFlowContext): void {
  const entitlementId = requireString(event, "entitlement_id");
  if (!context.entitlementId || entitlementId !== context.entitlementId) {
    fail("CATALOG_REFERENCE_MISMATCH", "Entitlement lifecycle event references a different entitlement_id", {
      expected: context.entitlementId,
      actual: entitlementId,
      eventType: event.type
    });
  }
}

function validateDisputeLifecycle(event: OrderFlowEvent, context: OrderFlowContext): void {
  const disputeId = requireString(event, "dispute_id");
  if (!context.disputeId || disputeId !== context.disputeId) {
    fail("CATALOG_REFERENCE_MISMATCH", "Dispute lifecycle event references a different dispute_id", {
      expected: context.disputeId,
      actual: disputeId,
      eventType: event.type
    });
  }
}

function requireIntentPaymentId(event: OrderFlowEvent, context: OrderFlowContext): string {
  const intent = requirePaymentIntent(context);
  const paymentId = requireString(event, "payment_id");
  if (paymentId !== intent.paymentId) {
    fail("PAYMENT_TERMS_MISMATCH", `${event.type} references a different payment_id`, {
      expected: intent.paymentId,
      actual: paymentId,
      eventType: event.type
    });
  }
  return paymentId;
}

function requireCapturedPaymentId(event: OrderFlowEvent, context: OrderFlowContext): string {
  const paymentId = requireString(event, "payment_id");
  if (!context.capturedPaymentId || paymentId !== context.capturedPaymentId) {
    fail("INVALID_STATE_TRANSITION", `${event.type} requires a captured payment`, {
      expected: context.capturedPaymentId,
      actual: paymentId,
      eventType: event.type
    });
  }
  return paymentId;
}

function requirePaymentIntent(context: OrderFlowContext): PaymentIntent {
  if (!context.paymentIntent) {
    fail("INVALID_STATE_TRANSITION", "Payment-bound event requires payment.intent.created first");
  }
  return context.paymentIntent;
}

function requireCapturePolicy(event: OrderFlowEvent): PaymentIntent["capturePolicy"] {
  const capturePolicy = requireString(event, "capture_policy");
  if (capturePolicy !== "before_entitlement" && capturePolicy !== "after_entitlement") {
    fail("PAYMENT_TERMS_MISMATCH", "Unsupported capture policy", { capturePolicy });
  }
  return capturePolicy;
}

function requireString(event: OrderFlowEvent, key: string): string {
  const value = getOptionalString(event.body, key);
  if (!value) {
    fail("MISSING_REQUIRED_FIELD", `${event.type} must include ${key}`, { eventType: event.type, key });
  }
  return value;
}

function getOptionalString(body: object, key: string): string | undefined {
  const value = (body as Record<string, unknown>)[key];
  return typeof value === "string" ? value : undefined;
}

function assertMoneyEqual(
  expectedAmount: string,
  expectedCurrency: string,
  actualAmount: string,
  actualCurrency: string,
  message: string
): void {
  if (expectedCurrency !== actualCurrency || !new Decimal(expectedAmount).equals(new Decimal(actualAmount))) {
    fail("PAYMENT_TERMS_MISMATCH", message, {
      expected: { amount: expectedAmount, currency: expectedCurrency },
      actual: { amount: actualAmount, currency: actualCurrency }
    });
  }
}

function fail(code: ValidationCode, message: string, details: Record<string, unknown> = {}): never {
  throw new MarketplaceValidationError(code, message, details);
}
