import { describe, expect, it } from "vitest";
import { MarketplaceValidationError } from "../../src/protocol/errors.js";
import { parseActorId, parseObjectInstance } from "../../src/protocol/ids.js";

describe("protocol ids", () => {
  it("extracts actor kind and instance", () => {
    expect(parseActorId("seller:shop.example:01JSELLER")).toEqual({
      kind: "seller",
      instanceId: "shop.example",
      localId: "01JSELLER"
    });
  });

  it("extracts object instance from offer ids", () => {
    expect(parseObjectInstance("offer:shop.example:01JOFFER")).toBe("shop.example");
  });

  it("rejects actor ids with extra segments", () => {
    expect(() => parseActorId("seller:shop.example:01J:extra")).toThrow(MarketplaceValidationError);
  });

  it("rejects object ids with extra segments", () => {
    expect(() => parseObjectInstance("offer:shop.example:01J:extra")).toThrow(MarketplaceValidationError);
  });

  it("rejects object ids with unsupported prefixes", () => {
    expect(() => parseObjectInstance("bad:shop.example:01J")).toThrow(MarketplaceValidationError);
  });
});
