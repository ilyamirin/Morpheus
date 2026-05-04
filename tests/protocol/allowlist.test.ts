import { describe, expect, it } from "vitest";
import { AllowlistPolicy, validateAllowlistPolicy } from "../../src/protocol/allowlist.js";

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

  it("denies expired and revoked entries for new orders while preserving replay policy", () => {
    const policy = new AllowlistPolicy({
      "shop.example": {
        capabilities: ["orders"],
        status: "revoked",
        validUntil: "2026-05-01T00:00:00Z",
        audit: { reason: "fraud", updatedBy: "@admin:local.example", updatedAt: "2026-05-01T00:00:00Z" }
      }
    });

    expect(policy.can("shop.example", "orders", new Date("2026-05-04T00:00:00Z"))).toBe(false);
    expect(policy.canReplayExistingOrder("shop.example")).toBe(true);
  });

  it("validates allowlist audit metadata", () => {
    expect(() =>
      validateAllowlistPolicy(
        {
          "shop.example": {
            capabilities: ["catalog", "indexing"],
            status: "active",
            audit: { reason: "partner", updatedBy: "@admin:local.example", updatedAt: "2026-05-01T00:00:00Z" }
          }
        },
        new Date("2026-05-04T00:00:00Z")
      )
    ).not.toThrow();
  });
});
