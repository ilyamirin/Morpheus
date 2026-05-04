import { MarketplaceValidationError } from "./errors.js";

export interface RetentionPolicy {
  catalogTombstoneDays: number;
  orderArchiveDays: number;
  completedEntitlementDays: number;
  suspendedActorDays: number;
}

export function validateRetentionPolicy(policy: RetentionPolicy): void {
  const invalid =
    policy.catalogTombstoneDays < 1 ||
    policy.orderArchiveDays < 1 ||
    policy.completedEntitlementDays < 1 ||
    policy.suspendedActorDays < 1;
  if (invalid) {
    throw new MarketplaceValidationError("POLICY_VIOLATION", "Retention windows must be positive", { policy });
  }
}
