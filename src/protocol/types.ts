import type {
  CATALOG_EVENT_TYPES,
  DISPUTE_RULINGS,
  ENTITLEMENT_TYPES,
  ORDER_EVENT_TYPES,
  PRODUCT_KINDS,
  ROOM_PROFILES
} from "./constants.js";

export type CatalogEventType = (typeof CATALOG_EVENT_TYPES)[number];
export type OrderEventType = (typeof ORDER_EVENT_TYPES)[number];
export type MarketplaceEventType = CatalogEventType | OrderEventType;
export type RoomProfile = (typeof ROOM_PROFILES)[keyof typeof ROOM_PROFILES];
export type ProductKind = (typeof PRODUCT_KINDS)[number];
export type EntitlementType = (typeof ENTITLEMENT_TYPES)[number];
export type DisputeRuling = (typeof DISPUTE_RULINGS)[number];

export interface Issuer {
  instance_id: string;
  actor_id?: string;
  matrix_user_id: string;
}

export interface MarketplaceEnvelope<TBody> {
  protocol: "io.marketplace";
  protocol_version: "0.1";
  event_id: string;
  created_at: string;
  issuer: Issuer;
  critical: string[];
  body: TBody;
}

export interface MatrixMarketplaceEvent<TBody = unknown> {
  type: MarketplaceEventType | string;
  room_id: string;
  event_id: string;
  sender: string;
  origin_server_ts: number;
  content: MarketplaceEnvelope<TBody>;
}

export interface Money {
  amount: string;
  currency: string;
}
