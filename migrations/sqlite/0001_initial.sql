CREATE TABLE IF NOT EXISTS appservice_transactions (
  txn_id TEXT PRIMARY KEY,
  event_ids TEXT NOT NULL,
  idempotency_hash TEXT,
  received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS raw_matrix_events (
  event_id TEXT PRIMARY KEY,
  room_id TEXT NOT NULL,
  sender TEXT NOT NULL,
  event_type TEXT NOT NULL,
  origin_server_ts INTEGER NOT NULL,
  raw_json TEXT NOT NULL,
  validation_status TEXT NOT NULL,
  validation_code TEXT,
  received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS marketplace_events (
  marketplace_event_id TEXT PRIMARY KEY,
  matrix_event_id TEXT NOT NULL,
  protocol_version TEXT NOT NULL,
  issuer_instance TEXT NOT NULL,
  actor_id TEXT,
  event_type TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_snapshots (
  snapshot_id TEXT PRIMARY KEY,
  instance_id TEXT NOT NULL,
  sequence INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  covers_events_until TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS catalog_sellers (
  seller_id TEXT PRIMARY KEY,
  issuer_instance TEXT NOT NULL,
  status TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_products (
  product_id TEXT PRIMARY KEY,
  seller_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_offers (
  offer_id TEXT PRIMARY KEY,
  product_id TEXT NOT NULL,
  seller_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  price TEXT NOT NULL,
  inventory_kind TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_tombstones (
  object_id TEXT PRIMARY KEY,
  object_type TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS order_rooms (
  room_id TEXT PRIMARY KEY,
  order_id TEXT UNIQUE,
  status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS orders (
  order_id TEXT PRIMARY KEY,
  room_id TEXT NOT NULL,
  customer_id TEXT NOT NULL,
  seller_id TEXT NOT NULL,
  offer_id TEXT NOT NULL,
  status TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS order_events (
  matrix_event_id TEXT PRIMARY KEY,
  order_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS payments (
  payment_id TEXT PRIMARY KEY,
  order_id TEXT NOT NULL,
  status TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS entitlements (
  entitlement_id TEXT PRIMARY KEY,
  order_id TEXT NOT NULL,
  status TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS disputes (
  dispute_id TEXT PRIMARY KEY,
  order_id TEXT NOT NULL,
  status TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS arbitration_rulings (
  ruling_id TEXT PRIMARY KEY,
  dispute_id TEXT NOT NULL,
  status TEXT NOT NULL,
  body TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS allowlist_entries (
  instance_id TEXT PRIMARY KEY,
  capabilities TEXT NOT NULL,
  status TEXT NOT NULL,
  valid_until TEXT,
  audit TEXT
);

CREATE TABLE IF NOT EXISTS config_revisions (
  revision_id INTEGER PRIMARY KEY AUTOINCREMENT,
  config TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS projection_errors (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  matrix_event_id TEXT,
  code TEXT NOT NULL,
  message TEXT NOT NULL,
  details TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS evm_escrow_logs (
  chain_id INTEGER NOT NULL,
  tx_hash TEXT NOT NULL,
  log_index INTEGER NOT NULL,
  block_number INTEGER NOT NULL,
  block_hash TEXT NOT NULL,
  escrow_contract TEXT NOT NULL,
  order_hash TEXT NOT NULL,
  event_name TEXT NOT NULL,
  payload TEXT NOT NULL,
  emitted_marketplace_event_id TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (chain_id, tx_hash, log_index)
);

CREATE TABLE IF NOT EXISTS evm_escrow_checkpoints (
  chain_id INTEGER NOT NULL,
  escrow_contract TEXT NOT NULL,
  latest_scanned_block INTEGER NOT NULL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (chain_id, escrow_contract)
);
