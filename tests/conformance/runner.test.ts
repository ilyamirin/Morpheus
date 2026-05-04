import { describe, expect, it } from "vitest";
import { ConformanceRunner } from "../../src/conformance/runner.js";

describe("ConformanceRunner", () => {
  it("runs required and optional vectors with stable results", () => {
    const runner = new ConformanceRunner([
      { id: "required.valid", group: "required", run: () => undefined },
      { id: "optional.invalid", group: "optional", run: () => { throw new Error("bad vector"); } }
    ]);

    expect(runner.runAll()).toEqual([
      { id: "required.valid", group: "required", status: "passed" },
      { id: "optional.invalid", group: "optional", status: "failed", message: "bad vector" }
    ]);
  });
});
