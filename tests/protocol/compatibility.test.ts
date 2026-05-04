import { describe, expect, it } from "vitest";
import { validateInstanceCompatibility } from "../../src/protocol/compatibility.js";
import { AllowlistPolicy } from "../../src/protocol/allowlist.js";

describe("inter-instance compatibility", () => {
  it("discovers compatible instances only from allowlisted catalog rooms", () => {
    expect(() =>
      validateInstanceCompatibility(
        {
          instance_id: "shop.example",
          catalog_room_id: "!catalog:shop.example",
          protocol_versions: ["0.1"],
          matrix_room_version: "10",
          payment_adapters: ["stripe"],
          arbitration_policies: ["standard-digital-v1"]
        },
        {
          allowlist: new AllowlistPolicy({ "shop.example": ["catalog", "indexing"] }),
          minimumRoomVersion: "9",
          requiredProtocolVersion: "0.1"
        }
      )
    ).not.toThrow();
  });

  it("rejects non-allowlisted instance profiles", () => {
    expect(() =>
      validateInstanceCompatibility(
        {
          instance_id: "shop.example",
          catalog_room_id: "!catalog:shop.example",
          protocol_versions: ["0.1"],
          matrix_room_version: "10",
          payment_adapters: ["stripe"],
          arbitration_policies: ["standard-digital-v1"]
        },
        {
          allowlist: new AllowlistPolicy({}),
          minimumRoomVersion: "9",
          requiredProtocolVersion: "0.1"
        }
      )
    ).toThrow(/allowlist/i);
  });
});
