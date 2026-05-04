import { createHash } from "node:crypto";
import { MarketplaceValidationError } from "./errors.js";

export const sha256SchemaPattern = /^sha256:[0-9a-f]{64}$/;

export function canonicalJson(value: unknown): string {
  return JSON.stringify(sortCanonical(value));
}

export function sha256Canonical(value: unknown): string {
  return `sha256:${createHash("sha256").update(canonicalJson(value), "utf8").digest("hex")}`;
}

export function assertSha256Matches(value: unknown, expected: string): void {
  if (!sha256SchemaPattern.test(expected)) {
    throw new MarketplaceValidationError("HASH_MISMATCH", "Expected hash must use sha256:<64 hex> format", {
      expected
    });
  }
  const actual = sha256Canonical(value);
  if (actual !== expected) {
    throw new MarketplaceValidationError("HASH_MISMATCH", "Canonical hash mismatch", { expected, actual });
  }
}

function sortCanonical(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sortCanonical);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, entry]) => [key, sortCanonical(entry)])
    );
  }
  return value;
}
