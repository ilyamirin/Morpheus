import { MarketplaceValidationError } from "./errors.js";

export type ObjectIdKind =
  | "prod"
  | "offer"
  | "ord"
  | "pay"
  | "refund"
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
  "refund",
  "ent",
  "disp",
  "seller",
  "customer",
  "arbiter",
  "evt",
  "snap"
]);

const instanceIdPattern = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$/;
const localIdPattern = /^[A-Z0-9][A-Z0-9_-]{2,63}$/;

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
    !isValidInstanceId(instanceId) ||
    !isValidLocalId(localId)
  ) {
    throw invalidId("actor", actorId);
  }
  return { kind, instanceId, localId };
}

export function parseObjectInstance(objectId: string): string {
  const segments = objectId.split(":");
  const [kind, instanceId, localId] = segments;
  if (segments.length !== 3 || !isObjectIdKind(kind) || !isValidInstanceId(instanceId) || !isValidLocalId(localId)) {
    throw invalidId("object", objectId);
  }
  return instanceId;
}

export function isProtocolObjectId(id: string, kind?: ObjectIdKind): boolean {
  const segments = id.split(":");
  const [actualKind, instanceId, localId] = segments;
  return (
    segments.length === 3 &&
    isObjectIdKind(actualKind) &&
    (!kind || actualKind === kind) &&
    isValidInstanceId(instanceId) &&
    isValidLocalId(localId)
  );
}

export function isValidInstanceId(instanceId: string | undefined): instanceId is string {
  return typeof instanceId === "string" && instanceIdPattern.test(instanceId);
}

export function isValidLocalId(localId: string | undefined): localId is string {
  return typeof localId === "string" && localIdPattern.test(localId);
}

function isObjectIdKind(kind: string | undefined): kind is ObjectIdKind {
  return kind !== undefined && OBJECT_ID_KINDS.has(kind);
}

function invalidId(idType: "actor" | "object", id: string): MarketplaceValidationError {
  return new MarketplaceValidationError("INVALID_ID", `Invalid ${idType} id: ${id}`, { idType, id });
}
