export const PROTOCOL_NAME = "io.marketplace" as const;
export const PROTOCOL_VERSION = "0.1" as const;

export const ROOM_PROFILES = {
  catalog: "catalog",
  order: "order",
  actorControl: "actor_control"
} as const;

export const CATALOG_EVENT_TYPES = [
  "io.marketplace.instance.profile",
  "io.marketplace.catalog.profile",
  "io.marketplace.catalog.snapshot.published",
  "io.marketplace.actor.seller.announced",
  "io.marketplace.actor.seller.suspended",
  "io.marketplace.product.upserted",
  "io.marketplace.product.withdrawn",
  "io.marketplace.offer.upserted",
  "io.marketplace.offer.withdrawn",
  "io.marketplace.inventory.updated"
] as const;

export const ORDER_EVENT_TYPES = [
  "io.marketplace.actor.customer.bound",
  "io.marketplace.order.created",
  "io.marketplace.order.accepted",
  "io.marketplace.order.cancelled",
  "io.marketplace.order.rejected",
  "io.marketplace.order.completed",
  "io.marketplace.payment.intent.created",
  "io.marketplace.payment.authorized",
  "io.marketplace.payment.captured",
  "io.marketplace.payment.failed",
  "io.marketplace.payment.cancelled",
  "io.marketplace.payment.refund.requested",
  "io.marketplace.payment.refunded",
  "io.marketplace.payment.chargeback.opened",
  "io.marketplace.entitlement.granted",
  "io.marketplace.entitlement.activated",
  "io.marketplace.entitlement.completed",
  "io.marketplace.entitlement.revoked",
  "io.marketplace.entitlement.expired",
  "io.marketplace.dispute.opened",
  "io.marketplace.dispute.evidence.submitted",
  "io.marketplace.dispute.ruling.issued",
  "io.marketplace.dispute.closed"
] as const;

export const PRODUCT_KINDS = [
  "digital_file",
  "license",
  "account_access",
  "digital_service",
  "booking",
  "subscription",
  "external_entitlement"
] as const;

export const ENTITLEMENT_TYPES = [
  "download_access",
  "license_key",
  "account_access",
  "service_delivery",
  "booking_slot",
  "subscription_access",
  "external_entitlement"
] as const;

export const DISPUTE_RULINGS = [
  "refund_required",
  "partial_refund_required",
  "entitlement_confirmed",
  "entitlement_reissue_required",
  "service_completion_required",
  "no_fault"
] as const;
