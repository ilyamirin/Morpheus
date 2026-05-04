import { MarketplaceValidationError } from "./errors.js";
import type { RoomProfile } from "./types.js";

const catalogForbiddenKeys = new Set([
  "order_id",
  "customer_id",
  "payment_id",
  "entitlement_id",
  "dispute_id",
  "email",
  "phone",
  "address"
]);

const secretKeyPattern = /(secret|token|password|credential|bearer|private_key)/i;
const bearerUrlPattern = /[?&](token|access_token|bearer|sig|signature)=/i;

export function validateMarketplacePrivacy(event: unknown, roomProfile: RoomProfile): void {
  const body = ((event as { content?: { body?: unknown } }).content?.body ?? {}) as unknown;
  const violations: string[] = [];
  walk(body, [], (path, value) => {
    const key = path[path.length - 1] ?? "";
    if (roomProfile === "catalog" && catalogForbiddenKeys.has(key)) {
      violations.push(`catalog event contains forbidden ${key}`);
    }
    if (roomProfile === "order" && (secretKeyPattern.test(key) || (typeof value === "string" && bearerUrlPattern.test(value)))) {
      violations.push(`order event contains secret at ${path.join(".")}`);
    }
  });

  if (violations.length > 0) {
    throw new MarketplaceValidationError("PRIVACY_VIOLATION", violations[0] ?? "Privacy violation", { violations });
  }
}

function walk(value: unknown, path: string[], visit: (path: string[], value: unknown) => void): void {
  if (Array.isArray(value)) {
    value.forEach((item, index) => walk(item, [...path, String(index)], visit));
    return;
  }
  if (value && typeof value === "object") {
    for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
      const nextPath = [...path, key];
      visit(nextPath, entry);
      walk(entry, nextPath, visit);
    }
  }
}
