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

interface CustomerBinding {
  customerId: string;
  status: string;
  acceptedPaymentAdapters: string[];
  acceptedArbitrationPolicies: string[];
}

interface OrderTerms {
  customerId: string;
  paymentAdapter: string;
  amount: string;
  currency: string;
  arbitrationPolicyId: string;
}

interface OrderFlowContext {
  orderId?: string;
  customerBinding?: CustomerBinding;
  orderTerms?: OrderTerms;
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
      validateCustomerBound(event, context);
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
    validateCreatedOrderTerms(event, context);
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

function validateCustomerBound(event: OrderFlowEvent, context: OrderFlowContext): void {
  if (context.orderId) {
    fail("INVALID_STATE_TRANSITION", "customer.bound must appear before order.created in an order sequence", {
      orderId: context.orderId
    });
  }
  const binding = {
    customerId: requireString(event, "customer_id"),
    status: requireString(event, "status"),
    acceptedPaymentAdapters: requireStringArray(event, "accepted_payment_adapters"),
    acceptedArbitrationPolicies: requireStringArray(event, "accepted_arbitration_policies")
  };
  if (binding.status !== "active") {
    fail("ACTOR_NOT_ACTIVE", `Customer ${binding.customerId} is not active`, {
      customerId: binding.customerId,
      status: binding.status
    });
  }
  context.customerBinding = binding;
}

function validateCreatedOrderTerms(event: OrderFlowEvent, context: OrderFlowContext): void {
  const customerId = requireString(event, "customer_id");
  const paymentAdapter = requireString(event, "payment_adapter");
  const arbitrationPolicyId = requireString(event, "arbitration_policy_id");
  const price = requireMoney(event, "price");
  const binding = context.customerBinding;

  if (!binding) {
    fail("CATALOG_REFERENCE_MISMATCH", "order.created requires a preceding customer.bound event", {
      customerId
    });
  }
  if (binding.customerId !== customerId) {
    fail("CATALOG_REFERENCE_MISMATCH", "order.created customer does not match customer.bound", {
      expected: binding.customerId,
      actual: customerId
    });
  }
  if (!binding.acceptedPaymentAdapters.includes(paymentAdapter)) {
    fail("CATALOG_REFERENCE_MISMATCH", "order.created payment adapter is not accepted by customer.bound", {
      paymentAdapter,
      acceptedPaymentAdapters: binding.acceptedPaymentAdapters
    });
  }
  if (!binding.acceptedArbitrationPolicies.includes(arbitrationPolicyId)) {
    fail("CATALOG_REFERENCE_MISMATCH", "order.created arbitration policy is not accepted by customer.bound", {
      arbitrationPolicyId,
      acceptedArbitrationPolicies: binding.acceptedArbitrationPolicies
    });
  }

  context.orderTerms = {
    customerId,
    paymentAdapter,
    amount: price.amount,
    currency: price.currency,
    arbitrationPolicyId
  };
}

function validatePaymentIntent(event: OrderFlowEvent, context: OrderFlowContext): void {
  if (context.paymentIntent) {
    fail("PAYMENT_TERMS_MISMATCH", "Order sequence contains multiple payment intents", {
      expected: context.paymentIntent.paymentId,
      actual: requireString(event, "payment_id")
    });
  }
  const intent = {
    paymentId: requireString(event, "payment_id"),
    adapter: requireString(event, "adapter"),
    amount: requireString(event, "amount"),
    currency: requireString(event, "currency"),
    capturePolicy: requireCapturePolicy(event)
  };
  const orderTerms = requireOrderTerms(context);
  if (intent.adapter !== orderTerms.paymentAdapter) {
    fail("PAYMENT_TERMS_MISMATCH", "payment.intent.created adapter does not match order.created", {
      expected: orderTerms.paymentAdapter,
      actual: intent.adapter
    });
  }
  assertMoneyEqual(orderTerms.amount, orderTerms.currency, intent.amount, intent.currency, "payment.intent.created amount does not match order.created");
  context.paymentIntent = intent;
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
  if (intent.capturePolicy === "after_entitlement" && !context.entitlementId) {
    fail("INVALID_STATE_TRANSITION", "payment.captured requires entitlement.granted first when capture_policy=after_entitlement", {
      paymentId: intent.paymentId
    });
  }
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

function requireOrderTerms(context: OrderFlowContext): OrderTerms {
  if (!context.orderTerms) {
    fail("INVALID_STATE_TRANSITION", "Payment-bound event requires order.created terms first");
  }
  return context.orderTerms;
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

function requireStringArray(event: OrderFlowEvent, key: string): string[] {
  const value = (event.body as Record<string, unknown>)[key];
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    fail("MISSING_REQUIRED_FIELD", `${event.type} must include string array ${key}`, {
      eventType: event.type,
      key
    });
  }
  return value;
}

function requireMoney(event: OrderFlowEvent, key: string): { amount: string; currency: string } {
  const value = (event.body as Record<string, unknown>)[key];
  if (!value || typeof value !== "object") {
    fail("MISSING_REQUIRED_FIELD", `${event.type} must include ${key}`, { eventType: event.type, key });
  }
  const amount = (value as Record<string, unknown>).amount;
  const currency = (value as Record<string, unknown>).currency;
  if (typeof amount !== "string" || typeof currency !== "string") {
    fail("MISSING_REQUIRED_FIELD", `${event.type} must include ${key}.amount and ${key}.currency`, {
      eventType: event.type,
      key
    });
  }
  return { amount, currency };
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
