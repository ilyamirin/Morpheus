import { Decimal } from "decimal.js";
import type { CatalogIndex } from "../catalog/catalog-index.js";
import type { AllowlistPolicy } from "../protocol/allowlist.js";
import { MarketplaceValidationError } from "../protocol/errors.js";
import { parseActorId, parseObjectInstance } from "../protocol/ids.js";
import type { EntitlementType, Money } from "../protocol/types.js";

export interface OrderCreatedBody {
  order_id: string;
  room_id: string;
  customer_id: string;
  seller_id: string;
  offer_id: string;
  offer_revision: number;
  catalog_snapshot_id: string;
  quantity: number;
  price: Money;
  payment_adapter: string;
  payment_capture_policy: "before_entitlement" | "after_entitlement";
  entitlement_type: EntitlementType;
  seller_terms_hash: string;
  offer_terms_hash: string;
  arbiter_instance: string;
  arbiter_actor: string;
  arbitration_policy_id: string;
  arbitration_policy_version: string;
  arbitration_window: string;
  expires_at: string;
}

export interface CustomerBinding {
  customer_id: string;
  status: "active" | "suspended";
  accepted_payment_adapters: string[];
  accepted_arbitration_policies: string[];
}

export function validateOrderCreated(
  order: OrderCreatedBody,
  catalog: CatalogIndex,
  allowlist: AllowlistPolicy,
  customer: CustomerBinding
): void {
  if (!customer) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Customer binding is required", {
      customerId: order.customer_id
    });
  }
  if (customer.customer_id !== order.customer_id) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order customer does not match customer binding",
      {
        expected: customer.customer_id,
        actual: order.customer_id
      }
    );
  }
  if (customer.status !== "active") {
    throw new MarketplaceValidationError("ACTOR_NOT_ACTIVE", `Customer ${order.customer_id} is not active`, {
      customerId: order.customer_id,
      status: customer.status
    });
  }
  if (!customer.accepted_payment_adapters.includes(order.payment_adapter)) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order payment adapter is not accepted by customer binding",
      {
        paymentAdapter: order.payment_adapter,
        acceptedPaymentAdapters: customer.accepted_payment_adapters
      }
    );
  }
  if (!customer.accepted_arbitration_policies.includes(order.arbitration_policy_id)) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order arbitration policy is not accepted by customer binding",
      {
        arbitrationPolicyId: order.arbitration_policy_id,
        acceptedArbitrationPolicies: customer.accepted_arbitration_policies
      }
    );
  }

  const sellerInstance = parseObjectInstance(order.offer_id);
  if (!allowlist.can(sellerInstance, "orders")) {
    throw new MarketplaceValidationError(
      "INSTANCE_NOT_ALLOWLISTED",
      `Seller instance ${sellerInstance} is not allowlisted for orders`
    );
  }
  if (!allowlist.can(order.arbiter_instance, "arbitration")) {
    throw new MarketplaceValidationError(
      "INSTANCE_NOT_ALLOWLISTED",
      `Order arbiter ${order.arbiter_instance} is not allowlisted`
    );
  }

  const offer = catalog.getOffer(order.offer_id);
  if (!offer) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", `Offer ${order.offer_id} not found`);
  }
  if (order.seller_id !== offer.sellerId) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order seller does not match trusted offer",
      {
        expected: offer.sellerId,
        actual: order.seller_id
      }
    );
  }
  if (offer.revision !== order.offer_revision) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order offer revision does not match trusted catalog",
      {
        expected: offer.revision,
        actual: order.offer_revision
      }
    );
  }
  if (offer.entitlementType !== order.entitlement_type) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order entitlement type does not match offer"
    );
  }
  if (offer.paymentCapturePolicy && offer.paymentCapturePolicy !== order.payment_capture_policy) {
    throw new MarketplaceValidationError(
      "PAYMENT_TERMS_MISMATCH",
      "Order payment capture policy does not match offer",
      {
        expected: offer.paymentCapturePolicy,
        actual: order.payment_capture_policy
      }
    );
  }
  if (offer.offerTermsHash && offer.offerTermsHash !== order.offer_terms_hash) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Order offer terms hash does not match offer", {
      expected: offer.offerTermsHash,
      actual: order.offer_terms_hash
    });
  }
  if (offer.sellerTermsHash && offer.sellerTermsHash !== order.seller_terms_hash) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Order seller terms hash does not match offer", {
      expected: offer.sellerTermsHash,
      actual: order.seller_terms_hash
    });
  }
  const arbiterActor = parseActorId(order.arbiter_actor);
  if (arbiterActor.kind !== "arbiter" || arbiterActor.instanceId !== order.arbiter_instance) {
    throw new MarketplaceValidationError(
      "CATALOG_REFERENCE_MISMATCH",
      "Order arbiter actor does not match arbiter instance",
      {
        expected: order.arbiter_instance,
        actual: order.arbiter_actor
      }
    );
  }
  assertMoneyEqual(offer.price, order.price);
}

function assertMoneyEqual(expected: Money, actual: Money): void {
  if (expected.currency !== actual.currency || !new Decimal(expected.amount).equals(new Decimal(actual.amount))) {
    throw new MarketplaceValidationError("PAYMENT_TERMS_MISMATCH", "Order price does not match offer price", {
      expected,
      actual
    });
  }
}
