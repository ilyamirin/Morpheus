import { describe, expect, it } from "vitest";
import { validateApplicationServiceSender, validateAppserviceTransaction } from "../../src/protocol/appservice.js";

describe("Application Service validation", () => {
  it("validates marketplace AS sender namespace", () => {
    expect(() =>
      validateApplicationServiceSender("@market:shop.example", {
        instanceId: "shop.example",
        serverName: "shop.example",
        exclusiveUserLocalpart: "market"
      })
    ).not.toThrow();
    expect(() =>
      validateApplicationServiceSender("@buyer:shop.example", {
        instanceId: "shop.example",
        serverName: "shop.example",
        exclusiveUserLocalpart: "market"
      })
    ).toThrow(/namespace/i);
  });

  it("rejects duplicate appservice transactions with different event ids", () => {
    const seen = new Map<string, string[]>();
    validateAppserviceTransaction({ txnId: "t1", eventIds: ["$a"] }, seen);
    expect(() => validateAppserviceTransaction({ txnId: "t1", eventIds: ["$b"] }, seen)).toThrow(/idempotent/i);
  });
});
