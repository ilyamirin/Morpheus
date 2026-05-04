import { describe, expect, it } from "vitest";
import { validateExtensionNamespace } from "../../src/protocol/versioning.js";

describe("versioning and extensions", () => {
  it("rejects non-standard extensions in the io.marketplace namespace", () => {
    expect(() => validateExtensionNamespace("io.marketplace.experimental.foo")).toThrow(/namespace/i);
  });

  it("accepts reverse-DNS non-critical extension namespaces", () => {
    expect(() => validateExtensionNamespace("com.example.marketplace.foo")).not.toThrow();
  });
});
