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
