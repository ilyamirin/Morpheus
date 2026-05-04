import { z } from "zod";
import { MarketplaceValidationError } from "./errors.js";
import { assertEventAllowedInRoom } from "./room-profile.js";
import { marketplaceEventSchema } from "./schemas.js";
import type { MatrixMarketplaceEvent, RoomProfile } from "./types.js";

export interface MarketplaceEventValidationContext {
  roomProfile: RoomProfile;
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
    event_id: z.string().startsWith("$"),
    created_at: z.string().datetime({ offset: true }),
    issuer: z.object({
      instance_id: z.string().min(1),
      actor_id: z.string().min(1).optional(),
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
  if (generic.event_id !== generic.content.event_id) {
    throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Matrix event_id must match content.event_id", {
      matrixEventId: generic.event_id,
      envelopeEventId: generic.content.event_id
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

  assertEventAllowedInRoom(context.roomProfile, known.data.type);
  return { status: "accepted", event: known.data as MatrixMarketplaceEvent };
}
