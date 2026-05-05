use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use morpheus_protocol::{ValidationCode, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row, SqlitePool, types::Json};
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServiceTransactionRecord {
    pub txn_id: String,
    pub event_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMatrixEventRecord {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub origin_server_ts: i64,
    pub raw_json: Value,
    pub validation_status: String,
    pub validation_code: Option<ValidationCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplaceEventRecord {
    pub marketplace_event_id: String,
    pub matrix_event_id: String,
    pub protocol_version: String,
    pub issuer_instance: String,
    pub actor_id: Option<String>,
    pub event_type: String,
    pub body: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionErrorRecord {
    pub matrix_event_id: Option<String>,
    pub code: ValidationCode,
    pub message: String,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogSellerRecord {
    pub seller_id: String,
    pub issuer_instance: String,
    pub status: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogProductRecord {
    pub product_id: String,
    pub seller_id: String,
    pub revision: i64,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogOfferProjectionRecord {
    pub offer_id: String,
    pub product_id: String,
    pub seller_id: String,
    pub revision: i64,
    pub price: Value,
    pub inventory_kind: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogTombstoneRecord {
    pub object_id: String,
    pub object_type: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderProjectionRecord {
    pub order_id: String,
    pub room_id: String,
    pub customer_id: String,
    pub seller_id: String,
    pub offer_id: String,
    pub status: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderEventRecord {
    pub order_id: String,
    pub marketplace_event_id: String,
    pub event_type: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentProjectionRecord {
    pub payment_id: String,
    pub order_id: String,
    pub status: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntitlementProjectionRecord {
    pub entitlement_id: String,
    pub order_id: String,
    pub status: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisputeProjectionRecord {
    pub dispute_id: String,
    pub order_id: String,
    pub status: String,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArbitrationRulingProjectionRecord {
    pub ruling_id: String,
    pub dispute_id: String,
    pub status: String,
    pub body: Value,
}

#[async_trait]
pub trait EventStore: Clone + Send + Sync + 'static {
    async fn record_appservice_transaction(
        &self,
        transaction: AppServiceTransactionRecord,
    ) -> Result<(), ValidationError>;

    async fn record_raw_event(&self, event: RawMatrixEventRecord) -> Result<(), ValidationError>;

    async fn raw_event(
        &self,
        event_id: &str,
    ) -> Result<Option<RawMatrixEventRecord>, ValidationError>;

    async fn record_marketplace_event(
        &self,
        event: MarketplaceEventRecord,
    ) -> Result<(), ValidationError>;

    async fn marketplace_events_by_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<MarketplaceEventRecord>, ValidationError>;

    async fn record_projection_error(
        &self,
        error: ProjectionErrorRecord,
    ) -> Result<(), ValidationError>;

    async fn projection_errors(&self) -> Result<Vec<ProjectionErrorRecord>, ValidationError>;

    async fn upsert_catalog_seller(
        &self,
        seller_id: &str,
        issuer_instance: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn catalog_sellers(&self) -> Result<Vec<CatalogSellerRecord>, ValidationError>;

    async fn upsert_catalog_product(
        &self,
        product_id: &str,
        seller_id: &str,
        revision: i64,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn catalog_products(&self) -> Result<Vec<CatalogProductRecord>, ValidationError>;

    async fn upsert_catalog_offer(
        &self,
        offer: CatalogOfferProjectionRecord,
    ) -> Result<(), ValidationError>;

    async fn catalog_offers(&self) -> Result<Vec<CatalogOfferProjectionRecord>, ValidationError>;

    async fn tombstone_catalog_object(
        &self,
        object_id: &str,
        object_type: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn catalog_tombstones(&self) -> Result<Vec<CatalogTombstoneRecord>, ValidationError>;

    async fn upsert_order(&self, order: OrderProjectionRecord) -> Result<(), ValidationError>;

    async fn order(&self, order_id: &str)
    -> Result<Option<OrderProjectionRecord>, ValidationError>;

    async fn orders(&self) -> Result<Vec<OrderProjectionRecord>, ValidationError>;

    async fn record_order_event(
        &self,
        order_id: &str,
        marketplace_event_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn order_events(&self, order_id: &str) -> Result<Vec<OrderEventRecord>, ValidationError>;

    async fn upsert_payment(
        &self,
        payment_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn payments(&self) -> Result<Vec<PaymentProjectionRecord>, ValidationError>;

    async fn upsert_entitlement(
        &self,
        entitlement_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn entitlements(&self) -> Result<Vec<EntitlementProjectionRecord>, ValidationError>;

    async fn upsert_dispute(
        &self,
        dispute_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn disputes(&self) -> Result<Vec<DisputeProjectionRecord>, ValidationError>;

    async fn upsert_arbitration_ruling(
        &self,
        ruling_id: &str,
        dispute_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError>;

    async fn arbitration_rulings(
        &self,
    ) -> Result<Vec<ArbitrationRulingProjectionRecord>, ValidationError>;
}

#[derive(Debug, Clone)]
pub struct SqliteEventStore {
    pool: SqlitePool,
}

impl SqliteEventStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn store_error(error: impl std::fmt::Display) -> ValidationError {
    ValidationError::new(
        ValidationCode::PolicyViolation,
        format!("event store error: {error}"),
    )
}

fn json_text(value: &Value) -> Result<String, ValidationError> {
    serde_json::to_string(value).map_err(store_error)
}

fn event_ids_text(event_ids: &[String]) -> Result<String, ValidationError> {
    serde_json::to_string(event_ids).map_err(store_error)
}

fn validation_code_text(code: ValidationCode) -> Result<String, ValidationError> {
    serde_json::from_value::<String>(serde_json::to_value(code).map_err(store_error)?)
        .map_err(store_error)
}

fn parse_json(text: String) -> Result<Value, ValidationError> {
    serde_json::from_str(&text).map_err(store_error)
}

fn parse_validation_code(text: String) -> Result<ValidationCode, ValidationError> {
    serde_json::from_value(Value::String(text)).map_err(store_error)
}

fn pg_json(value: Value) -> Json<Value> {
    Json(value)
}

fn pg_json_ref(value: &Value) -> Json<Value> {
    Json(value.clone())
}

#[async_trait]
impl EventStore for SqliteEventStore {
    async fn record_appservice_transaction(
        &self,
        transaction: AppServiceTransactionRecord,
    ) -> Result<(), ValidationError> {
        let event_ids = event_ids_text(&transaction.event_ids)?;
        if let Some(row) =
            sqlx::query("SELECT event_ids FROM appservice_transactions WHERE txn_id = ?")
                .bind(&transaction.txn_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(store_error)?
        {
            let previous: String = row.try_get("event_ids").map_err(store_error)?;
            if previous == event_ids {
                return Ok(());
            }
            return Err(ValidationError::new(
                ValidationCode::DuplicateEvent,
                "AppService transactions must be idempotent",
            ));
        }

        sqlx::query("INSERT INTO appservice_transactions (txn_id, event_ids) VALUES (?, ?)")
            .bind(transaction.txn_id)
            .bind(event_ids)
            .execute(&self.pool)
            .await
            .map_err(store_error)?;
        Ok(())
    }

    async fn record_raw_event(&self, event: RawMatrixEventRecord) -> Result<(), ValidationError> {
        let raw_json = json_text(&event.raw_json)?;
        let validation_code = event
            .validation_code
            .map(validation_code_text)
            .transpose()?;
        sqlx::query(
            "INSERT INTO raw_matrix_events
             (event_id, room_id, sender, event_type, origin_server_ts, raw_json, validation_status, validation_code)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(event_id) DO UPDATE SET
               room_id = excluded.room_id,
               sender = excluded.sender,
               event_type = excluded.event_type,
               origin_server_ts = excluded.origin_server_ts,
               raw_json = excluded.raw_json,
               validation_status = excluded.validation_status,
               validation_code = excluded.validation_code",
        )
        .bind(event.event_id)
        .bind(event.room_id)
        .bind(event.sender)
        .bind(event.event_type)
        .bind(event.origin_server_ts)
        .bind(raw_json)
        .bind(event.validation_status)
        .bind(validation_code)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn raw_event(
        &self,
        event_id: &str,
    ) -> Result<Option<RawMatrixEventRecord>, ValidationError> {
        sqlx::query(
            "SELECT event_id, room_id, sender, event_type, origin_server_ts, raw_json,
                    validation_status, validation_code
             FROM raw_matrix_events
             WHERE event_id = ?",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .map(|row| {
            let validation_code: Option<String> =
                row.try_get("validation_code").map_err(store_error)?;
            Ok(RawMatrixEventRecord {
                event_id: row.try_get("event_id").map_err(store_error)?,
                room_id: row.try_get("room_id").map_err(store_error)?,
                sender: row.try_get("sender").map_err(store_error)?,
                event_type: row.try_get("event_type").map_err(store_error)?,
                origin_server_ts: row.try_get("origin_server_ts").map_err(store_error)?,
                raw_json: parse_json(row.try_get("raw_json").map_err(store_error)?)?,
                validation_status: row.try_get("validation_status").map_err(store_error)?,
                validation_code: validation_code.map(parse_validation_code).transpose()?,
            })
        })
        .transpose()
    }

    async fn record_marketplace_event(
        &self,
        event: MarketplaceEventRecord,
    ) -> Result<(), ValidationError> {
        let body = json_text(&event.body)?;
        sqlx::query(
            "INSERT INTO marketplace_events
             (marketplace_event_id, matrix_event_id, protocol_version, issuer_instance, actor_id, event_type, body, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(marketplace_event_id) DO UPDATE SET
               matrix_event_id = excluded.matrix_event_id,
               protocol_version = excluded.protocol_version,
               issuer_instance = excluded.issuer_instance,
               actor_id = excluded.actor_id,
               event_type = excluded.event_type,
               body = excluded.body,
               created_at = excluded.created_at",
        )
        .bind(event.marketplace_event_id)
        .bind(event.matrix_event_id)
        .bind(event.protocol_version)
        .bind(event.issuer_instance)
        .bind(event.actor_id)
        .bind(event.event_type)
        .bind(body)
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn marketplace_events_by_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<MarketplaceEventRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT me.marketplace_event_id, me.matrix_event_id, me.protocol_version,
                    me.issuer_instance, me.actor_id, me.event_type, me.body, me.created_at
             FROM marketplace_events me
             INNER JOIN raw_matrix_events rme ON rme.event_id = me.matrix_event_id
             WHERE rme.room_id = ?
             ORDER BY me.rowid",
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(MarketplaceEventRecord {
                    marketplace_event_id: row
                        .try_get("marketplace_event_id")
                        .map_err(store_error)?,
                    matrix_event_id: row.try_get("matrix_event_id").map_err(store_error)?,
                    protocol_version: row.try_get("protocol_version").map_err(store_error)?,
                    issuer_instance: row.try_get("issuer_instance").map_err(store_error)?,
                    actor_id: row.try_get("actor_id").map_err(store_error)?,
                    event_type: row.try_get("event_type").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                    created_at: row.try_get("created_at").map_err(store_error)?,
                })
            })
            .collect()
    }

    async fn record_projection_error(
        &self,
        error: ProjectionErrorRecord,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO projection_errors (matrix_event_id, code, message, details) VALUES (?, ?, ?, ?)",
        )
        .bind(error.matrix_event_id)
        .bind(validation_code_text(error.code)?)
        .bind(error.message)
        .bind(json_text(&error.details)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn projection_errors(&self) -> Result<Vec<ProjectionErrorRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT matrix_event_id, code, message, details FROM projection_errors ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(ProjectionErrorRecord {
                    matrix_event_id: row.try_get("matrix_event_id").map_err(store_error)?,
                    code: parse_validation_code(row.try_get("code").map_err(store_error)?)?,
                    message: row.try_get("message").map_err(store_error)?,
                    details: parse_json(row.try_get("details").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_catalog_seller(
        &self,
        seller_id: &str,
        issuer_instance: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_sellers (seller_id, issuer_instance, status, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(seller_id) DO UPDATE SET
               issuer_instance = excluded.issuer_instance,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(seller_id)
        .bind(issuer_instance)
        .bind(status)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_sellers(&self) -> Result<Vec<CatalogSellerRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT seller_id, issuer_instance, status, body FROM catalog_sellers ORDER BY seller_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(CatalogSellerRecord {
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    issuer_instance: row.try_get("issuer_instance").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_catalog_product(
        &self,
        product_id: &str,
        seller_id: &str,
        revision: i64,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_products (product_id, seller_id, revision, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(product_id) DO UPDATE SET
               seller_id = excluded.seller_id,
               revision = excluded.revision,
               body = excluded.body",
        )
        .bind(product_id)
        .bind(seller_id)
        .bind(revision)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_products(&self) -> Result<Vec<CatalogProductRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT product_id, seller_id, revision, body FROM catalog_products ORDER BY product_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(CatalogProductRecord {
                    product_id: row.try_get("product_id").map_err(store_error)?,
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    revision: row.try_get("revision").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_catalog_offer(
        &self,
        offer: CatalogOfferProjectionRecord,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_offers
             (offer_id, product_id, seller_id, revision, price, inventory_kind, body)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(offer_id) DO UPDATE SET
               product_id = excluded.product_id,
               seller_id = excluded.seller_id,
               revision = excluded.revision,
               price = excluded.price,
               inventory_kind = excluded.inventory_kind,
               body = excluded.body",
        )
        .bind(offer.offer_id)
        .bind(offer.product_id)
        .bind(offer.seller_id)
        .bind(offer.revision)
        .bind(json_text(&offer.price)?)
        .bind(offer.inventory_kind)
        .bind(json_text(&offer.body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_offers(&self) -> Result<Vec<CatalogOfferProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT offer_id, product_id, seller_id, revision, price, inventory_kind, body
             FROM catalog_offers
             ORDER BY offer_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(CatalogOfferProjectionRecord {
                    offer_id: row.try_get("offer_id").map_err(store_error)?,
                    product_id: row.try_get("product_id").map_err(store_error)?,
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    revision: row.try_get("revision").map_err(store_error)?,
                    price: parse_json(row.try_get("price").map_err(store_error)?)?,
                    inventory_kind: row.try_get("inventory_kind").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn tombstone_catalog_object(
        &self,
        object_id: &str,
        object_type: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_tombstones (object_id, object_type, body)
             VALUES (?, ?, ?)
             ON CONFLICT(object_id) DO UPDATE SET
               object_type = excluded.object_type,
               body = excluded.body",
        )
        .bind(object_id)
        .bind(object_type)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_tombstones(&self) -> Result<Vec<CatalogTombstoneRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT object_id, object_type, body FROM catalog_tombstones ORDER BY object_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(CatalogTombstoneRecord {
                    object_id: row.try_get("object_id").map_err(store_error)?,
                    object_type: row.try_get("object_type").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_order(&self, order: OrderProjectionRecord) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO orders (order_id, room_id, customer_id, seller_id, offer_id, status, body)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(order_id) DO UPDATE SET
               room_id = excluded.room_id,
               customer_id = excluded.customer_id,
               seller_id = excluded.seller_id,
               offer_id = excluded.offer_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(order.order_id)
        .bind(order.room_id)
        .bind(order.customer_id)
        .bind(order.seller_id)
        .bind(order.offer_id)
        .bind(order.status)
        .bind(json_text(&order.body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn order(
        &self,
        order_id: &str,
    ) -> Result<Option<OrderProjectionRecord>, ValidationError> {
        sqlx::query(
            "SELECT order_id, room_id, customer_id, seller_id, offer_id, status, body
             FROM orders
             WHERE order_id = ?",
        )
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .map(|row| {
            Ok(OrderProjectionRecord {
                order_id: row.try_get("order_id").map_err(store_error)?,
                room_id: row.try_get("room_id").map_err(store_error)?,
                customer_id: row.try_get("customer_id").map_err(store_error)?,
                seller_id: row.try_get("seller_id").map_err(store_error)?,
                offer_id: row.try_get("offer_id").map_err(store_error)?,
                status: row.try_get("status").map_err(store_error)?,
                body: parse_json(row.try_get("body").map_err(store_error)?)?,
            })
        })
        .transpose()
    }

    async fn orders(&self) -> Result<Vec<OrderProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT order_id, room_id, customer_id, seller_id, offer_id, status, body
             FROM orders
             ORDER BY order_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(OrderProjectionRecord {
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    room_id: row.try_get("room_id").map_err(store_error)?,
                    customer_id: row.try_get("customer_id").map_err(store_error)?,
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    offer_id: row.try_get("offer_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn record_order_event(
        &self,
        order_id: &str,
        marketplace_event_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO order_events (matrix_event_id, order_id, event_type, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(matrix_event_id) DO UPDATE SET
               order_id = excluded.order_id,
               event_type = excluded.event_type,
               body = excluded.body",
        )
        .bind(marketplace_event_id)
        .bind(order_id)
        .bind(event_type)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn order_events(&self, order_id: &str) -> Result<Vec<OrderEventRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT order_id, matrix_event_id, event_type, body
             FROM order_events
             WHERE order_id = ?
             ORDER BY matrix_event_id",
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(OrderEventRecord {
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    marketplace_event_id: row.try_get("matrix_event_id").map_err(store_error)?,
                    event_type: row.try_get("event_type").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_payment(
        &self,
        payment_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO payments (payment_id, order_id, status, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(payment_id) DO UPDATE SET
               order_id = excluded.order_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(payment_id)
        .bind(order_id)
        .bind(status)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn payments(&self) -> Result<Vec<PaymentProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT payment_id, order_id, status, body FROM payments ORDER BY payment_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(PaymentProjectionRecord {
                    payment_id: row.try_get("payment_id").map_err(store_error)?,
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_entitlement(
        &self,
        entitlement_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO entitlements (entitlement_id, order_id, status, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(entitlement_id) DO UPDATE SET
               order_id = excluded.order_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(entitlement_id)
        .bind(order_id)
        .bind(status)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn entitlements(&self) -> Result<Vec<EntitlementProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT entitlement_id, order_id, status, body
             FROM entitlements
             ORDER BY entitlement_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(EntitlementProjectionRecord {
                    entitlement_id: row.try_get("entitlement_id").map_err(store_error)?,
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_dispute(
        &self,
        dispute_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO disputes (dispute_id, order_id, status, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(dispute_id) DO UPDATE SET
               order_id = excluded.order_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(dispute_id)
        .bind(order_id)
        .bind(status)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn disputes(&self) -> Result<Vec<DisputeProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT dispute_id, order_id, status, body FROM disputes ORDER BY dispute_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(DisputeProjectionRecord {
                    dispute_id: row.try_get("dispute_id").map_err(store_error)?,
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }

    async fn upsert_arbitration_ruling(
        &self,
        ruling_id: &str,
        dispute_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO arbitration_rulings (ruling_id, dispute_id, status, body)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(ruling_id) DO UPDATE SET
               dispute_id = excluded.dispute_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(ruling_id)
        .bind(dispute_id)
        .bind(status)
        .bind(json_text(&body)?)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn arbitration_rulings(
        &self,
    ) -> Result<Vec<ArbitrationRulingProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT ruling_id, dispute_id, status, body
             FROM arbitration_rulings
             ORDER BY ruling_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(ArbitrationRulingProjectionRecord {
                    ruling_id: row.try_get("ruling_id").map_err(store_error)?,
                    dispute_id: row.try_get("dispute_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: parse_json(row.try_get("body").map_err(store_error)?)?,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PostgresEventStore {
    pool: PgPool,
}

impl PostgresEventStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl EventStore for PostgresEventStore {
    async fn record_appservice_transaction(
        &self,
        transaction: AppServiceTransactionRecord,
    ) -> Result<(), ValidationError> {
        if let Some(row) =
            sqlx::query("SELECT event_ids FROM appservice_transactions WHERE txn_id = $1")
                .bind(&transaction.txn_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(store_error)?
        {
            let previous: Json<Vec<String>> = row.try_get("event_ids").map_err(store_error)?;
            if previous.0 == transaction.event_ids {
                return Ok(());
            }
            return Err(ValidationError::new(
                ValidationCode::DuplicateEvent,
                "AppService transactions must be idempotent",
            ));
        }

        sqlx::query("INSERT INTO appservice_transactions (txn_id, event_ids) VALUES ($1, $2)")
            .bind(transaction.txn_id)
            .bind(Json(transaction.event_ids))
            .execute(&self.pool)
            .await
            .map_err(store_error)?;
        Ok(())
    }

    async fn record_raw_event(&self, event: RawMatrixEventRecord) -> Result<(), ValidationError> {
        let validation_code = event
            .validation_code
            .map(validation_code_text)
            .transpose()?;
        sqlx::query(
            "INSERT INTO raw_matrix_events
             (event_id, room_id, sender, event_type, origin_server_ts, raw_json, validation_status, validation_code)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT(event_id) DO UPDATE SET
               room_id = excluded.room_id,
               sender = excluded.sender,
               event_type = excluded.event_type,
               origin_server_ts = excluded.origin_server_ts,
               raw_json = excluded.raw_json,
               validation_status = excluded.validation_status,
               validation_code = excluded.validation_code",
        )
        .bind(event.event_id)
        .bind(event.room_id)
        .bind(event.sender)
        .bind(event.event_type)
        .bind(event.origin_server_ts)
        .bind(pg_json(event.raw_json))
        .bind(event.validation_status)
        .bind(validation_code)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn raw_event(
        &self,
        event_id: &str,
    ) -> Result<Option<RawMatrixEventRecord>, ValidationError> {
        sqlx::query(
            "SELECT event_id, room_id, sender, event_type, origin_server_ts, raw_json,
                    validation_status, validation_code
             FROM raw_matrix_events
             WHERE event_id = $1",
        )
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .map(|row| {
            let validation_code: Option<String> =
                row.try_get("validation_code").map_err(store_error)?;
            let raw_json: Json<Value> = row.try_get("raw_json").map_err(store_error)?;
            Ok(RawMatrixEventRecord {
                event_id: row.try_get("event_id").map_err(store_error)?,
                room_id: row.try_get("room_id").map_err(store_error)?,
                sender: row.try_get("sender").map_err(store_error)?,
                event_type: row.try_get("event_type").map_err(store_error)?,
                origin_server_ts: row.try_get("origin_server_ts").map_err(store_error)?,
                raw_json: raw_json.0,
                validation_status: row.try_get("validation_status").map_err(store_error)?,
                validation_code: validation_code.map(parse_validation_code).transpose()?,
            })
        })
        .transpose()
    }

    async fn record_marketplace_event(
        &self,
        event: MarketplaceEventRecord,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO marketplace_events
             (marketplace_event_id, matrix_event_id, protocol_version, issuer_instance, actor_id, event_type, body, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT(marketplace_event_id) DO UPDATE SET
               matrix_event_id = excluded.matrix_event_id,
               protocol_version = excluded.protocol_version,
               issuer_instance = excluded.issuer_instance,
               actor_id = excluded.actor_id,
               event_type = excluded.event_type,
               body = excluded.body,
               created_at = excluded.created_at",
        )
        .bind(event.marketplace_event_id)
        .bind(event.matrix_event_id)
        .bind(event.protocol_version)
        .bind(event.issuer_instance)
        .bind(event.actor_id)
        .bind(event.event_type)
        .bind(pg_json(event.body))
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn marketplace_events_by_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<MarketplaceEventRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT me.marketplace_event_id, me.matrix_event_id, me.protocol_version,
                    me.issuer_instance, me.actor_id, me.event_type, me.body, me.created_at
             FROM marketplace_events me
             INNER JOIN raw_matrix_events rme ON rme.event_id = me.matrix_event_id
             WHERE rme.room_id = $1
             ORDER BY me.sequence_id",
        )
        .bind(room_id)
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(MarketplaceEventRecord {
                    marketplace_event_id: row
                        .try_get("marketplace_event_id")
                        .map_err(store_error)?,
                    matrix_event_id: row.try_get("matrix_event_id").map_err(store_error)?,
                    protocol_version: row.try_get("protocol_version").map_err(store_error)?,
                    issuer_instance: row.try_get("issuer_instance").map_err(store_error)?,
                    actor_id: row.try_get("actor_id").map_err(store_error)?,
                    event_type: row.try_get("event_type").map_err(store_error)?,
                    body: body.0,
                    created_at: row.try_get("created_at").map_err(store_error)?,
                })
            })
            .collect()
    }

    async fn record_projection_error(
        &self,
        error: ProjectionErrorRecord,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO projection_errors (matrix_event_id, code, message, details) VALUES ($1, $2, $3, $4)",
        )
        .bind(error.matrix_event_id)
        .bind(validation_code_text(error.code)?)
        .bind(error.message)
        .bind(pg_json(error.details))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn projection_errors(&self) -> Result<Vec<ProjectionErrorRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT matrix_event_id, code, message, details FROM projection_errors ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let details: Json<Value> = row.try_get("details").map_err(store_error)?;
                Ok(ProjectionErrorRecord {
                    matrix_event_id: row.try_get("matrix_event_id").map_err(store_error)?,
                    code: parse_validation_code(row.try_get("code").map_err(store_error)?)?,
                    message: row.try_get("message").map_err(store_error)?,
                    details: details.0,
                })
            })
            .collect()
    }

    async fn upsert_catalog_seller(
        &self,
        seller_id: &str,
        issuer_instance: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_sellers (seller_id, issuer_instance, status, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(seller_id) DO UPDATE SET
               issuer_instance = excluded.issuer_instance,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(seller_id)
        .bind(issuer_instance)
        .bind(status)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_sellers(&self) -> Result<Vec<CatalogSellerRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT seller_id, issuer_instance, status, body FROM catalog_sellers ORDER BY seller_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(CatalogSellerRecord {
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    issuer_instance: row.try_get("issuer_instance").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_catalog_product(
        &self,
        product_id: &str,
        seller_id: &str,
        revision: i64,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_products (product_id, seller_id, revision, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(product_id) DO UPDATE SET
               seller_id = excluded.seller_id,
               revision = excluded.revision,
               body = excluded.body",
        )
        .bind(product_id)
        .bind(seller_id)
        .bind(revision)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_products(&self) -> Result<Vec<CatalogProductRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT product_id, seller_id, revision, body FROM catalog_products ORDER BY product_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(CatalogProductRecord {
                    product_id: row.try_get("product_id").map_err(store_error)?,
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    revision: row.try_get("revision").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_catalog_offer(
        &self,
        offer: CatalogOfferProjectionRecord,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_offers
             (offer_id, product_id, seller_id, revision, price, inventory_kind, body)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT(offer_id) DO UPDATE SET
               product_id = excluded.product_id,
               seller_id = excluded.seller_id,
               revision = excluded.revision,
               price = excluded.price,
               inventory_kind = excluded.inventory_kind,
               body = excluded.body",
        )
        .bind(offer.offer_id)
        .bind(offer.product_id)
        .bind(offer.seller_id)
        .bind(offer.revision)
        .bind(pg_json_ref(&offer.price))
        .bind(offer.inventory_kind)
        .bind(pg_json(offer.body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_offers(&self) -> Result<Vec<CatalogOfferProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT offer_id, product_id, seller_id, revision, price, inventory_kind, body
             FROM catalog_offers
             ORDER BY offer_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let price: Json<Value> = row.try_get("price").map_err(store_error)?;
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(CatalogOfferProjectionRecord {
                    offer_id: row.try_get("offer_id").map_err(store_error)?,
                    product_id: row.try_get("product_id").map_err(store_error)?,
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    revision: row.try_get("revision").map_err(store_error)?,
                    price: price.0,
                    inventory_kind: row.try_get("inventory_kind").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn tombstone_catalog_object(
        &self,
        object_id: &str,
        object_type: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO catalog_tombstones (object_id, object_type, body)
             VALUES ($1, $2, $3)
             ON CONFLICT(object_id) DO UPDATE SET
               object_type = excluded.object_type,
               body = excluded.body",
        )
        .bind(object_id)
        .bind(object_type)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn catalog_tombstones(&self) -> Result<Vec<CatalogTombstoneRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT object_id, object_type, body FROM catalog_tombstones ORDER BY object_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(CatalogTombstoneRecord {
                    object_id: row.try_get("object_id").map_err(store_error)?,
                    object_type: row.try_get("object_type").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_order(&self, order: OrderProjectionRecord) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO orders (order_id, room_id, customer_id, seller_id, offer_id, status, body)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT(order_id) DO UPDATE SET
               room_id = excluded.room_id,
               customer_id = excluded.customer_id,
               seller_id = excluded.seller_id,
               offer_id = excluded.offer_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(order.order_id)
        .bind(order.room_id)
        .bind(order.customer_id)
        .bind(order.seller_id)
        .bind(order.offer_id)
        .bind(order.status)
        .bind(pg_json(order.body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn order(
        &self,
        order_id: &str,
    ) -> Result<Option<OrderProjectionRecord>, ValidationError> {
        sqlx::query(
            "SELECT order_id, room_id, customer_id, seller_id, offer_id, status, body
             FROM orders
             WHERE order_id = $1",
        )
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(store_error)?
        .map(|row| {
            let body: Json<Value> = row.try_get("body").map_err(store_error)?;
            Ok(OrderProjectionRecord {
                order_id: row.try_get("order_id").map_err(store_error)?,
                room_id: row.try_get("room_id").map_err(store_error)?,
                customer_id: row.try_get("customer_id").map_err(store_error)?,
                seller_id: row.try_get("seller_id").map_err(store_error)?,
                offer_id: row.try_get("offer_id").map_err(store_error)?,
                status: row.try_get("status").map_err(store_error)?,
                body: body.0,
            })
        })
        .transpose()
    }

    async fn orders(&self) -> Result<Vec<OrderProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT order_id, room_id, customer_id, seller_id, offer_id, status, body
             FROM orders
             ORDER BY order_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(OrderProjectionRecord {
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    room_id: row.try_get("room_id").map_err(store_error)?,
                    customer_id: row.try_get("customer_id").map_err(store_error)?,
                    seller_id: row.try_get("seller_id").map_err(store_error)?,
                    offer_id: row.try_get("offer_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn record_order_event(
        &self,
        order_id: &str,
        marketplace_event_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO order_events (matrix_event_id, order_id, event_type, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(matrix_event_id) DO UPDATE SET
               order_id = excluded.order_id,
               event_type = excluded.event_type,
               body = excluded.body",
        )
        .bind(marketplace_event_id)
        .bind(order_id)
        .bind(event_type)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn order_events(&self, order_id: &str) -> Result<Vec<OrderEventRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT order_id, matrix_event_id, event_type, body
             FROM order_events
             WHERE order_id = $1
             ORDER BY matrix_event_id",
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(OrderEventRecord {
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    marketplace_event_id: row.try_get("matrix_event_id").map_err(store_error)?,
                    event_type: row.try_get("event_type").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_payment(
        &self,
        payment_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO payments (payment_id, order_id, status, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(payment_id) DO UPDATE SET
               order_id = excluded.order_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(payment_id)
        .bind(order_id)
        .bind(status)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn payments(&self) -> Result<Vec<PaymentProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT payment_id, order_id, status, body FROM payments ORDER BY payment_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(PaymentProjectionRecord {
                    payment_id: row.try_get("payment_id").map_err(store_error)?,
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_entitlement(
        &self,
        entitlement_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO entitlements (entitlement_id, order_id, status, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(entitlement_id) DO UPDATE SET
               order_id = excluded.order_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(entitlement_id)
        .bind(order_id)
        .bind(status)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn entitlements(&self) -> Result<Vec<EntitlementProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT entitlement_id, order_id, status, body
             FROM entitlements
             ORDER BY entitlement_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(EntitlementProjectionRecord {
                    entitlement_id: row.try_get("entitlement_id").map_err(store_error)?,
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_dispute(
        &self,
        dispute_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO disputes (dispute_id, order_id, status, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(dispute_id) DO UPDATE SET
               order_id = excluded.order_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(dispute_id)
        .bind(order_id)
        .bind(status)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn disputes(&self) -> Result<Vec<DisputeProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT dispute_id, order_id, status, body FROM disputes ORDER BY dispute_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(DisputeProjectionRecord {
                    dispute_id: row.try_get("dispute_id").map_err(store_error)?,
                    order_id: row.try_get("order_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }

    async fn upsert_arbitration_ruling(
        &self,
        ruling_id: &str,
        dispute_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        sqlx::query(
            "INSERT INTO arbitration_rulings (ruling_id, dispute_id, status, body)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT(ruling_id) DO UPDATE SET
               dispute_id = excluded.dispute_id,
               status = excluded.status,
               body = excluded.body",
        )
        .bind(ruling_id)
        .bind(dispute_id)
        .bind(status)
        .bind(pg_json(body))
        .execute(&self.pool)
        .await
        .map_err(store_error)?;
        Ok(())
    }

    async fn arbitration_rulings(
        &self,
    ) -> Result<Vec<ArbitrationRulingProjectionRecord>, ValidationError> {
        let rows = sqlx::query(
            "SELECT ruling_id, dispute_id, status, body
             FROM arbitration_rulings
             ORDER BY ruling_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(store_error)?;

        rows.into_iter()
            .map(|row| {
                let body: Json<Value> = row.try_get("body").map_err(store_error)?;
                Ok(ArbitrationRulingProjectionRecord {
                    ruling_id: row.try_get("ruling_id").map_err(store_error)?,
                    dispute_id: row.try_get("dispute_id").map_err(store_error)?,
                    status: row.try_get("status").map_err(store_error)?,
                    body: body.0,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryEventStore {
    inner: Arc<Mutex<InMemoryState>>,
}

#[derive(Debug, Default)]
struct InMemoryState {
    transactions: HashMap<String, Vec<String>>,
    raw_events: HashMap<String, RawMatrixEventRecord>,
    marketplace_events: HashMap<String, MarketplaceEventRecord>,
    marketplace_event_order: Vec<String>,
    projection_errors: Vec<ProjectionErrorRecord>,
    raw_event_rooms: HashMap<String, String>,
    catalog_sellers: HashMap<String, CatalogSellerRecord>,
    catalog_products: HashMap<String, CatalogProductRecord>,
    catalog_offers: HashMap<String, CatalogOfferProjectionRecord>,
    catalog_tombstones: HashMap<String, CatalogTombstoneRecord>,
    orders: HashMap<String, OrderProjectionRecord>,
    order_events: HashMap<String, Vec<OrderEventRecord>>,
    payments: HashMap<String, PaymentProjectionRecord>,
    entitlements: HashMap<String, EntitlementProjectionRecord>,
    disputes: HashMap<String, DisputeProjectionRecord>,
    arbitration_rulings: HashMap<String, ArbitrationRulingProjectionRecord>,
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn record_appservice_transaction(
        &self,
        transaction: AppServiceTransactionRecord,
    ) -> Result<(), ValidationError> {
        let mut inner = self.inner.lock().await;
        if let Some(previous) = inner.transactions.get(&transaction.txn_id) {
            if previous == &transaction.event_ids {
                return Ok(());
            }
            return Err(ValidationError::new(
                ValidationCode::DuplicateEvent,
                "AppService transactions must be idempotent",
            ));
        }
        inner
            .transactions
            .insert(transaction.txn_id, transaction.event_ids);
        Ok(())
    }

    async fn record_raw_event(&self, event: RawMatrixEventRecord) -> Result<(), ValidationError> {
        let mut inner = self.inner.lock().await;
        inner
            .raw_event_rooms
            .insert(event.event_id.clone(), event.room_id.clone());
        inner.raw_events.insert(event.event_id.clone(), event);
        Ok(())
    }

    async fn raw_event(
        &self,
        event_id: &str,
    ) -> Result<Option<RawMatrixEventRecord>, ValidationError> {
        Ok(self.inner.lock().await.raw_events.get(event_id).cloned())
    }

    async fn record_marketplace_event(
        &self,
        event: MarketplaceEventRecord,
    ) -> Result<(), ValidationError> {
        let mut inner = self.inner.lock().await;
        if !inner
            .marketplace_events
            .contains_key(&event.marketplace_event_id)
        {
            inner
                .marketplace_event_order
                .push(event.marketplace_event_id.clone());
        }
        inner
            .marketplace_events
            .insert(event.marketplace_event_id.clone(), event);
        Ok(())
    }

    async fn marketplace_events_by_room(
        &self,
        room_id: &str,
    ) -> Result<Vec<MarketplaceEventRecord>, ValidationError> {
        let inner = self.inner.lock().await;
        let events = inner
            .marketplace_event_order
            .iter()
            .filter_map(|event_id| inner.marketplace_events.get(event_id))
            .filter(|event| {
                inner
                    .raw_event_rooms
                    .get(&event.matrix_event_id)
                    .is_some_and(|event_room_id| event_room_id == room_id)
            })
            .cloned()
            .collect();
        Ok(events)
    }

    async fn record_projection_error(
        &self,
        error: ProjectionErrorRecord,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.projection_errors.push(error);
        Ok(())
    }

    async fn projection_errors(&self) -> Result<Vec<ProjectionErrorRecord>, ValidationError> {
        Ok(self.inner.lock().await.projection_errors.clone())
    }

    async fn upsert_catalog_seller(
        &self,
        seller_id: &str,
        issuer_instance: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.catalog_sellers.insert(
            seller_id.into(),
            CatalogSellerRecord {
                seller_id: seller_id.into(),
                issuer_instance: issuer_instance.into(),
                status: status.into(),
                body,
            },
        );
        Ok(())
    }

    async fn catalog_sellers(&self) -> Result<Vec<CatalogSellerRecord>, ValidationError> {
        let mut sellers: Vec<_> = self
            .inner
            .lock()
            .await
            .catalog_sellers
            .values()
            .cloned()
            .collect();
        sellers.sort_by(|left, right| left.seller_id.cmp(&right.seller_id));
        Ok(sellers)
    }

    async fn upsert_catalog_product(
        &self,
        product_id: &str,
        seller_id: &str,
        revision: i64,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.catalog_products.insert(
            product_id.into(),
            CatalogProductRecord {
                product_id: product_id.into(),
                seller_id: seller_id.into(),
                revision,
                body,
            },
        );
        Ok(())
    }

    async fn catalog_products(&self) -> Result<Vec<CatalogProductRecord>, ValidationError> {
        let mut products: Vec<_> = self
            .inner
            .lock()
            .await
            .catalog_products
            .values()
            .cloned()
            .collect();
        products.sort_by(|left, right| left.product_id.cmp(&right.product_id));
        Ok(products)
    }

    async fn upsert_catalog_offer(
        &self,
        offer: CatalogOfferProjectionRecord,
    ) -> Result<(), ValidationError> {
        self.inner
            .lock()
            .await
            .catalog_offers
            .insert(offer.offer_id.clone(), offer);
        Ok(())
    }

    async fn catalog_offers(&self) -> Result<Vec<CatalogOfferProjectionRecord>, ValidationError> {
        let mut offers: Vec<_> = self
            .inner
            .lock()
            .await
            .catalog_offers
            .values()
            .cloned()
            .collect();
        offers.sort_by(|left, right| left.offer_id.cmp(&right.offer_id));
        Ok(offers)
    }

    async fn tombstone_catalog_object(
        &self,
        object_id: &str,
        object_type: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.catalog_tombstones.insert(
            object_id.into(),
            CatalogTombstoneRecord {
                object_id: object_id.into(),
                object_type: object_type.into(),
                body,
            },
        );
        Ok(())
    }

    async fn catalog_tombstones(&self) -> Result<Vec<CatalogTombstoneRecord>, ValidationError> {
        let mut tombstones: Vec<_> = self
            .inner
            .lock()
            .await
            .catalog_tombstones
            .values()
            .cloned()
            .collect();
        tombstones.sort_by(|left, right| left.object_id.cmp(&right.object_id));
        Ok(tombstones)
    }

    async fn upsert_order(&self, order: OrderProjectionRecord) -> Result<(), ValidationError> {
        self.inner
            .lock()
            .await
            .orders
            .insert(order.order_id.clone(), order);
        Ok(())
    }

    async fn order(
        &self,
        order_id: &str,
    ) -> Result<Option<OrderProjectionRecord>, ValidationError> {
        Ok(self.inner.lock().await.orders.get(order_id).cloned())
    }

    async fn orders(&self) -> Result<Vec<OrderProjectionRecord>, ValidationError> {
        let mut orders: Vec<_> = self.inner.lock().await.orders.values().cloned().collect();
        orders.sort_by(|left, right| left.order_id.cmp(&right.order_id));
        Ok(orders)
    }

    async fn record_order_event(
        &self,
        order_id: &str,
        marketplace_event_id: &str,
        event_type: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        let mut inner = self.inner.lock().await;
        inner
            .order_events
            .entry(order_id.into())
            .or_default()
            .push(OrderEventRecord {
                order_id: order_id.into(),
                marketplace_event_id: marketplace_event_id.into(),
                event_type: event_type.into(),
                body,
            });
        Ok(())
    }

    async fn order_events(&self, order_id: &str) -> Result<Vec<OrderEventRecord>, ValidationError> {
        let mut events = self
            .inner
            .lock()
            .await
            .order_events
            .get(order_id)
            .cloned()
            .unwrap_or_default();
        events.sort_by(|left, right| left.marketplace_event_id.cmp(&right.marketplace_event_id));
        Ok(events)
    }

    async fn upsert_payment(
        &self,
        payment_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.payments.insert(
            payment_id.into(),
            PaymentProjectionRecord {
                payment_id: payment_id.into(),
                order_id: order_id.into(),
                status: status.into(),
                body,
            },
        );
        Ok(())
    }

    async fn payments(&self) -> Result<Vec<PaymentProjectionRecord>, ValidationError> {
        let mut payments: Vec<_> = self.inner.lock().await.payments.values().cloned().collect();
        payments.sort_by(|left, right| left.payment_id.cmp(&right.payment_id));
        Ok(payments)
    }

    async fn upsert_entitlement(
        &self,
        entitlement_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.entitlements.insert(
            entitlement_id.into(),
            EntitlementProjectionRecord {
                entitlement_id: entitlement_id.into(),
                order_id: order_id.into(),
                status: status.into(),
                body,
            },
        );
        Ok(())
    }

    async fn entitlements(&self) -> Result<Vec<EntitlementProjectionRecord>, ValidationError> {
        let mut entitlements: Vec<_> = self
            .inner
            .lock()
            .await
            .entitlements
            .values()
            .cloned()
            .collect();
        entitlements.sort_by(|left, right| left.entitlement_id.cmp(&right.entitlement_id));
        Ok(entitlements)
    }

    async fn upsert_dispute(
        &self,
        dispute_id: &str,
        order_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.disputes.insert(
            dispute_id.into(),
            DisputeProjectionRecord {
                dispute_id: dispute_id.into(),
                order_id: order_id.into(),
                status: status.into(),
                body,
            },
        );
        Ok(())
    }

    async fn disputes(&self) -> Result<Vec<DisputeProjectionRecord>, ValidationError> {
        let mut disputes: Vec<_> = self.inner.lock().await.disputes.values().cloned().collect();
        disputes.sort_by(|left, right| left.dispute_id.cmp(&right.dispute_id));
        Ok(disputes)
    }

    async fn upsert_arbitration_ruling(
        &self,
        ruling_id: &str,
        dispute_id: &str,
        status: &str,
        body: Value,
    ) -> Result<(), ValidationError> {
        self.inner.lock().await.arbitration_rulings.insert(
            ruling_id.into(),
            ArbitrationRulingProjectionRecord {
                ruling_id: ruling_id.into(),
                dispute_id: dispute_id.into(),
                status: status.into(),
                body,
            },
        );
        Ok(())
    }

    async fn arbitration_rulings(
        &self,
    ) -> Result<Vec<ArbitrationRulingProjectionRecord>, ValidationError> {
        let mut rulings: Vec<_> = self
            .inner
            .lock()
            .await
            .arbitration_rulings
            .values()
            .cloned()
            .collect();
        rulings.sort_by(|left, right| left.ruling_id.cmp(&right.ruling_id));
        Ok(rulings)
    }
}

pub mod migrations {
    pub const POSTGRES_0001: &str = include_str!("../../../migrations/postgres/0001_initial.sql");
    pub const SQLITE_0001: &str = include_str!("../../../migrations/sqlite/0001_initial.sql");
}
