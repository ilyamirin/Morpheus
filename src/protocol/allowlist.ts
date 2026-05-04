export type AllowlistCapability = "catalog" | "orders" | "arbitration" | "payments";

export class AllowlistPolicy {
  private readonly entries: Map<string, Set<AllowlistCapability>>;

  constructor(config: Record<string, AllowlistCapability[]>) {
    this.entries = new Map(
      Object.entries(config).map(([instanceId, capabilities]) => [instanceId, new Set(capabilities)])
    );
  }

  can(instanceId: string, capability: AllowlistCapability): boolean {
    return this.entries.get(instanceId)?.has(capability) ?? false;
  }
}
