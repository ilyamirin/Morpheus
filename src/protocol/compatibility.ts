import type { AllowlistPolicy } from "./allowlist.js";
import { MarketplaceValidationError } from "./errors.js";

export interface InstanceCompatibilityProfile {
  instance_id: string;
  catalog_room_id: string;
  protocol_versions: string[];
  matrix_room_version: string;
  payment_adapters: string[];
  arbitration_policies: string[];
}

export interface InstanceCompatibilityContext {
  allowlist: AllowlistPolicy;
  minimumRoomVersion: string;
  requiredProtocolVersion: string;
}

export function validateInstanceCompatibility(
  profile: InstanceCompatibilityProfile,
  context: InstanceCompatibilityContext
): void {
  if (!context.allowlist.can(profile.instance_id, "catalog") || !context.allowlist.can(profile.instance_id, "indexing")) {
    throw new MarketplaceValidationError("INSTANCE_NOT_ALLOWLISTED", "Instance profile is not allowlisted for catalog indexing", {
      instanceId: profile.instance_id
    });
  }
  if (!profile.protocol_versions.includes(context.requiredProtocolVersion)) {
    throw new MarketplaceValidationError("UNSUPPORTED_PROTOCOL_VERSION", "Instance does not support required protocol version", {
      required: context.requiredProtocolVersion,
      supported: profile.protocol_versions
    });
  }
  if (Number(profile.matrix_room_version) < Number(context.minimumRoomVersion)) {
    throw new MarketplaceValidationError("ROOM_PROFILE_VIOLATION", "Instance Matrix room version is below minimum", {
      minimum: context.minimumRoomVersion,
      actual: profile.matrix_room_version
    });
  }
}
