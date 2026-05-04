import { assertEventAuthority, type OrderAuthorities } from "./authority.js";
import { validateOrderEventSequence, type OrderFlowEvent } from "./order-flow-validator.js";
import { MarketplaceValidationError } from "../protocol/errors.js";
import { validateMarketplaceEvent } from "../protocol/marketplace-event-validator.js";
import type { MatrixMarketplaceEvent } from "../protocol/types.js";

export interface OrderRoomTimelineContext extends OrderAuthorities {
  roomId: string;
  requiredMembers: string[];
  members: string[];
}

export function validateOrderRoomTimeline(events: unknown[], context: OrderRoomTimelineContext): void {
  for (const requiredMember of context.requiredMembers) {
    if (!context.members.includes(requiredMember)) {
      throw new MarketplaceValidationError("ROOM_MEMBERSHIP_VIOLATION", "Order room is missing required member", {
        requiredMember,
        members: context.members
      });
    }
  }

  const flowEvents: OrderFlowEvent[] = [];
  const unjoinedRepresentatives = new Set<string>();
  let sellerAccepted = false;
  for (const rawEvent of events) {
    const result = validateMarketplaceEvent(rawEvent, { roomProfile: "order" });
    if (result.status === "ignored") {
      continue;
    }
    const event = result.event as MatrixMarketplaceEvent;
    if (event.room_id !== context.roomId) {
      throw new MarketplaceValidationError("CATALOG_REFERENCE_MISMATCH", "Order event was replayed into another room", {
        expected: context.roomId,
        actual: event.room_id
      });
    }
    assertEventAuthority(event.type, event.sender, context);
    collectUnjoinedCustomerRepresentatives(event, context, unjoinedRepresentatives);
    if (event.type === "io.marketplace.order.accepted") {
      sellerAccepted = true;
    }
    flowEvents.push({ type: event.type, body: event.content.body as object });
  }
  if (!sellerAccepted && unjoinedRepresentatives.size > 0) {
    throw new MarketplaceValidationError(
      "ROOM_MEMBERSHIP_VIOLATION",
      "Customer representative disclosed in customer.bound is not joined to the order room",
      {
        representative: Array.from(unjoinedRepresentatives)[0],
        members: context.members
      }
    );
  }
  validateOrderEventSequence(flowEvents);
}

function collectUnjoinedCustomerRepresentatives(
  event: MatrixMarketplaceEvent,
  context: OrderRoomTimelineContext,
  unjoinedRepresentatives: Set<string>
): void {
  if (event.type !== "io.marketplace.actor.customer.bound") {
    return;
  }
  const body = event.content.body;
  if (!body || typeof body !== "object") {
    return;
  }
  const representatives = (body as Record<string, unknown>).authorized_representatives;
  if (!Array.isArray(representatives)) {
    return;
  }
  for (const representative of representatives) {
    if (typeof representative === "string" && !context.members.includes(representative)) {
      unjoinedRepresentatives.add(representative);
    }
  }
}
