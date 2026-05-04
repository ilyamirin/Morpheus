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
    flowEvents.push({ type: event.type, body: event.content.body as object });
  }
  validateOrderEventSequence(flowEvents);
}
