export type ValidationCode =
  | "UNSUPPORTED_PROTOCOL_VERSION"
  | "ROOM_PROFILE_VIOLATION"
  | "UNAUTHORIZED_SENDER"
  | "INSTANCE_NOT_ALLOWLISTED"
  | "ACTOR_NOT_ACTIVE"
  | "REVISION_ROLLBACK"
  | "MISSING_REQUIRED_FIELD"
  | "UNKNOWN_CRITICAL_EXTENSION"
  | "INVALID_STATE_TRANSITION"
  | "INVALID_ID"
  | "CATALOG_REFERENCE_MISMATCH"
  | "PAYMENT_TERMS_MISMATCH"
  | "HASH_MISMATCH"
  | "REDACTED_EVENT"
  | "ROOM_MEMBERSHIP_VIOLATION"
  | "PRIVACY_VIOLATION"
  | "POLICY_VIOLATION"
  | "DUPLICATE_EVENT";

export type ValidationDisposition = "retryable" | "terminal";

export function validationDisposition(code: ValidationCode): ValidationDisposition {
  return code === "ROOM_PROFILE_VIOLATION" || code === "MISSING_REQUIRED_FIELD" ? "retryable" : "terminal";
}

export class MarketplaceValidationError extends Error {
  constructor(
    public readonly code: ValidationCode,
    message: string,
    public readonly details: Record<string, unknown> = {}
  ) {
    super(message);
    this.name = "MarketplaceValidationError";
  }

  get disposition(): ValidationDisposition {
    return validationDisposition(this.code);
  }
}
