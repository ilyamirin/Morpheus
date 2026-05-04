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
