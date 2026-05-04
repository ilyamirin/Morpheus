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
