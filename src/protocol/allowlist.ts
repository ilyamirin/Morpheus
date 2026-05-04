import { z } from "zod";
import { MarketplaceValidationError } from "./errors.js";

export type AllowlistCapability = "catalog" | "orders" | "arbitration" | "payments" | "indexing";

export type AllowlistEntryStatus = "active" | "revoked";

export interface AllowlistAudit {
  reason: string;
  updatedBy: string;
  updatedAt: string;
}

export interface AllowlistEntry {
  capabilities: AllowlistCapability[];
  status?: AllowlistEntryStatus;
  validUntil?: string;
  audit?: AllowlistAudit;
}

export type AllowlistConfig = Record<string, AllowlistCapability[] | AllowlistEntry>;

const allowlistEntrySchema = z.object({
  capabilities: z.array(z.enum(["catalog", "orders", "arbitration", "payments", "indexing"])),
  status: z.enum(["active", "revoked"]).optional(),
  validUntil: z.string().datetime({ offset: true }).optional(),
  audit: z.object({
    reason: z.string().min(1),
    updatedBy: z.string().regex(/^@[^:]+:[^:]+$/),
    updatedAt: z.string().datetime({ offset: true })
  }).optional()
});

export class AllowlistPolicy {
  private readonly entries: Map<string, Required<Pick<AllowlistEntry, "capabilities" | "status">> & Omit<AllowlistEntry, "capabilities" | "status">>;

  constructor(config: AllowlistConfig) {
    this.entries = new Map(
      Object.entries(config).map(([instanceId, entry]) => {
        const normalized = Array.isArray(entry)
          ? { capabilities: entry, status: "active" as const }
          : { status: "active" as const, ...entry };
        return [instanceId, normalized];
      })
    );
  }

  can(instanceId: string, capability: AllowlistCapability, now = new Date()): boolean {
    const entry = this.entries.get(instanceId);
    if (!entry || entry.status !== "active") {
      return false;
    }
    if (entry.validUntil && new Date(entry.validUntil).getTime() <= now.getTime()) {
      return false;
    }
    return entry.capabilities.includes(capability);
  }

  canReplayExistingOrder(instanceId: string): boolean {
    return this.entries.has(instanceId);
  }
}

export function validateAllowlistPolicy(config: AllowlistConfig, now = new Date()): void {
  for (const [instanceId, rawEntry] of Object.entries(config)) {
    if (!instanceId) {
      throw new MarketplaceValidationError("POLICY_VIOLATION", "Allowlist instance id is required");
    }
    const entry = Array.isArray(rawEntry) ? { capabilities: rawEntry, status: "active" } : rawEntry;
    const result = allowlistEntrySchema.safeParse(entry);
    if (!result.success) {
      throw new MarketplaceValidationError("POLICY_VIOLATION", "Invalid allowlist entry", {
        instanceId,
        issues: result.error.issues
      });
    }
    if (result.data.validUntil && new Date(result.data.validUntil).getTime() <= now.getTime() && result.data.status === "active") {
      throw new MarketplaceValidationError("POLICY_VIOLATION", "Expired allowlist entries must be revoked", {
        instanceId,
        validUntil: result.data.validUntil
      });
    }
  }
}
