use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
};
use morpheus_matrix::AppServiceTransaction;
use morpheus_protocol::validate_event_envelope;
use morpheus_store::{AppServiceTransactionRecord, EventStore, RawMatrixEventRecord};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub homeserver_token: String,
    pub admin_token: String,
}

#[derive(Clone)]
struct AppState<S> {
    config: ServerConfig,
    store: S,
}

pub fn build_router<S>(config: ServerConfig, store: S) -> Router
where
    S: EventStore,
{
    let state = AppState { config, store };
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route(
            "/_matrix/app/v1/transactions/{txn_id}",
            put(appservice_transaction::<S>),
        )
        .route("/admin/config", get(admin_config::<S>))
        .route("/admin/allowlist", get(admin_allowlist::<S>))
        .route("/admin/catalog/rebuild", post(admin_catalog_rebuild::<S>))
        .route(
            "/admin/orders/{order_id}/replay",
            post(admin_order_replay::<S>),
        )
        .with_state(state)
}

async fn healthz() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn readyz() -> impl IntoResponse {
    Json(json!({ "status": "ready" }))
}

async fn metrics() -> impl IntoResponse {
    "morpheus_server_info 1\n"
}

#[derive(Debug, Deserialize)]
struct AccessTokenQuery {
    access_token: Option<String>,
}

async fn appservice_transaction<S>(
    State(state): State<AppState<S>>,
    Path(txn_id): Path<String>,
    Query(query): Query<AccessTokenQuery>,
    Json(transaction): Json<AppServiceTransaction>,
) -> impl IntoResponse
where
    S: EventStore,
{
    if query.access_token.as_deref() != Some(state.config.homeserver_token.as_str()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    let event_ids = transaction
        .events
        .iter()
        .filter_map(|event| {
            event
                .get("event_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();

    if let Err(err) = state
        .store
        .record_appservice_transaction(AppServiceTransactionRecord { txn_id, event_ids })
        .await
    {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": err.message, "code": err.code })),
        )
            .into_response();
    }

    for raw in transaction.events {
        let status = match validate_event_envelope(&raw) {
            Ok(validated) => RawMatrixEventRecord {
                event_id: validated.matrix_event_id,
                room_id: validated.room_id,
                sender: validated.sender,
                event_type: validated.event_type,
                origin_server_ts: raw
                    .get("origin_server_ts")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                raw_json: raw,
                validation_status: "accepted".into(),
                validation_code: None,
            },
            Err(err) => RawMatrixEventRecord {
                event_id: raw
                    .get("event_id")
                    .and_then(Value::as_str)
                    .unwrap_or("$unknown")
                    .to_string(),
                room_id: raw
                    .get("room_id")
                    .and_then(Value::as_str)
                    .unwrap_or("!unknown")
                    .to_string(),
                sender: raw
                    .get("sender")
                    .and_then(Value::as_str)
                    .unwrap_or("@unknown:unknown")
                    .to_string(),
                event_type: raw
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                origin_server_ts: raw
                    .get("origin_server_ts")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                raw_json: raw,
                validation_status: "rejected".into(),
                validation_code: Some(err.code),
            },
        };
        if let Err(err) = state.store.record_raw_event(status).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.message, "code": err.code })),
            )
                .into_response();
        }
    }

    (StatusCode::OK, Json(json!({}))).into_response()
}

async fn admin_config<S>(State(state): State<AppState<S>>, headers: HeaderMap) -> impl IntoResponse
where
    S: EventStore,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "admin": "configured" })).into_response()
}

async fn admin_allowlist<S>(
    State(state): State<AppState<S>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "allowlist": [] })).into_response()
}

async fn admin_catalog_rebuild<S>(
    State(state): State<AppState<S>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "status": "scheduled" })).into_response()
}

async fn admin_order_replay<S>(
    State(state): State<AppState<S>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> impl IntoResponse
where
    S: EventStore,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({ "order_id": order_id, "status": "scheduled" })).into_response()
}

fn admin_authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub instance: Value,
    pub appservice: Value,
    pub database: Value,
    pub admin: Value,
    #[serde(default)]
    pub allowlist: Value,
}
