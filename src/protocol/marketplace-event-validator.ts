import { z } from "zod";
import { MarketplaceValidationError } from "./errors.js";
import { assertEventAllowedInRoom } from "./room-profile.js";
import { marketplaceEventSchema } from "./schemas.js";
import type { MatrixMarketplaceEvent, RoomProfile } from "./types.js";
import { isProtocolObjectId, isValidInstanceId } from "./ids.js";

export interface MarketplaceEventValidationContext {
  roomProfile: RoomProfile;
  supportedCritical?: string[];
}

export type MarketplaceEventValidationResult =
  | { status: "accepted"; event: MatrixMarketplaceEvent }
  | { status: "ignored"; reason: "unknown_event_type" };

const genericMarketplaceEventSchema = z.object({
  type: z.string().min(1),
  room_id: z.string().startsWith("!"),
  event_id: z.string().startsWith("$"),
  sender: z.string().regex(/^@[^:]+:[^:]+$/),
  origin_server_ts: z.number().int().nonnegative(),
  unsigned: z.object({ redacted_because: z.unknown().optional() }).optional(),
  content: z.object({
    protocol: z.literal("io.marketplace"),
    protocol_version: z.literal("0.1"),
    protocol_event_id: z.string().refine((id) => isProtocolObjectId(id, "evt"), "Invalid protocol_event_id"),
    created_at: z.string().datetime({ offset: true }),
    issuer: z.object({
      instance_id: z.string().refine((id) => isValidInstanceId(id), "Invalid instance id"),
      actor_id: z
        .string()
        .refine(
          (id) =>
            isProtocolObjectId(id, "seller") ||
            isProtocolObjectId(id, "customer") ||
            isProtocolObjectId(id, "arbiter"),
          "Invalid actor id"
        )
        .optional(),
      matrix_user_id: z.string().regex(/^@[^:]+:[^:]+$/)
    }),
    critical: z.array(z.string()),
    body: z.unknown()
  })
});

export function validateMarketplaceEvent(
  event: unknown,
  context: MarketplaceEventValidationContext
): MarketplaceEventValidationResult {
  const generic = genericMarketplaceEventSchema.parse(event);
  if (generic.unsigned?.redacted_because) {
    throw new MarketplaceValidationError("REDACTED_EVENT", "Redacted marketplace events are not protocol-valid", {
      eventId: generic.event_id
    });
  }
  if (generic.sender !== generic.content.issuer.matrix_user_id) {
    throw new MarketplaceValidationError("UNAUTHORIZED_SENDER", "Matrix sender must match issuer matrix_user_id", {
      sender: generic.sender,
      issuer: generic.content.issuer.matrix_user_id
    });
  }

  const known = marketplaceEventSchema.safeParse(generic);
  if (!known.success) {
    const isUnknownType = known.error.issues.some((issue) => issue.path[0] === "type");
    if (isUnknownType) {
      if (generic.content.critical.length > 0) {
        throw new MarketplaceValidationError("UNKNOWN_CRITICAL_EXTENSION", "Unknown event type has critical extensions", {
          eventType: generic.type,
          critical: generic.content.critical
        });
      }
      return { status: "ignored", reason: "unknown_event_type" };
    }
    throw known.error;
  }

  assertSupportedCritical(generic.content.critical, context.supportedCritical ?? []);

  assertEventAllowedInRoom(context.roomProfile, known.data.type);
  return { status: "accepted", event: known.data as MatrixMarketplaceEvent };
}

function assertSupportedCritical(critical: string[], supported: string[]): void {
  const unsupported = critical.filter((extension) => !supported.includes(extension));
  if (unsupported.length > 0) {
    throw new MarketplaceValidationError("UNKNOWN_CRITICAL_EXTENSION", "Unsupported critical extension", {
      unsupported,
      supported
    });
  }
}
