import { MarketplaceValidationError } from "./errors.js";

export type ObjectIdKind =
  | "prod"
  | "offer"
  | "ord"
  | "pay"
  | "ent"
  | "disp"
  | "seller"
  | "customer"
  | "arbiter"
  | "evt"
  | "snap";

const OBJECT_ID_KINDS = new Set<string>([
  "prod",
  "offer",
  "ord",
  "pay",
  "ent",
  "disp",
  "seller",
  "customer",
  "arbiter",
  "evt",
  "snap"
]);

export interface ParsedActorId {
  kind: "seller" | "customer" | "arbiter";
  instanceId: string;
  localId: string;
}

export function parseActorId(actorId: string): ParsedActorId {
  const segments = actorId.split(":");
  const [kind, instanceId, localId] = segments;
  if (
    segments.length !== 3 ||
    (kind !== "seller" && kind !== "customer" && kind !== "arbiter") ||
    !instanceId ||
    !localId
  ) {
    throw invalidId("actor", actorId);
  }
  return { kind, instanceId, localId };
}

export function parseObjectInstance(objectId: string): string {
  const segments = objectId.split(":");
  const [kind, instanceId, localId] = segments;
  if (segments.length !== 3 || !isObjectIdKind(kind) || !instanceId || !localId) {
    throw invalidId("object", objectId);
  }
  return instanceId;
}

function isObjectIdKind(kind: string | undefined): kind is ObjectIdKind {
  return kind !== undefined && OBJECT_ID_KINDS.has(kind);
}

function invalidId(idType: "actor" | "object", id: string): MarketplaceValidationError {
  return new MarketplaceValidationError("INVALID_ID", `Invalid ${idType} id: ${id}`, { idType, id });
}
