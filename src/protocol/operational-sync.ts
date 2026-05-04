import { MarketplaceValidationError } from "./errors.js";

export interface BackfillEventRef {
  event_id: string;
}

export interface SnapshotCacheEntry {
  snapshotId: string;
  sequence: number;
  sha256: string;
}

export function validateBackfillPage(events: BackfillEventRef[]): void {
  const seen = new Set<string>();
  for (const event of events) {
    if (seen.has(event.event_id)) {
      throw new MarketplaceValidationError("DUPLICATE_EVENT", "Backfill page contains duplicate Matrix event ids", {
        eventId: event.event_id
      });
    }
    seen.add(event.event_id);
  }
}

export function validateSnapshotCache(previous: SnapshotCacheEntry, next: SnapshotCacheEntry): void {
  if (previous.sequence === next.sequence && previous.sha256 !== next.sha256) {
    throw new MarketplaceValidationError("HASH_MISMATCH", "Snapshot cache entry changed hash for the same sequence", {
      previous,
      next
    });
  }
}
