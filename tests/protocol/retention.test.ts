import { describe, expect, it } from "vitest";
import { validateRetentionPolicy } from "../../src/protocol/retention.js";

describe("retention policy", () => {
  it("requires catalog tombstones and completed entitlement records to be retained", () => {
    expect(() =>
      validateRetentionPolicy({
        catalogTombstoneDays: 0,
        orderArchiveDays: 365,
        completedEntitlementDays: 0,
        suspendedActorDays: 90
      })
    ).toThrow(/retention/i);
  });
});
