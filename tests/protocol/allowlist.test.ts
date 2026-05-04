import { describe, expect, it } from "vitest";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";

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
});
