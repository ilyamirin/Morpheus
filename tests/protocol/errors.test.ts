import { describe, expect, it } from "vitest";
import { MarketplaceValidationError, validationDisposition } from "../../src/protocol/errors.js";

describe("canonical error model", () => {
  it("classifies validation errors as retryable or terminal", () => {
    expect(validationDisposition("MISSING_REQUIRED_FIELD")).toBe("retryable");
    expect(validationDisposition("UNAUTHORIZED_SENDER")).toBe("terminal");
    expect(new MarketplaceValidationError("POLICY_VIOLATION", "bad").disposition).toBe("terminal");
  });
});
