use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use morpheus_protocol::{ValidationCode, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[async_trait]
pub trait EventStore: Clone + Send + Sync + 'static {
    async fn record_appservice_transaction(
        &self,
        transaction: AppServiceTransactionRecord,
    ) -> Result<(), ValidationError>;

    async fn record_raw_event(&self, event: RawMatrixEventRecord) -> Result<(), ValidationError>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryEventStore {
    inner: Arc<Mutex<InMemoryState>>,
}

#[derive(Debug, Default)]
struct InMemoryState {
    transactions: HashMap<String, Vec<String>>,
    raw_events: HashMap<String, RawMatrixEventRecord>,
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
        self.inner
            .lock()
            .await
            .raw_events
            .insert(event.event_id.clone(), event);
        Ok(())
    }
}

pub mod migrations {
    pub const POSTGRES_0001: &str = include_str!("../../../migrations/postgres/0001_initial.sql");
    pub const SQLITE_0001: &str = include_str!("../../../migrations/sqlite/0001_initial.sql");
}
