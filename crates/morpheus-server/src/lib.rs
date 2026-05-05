use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
};
use morpheus_matrix::{AppServiceTransaction, validate_transaction_event_ids};
use morpheus_protocol::{ValidationCode, validate_event_envelope};
use morpheus_store::{
    AppServiceTransactionRecord, EventStore, ProjectionErrorRecord, RawMatrixEventRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

mod context_validation;
mod projection;

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

    let event_ids = match validate_transaction_event_ids(&transaction) {
        Ok(event_ids) => event_ids,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": err.to_string(), "code": "INVALID_TRANSACTION" })),
            )
                .into_response();
        }
    };

    match state.store.appservice_transaction_event_ids(&txn_id).await {
        Ok(Some(previous)) if previous == event_ids => {
            return (StatusCode::OK, Json(json!({}))).into_response();
        }
        Ok(Some(_)) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "AppService transactions must be idempotent",
                    "code": ValidationCode::DuplicateEvent,
                })),
            )
                .into_response();
        }
        Ok(None) => {}
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.message, "code": err.code })),
            )
                .into_response();
        }
    }

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
        let origin_server_ts = raw
            .get("origin_server_ts")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let protocol_version = raw
            .pointer("/content/protocol_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let created_at = raw
            .pointer("/content/created_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let status = match validate_event_envelope(&raw) {
            Ok(validated) => {
                if let Err(err) =
                    context_validation::validate_event_context(&state.store, &validated).await
                {
                    if let Err(store_err) = state
                        .store
                        .record_raw_event(RawMatrixEventRecord {
                            event_id: validated.matrix_event_id.clone(),
                            room_id: validated.room_id.clone(),
                            sender: validated.sender.clone(),
                            event_type: validated.event_type.clone(),
                            origin_server_ts,
                            raw_json: raw,
                            validation_status: "rejected".into(),
                            validation_code: Some(err.code),
                        })
                        .await
                    {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": store_err.message, "code": store_err.code })),
                        )
                            .into_response();
                    }
                    let _ = state
                        .store
                        .record_projection_error(ProjectionErrorRecord {
                            matrix_event_id: Some(validated.matrix_event_id),
                            code: err.code,
                            message: err.message.clone(),
                            details: err.details.clone(),
                        })
                        .await;
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": err.message, "code": err.code })),
                    )
                        .into_response();
                }

                let record = RawMatrixEventRecord {
                    event_id: validated.matrix_event_id.clone(),
                    room_id: validated.room_id.clone(),
                    sender: validated.sender.clone(),
                    event_type: validated.event_type.clone(),
                    origin_server_ts,
                    raw_json: raw,
                    validation_status: "accepted".into(),
                    validation_code: None,
                };
                if let Err(err) = state.store.record_raw_event(record.clone()).await {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": err.message, "code": err.code })),
                    )
                        .into_response();
                }
                if let Err(err) = projection::persist_and_project(
                    &state.store,
                    &validated,
                    protocol_version.as_str(),
                    created_at.as_str(),
                )
                .await
                {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": err.message, "code": err.code })),
                    )
                        .into_response();
                }
                continue;
            }
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
                origin_server_ts,
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
        return admin_unauthorized();
    }
    Json(json!({
        "admin": {
            "auth_scheme": "Bearer",
            "token_configured": !state.config.admin_token.is_empty(),
        },
        "appservice": {
            "homeserver_token_configured": !state.config.homeserver_token.is_empty(),
        },
    }))
    .into_response()
}

async fn admin_allowlist<S>(
    State(state): State<AppState<S>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    Json(json!({
        "allowlist": [],
        "configured": false,
        "source": "server_config",
    }))
    .into_response()
}

async fn admin_catalog_rebuild<S>(
    State(state): State<AppState<S>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    let catalog = match catalog_summary(&state.store).await {
        Ok(summary) => summary,
        Err(response) => return response,
    };
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "scheduled",
            "catalog": catalog,
        })),
    )
        .into_response()
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
        return admin_unauthorized();
    }
    let order = match state.store.order(&order_id).await {
        Ok(Some(order)) => order,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "order not found",
                    "code": "ORDER_NOT_FOUND",
                    "order_id": order_id,
                })),
            )
                .into_response();
        }
        Err(err) => return store_error_response(err.message, err.code),
    };
    let event_count = match state.store.order_events(&order_id).await {
        Ok(events) => events.len(),
        Err(err) => return store_error_response(err.message, err.code),
    };
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "order_id": order_id,
            "status": "scheduled",
            "order": {
                "current_status": order.status,
                "event_count": event_count,
            },
        })),
    )
        .into_response()
}

fn admin_authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

fn admin_unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized", "code": "ADMIN_UNAUTHORIZED" })),
    )
        .into_response()
}

fn store_error_response(message: String, code: ValidationCode) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": message, "code": code })),
    )
        .into_response()
}

async fn catalog_summary<S>(store: &S) -> Result<Value, axum::response::Response>
where
    S: EventStore,
{
    let sellers = store
        .catalog_sellers()
        .await
        .map_err(|err| store_error_response(err.message, err.code))?;
    let products = store
        .catalog_products()
        .await
        .map_err(|err| store_error_response(err.message, err.code))?;
    let offers = store
        .catalog_offers()
        .await
        .map_err(|err| store_error_response(err.message, err.code))?;
    let tombstones = store
        .catalog_tombstones()
        .await
        .map_err(|err| store_error_response(err.message, err.code))?;

    Ok(json!({
        "sellers": sellers.len(),
        "products": products.len(),
        "offers": offers.len(),
        "tombstones": tombstones.len(),
    }))
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
