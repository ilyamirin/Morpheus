import { z } from "zod";
import {
  CATALOG_EVENT_TYPES,
  DISPUTE_RULINGS,
  ENTITLEMENT_TYPES,
  ORDER_EVENT_TYPES,
  PRODUCT_KINDS,
  PROTOCOL_NAME,
  PROTOCOL_VERSION
} from "./constants.js";
import { isProtocolObjectId, isValidInstanceId } from "./ids.js";

const moneyAmountSchema = z.string().regex(/^[0-9]+(\.[0-9]{1,8})?$/);
const isoDateSchema = z.string().datetime({ offset: true }).refine((value) => value.endsWith("Z"), {
  message: "Timestamp must be UTC and end with Z"
});
const instanceIdSchema = z.string().refine((id) => isValidInstanceId(id), "Invalid instance id");
const protocolEventIdSchema = z.string().refine((id) => isProtocolObjectId(id, "evt"), "Invalid protocol_event_id");
const orderIdSchema = z.string().refine((id) => isProtocolObjectId(id, "ord"), "Invalid order id");
const paymentIdSchema = z.string().refine((id) => isProtocolObjectId(id, "pay"), "Invalid payment id");
const refundIdSchema = z.string().refine((id) => isProtocolObjectId(id, "refund"), "Invalid refund id");
const entitlementIdSchema = z.string().refine((id) => isProtocolObjectId(id, "ent"), "Invalid entitlement id");
const disputeIdSchema = z.string().refine((id) => isProtocolObjectId(id, "disp"), "Invalid dispute id");
const productIdSchema = z.string().refine((id) => isProtocolObjectId(id, "prod"), "Invalid product id");
const offerIdSchema = z.string().refine((id) => isProtocolObjectId(id, "offer"), "Invalid offer id");
const sellerIdSchema = z.string().refine((id) => isProtocolObjectId(id, "seller"), "Invalid seller id");
const customerIdSchema = z.string().refine((id) => isProtocolObjectId(id, "customer"), "Invalid customer id");
const arbiterIdSchema = z.string().refine((id) => isProtocolObjectId(id, "arbiter"), "Invalid arbiter id");
const actorIdSchema = z
  .string()
  .refine(
    (id) => isProtocolObjectId(id, "seller") || isProtocolObjectId(id, "customer") || isProtocolObjectId(id, "arbiter"),
    "Invalid actor id"
  );
const snapshotIdSchema = z.string().refine((id) => isProtocolObjectId(id, "snap"), "Invalid snapshot id");
const sha256Schema = z.string().regex(/^sha256:[0-9a-f]{64}$/);
const evidenceSchema = z.object({
  kind: z.string().min(1),
  uri: z.string().url().or(z.string().startsWith("mxc://")),
  sha256: sha256Schema
});

export const moneySchema = z.object({
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/)
});

export const issuerSchema = z.object({
  instance_id: instanceIdSchema,
  actor_id: actorIdSchema.optional(),
  matrix_user_id: z.string().regex(/^@[^:]+:[^:]+$/)
});

export const envelopeSchema = z.object({
  protocol: z.literal(PROTOCOL_NAME),
  protocol_version: z.literal(PROTOCOL_VERSION),
  protocol_event_id: protocolEventIdSchema,
  created_at: isoDateSchema,
  issuer: issuerSchema,
  critical: z.array(z.string()),
  body: z.unknown()
});

export const instanceProfileBodySchema = z.object({
  instance_id: instanceIdSchema,
  matrix_server_name: instanceIdSchema,
  application_service_id: z.string().min(1),
  catalog_room_id: z.string().startsWith("!"),
  protocol_versions: z.array(z.literal(PROTOCOL_VERSION)),
  payment_adapters: z.array(z.string().min(1)),
  entitlement_types: z.array(z.enum(ENTITLEMENT_TYPES)),
  arbitration_policies: z.array(z.string().min(1))
});

export const sellerAnnouncedBodySchema = z.object({
  seller_id: sellerIdSchema,
  status: z.enum(["active", "suspended"]),
  display_name: z.string().min(1),
  legal_profile_ref: z.string().url(),
  terms_ref: z.string().url(),
  terms_hash: sha256Schema,
  supported_payment_adapters: z.array(z.string().min(1)),
  supported_entitlement_types: z.array(z.enum(ENTITLEMENT_TYPES))
});

export const sellerSuspendedBodySchema = z.object({
  seller_id: sellerIdSchema,
  status: z.literal("suspended"),
  reason_code: z.string().min(1).optional()
});

export const customerBoundBodySchema = z.object({
  customer_id: customerIdSchema,
  status: z.enum(["active", "suspended"]),
  display_name: z.string().min(1),
  instance_id: instanceIdSchema,
  authorized_representatives: z.array(z.string().regex(/^@[^:]+:[^:]+$/)),
  accepted_payment_adapters: z.array(z.string().min(1)),
  accepted_arbitration_policies: z.array(z.string().min(1))
});

export const snapshotPublishedBodySchema = z.object({
  snapshot_id: snapshotIdSchema,
  sequence: z.number().int().nonnegative(),
  format: z.literal("application/json+io.marketplace.catalog.v0"),
  uri: z.string().min(1),
  sha256: sha256Schema,
  covers_events_until: z.string().startsWith("$"),
  product_count: z.number().int().nonnegative(),
  offer_count: z.number().int().nonnegative(),
  created_at: isoDateSchema
});

export const catalogProfileBodySchema = z.object({
  instance_id: instanceIdSchema,
  snapshot_required: z.boolean(),
  delta_required: z.boolean()
});

export const productUpsertedBodySchema = z.object({
  product_id: productIdSchema,
  seller_id: sellerIdSchema,
  revision: z.number().int().positive(),
  status: z.enum(["active", "withdrawn"]),
  kind: z.enum(PRODUCT_KINDS),
  title: z.string().min(1),
  description: z.string().min(1),
  categories: z.array(z.string().min(1)),
  tags: z.array(z.string().min(1)),
  media: z.array(z.object({ uri: z.string().min(1), sha256: z.string().min(1) })),
  terms_hash: sha256Schema
});

export const productWithdrawnBodySchema = z.object({
  product_id: productIdSchema,
  revision: z.number().int().positive()
});

export const offerUpsertedBodySchema = z.object({
  offer_id: offerIdSchema,
  product_id: productIdSchema,
  seller_id: sellerIdSchema,
  revision: z.number().int().positive(),
  status: z.enum(["active", "withdrawn"]),
  price: moneySchema,
  payment_terms: z.object({
    capture_policy: z.enum(["before_entitlement", "after_entitlement"]),
    adapter_policy: z.enum(["seller_supported"])
  }),
  entitlement: z.object({
    type: z.enum(ENTITLEMENT_TYPES),
    duration: z.string().optional(),
    delivery: z.enum(["external"])
  }),
  availability: z.object({
    mode: z.enum(["unlimited", "limited"]),
    quantity: z.number().int().nonnegative().optional(),
    valid_until: isoDateSchema.optional()
  }),
  seller_terms_hash: sha256Schema,
  offer_terms_hash: sha256Schema
});

export const offerWithdrawnBodySchema = z.object({
  offer_id: offerIdSchema,
  revision: z.number().int().positive()
});

export const inventoryUpdatedBodySchema = z.object({
  offer_id: offerIdSchema,
  revision: z.number().int().positive(),
  available_quantity: z.number().int().nonnegative()
});

export const orderCreatedBodySchema = z.object({
  order_id: orderIdSchema,
  room_id: z.string().startsWith("!"),
  customer_id: customerIdSchema,
  seller_id: sellerIdSchema,
  offer_id: offerIdSchema,
  offer_revision: z.number().int().positive(),
  catalog_snapshot_id: snapshotIdSchema,
  quantity: z.number().int().positive().max(1, "quantity is limited to one in v0.1"),
  price: moneySchema,
  payment_adapter: z.string().min(1),
  payment_capture_policy: z.enum(["before_entitlement", "after_entitlement"]),
  entitlement_type: z.enum(ENTITLEMENT_TYPES),
  arbiter_instance: instanceIdSchema,
  arbiter_actor: arbiterIdSchema,
  seller_terms_hash: sha256Schema,
  offer_terms_hash: sha256Schema,
  arbitration_policy_id: z.string().min(1),
  arbitration_policy_version: z.string().min(1),
  arbitration_window: z.string().min(1),
  expires_at: isoDateSchema
});

export const orderLifecycleBodySchema = z.object({
  order_id: orderIdSchema
});

export const orderAcceptedBodySchema = z.object({
  order_id: orderIdSchema,
  offer_revision: z.number().int().positive(),
  seller_terms_hash: sha256Schema,
  offer_terms_hash: sha256Schema,
  payment_capture_policy: z.enum(["before_entitlement", "after_entitlement"]),
  arbitration_policy_version: z.string().min(1)
});

export const paymentIntentCreatedBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema,
  adapter: z.string().min(1),
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/),
  capture_policy: z.enum(["before_entitlement", "after_entitlement"]),
  idempotency_key: z.string().min(1),
  provider_ref: z.string().min(1),
  confirmation: z.object({
    method: z.string().min(1),
    uri: z.string().url()
  }),
  expires_at: isoDateSchema
});

export const paymentLifecycleBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema
});

export const paymentRefundBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema,
  refund_id: refundIdSchema,
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/),
  provider_ref: z.string().min(1),
  evidence: evidenceSchema
});

export const paymentCapturedBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema,
  adapter: z.string().min(1),
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/),
  provider_ref: z.string().min(1),
  evidence: evidenceSchema
});

export const entitlementGrantedBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema.optional(),
  entitlement_id: entitlementIdSchema,
  type: z.enum(ENTITLEMENT_TYPES),
  external_ref: z.string().min(1),
  valid_from: isoDateSchema.optional(),
  valid_until: isoDateSchema.optional(),
  evidence: evidenceSchema.optional()
}).superRefine((body, ctx) => {
  if ((body.type === "booking_slot" || body.type === "subscription_access") && (!body.valid_from || !body.valid_until)) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["valid_from"],
      message: `${body.type} entitlements require valid_from and valid_until`
    });
  }
  if ((body.type === "service_delivery" || body.type === "external_entitlement") && !body.evidence) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["evidence"],
      message: `${body.type} entitlements require evidence`
    });
  }
});

export const entitlementLifecycleBodySchema = z.object({
  order_id: orderIdSchema,
  entitlement_id: entitlementIdSchema
});

export const disputeLifecycleBodySchema = z.object({
  order_id: orderIdSchema,
  dispute_id: disputeIdSchema
});

export const disputeEvidenceBodySchema = z.object({
  order_id: orderIdSchema,
  dispute_id: disputeIdSchema,
  evidence: evidenceSchema
});

export const disputeRulingBodySchema = z.object({
  order_id: orderIdSchema,
  dispute_id: disputeIdSchema,
  ruling: z.enum(DISPUTE_RULINGS),
  reason_code: z.string().min(1),
  remedy: z.object({
    type: z.enum(["full_refund", "partial_refund", "entitlement_reissue", "service_completion", "no_fault"]),
    amount: moneyAmountSchema.optional(),
    currency: z.string().regex(/^[A-Z]{3}$/).optional()
  }),
  evidence_refs: z.array(z.string().startsWith("$")),
  binding: z.boolean()
});

const knownEventTypes = [...CATALOG_EVENT_TYPES, ...ORDER_EVENT_TYPES] as const;
type KnownEventType = (typeof knownEventTypes)[number];

export const knownEventTypeSchema = z.enum(knownEventTypes);

const bodySchemas: Record<KnownEventType, z.ZodTypeAny> = {
  "io.marketplace.instance.profile": instanceProfileBodySchema,
  "io.marketplace.catalog.profile": catalogProfileBodySchema,
  "io.marketplace.catalog.snapshot.published": snapshotPublishedBodySchema,
  "io.marketplace.actor.seller.announced": sellerAnnouncedBodySchema,
  "io.marketplace.actor.seller.suspended": sellerSuspendedBodySchema,
  "io.marketplace.product.upserted": productUpsertedBodySchema,
  "io.marketplace.product.withdrawn": productWithdrawnBodySchema,
  "io.marketplace.offer.upserted": offerUpsertedBodySchema,
  "io.marketplace.offer.withdrawn": offerWithdrawnBodySchema,
  "io.marketplace.inventory.updated": inventoryUpdatedBodySchema,
  "io.marketplace.actor.customer.bound": customerBoundBodySchema,
  "io.marketplace.order.created": orderCreatedBodySchema,
  "io.marketplace.order.accepted": orderAcceptedBodySchema,
  "io.marketplace.order.cancelled": orderLifecycleBodySchema,
  "io.marketplace.order.rejected": orderLifecycleBodySchema,
  "io.marketplace.order.completed": orderLifecycleBodySchema,
  "io.marketplace.payment.intent.created": paymentIntentCreatedBodySchema,
  "io.marketplace.payment.authorized": paymentLifecycleBodySchema,
  "io.marketplace.payment.captured": paymentCapturedBodySchema,
  "io.marketplace.payment.failed": paymentLifecycleBodySchema,
  "io.marketplace.payment.cancelled": paymentLifecycleBodySchema,
  "io.marketplace.payment.refund.requested": paymentRefundBodySchema,
  "io.marketplace.payment.refunded": paymentRefundBodySchema,
  "io.marketplace.payment.chargeback.opened": paymentLifecycleBodySchema,
  "io.marketplace.entitlement.granted": entitlementGrantedBodySchema,
  "io.marketplace.entitlement.activated": entitlementLifecycleBodySchema,
  "io.marketplace.entitlement.completed": entitlementLifecycleBodySchema,
  "io.marketplace.entitlement.revoked": entitlementLifecycleBodySchema,
  "io.marketplace.entitlement.expired": entitlementLifecycleBodySchema,
  "io.marketplace.dispute.opened": disputeLifecycleBodySchema,
  "io.marketplace.dispute.evidence.submitted": disputeEvidenceBodySchema,
  "io.marketplace.dispute.ruling.issued": disputeRulingBodySchema,
  "io.marketplace.dispute.closed": disputeLifecycleBodySchema
};

export const marketplaceEventSchema = z.object({
  type: knownEventTypeSchema,
  room_id: z.string().startsWith("!"),
  event_id: z.string().startsWith("$"),
  sender: z.string().regex(/^@[^:]+:[^:]+$/),
  origin_server_ts: z.number().int().nonnegative(),
  content: envelopeSchema
}).superRefine((event, ctx) => {
  const schema = bodySchemas[event.type];
  if (!schema) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["content", "body"],
      message: `No body schema is registered for ${event.type}`
    });
    return;
  }
  const parsed = schema.safeParse(event.content.body);
  if (!parsed.success) {
    for (const issue of parsed.error.issues) {
      ctx.addIssue(issue);
    }
    return;
  }
  if (event.sender !== event.content.issuer.matrix_user_id) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["sender"],
      message: "Matrix sender must match content.issuer.matrix_user_id"
    });
  }
  if (requiresActorIssuer(event.type) && !event.content.issuer.actor_id) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["content", "issuer", "actor_id"],
      message: `${event.type} requires content.issuer.actor_id`
    });
  }
  if (event.type === "io.marketplace.order.created" && parsed.data.room_id !== event.room_id) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["content", "body", "room_id"],
      message: "Order room mismatch between event room_id and content.body.room_id"
    });
  }
});

function requiresActorIssuer(eventType: KnownEventType): boolean {
  return ![
    "io.marketplace.instance.profile",
    "io.marketplace.catalog.profile",
    "io.marketplace.catalog.snapshot.published"
  ].includes(eventType);
}
