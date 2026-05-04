import { describe, expect, it } from "vitest";
import { canonicalJson, sha256Canonical, assertSha256Matches } from "../../src/protocol/canonical-json.js";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";

describe("canonical JSON and hashes", () => {
  it("serializes objects with stable sorted keys", () => {
    expect(canonicalJson({ b: 2, a: { d: 4, c: 3 } })).toBe('{"a":{"c":3,"d":4},"b":2}');
  });

  it("computes sha256-prefixed canonical hashes", () => {
    expect(sha256Canonical({ b: 2, a: 1 })).toMatch(/^sha256:[0-9a-f]{64}$/);
    expect(sha256Canonical({ b: 2, a: 1 })).toBe(sha256Canonical({ a: 1, b: 2 }));
  });

  it("rejects canonical hash mismatches", () => {
    expect(() => assertSha256Matches({ a: 1 }, "sha256:" + "0".repeat(64))).toThrow(MarketplaceValidationError);
  });
});
