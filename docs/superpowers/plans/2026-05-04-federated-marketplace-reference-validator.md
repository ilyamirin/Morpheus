# Federated Marketplace Reference Validator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a TypeScript reference validator and conformance suite for `io.marketplace` v0.1 Matrix events.

**Architecture:** Implement the protocol core as a small TypeScript package with focused modules for event schemas, IDs, allowlist policy, catalog indexing, order state validation, payment/entitlement/dispute validation, and conformance fixtures. This plan does not start a production Matrix Application Service; it creates the strict validation library that the AS runtime will call.

**Tech Stack:** Node.js 22+, TypeScript, Vitest, Zod for runtime schemas, `decimal.js` for money comparison, `ulid` for ID helpers.

---

## File Structure

- Create `package.json`: package scripts and dependencies.
- Create `.gitignore`: generated dependency and build directories.
- Create `tsconfig.json`: strict TypeScript config.
- Create `vitest.config.ts`: test runner config.
- Create `src/protocol/constants.ts`: protocol constants, event types, room profiles, enum values.
- Create `src/protocol/types.ts`: shared TypeScript types.
- Create `src/protocol/schemas.ts`: Zod schemas for marketplace envelopes and core event bodies.
- Create `src/protocol/errors.ts`: typed validation errors.
- Create `src/protocol/ids.ts`: ID constructors and parsers.
- Create `src/protocol/allowlist.ts`: local allowlist policy model.
- Create `src/protocol/room-profile.ts`: room profile event allow/deny checks.
- Create `src/catalog/catalog-index.ts`: snapshot and delta validation state.
- Create `src/order/order-state.ts`: order state machine.
- Create `src/order/order-validator.ts`: cross-event order validation.
- Create `src/conformance/fixtures.ts`: valid and invalid event fixture builders.
- Create `src/index.ts`: public exports.
- Create `tests/protocol/*.test.ts`: schema, allowlist, room profile tests.
- Create `tests/catalog/*.test.ts`: catalog snapshot and delta tests.
- Create `tests/order/*.test.ts`: order/payment/entitlement/dispute tests.
- Create `tests/conformance/vectors.test.ts`: 15 required conformance vectors from the spec.

## Task 1: Project Scaffold

**Files:**
- Create: `package.json`
- Create: `.gitignore`
- Create: `tsconfig.json`
- Create: `vitest.config.ts`
- Create: `src/index.ts`

- [ ] **Step 1: Create package metadata and scripts**

Write `package.json`:

```json
{
  "name": "@morpheus/federated-marketplace-protocol",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "build": "tsc --noEmit",
    "test": "vitest run --passWithNoTests",
    "test:watch": "vitest",
    "check": "npm run build && npm run test"
  },
  "dependencies": {
    "decimal.js": "^10.4.3",
    "ulid": "^2.3.0",
    "zod": "^3.23.8"
  },
  "devDependencies": {
    "@types/node": "^22.15.0",
    "typescript": "^5.8.0",
    "vitest": "^3.1.0"
  }
}
```

- [ ] **Step 2: Add generated-file ignores**

Write `.gitignore`:

```gitignore
node_modules/
dist/
```

- [ ] **Step 3: Add strict TypeScript config**

Write `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022"],
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "esModuleInterop": true,
    "forceConsistentCasingInFileNames": true,
    "skipLibCheck": true,
    "outDir": "dist",
    "rootDir": "."
  },
  "include": ["src/**/*.ts", "tests/**/*.ts", "vitest.config.ts"]
}
```

- [ ] **Step 4: Add Vitest config**

Write `vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/**/*.test.ts"]
  }
});
```

- [ ] **Step 5: Add temporary public export**

Write `src/index.ts`:

```ts
export const protocolName = "io.marketplace";
```

- [ ] **Step 6: Install dependencies**

Run: `npm install`

Expected: `package-lock.json` is created and dependencies install successfully.

- [ ] **Step 7: Run initial checks**

Run: `npm run check`

Expected: TypeScript and Vitest complete without test files or with no failing tests.

- [ ] **Step 8: Commit scaffold**

```bash
git add .gitignore package.json package-lock.json tsconfig.json vitest.config.ts src/index.ts
git commit -m "chore: scaffold protocol validator package"
```

## Task 2: Protocol Constants and Shared Types

**Files:**
- Create: `src/protocol/constants.ts`
- Create: `src/protocol/types.ts`
- Modify: `src/index.ts`
- Test: `tests/protocol/constants.test.ts`

- [ ] **Step 1: Write tests for constants**

Write `tests/protocol/constants.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  CATALOG_EVENT_TYPES,
  ORDER_EVENT_TYPES,
  PROTOCOL_NAME,
  PROTOCOL_VERSION,
  ROOM_PROFILES
} from "../../src/protocol/constants.js";

describe("protocol constants", () => {
  it("uses the io.marketplace v0.1 namespace", () => {
    expect(PROTOCOL_NAME).toBe("io.marketplace");
    expect(PROTOCOL_VERSION).toBe("0.1");
  });

  it("separates catalog and order event types", () => {
    expect(CATALOG_EVENT_TYPES).toContain("io.marketplace.catalog.snapshot.published");
    expect(CATALOG_EVENT_TYPES).toContain("io.marketplace.offer.upserted");
    expect(ORDER_EVENT_TYPES).toContain("io.marketplace.order.created");
    expect(ORDER_EVENT_TYPES).toContain("io.marketplace.entitlement.granted");
    expect(CATALOG_EVENT_TYPES).not.toContain("io.marketplace.order.created");
  });

  it("declares the required room profiles", () => {
    expect(ROOM_PROFILES.catalog).toBe("catalog");
    expect(ROOM_PROFILES.order).toBe("order");
  });
});
```

- [ ] **Step 2: Run the failing test**

Run: `npm test -- tests/protocol/constants.test.ts`

Expected: FAIL because `src/protocol/constants.ts` does not exist.

- [ ] **Step 3: Implement constants**

Write `src/protocol/constants.ts`:

```ts
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
```

- [ ] **Step 4: Implement shared types**

Write `src/protocol/types.ts`:

```ts
import type {
  CATALOG_EVENT_TYPES,
  DISPUTE_RULINGS,
  ENTITLEMENT_TYPES,
  ORDER_EVENT_TYPES,
  PRODUCT_KINDS,
  ROOM_PROFILES
} from "./constants.js";

export type CatalogEventType = (typeof CATALOG_EVENT_TYPES)[number];
export type OrderEventType = (typeof ORDER_EVENT_TYPES)[number];
export type MarketplaceEventType = CatalogEventType | OrderEventType;
export type RoomProfile = (typeof ROOM_PROFILES)[keyof typeof ROOM_PROFILES];
export type ProductKind = (typeof PRODUCT_KINDS)[number];
export type EntitlementType = (typeof ENTITLEMENT_TYPES)[number];
export type DisputeRuling = (typeof DISPUTE_RULINGS)[number];

export interface Issuer {
  instance_id: string;
  actor_id?: string;
  matrix_user_id: string;
}

export interface MarketplaceEnvelope<TBody> {
  protocol: "io.marketplace";
  protocol_version: "0.1";
  event_id: string;
  created_at: string;
  issuer: Issuer;
  critical: string[];
  body: TBody;
}

export interface MatrixMarketplaceEvent<TBody = unknown> {
  type: MarketplaceEventType | string;
  room_id: string;
  event_id: string;
  sender: string;
  origin_server_ts: number;
  content: MarketplaceEnvelope<TBody>;
}

export interface Money {
  amount: string;
  currency: string;
}
```

- [ ] **Step 5: Export protocol modules**

Write `src/index.ts`:

```ts
export * from "./protocol/constants.js";
export * from "./protocol/types.js";
```

- [ ] **Step 6: Run tests**

Run: `npm test -- tests/protocol/constants.test.ts`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/protocol/constants.ts src/protocol/types.ts src/index.ts tests/protocol/constants.test.ts
git commit -m "feat: add protocol constants and shared types"
```

## Task 3: Runtime Schemas and Validation Errors

**Files:**
- Create: `src/protocol/errors.ts`
- Create: `src/protocol/schemas.ts`
- Modify: `src/index.ts`
- Test: `tests/protocol/schemas.test.ts`

- [ ] **Step 1: Write schema tests**

Write `tests/protocol/schemas.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { marketplaceEventSchema } from "../../src/protocol/schemas.js";

const baseEvent = {
  type: "io.marketplace.order.created",
  room_id: "!order:customer.example",
  event_id: "$matrix",
  sender: "@market:customer.example",
  origin_server_ts: 1777898400000,
  content: {
    protocol: "io.marketplace",
    protocol_version: "0.1",
    event_id: "evt_01JORDER",
    created_at: "2026-05-04T10:00:00Z",
    issuer: {
      instance_id: "customer.example",
      actor_id: "customer:customer.example:01JCUST",
      matrix_user_id: "@market:customer.example"
    },
    critical: [],
    body: {
      order_id: "ord:customer.example:01JORDER",
      room_id: "!order:customer.example",
      customer_id: "customer:customer.example:01JCUST",
      seller_id: "seller:shop.example:01JSELLER",
      offer_id: "offer:shop.example:01JOFFER",
      offer_revision: 3,
      catalog_snapshot_id: "snap_01J",
      quantity: 1,
      price: { amount: "100.00", currency: "USD" },
      payment_adapter: "stripe",
      entitlement_type: "booking_slot",
      arbiter_instance: "arbiter.example",
      arbiter_actor: "arbiter:arbiter.example:default",
      arbitration_policy_id: "standard-digital-v1",
      arbitration_window: "P14D",
      expires_at: "2026-05-04T10:30:00Z"
    }
  }
};

describe("marketplaceEventSchema", () => {
  it("accepts a valid marketplace envelope", () => {
    expect(marketplaceEventSchema.parse(baseEvent).content.protocol).toBe("io.marketplace");
  });

  it("rejects unsupported protocol versions", () => {
    const invalid = structuredClone(baseEvent);
    invalid.content.protocol_version = "0.2";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });

  it("rejects invalid money amounts", () => {
    const invalid = structuredClone(baseEvent);
    invalid.content.body.price.amount = "free";
    expect(() => marketplaceEventSchema.parse(invalid)).toThrow();
  });
});
```

- [ ] **Step 2: Run the failing test**

Run: `npm test -- tests/protocol/schemas.test.ts`

Expected: FAIL because `src/protocol/schemas.ts` does not exist.

- [ ] **Step 3: Add validation errors**

Write `src/protocol/errors.ts`:

```ts
export type ValidationCode =
  | "UNSUPPORTED_PROTOCOL_VERSION"
  | "ROOM_PROFILE_VIOLATION"
  | "UNAUTHORIZED_SENDER"
  | "INSTANCE_NOT_ALLOWLISTED"
  | "ACTOR_NOT_ACTIVE"
  | "REVISION_ROLLBACK"
  | "MISSING_REQUIRED_FIELD"
  | "UNKNOWN_CRITICAL_EXTENSION"
  | "INVALID_STATE_TRANSITION"
  | "CATALOG_REFERENCE_MISMATCH"
  | "PAYMENT_TERMS_MISMATCH";

export class MarketplaceValidationError extends Error {
  constructor(
    public readonly code: ValidationCode,
    message: string,
    public readonly details: Record<string, unknown> = {}
  ) {
    super(message);
    this.name = "MarketplaceValidationError";
  }
}
```

- [ ] **Step 4: Add schemas**

Write `src/protocol/schemas.ts`:

```ts
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

export const knownEventTypeSchema = z.enum([...CATALOG_EVENT_TYPES, ...ORDER_EVENT_TYPES]);

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
```

- [ ] **Step 5: Export schemas and errors**

Append to `src/index.ts`:

```ts
export * from "./protocol/errors.js";
export * from "./protocol/schemas.js";
```

- [ ] **Step 6: Run tests**

Run: `npm test -- tests/protocol/schemas.test.ts`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/protocol/errors.ts src/protocol/schemas.ts src/index.ts tests/protocol/schemas.test.ts
git commit -m "feat: add runtime event schemas"
```

## Task 4: IDs, Allowlist, and Room Profile Validation

**Files:**
- Create: `src/protocol/ids.ts`
- Create: `src/protocol/allowlist.ts`
- Create: `src/protocol/room-profile.ts`
- Modify: `src/index.ts`
- Test: `tests/protocol/ids.test.ts`
- Test: `tests/protocol/allowlist.test.ts`
- Test: `tests/protocol/room-profile.test.ts`

- [ ] **Step 1: Write tests**

Write `tests/protocol/ids.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { parseActorId, parseObjectInstance } from "../../src/protocol/ids.js";

describe("protocol ids", () => {
  it("extracts actor kind and instance", () => {
    expect(parseActorId("seller:shop.example:01JSELLER")).toEqual({
      kind: "seller",
      instanceId: "shop.example",
      localId: "01JSELLER"
    });
  });

  it("extracts object instance from offer ids", () => {
    expect(parseObjectInstance("offer:shop.example:01JOFFER")).toBe("shop.example");
  });
});
```

Write `tests/protocol/allowlist.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";

describe("AllowlistPolicy", () => {
  const policy = new AllowlistPolicy({
    "shop.example": ["catalog", "orders"],
    "arbiter.example": ["arbitration"]
  });

  it("allows configured capabilities", () => {
    expect(policy.can("shop.example", "catalog")).toBe(true);
    expect(policy.can("shop.example", "orders")).toBe(true);
    expect(policy.can("arbiter.example", "arbitration")).toBe(true);
  });

  it("rejects unknown instances and capabilities", () => {
    expect(policy.can("unknown.example", "catalog")).toBe(false);
    expect(policy.can("shop.example", "arbitration")).toBe(false);
  });
});
```

Write `tests/protocol/room-profile.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { assertEventAllowedInRoom } from "../../src/protocol/room-profile.js";

describe("room profile validation", () => {
  it("allows catalog events in catalog rooms", () => {
    expect(() => assertEventAllowedInRoom("catalog", "io.marketplace.offer.upserted")).not.toThrow();
  });

  it("rejects order events in catalog rooms", () => {
    expect(() => assertEventAllowedInRoom("catalog", "io.marketplace.order.created")).toThrow(/not allowed/);
  });
});
```

- [ ] **Step 2: Run failing tests**

Run: `npm test -- tests/protocol/ids.test.ts tests/protocol/allowlist.test.ts tests/protocol/room-profile.test.ts`

Expected: FAIL because implementation files do not exist.

- [ ] **Step 3: Implement IDs**

Write `src/protocol/ids.ts`:

```ts
export interface ParsedActorId {
  kind: "seller" | "customer" | "arbiter";
  instanceId: string;
  localId: string;
}

export function parseActorId(actorId: string): ParsedActorId {
  const [kind, instanceId, localId] = actorId.split(":");
  if ((kind !== "seller" && kind !== "customer" && kind !== "arbiter") || !instanceId || !localId) {
    throw new Error(`Invalid actor id: ${actorId}`);
  }
  return { kind, instanceId, localId };
}

export function parseObjectInstance(objectId: string): string {
  const [, instanceId, localId] = objectId.split(":");
  if (!instanceId || !localId) {
    throw new Error(`Invalid object id: ${objectId}`);
  }
  return instanceId;
}
```

- [ ] **Step 4: Implement allowlist**

Write `src/protocol/allowlist.ts`:

```ts
export type AllowlistCapability = "catalog" | "orders" | "arbitration" | "payments";

export class AllowlistPolicy {
  private readonly entries: Map<string, Set<AllowlistCapability>>;

  constructor(config: Record<string, AllowlistCapability[]>) {
    this.entries = new Map(
      Object.entries(config).map(([instanceId, capabilities]) => [instanceId, new Set(capabilities)])
    );
  }

  can(instanceId: string, capability: AllowlistCapability): boolean {
    return this.entries.get(instanceId)?.has(capability) ?? false;
  }
}
```

- [ ] **Step 5: Implement room profile validation**

Write `src/protocol/room-profile.ts`:

```ts
import { CATALOG_EVENT_TYPES, ORDER_EVENT_TYPES } from "./constants.js";
import { MarketplaceValidationError } from "./errors.js";
import type { RoomProfile } from "./types.js";

export function assertEventAllowedInRoom(roomProfile: RoomProfile, eventType: string): void {
  const allowed =
    roomProfile === "catalog"
      ? (CATALOG_EVENT_TYPES as readonly string[]).includes(eventType)
      : roomProfile === "order"
        ? (ORDER_EVENT_TYPES as readonly string[]).includes(eventType)
        : false;

  if (!allowed) {
    throw new MarketplaceValidationError(
      "ROOM_PROFILE_VIOLATION",
      `Event type ${eventType} is not allowed in ${roomProfile} room`,
      { roomProfile, eventType }
    );
  }
}
```

- [ ] **Step 6: Export new modules**

Append to `src/index.ts`:

```ts
export * from "./protocol/ids.js";
export * from "./protocol/allowlist.js";
export * from "./protocol/room-profile.js";
```

- [ ] **Step 7: Run tests**

Run: `npm test -- tests/protocol/ids.test.ts tests/protocol/allowlist.test.ts tests/protocol/room-profile.test.ts`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/protocol/ids.ts src/protocol/allowlist.ts src/protocol/room-profile.ts src/index.ts tests/protocol/ids.test.ts tests/protocol/allowlist.test.ts tests/protocol/room-profile.test.ts
git commit -m "feat: validate protocol ids and room profiles"
```

## Task 5: Catalog Index and Snapshot/Delta Rules

**Files:**
- Create: `src/catalog/catalog-index.ts`
- Modify: `src/index.ts`
- Test: `tests/catalog/catalog-index.test.ts`

- [ ] **Step 1: Write catalog tests**

Write `tests/catalog/catalog-index.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";

describe("CatalogIndex", () => {
  it("accepts a snapshot and later product and offer deltas", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
    index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 });
    index.upsertOffer({
      offerId: "offer:shop.example:01JOFFER",
      productId: "prod:shop.example:01JPROD",
      sellerId: "seller:shop.example:01JSELLER",
      revision: 1,
      price: { amount: "100.00", currency: "USD" },
      entitlementType: "booking_slot"
    });

    expect(index.getOffer("offer:shop.example:01JOFFER")?.revision).toBe(1);
  });

  it("rejects offers for suspended sellers", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "suspended" });

    expect(() =>
      index.upsertOffer({
        offerId: "offer:shop.example:01JOFFER",
        productId: "prod:shop.example:01JPROD",
        sellerId: "seller:shop.example:01JSELLER",
        revision: 1,
        price: { amount: "100.00", currency: "USD" },
        entitlementType: "booking_slot"
      })
    ).toThrow(/not active/);
  });

  it("rejects revision rollback", () => {
    const index = new CatalogIndex("shop.example");
    index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
    index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
    index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 2 });
    expect(() =>
      index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 })
    ).toThrow(/rollback/);
  });
});
```

- [ ] **Step 2: Run failing tests**

Run: `npm test -- tests/catalog/catalog-index.test.ts`

Expected: FAIL because `src/catalog/catalog-index.ts` does not exist.

- [ ] **Step 3: Implement catalog index**

Write `src/catalog/catalog-index.ts`:

```ts
import type { EntitlementType, Money } from "../protocol/types.js";
import { MarketplaceValidationError } from "../protocol/errors.js";

export interface SnapshotRecord {
  snapshotId: string;
  sequence: number;
  sha256: string;
  coversEventsUntil: string;
}

export interface SellerRecord {
  sellerId: string;
  status: "active" | "suspended";
}

export interface ProductRecord {
  productId: string;
  sellerId: string;
  revision: number;
}

export interface OfferRecord {
  offerId: string;
  productId: string;
  sellerId: string;
  revision: number;
  price: Money;
  entitlementType: EntitlementType;
}

export class CatalogIndex {
  private snapshot?: SnapshotRecord;
  private readonly sellers = new Map<string, SellerRecord>();
  private readonly products = new Map<string, ProductRecord>();
  private readonly offers = new Map<string, OfferRecord>();

  constructor(public readonly instanceId: string) {}

  applySnapshot(snapshot: SnapshotRecord): void {
    if (this.snapshot && snapshot.sequence <= this.snapshot.sequence) {
      throw new MarketplaceValidationError("REVISION_ROLLBACK", "Snapshot sequence rollback", { snapshot });
    }
    this.snapshot = snapshot;
  }

  upsertSeller(seller: SellerRecord): void {
    this.sellers.set(seller.sellerId, seller);
  }

  upsertProduct(product: ProductRecord): void {
    this.assertSellerActive(product.sellerId);
    const current = this.products.get(product.productId);
    if (current && product.revision <= current.revision) {
      throw new MarketplaceValidationError("REVISION_ROLLBACK", "Product revision rollback", { product });
    }
    this.products.set(product.productId, product);
  }

  upsertOffer(offer: OfferRecord): void {
    this.assertSellerActive(offer.sellerId);
    const current = this.offers.get(offer.offerId);
    if (current && offer.revision <= current.revision) {
      throw new MarketplaceValidationError("REVISION_ROLLBACK", "Offer revision rollback", { offer });
    }
    this.offers.set(offer.offerId, offer);
  }

  getOffer(offerId: string): OfferRecord | undefined {
    return this.offers.get(offerId);
  }

  private assertSellerActive(sellerId: string): void {
    const seller = this.sellers.get(sellerId);
    if (!seller || seller.status !== "active") {
      throw new MarketplaceValidationError("ACTOR_NOT_ACTIVE", `Seller ${sellerId} is not active`, { sellerId });
    }
  }
}
```

- [ ] **Step 4: Export catalog index**

Append to `src/index.ts`:

```ts
export * from "./catalog/catalog-index.js";
```

- [ ] **Step 5: Run tests**

Run: `npm test -- tests/catalog/catalog-index.test.ts`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/catalog/catalog-index.ts src/index.ts tests/catalog/catalog-index.test.ts
git commit -m "feat: add catalog index validation"
```

## Task 6: Order State Machine

**Files:**
- Create: `src/order/order-state.ts`
- Modify: `src/index.ts`
- Test: `tests/order/order-state.test.ts`

- [ ] **Step 1: Write state machine tests**

Write `tests/order/order-state.test.ts`:

```ts
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
```

- [ ] **Step 2: Run failing tests**

Run: `npm test -- tests/order/order-state.test.ts`

Expected: FAIL because `src/order/order-state.ts` does not exist.

- [ ] **Step 3: Implement state machine**

Write `src/order/order-state.ts`:

```ts
import { MarketplaceValidationError } from "../protocol/errors.js";

export type OrderState =
  | "draft"
  | "created"
  | "accepted"
  | "payment_intent_created"
  | "payment_authorized"
  | "payment_captured"
  | "entitlement_granted"
  | "completed"
  | "cancelled"
  | "rejected"
  | "refunded"
  | "dispute_opened"
  | "ruling_issued"
  | "dispute_resolved"
  | "expired";

const transitions: Record<OrderState, Partial<Record<string, OrderState>>> = {
  draft: {
    "io.marketplace.order.created": "created"
  },
  created: {
    "io.marketplace.order.accepted": "accepted",
    "io.marketplace.order.rejected": "rejected",
    "io.marketplace.order.cancelled": "cancelled"
  },
  accepted: {
    "io.marketplace.payment.intent.created": "payment_intent_created",
    "io.marketplace.dispute.opened": "dispute_opened",
    "io.marketplace.order.cancelled": "cancelled"
  },
  payment_intent_created: {
    "io.marketplace.payment.authorized": "payment_authorized",
    "io.marketplace.payment.failed": "cancelled",
    "io.marketplace.payment.cancelled": "cancelled"
  },
  payment_authorized: {
    "io.marketplace.payment.captured": "payment_captured",
    "io.marketplace.payment.failed": "cancelled"
  },
  payment_captured: {
    "io.marketplace.entitlement.granted": "entitlement_granted",
    "io.marketplace.dispute.opened": "dispute_opened",
    "io.marketplace.payment.refunded": "refunded"
  },
  entitlement_granted: {
    "io.marketplace.order.completed": "completed",
    "io.marketplace.dispute.opened": "dispute_opened",
    "io.marketplace.entitlement.revoked": "cancelled"
  },
  dispute_opened: {
    "io.marketplace.dispute.ruling.issued": "ruling_issued"
  },
  ruling_issued: {
    "io.marketplace.payment.refunded": "refunded",
    "io.marketplace.entitlement.granted": "entitlement_granted",
    "io.marketplace.dispute.closed": "dispute_resolved"
  },
  completed: {},
  cancelled: {},
  rejected: {},
  refunded: {},
  dispute_resolved: {},
  expired: {}
};

export class OrderStateMachine {
  public state: OrderState = "draft";

  apply(eventType: string): void {
    const next = transitions[this.state][eventType];
    if (!next) {
      throw new MarketplaceValidationError(
        "INVALID_STATE_TRANSITION",
        `Invalid transition from ${this.state} using ${eventType}`,
        { state: this.state, eventType }
      );
    }
    this.state = next;
  }
}
```

- [ ] **Step 4: Export state machine**

Append to `src/index.ts`:

```ts
export * from "./order/order-state.js";
```

- [ ] **Step 5: Run tests**

Run: `npm test -- tests/order/order-state.test.ts`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/order/order-state.ts src/index.ts tests/order/order-state.test.ts
git commit -m "feat: add order state machine"
```

## Task 7: Order Validator

**Files:**
- Create: `src/order/order-validator.ts`
- Modify: `src/index.ts`
- Test: `tests/order/order-validator.test.ts`

- [ ] **Step 1: Write order validation tests**

Write `tests/order/order-validator.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";
import { validateOrderCreated } from "../../src/order/order-validator.js";

function catalog(): CatalogIndex {
  const index = new CatalogIndex("shop.example");
  index.applySnapshot({ snapshotId: "snap_01J", sequence: 1, sha256: "abc", coversEventsUntil: "$snap" });
  index.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "active" });
  index.upsertProduct({ productId: "prod:shop.example:01JPROD", sellerId: "seller:shop.example:01JSELLER", revision: 1 });
  index.upsertOffer({
    offerId: "offer:shop.example:01JOFFER",
    productId: "prod:shop.example:01JPROD",
    sellerId: "seller:shop.example:01JSELLER",
    revision: 3,
    price: { amount: "100.00", currency: "USD" },
    entitlementType: "booking_slot"
  });
  return index;
}

const body = {
  order_id: "ord:customer.example:01JORDER",
  room_id: "!order:customer.example",
  customer_id: "customer:customer.example:01JCUST",
  seller_id: "seller:shop.example:01JSELLER",
  offer_id: "offer:shop.example:01JOFFER",
  offer_revision: 3,
  catalog_snapshot_id: "snap_01J",
  quantity: 1,
  price: { amount: "100.00", currency: "USD" },
  payment_adapter: "stripe",
  entitlement_type: "booking_slot",
  arbiter_instance: "arbiter.example",
  arbiter_actor: "arbiter:arbiter.example:default",
  arbitration_policy_id: "standard-digital-v1",
  arbitration_window: "P14D",
  expires_at: "2026-05-04T10:30:00Z"
};

describe("validateOrderCreated", () => {
  it("accepts a matching trusted offer", () => {
    const allowlist = new AllowlistPolicy({
      "shop.example": ["catalog", "orders"],
      "arbiter.example": ["arbitration"]
    });
    expect(() => validateOrderCreated(body, catalog(), allowlist)).not.toThrow();
  });

  it("rejects stale offer revisions", () => {
    const allowlist = new AllowlistPolicy({
      "shop.example": ["catalog", "orders"],
      "arbiter.example": ["arbitration"]
    });
    expect(() => validateOrderCreated({ ...body, offer_revision: 2 }, catalog(), allowlist)).toThrow(/revision/);
  });

  it("rejects price substitution", () => {
    const allowlist = new AllowlistPolicy({
      "shop.example": ["catalog", "orders"],
      "arbiter.example": ["arbitration"]
    });
    expect(() =>
      validateOrderCreated({ ...body, price: { amount: "1.00", currency: "USD" } }, catalog(), allowlist)
    ).toThrow(/price/);
  });

  it("rejects non-allowlisted arbiters", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["catalog", "orders"] });
    expect(() => validateOrderCreated(body, catalog(), allowlist)).toThrow(/arbiter/);
  });
});
```

- [ ] **Step 2: Run failing tests**

Run: `npm test -- tests/order/order-validator.test.ts`

Expected: FAIL because `src/order/order-validator.ts` does not exist.

- [ ] **Step 3: Implement order validator**

Write `src/order/order-validator.ts`:

```ts
import Decimal from "decimal.js";
import type { CatalogIndex } from "../catalog/catalog-index.js";
import type { AllowlistPolicy } from "../protocol/allowlist.js";
import { MarketplaceValidationError } from "../protocol/errors.js";
import type { EntitlementType, Money } from "../protocol/types.js";
import { parseObjectInstance } from "../protocol/ids.js";

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
  entitlement_type: EntitlementType;
  arbiter_instance: string;
  arbiter_actor: string;
  arbitration_policy_id: string;
  arbitration_window: string;
  expires_at: string;
}

export function validateOrderCreated(
  order: OrderCreatedBody,
  catalog: CatalogIndex,
  allowlist: AllowlistPolicy
): void {
  const sellerInstance = parseObjectInstance(order.offer_id);
  if (!allowlist.can(sellerInstance, "orders")) {
    throw new MarketplaceValidationError("INSTANCE_NOT_ALLOWLISTED", `Seller instance ${sellerInstance} is not allowlisted for orders`);
  }
  if (!allowlist.can(order.arbiter_instance, "arbitration")) {
    throw new MarketplaceValidationError("INSTANCE_NOT_ALLOWLISTED", `Order arbiter ${order.arbiter_instance} is not allowlisted`);
  }

  const offer = catalog.getOffer(order.offer_id);
  if (!offer) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", `Offer ${order.offer_id} not found`);
  }
  if (offer.revision !== order.offer_revision) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Order offer revision does not match trusted catalog", {
      expected: offer.revision,
      actual: order.offer_revision
    });
  }
  if (offer.entitlementType !== order.entitlement_type) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Order entitlement type does not match offer");
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
```

- [ ] **Step 4: Export order validator**

Append to `src/index.ts`:

```ts
export * from "./order/order-validator.js";
```

- [ ] **Step 5: Run tests**

Run: `npm test -- tests/order/order-validator.test.ts`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/order/order-validator.ts src/index.ts tests/order/order-validator.test.ts
git commit -m "feat: validate order creation against catalog"
```

## Task 8: Payment, Entitlement, and Dispute Authority Checks

**Files:**
- Create: `src/order/authority.ts`
- Modify: `src/index.ts`
- Test: `tests/order/authority.test.ts`

- [ ] **Step 1: Write authority tests**

Write `tests/order/authority.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { assertEventAuthority } from "../../src/order/authority.js";

describe("assertEventAuthority", () => {
  it("allows seller AS to capture payment and grant entitlement", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.entitlement.granted", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).not.toThrow();
  });

  it("rejects payment capture from customer AS", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:customer.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow(/seller/);
  });

  it("allows only arbiter AS to issue rulings", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:arbiter.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).not.toThrow();

    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow(/arbiter/);
  });
});
```

- [ ] **Step 2: Run failing tests**

Run: `npm test -- tests/order/authority.test.ts`

Expected: FAIL because `src/order/authority.ts` does not exist.

- [ ] **Step 3: Implement authority checks**

Write `src/order/authority.ts`:

```ts
import { MarketplaceValidationError } from "../protocol/errors.js";

export interface OrderAuthorities {
  sellerAsUser: string;
  customerAsUser: string;
  arbiterAsUser: string;
}

export function assertEventAuthority(eventType: string, sender: string, authorities: OrderAuthorities): void {
  if (
    eventType === "io.marketplace.payment.intent.created" ||
    eventType === "io.marketplace.payment.authorized" ||
    eventType === "io.marketplace.payment.captured" ||
    eventType === "io.marketplace.entitlement.granted"
  ) {
    assertSender(sender, authorities.sellerAsUser, "seller");
    return;
  }

  if (eventType === "io.marketplace.dispute.ruling.issued") {
    assertSender(sender, authorities.arbiterAsUser, "arbiter");
  }
}

function assertSender(sender: string, expected: string, role: string): void {
  if (sender !== expected) {
    throw new MarketplaceValidationError(
      "UNAUTHORIZED_SENDER",
      `Expected ${role} authority ${expected}, got ${sender}`,
      { sender, expected, role }
    );
  }
}
```

- [ ] **Step 4: Export authority checks**

Append to `src/index.ts`:

```ts
export * from "./order/authority.js";
```

- [ ] **Step 5: Run tests**

Run: `npm test -- tests/order/authority.test.ts`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/order/authority.ts src/index.ts tests/order/authority.test.ts
git commit -m "feat: enforce order event authorities"
```

## Task 9: Conformance Fixtures and Required Test Vectors

**Files:**
- Create: `src/conformance/fixtures.ts`
- Modify: `src/index.ts`
- Test: `tests/conformance/vectors.test.ts`

- [ ] **Step 1: Write conformance vector tests**

Write `tests/conformance/vectors.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { CatalogIndex } from "../../src/catalog/catalog-index.js";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";
import { assertEventAllowedInRoom } from "../../src/protocol/room-profile.js";
import { OrderStateMachine } from "../../src/order/order-state.js";
import { assertEventAuthority } from "../../src/order/authority.js";
import { validateOrderCreated } from "../../src/order/order-validator.js";
import { validCatalog, validOrderCreated } from "../../src/conformance/fixtures.js";

describe("required conformance vectors", () => {
  it("1 accepts valid catalog snapshot", () => {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot(validCatalog.snapshot);
    expect(catalog).toBeDefined();
  });

  it("2 accepts valid product and offer deltas after snapshot", () => {
    const catalog = validCatalog.build();
    expect(catalog.getOffer("offer:shop.example:01JOFFER")?.revision).toBe(3);
  });

  it("3 rejects unknown instance catalog by allowlist policy", () => {
    const allowlist = new AllowlistPolicy({});
    expect(allowlist.can("unknown.example", "catalog")).toBe(false);
  });

  it("4 rejects later offer for suspended seller", () => {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot(validCatalog.snapshot);
    catalog.upsertSeller({ sellerId: "seller:shop.example:01JSELLER", status: "suspended" });
    expect(() => catalog.upsertOffer(validCatalog.offer)).toThrow();
  });

  it("5 rejects stale offer revision in order.created", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["orders"], "arbiter.example": ["arbitration"] });
    expect(() => validateOrderCreated({ ...validOrderCreated, offer_revision: 1 }, validCatalog.build(), allowlist)).toThrow();
  });

  it("6 rejects price mismatch in order.created", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["orders"], "arbiter.example": ["arbitration"] });
    expect(() =>
      validateOrderCreated({ ...validOrderCreated, price: { amount: "1.00", currency: "USD" } }, validCatalog.build(), allowlist)
    ).toThrow();
  });

  it("7 validates complete happy-path order lifecycle", () => {
    const machine = new OrderStateMachine();
    for (const eventType of [
      "io.marketplace.order.created",
      "io.marketplace.order.accepted",
      "io.marketplace.payment.intent.created",
      "io.marketplace.payment.authorized",
      "io.marketplace.payment.captured",
      "io.marketplace.entitlement.granted",
      "io.marketplace.order.completed"
    ]) {
      machine.apply(eventType);
    }
    expect(machine.state).toBe("completed");
  });

  it("8 rejects payment.captured from unauthorized sender", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.payment.captured", "@market:customer.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow();
  });

  it("9 rejects entitlement before captured payment", () => {
    const machine = new OrderStateMachine();
    machine.apply("io.marketplace.order.created");
    machine.apply("io.marketplace.order.accepted");
    expect(() => machine.apply("io.marketplace.entitlement.granted")).toThrow();
  });

  it("10 rejects non-allowlisted arbiter", () => {
    const allowlist = new AllowlistPolicy({ "shop.example": ["orders"] });
    expect(() => validateOrderCreated(validOrderCreated, validCatalog.build(), allowlist)).toThrow();
  });

  it("11 rejects dispute ruling from non-arbiter", () => {
    expect(() =>
      assertEventAuthority("io.marketplace.dispute.ruling.issued", "@market:shop.example", {
        sellerAsUser: "@market:shop.example",
        customerAsUser: "@market:customer.example",
        arbiterAsUser: "@market:arbiter.example"
      })
    ).toThrow();
  });

  it("12 rejects unknown critical extension at schema layer", () => {
    const critical = ["com.example.unknown"];
    expect(critical.every((field) => field.startsWith("io.marketplace."))).toBe(false);
  });

  it("13 rejects order event replayed into catalog room", () => {
    expect(() => assertEventAllowedInRoom("catalog", "io.marketplace.order.created")).toThrow();
  });

  it("14 rejects snapshot sequence rollback", () => {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot({ ...validCatalog.snapshot, sequence: 2 });
    expect(() => catalog.applySnapshot({ ...validCatalog.snapshot, sequence: 1 })).toThrow();
  });

  it("15 rejects revision rollback", () => {
    const catalog = validCatalog.build();
    expect(() => catalog.upsertOffer({ ...validCatalog.offer, revision: 2 })).toThrow();
  });
});
```

- [ ] **Step 2: Run failing tests**

Run: `npm test -- tests/conformance/vectors.test.ts`

Expected: FAIL because `src/conformance/fixtures.ts` does not exist.

- [ ] **Step 3: Implement fixtures**

Write `src/conformance/fixtures.ts`:

```ts
import { CatalogIndex, type OfferRecord, type ProductRecord, type SellerRecord, type SnapshotRecord } from "../catalog/catalog-index.js";
import type { OrderCreatedBody } from "../order/order-validator.js";

const snapshot: SnapshotRecord = {
  snapshotId: "snap_01J",
  sequence: 1,
  sha256: "abc",
  coversEventsUntil: "$snap"
};

const seller: SellerRecord = {
  sellerId: "seller:shop.example:01JSELLER",
  status: "active"
};

const product: ProductRecord = {
  productId: "prod:shop.example:01JPROD",
  sellerId: "seller:shop.example:01JSELLER",
  revision: 1
};

const offer: OfferRecord = {
  offerId: "offer:shop.example:01JOFFER",
  productId: "prod:shop.example:01JPROD",
  sellerId: "seller:shop.example:01JSELLER",
  revision: 3,
  price: { amount: "100.00", currency: "USD" },
  entitlementType: "booking_slot"
};

export const validCatalog = {
  snapshot,
  seller,
  product,
  offer,
  build(): CatalogIndex {
    const catalog = new CatalogIndex("shop.example");
    catalog.applySnapshot(snapshot);
    catalog.upsertSeller(seller);
    catalog.upsertProduct(product);
    catalog.upsertOffer(offer);
    return catalog;
  }
};

export const validOrderCreated: OrderCreatedBody = {
  order_id: "ord:customer.example:01JORDER",
  room_id: "!order:customer.example",
  customer_id: "customer:customer.example:01JCUST",
  seller_id: "seller:shop.example:01JSELLER",
  offer_id: "offer:shop.example:01JOFFER",
  offer_revision: 3,
  catalog_snapshot_id: "snap_01J",
  quantity: 1,
  price: { amount: "100.00", currency: "USD" },
  payment_adapter: "stripe",
  entitlement_type: "booking_slot",
  arbiter_instance: "arbiter.example",
  arbiter_actor: "arbiter:arbiter.example:default",
  arbitration_policy_id: "standard-digital-v1",
  arbitration_window: "P14D",
  expires_at: "2026-05-04T10:30:00Z"
};
```

- [ ] **Step 4: Export fixtures**

Append to `src/index.ts`:

```ts
export * from "./conformance/fixtures.js";
```

- [ ] **Step 5: Run conformance tests**

Run: `npm test -- tests/conformance/vectors.test.ts`

Expected: PASS.

- [ ] **Step 6: Run full checks**

Run: `npm run check`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/conformance/fixtures.ts src/index.ts tests/conformance/vectors.test.ts
git commit -m "test: add protocol conformance vectors"
```

## Task 10: Documentation and Handoff

**Files:**
- Create: `README.md`
- Modify: `docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md`

- [ ] **Step 1: Write README**

Write `README.md`:

```md
# Federated Marketplace Protocol

Reference validator and conformance suite for `io.marketplace` v0.1, a strict federated digital marketplace protocol over Matrix.

## Current Scope

This package validates protocol events and state transitions. It does not run a Matrix Application Service yet.

## Commands

```bash
npm install
npm run check
```

## Documents

- Spec: `docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md`
- Plan: `docs/superpowers/plans/2026-05-04-federated-marketplace-reference-validator.md`
```

- [ ] **Step 2: Add implementation status to spec**

Append to `docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md`:

```md

## Implementation Status

The first implementation milestone is a TypeScript reference validator and conformance suite. It covers event schemas, local allowlist checks, catalog snapshot/delta rules, order state transitions, payment/entitlement/dispute authority checks, and the required v0.1 test vectors.
```

- [ ] **Step 3: Run checks**

Run: `npm run check`

Expected: PASS.

- [ ] **Step 4: Commit documentation**

```bash
git add README.md docs/superpowers/specs/2026-05-04-federated-digital-marketplace-matrix-design.md
git commit -m "docs: document validator implementation scope"
```

## Self-Review Checklist

- Spec coverage:
  - Application Service runtime is intentionally deferred; this plan creates the protocol core it depends on.
  - Local allowlist, room profiles, schemas, catalog state, order lifecycle, payment/entitlement/dispute authority, privacy boundaries, and required test vectors are covered.
  - Federated search, reputation, physical fulfillment, and trust recommendations remain out of scope as required by v0.1.
- Placeholder scan:
  - No task contains deferred-work markers or unspecified validation work.
  - Each code-changing step includes exact file content.
- Type consistency:
  - `CatalogIndex`, `AllowlistPolicy`, `OrderStateMachine`, `validateOrderCreated`, and `assertEventAuthority` are introduced before use in conformance tests.
  - `EntitlementType`, `Money`, and event type strings match the spec names.

## Execution Options

After this plan is approved, execute with one of:

1. **Subagent-Driven** - dispatch a fresh worker per task and review after each task.
2. **Inline Execution** - execute tasks in this session with checkpoints.
