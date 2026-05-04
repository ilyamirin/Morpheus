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
  | "CATALOG_REFERENCE_MISMATCH"
  | "PAYMENT_TERMS_MISMATCH";

export class MarketplaceValidationError extends Error {
  constructor(
    public readonly code: ValidationCode,
    message: string,
    public readonly details: Record<string, unknown> = {}
  ) {
    super(message);
    this.name = "MarketplaceValidationError";
  }
}
