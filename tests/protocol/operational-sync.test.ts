import { describe, expect, it } from "vitest";
import { validateBackfillPage, validateSnapshotCache } from "../../src/protocol/operational-sync.js";

describe("operational sync rules", () => {
  it("rejects backfill pages with duplicate Matrix event ids", () => {
    expect(() => validateBackfillPage([{ event_id: "$a" }, { event_id: "$a" }])).toThrow(/duplicate/i);
  });

  it("invalidates snapshot cache entries when the hash changes for the same sequence", () => {
    expect(() =>
      validateSnapshotCache(
        { snapshotId: "snap:shop.example:01J", sequence: 1, sha256: "sha256:" + "1".repeat(64) },
        { snapshotId: "snap:shop.example:01J", sequence: 1, sha256: "sha256:" + "2".repeat(64) }
      )
    ).toThrow(/cache/i);
  });
});
