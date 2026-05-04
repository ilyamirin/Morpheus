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
  critical: z.array(z.string()),
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
  seller_id: z.string().startsWith("seller:"),
  status: z.enum(["active", "suspended"]),
  display_name: z.string().min(1),
  legal_profile_ref: z.string().url(),
  terms_ref: z.string().url(),
  terms_hash: z.string().startsWith("sha256:"),
  supported_payment_adapters: z.array(z.string().min(1)),
  supported_entitlement_types: z.array(z.enum(ENTITLEMENT_TYPES))
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

export const productUpsertedBodySchema = z.object({
  product_id: z.string().startsWith("prod:"),
  seller_id: z.string().startsWith("seller:"),
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

export const offerUpsertedBodySchema = z.object({
  offer_id: z.string().startsWith("offer:"),
  product_id: z.string().startsWith("prod:"),
  seller_id: z.string().startsWith("seller:"),
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

export const orderCreatedBodySchema = z.object({
  order_id: z.string().startsWith("ord:"),
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

export const paymentCapturedBodySchema = z.object({
  order_id: z.string().startsWith("ord:"),
  payment_id: z.string().startsWith("pay:"),
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
  order_id: z.string().startsWith("ord:"),
  payment_id: z.string().startsWith("pay:").optional(),
  entitlement_id: z.string().startsWith("ent:"),
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

export const disputeRulingBodySchema = z.object({
  order_id: z.string().startsWith("ord:"),
  dispute_id: z.string().startsWith("disp:"),
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

export const knownEventTypeSchema = z.enum(knownEventTypes);

export const marketplaceEventSchema = z.object({
  type: knownEventTypeSchema,
  room_id: z.string().startsWith("!"),
  event_id: z.string().startsWith("$"),
  sender: z.string().regex(/^@[^:]+:[^:]+$/),
  origin_server_ts: z.number().int().nonnegative(),
  content: envelopeSchema
}).superRefine((event, ctx) => {
  const bodySchemas: Record<string, z.ZodTypeAny> = {
    "io.marketplace.instance.profile": instanceProfileBodySchema,
    "io.marketplace.actor.seller.announced": sellerAnnouncedBodySchema,
    "io.marketplace.actor.customer.bound": customerBoundBodySchema,
    "io.marketplace.catalog.snapshot.published": snapshotPublishedBodySchema,
    "io.marketplace.product.upserted": productUpsertedBodySchema,
    "io.marketplace.offer.upserted": offerUpsertedBodySchema,
    "io.marketplace.order.created": orderCreatedBodySchema,
    "io.marketplace.payment.captured": paymentCapturedBodySchema,
    "io.marketplace.entitlement.granted": entitlementGrantedBodySchema,
    "io.marketplace.dispute.ruling.issued": disputeRulingBodySchema
  };
  const schema = bodySchemas[event.type];
  if (!schema) {
    return;
  }
  const parsed = schema.safeParse(event.content.body);
  if (!parsed.success) {
    for (const issue of parsed.error.issues) {
      ctx.addIssue(issue);
    }
  }
});
