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

const moneyAmountSchema = z.string().regex(/^[0-9]+(\.[0-9]{1,8})?$/);
const isoDateSchema = z.string().datetime({ offset: true });
const orderIdSchema = z.string().startsWith("ord:");
const paymentIdSchema = z.string().startsWith("pay:");
const entitlementIdSchema = z.string().startsWith("ent:");
const disputeIdSchema = z.string().startsWith("disp:");
const productIdSchema = z.string().startsWith("prod:");
const offerIdSchema = z.string().startsWith("offer:");
const sellerIdSchema = z.string().startsWith("seller:");

export const moneySchema = z.object({
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/)
});

export const issuerSchema = z.object({
  instance_id: z.string().min(1),
  actor_id: z.string().min(1).optional(),
  matrix_user_id: z.string().regex(/^@[^:]+:[^:]+$/)
});

export const envelopeSchema = z.object({
  protocol: z.literal(PROTOCOL_NAME),
  protocol_version: z.literal(PROTOCOL_VERSION),
  event_id: z.string().min(1),
  created_at: isoDateSchema,
  issuer: issuerSchema,
  critical: z.array(z.string()).max(0),
  body: z.unknown()
});

export const instanceProfileBodySchema = z.object({
  instance_id: z.string().min(1),
  matrix_server_name: z.string().min(1),
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
  terms_hash: z.string().startsWith("sha256:"),
  supported_payment_adapters: z.array(z.string().min(1)),
  supported_entitlement_types: z.array(z.enum(ENTITLEMENT_TYPES))
});

export const sellerSuspendedBodySchema = z.object({
  seller_id: sellerIdSchema,
  status: z.literal("suspended"),
  reason_code: z.string().min(1).optional()
});

export const customerBoundBodySchema = z.object({
  customer_id: z.string().startsWith("customer:"),
  status: z.enum(["active", "suspended"]),
  display_name: z.string().min(1),
  instance_id: z.string().min(1),
  authorized_representatives: z.array(z.string().regex(/^@[^:]+:[^:]+$/)),
  accepted_payment_adapters: z.array(z.string().min(1)),
  accepted_arbitration_policies: z.array(z.string().min(1))
});

export const snapshotPublishedBodySchema = z.object({
  snapshot_id: z.string().startsWith("snap_"),
  sequence: z.number().int().nonnegative(),
  format: z.literal("application/json+io.marketplace.catalog.v0"),
  uri: z.string().min(1),
  sha256: z.string().min(32),
  covers_events_until: z.string().startsWith("$"),
  product_count: z.number().int().nonnegative(),
  offer_count: z.number().int().nonnegative(),
  created_at: isoDateSchema
});

export const catalogProfileBodySchema = z.object({
  instance_id: z.string().min(1),
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
  terms_hash: z.string().startsWith("sha256:")
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
    capture: z.enum(["before_entitlement", "after_entitlement"]),
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
  })
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
  customer_id: z.string().startsWith("customer:"),
  seller_id: z.string().startsWith("seller:"),
  offer_id: z.string().startsWith("offer:"),
  offer_revision: z.number().int().positive(),
  catalog_snapshot_id: z.string().startsWith("snap_"),
  quantity: z.number().int().positive(),
  price: moneySchema,
  payment_adapter: z.string().min(1),
  entitlement_type: z.enum(ENTITLEMENT_TYPES),
  arbiter_instance: z.string().min(1),
  arbiter_actor: z.string().startsWith("arbiter:"),
  arbitration_policy_id: z.string().min(1),
  arbitration_window: z.string().min(1),
  expires_at: isoDateSchema
});

export const orderLifecycleBodySchema = z.object({
  order_id: orderIdSchema
});

export const paymentIntentCreatedBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema,
  adapter: z.string().min(1),
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/),
  capture_policy: z.enum(["before_entitlement", "after_entitlement"]),
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

export const paymentCapturedBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema,
  adapter: z.string().min(1),
  amount: moneyAmountSchema,
  currency: z.string().regex(/^[A-Z]{3}$/),
  provider_ref: z.string().min(1),
  evidence: z.object({
    kind: z.literal("provider_receipt"),
    uri: z.string().url(),
    sha256: z.string().min(1)
  })
});

export const entitlementGrantedBodySchema = z.object({
  order_id: orderIdSchema,
  payment_id: paymentIdSchema.optional(),
  entitlement_id: entitlementIdSchema,
  type: z.enum(ENTITLEMENT_TYPES),
  external_ref: z.string().min(1),
  valid_from: isoDateSchema.optional(),
  valid_until: isoDateSchema.optional(),
  evidence: z.object({
    kind: z.literal("provider_receipt"),
    uri: z.string().url(),
    sha256: z.string().min(1)
  }).optional()
});

export const entitlementLifecycleBodySchema = z.object({
  order_id: orderIdSchema,
  entitlement_id: entitlementIdSchema
});

export const disputeLifecycleBodySchema = z.object({
  order_id: orderIdSchema,
  dispute_id: disputeIdSchema
});

export const disputeRulingBodySchema = z.object({
  order_id: orderIdSchema,
  dispute_id: disputeIdSchema,
  ruling: z.enum(DISPUTE_RULINGS),
  reason_code: z.string().min(1),
  remedy: z.object({
    type: z.string().min(1),
    amount: z.string().optional(),
    currency: z.string().regex(/^[A-Z]{3}$/).optional()
  }),
  evidence_refs: z.array(z.string().min(1)),
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
  "io.marketplace.order.accepted": orderLifecycleBodySchema,
  "io.marketplace.order.cancelled": orderLifecycleBodySchema,
  "io.marketplace.order.rejected": orderLifecycleBodySchema,
  "io.marketplace.order.completed": orderLifecycleBodySchema,
  "io.marketplace.payment.intent.created": paymentIntentCreatedBodySchema,
  "io.marketplace.payment.authorized": paymentLifecycleBodySchema,
  "io.marketplace.payment.captured": paymentCapturedBodySchema,
  "io.marketplace.payment.failed": paymentLifecycleBodySchema,
  "io.marketplace.payment.cancelled": paymentLifecycleBodySchema,
  "io.marketplace.payment.refund.requested": paymentLifecycleBodySchema,
  "io.marketplace.payment.refunded": paymentLifecycleBodySchema,
  "io.marketplace.payment.chargeback.opened": paymentLifecycleBodySchema,
  "io.marketplace.entitlement.granted": entitlementGrantedBodySchema,
  "io.marketplace.entitlement.activated": entitlementLifecycleBodySchema,
  "io.marketplace.entitlement.completed": entitlementLifecycleBodySchema,
  "io.marketplace.entitlement.revoked": entitlementLifecycleBodySchema,
  "io.marketplace.entitlement.expired": entitlementLifecycleBodySchema,
  "io.marketplace.dispute.opened": disputeLifecycleBodySchema,
  "io.marketplace.dispute.evidence.submitted": disputeLifecycleBodySchema,
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
  if (event.type === "io.marketplace.order.created" && parsed.data.room_id !== event.room_id) {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["content", "body", "room_id"],
      message: "Order room mismatch between event room_id and content.body.room_id"
    });
  }
});
