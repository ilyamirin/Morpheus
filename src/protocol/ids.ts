export interface ParsedActorId {
  kind: "seller" | "customer" | "arbiter";
  instanceId: string;
  localId: string;
}

export function parseActorId(actorId: string): ParsedActorId {
  const [kind, instanceId, localId] = actorId.split(":");
  if ((kind !== "seller" && kind !== "customer" && kind !== "arbiter") || !instanceId || !localId) {
    throw new Error(`Invalid actor id: ${actorId}`);
  }
  return { kind, instanceId, localId };
}

export function parseObjectInstance(objectId: string): string {
  const [, instanceId, localId] = objectId.split(":");
  if (!instanceId || !localId) {
    throw new Error(`Invalid object id: ${objectId}`);
  }
  return instanceId;
}
