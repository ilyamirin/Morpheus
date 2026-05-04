import { describe, expect, it } from "vitest";
import { validateSecurityEnvelope } from "../../src/protocol/security.js";

describe("security policy", () => {
  it("rejects downgrade attempts below the minimum consumer version", () => {
    expect(() =>
      validateSecurityEnvelope({ protocol_version: "0.1", min_consumer_version: "0.2" }, { supportedVersion: "0.1" })
    ).toThrow(/downgrade/i);
  });

  it("rejects confused-deputy sender and issuer server mismatches", () => {
    expect(() =>
      validateSecurityEnvelope(
        { sender: "@market:evil.example", issuer: { instance_id: "shop.example", matrix_user_id: "@market:evil.example" } },
        { supportedVersion: "0.1" }
      )
    ).toThrow(/issuer/i);
  });
});
