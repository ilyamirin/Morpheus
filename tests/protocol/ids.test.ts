import { describe, expect, it } from "vitest";
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
});
