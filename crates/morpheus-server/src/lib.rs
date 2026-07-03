use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post, put},
};
use chrono::Utc;
use morpheus_api::{
    BuyerOrderCreateRequest, EntitlementGrantRequest, EvmEscrowPaymentIntentRequest,
    OfferUpsertRequest, OfferWithdrawRequest, OrderAcceptRequest, OrderActionRequest,
    PaymentCaptureRequest, PaymentIntentRequest, ProductUpsertRequest, SellerAnnounceRequest,
};
use morpheus_config::EvmEscrowConfig;
use morpheus_matrix::{AppServiceTransaction, validate_transaction_event_ids};
use morpheus_protocol::{
    ValidationCode, ValidationError, parse_actor_id, parse_object_instance, validate_event_envelope,
};
use morpheus_store::{
    AppServiceTransactionRecord, CatalogOfferProjectionRecord, CatalogProductRecord,
    CatalogSellerRecord, EventStore, OrderEventRecord, OrderProjectionRecord,
    PaymentProjectionRecord, ProjectionErrorRecord, RawMatrixEventRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    env,
};
use ulid::Ulid;

mod context_validation;
pub mod evm_escrow;
pub mod evm_rpc;
pub mod evm_watcher;
mod projection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub instance_id: String,
    pub matrix_server_name: String,
    pub catalog_room_id: String,
    pub catalog_room_alias: Option<String>,
    pub order_room_alias_prefix: Option<String>,
    pub appservice_sender_localpart: String,
    pub homeserver_token: String,
    pub admin_token: String,
    pub seller_token: String,
    pub buyer_token: String,
    pub evm_escrow: Option<EvmEscrowConfig>,
}

#[derive(Clone)]
struct AppState<S, P> {
    config: ServerConfig,
    store: S,
    publisher: P,
}

#[async_trait::async_trait]
pub trait MatrixPublisher: Clone + Send + Sync + 'static {
    async fn publish(&self, events: Vec<Value>) -> Result<Vec<Value>, ValidationError>;

    async fn ensure_order_room(
        &self,
        alias: &str,
        _order_id: &str,
        _invite_user_ids: &[String],
    ) -> Result<String, ValidationError> {
        room_id_from_alias(alias)
    }

    async fn ensure_room_joined(&self, _room_id: &str) -> Result<(), ValidationError> {
        Ok(())
    }

    fn ingest_after_publish(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct InProcessMatrixPublisher;

#[async_trait::async_trait]
impl MatrixPublisher for InProcessMatrixPublisher {
    async fn publish(&self, events: Vec<Value>) -> Result<Vec<Value>, ValidationError> {
        Ok(events)
    }
}

#[derive(Debug, Clone)]
pub struct SynapseMatrixPublisher {
    homeserver_url: String,
    appservice_token: String,
    sender_user_id: String,
    client: reqwest::Client,
}

impl SynapseMatrixPublisher {
    pub fn new(homeserver_url: String, appservice_token: String, sender_user_id: String) -> Self {
        Self {
            homeserver_url,
            appservice_token,
            sender_user_id,
            client: reqwest::Client::new(),
        }
    }
}

pub async fn ensure_catalog_room(
    homeserver_url: &str,
    appservice_token: &str,
    sender_user_id: &str,
    catalog_room_alias: &str,
    instance_id: &str,
) -> Result<String, ValidationError> {
    let client = reqwest::Client::new();
    let create_body = matrix_create_room_body(catalog_room_alias, instance_id)?;
    ensure_room_with_body(
        &client,
        homeserver_url,
        appservice_token,
        sender_user_id,
        catalog_room_alias,
        create_body,
        "catalog",
    )
    .await
}

async fn ensure_room_with_body(
    client: &reqwest::Client,
    homeserver_url: &str,
    appservice_token: &str,
    sender_user_id: &str,
    room_alias: &str,
    create_body: Value,
    room_kind: &str,
) -> Result<String, ValidationError> {
    let create_url = matrix_create_room_url(homeserver_url, appservice_token, sender_user_id)?;
    let mut last_error = None;
    let mut response = None;
    for _ in 0..60 {
        match client
            .post(create_url.clone())
            .json(&create_body)
            .send()
            .await
        {
            Ok(ok) => {
                response = Some(ok);
                break;
            }
            Err(err) => {
                last_error = Some(err);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
    let response = response.ok_or_else(|| {
        publisher_error(&format!(
            "creating {room_kind} room failed: {}",
            last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "request was not attempted".into())
        ))
    })?;
    let status = response.status();
    let value = response
        .json::<Value>()
        .await
        .map_err(|err| publisher_error(&format!("reading createRoom response failed: {err}")))?;
    if status.is_success() {
        return value
            .get("room_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| publisher_error("createRoom response is missing room_id"));
    }
    if value.get("errcode").and_then(Value::as_str) != Some("M_ROOM_IN_USE") {
        return Err(publisher_error(&format!(
            "createRoom returned {status}: {value}"
        )));
    }
    let alias_url =
        matrix_room_alias_url(homeserver_url, room_alias, appservice_token, sender_user_id)?;
    let response = client.get(alias_url).send().await.map_err(|err| {
        publisher_error(&format!("resolving {room_kind} room alias failed: {err}"))
    })?;
    let status = response.status();
    let value = response.json::<Value>().await.map_err(|err| {
        publisher_error(&format!(
            "reading room alias resolution response failed: {err}"
        ))
    })?;
    if !status.is_success() {
        return Err(publisher_error(&format!(
            "room alias resolution returned {status}: {value}"
        )));
    }
    value
        .get("room_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| publisher_error("room alias response is missing room_id"))
}

async fn wait_for_joined_members(
    client: &reqwest::Client,
    homeserver_url: &str,
    appservice_token: &str,
    sender_user_id: &str,
    room_id: &str,
    user_ids: &[String],
) -> Result<(), ValidationError> {
    let mut pending = user_ids.iter().cloned().collect::<BTreeSet<_>>();
    if pending.is_empty() {
        return Ok(());
    }

    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        for user_id in pending.clone() {
            let url = matrix_room_member_state_url(
                homeserver_url,
                room_id,
                &user_id,
                appservice_token,
                sender_user_id,
            )?;
            let Ok(response) = client.get(url).send().await else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let Ok(value) = response.json::<Value>().await else {
                continue;
            };
            if value.get("membership").and_then(Value::as_str) == Some("join") {
                pending.remove(&user_id);
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
    }

    Err(publisher_error(&format!(
        "invited Matrix users did not join order room {room_id}: {}",
        pending.into_iter().collect::<Vec<_>>().join(", ")
    )))
}

#[async_trait::async_trait]
impl MatrixPublisher for SynapseMatrixPublisher {
    async fn publish(&self, events: Vec<Value>) -> Result<Vec<Value>, ValidationError> {
        let mut published = Vec::with_capacity(events.len());
        for mut event in events {
            let room_id = event
                .get("room_id")
                .and_then(Value::as_str)
                .ok_or_else(|| publisher_error("Matrix event is missing room_id"))?;
            let event_type = event
                .get("type")
                .and_then(Value::as_str)
                .ok_or_else(|| publisher_error("Matrix event is missing type"))?;
            let txn_id = format!("api-{}", Ulid::new());
            let url = matrix_send_url(
                &self.homeserver_url,
                room_id,
                event_type,
                &txn_id,
                &self.appservice_token,
                &self.sender_user_id,
            )?;
            let body = matrix_send_body(&event)?;
            let response = self
                .client
                .put(url)
                .json(&body)
                .send()
                .await
                .map_err(|err| publisher_error(&format!("sending Matrix event failed: {err}")))?;
            let status = response.status();
            let value = response.json::<Value>().await.map_err(|err| {
                publisher_error(&format!("reading Matrix send response failed: {err}"))
            })?;
            if !status.is_success() {
                return Err(publisher_error(&format!(
                    "Matrix send returned {status}: {value}"
                )));
            }
            let event_id = value
                .get("event_id")
                .and_then(Value::as_str)
                .ok_or_else(|| publisher_error("Matrix send response is missing event_id"))?;
            event["event_id"] = Value::String(event_id.to_string());
            published.push(event);
        }
        Ok(published)
    }

    async fn ensure_order_room(
        &self,
        alias: &str,
        order_id: &str,
        invite_user_ids: &[String],
    ) -> Result<String, ValidationError> {
        let create_body = matrix_create_order_room_body(alias, order_id, invite_user_ids)?;
        let room_id = ensure_room_with_body(
            &self.client,
            &self.homeserver_url,
            &self.appservice_token,
            &self.sender_user_id,
            alias,
            create_body,
            "order",
        )
        .await?;
        wait_for_joined_members(
            &self.client,
            &self.homeserver_url,
            &self.appservice_token,
            &self.sender_user_id,
            &room_id,
            invite_user_ids,
        )
        .await?;
        Ok(room_id)
    }

    async fn ensure_room_joined(&self, room_id: &str) -> Result<(), ValidationError> {
        let url = matrix_join_room_url(
            &self.homeserver_url,
            room_id,
            &self.appservice_token,
            &self.sender_user_id,
        )?;
        let response = self
            .client
            .post(url)
            .json(&matrix_join_room_body())
            .send()
            .await
            .map_err(|err| publisher_error(&format!("joining Matrix room failed: {err}")))?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(|err| {
            publisher_error(&format!("reading Matrix join response failed: {err}"))
        })?;
        if status.is_success() {
            return Ok(());
        }
        Err(publisher_error(&format!(
            "Matrix join returned {status}: {value}"
        )))
    }

    fn ingest_after_publish(&self) -> bool {
        false
    }
}

pub fn matrix_send_body(event: &Value) -> Result<Value, ValidationError> {
    event
        .get("content")
        .cloned()
        .ok_or_else(|| publisher_error("Matrix event is missing content"))
}

pub fn matrix_send_url(
    homeserver_url: &str,
    room_id: &str,
    event_type: &str,
    txn_id: &str,
    appservice_token: &str,
    sender_user_id: &str,
) -> Result<reqwest::Url, ValidationError> {
    let base = homeserver_url.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!(
        "{base}/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}"
    ))
    .map_err(|err| publisher_error(&format!("invalid Matrix send URL: {err}")))?;
    url.query_pairs_mut()
        .append_pair("access_token", appservice_token)
        .append_pair("user_id", sender_user_id);
    Ok(url)
}

pub fn catalog_alias_localpart(alias: &str) -> Result<String, ValidationError> {
    alias_localpart(alias, "catalog room alias")
}

fn alias_localpart(alias: &str, label: &str) -> Result<String, ValidationError> {
    alias
        .strip_prefix('#')
        .and_then(|without_hash| without_hash.split_once(':').map(|(local, _)| local))
        .filter(|local| !local.is_empty())
        .map(str::to_string)
        .ok_or_else(|| publisher_error(&format!("{label} must look like #local:server")))
}

pub fn matrix_create_room_body(alias: &str, instance_id: &str) -> Result<Value, ValidationError> {
    Ok(json!({
        "visibility": "public",
        "preset": "public_chat",
        "room_alias_name": catalog_alias_localpart(alias)?,
        "name": format!("Morpheus catalog {instance_id}"),
        "topic": format!("Morpheus marketplace catalog for {instance_id}"),
        "creation_content": {"m.federate": true},
    }))
}

pub fn order_room_alias(prefix: &str, order_id: &str, matrix_server_name: &str) -> String {
    let local = order_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{prefix}{local}:{matrix_server_name}")
}

pub fn matrix_create_order_room_body(
    alias: &str,
    order_id: &str,
    invite_user_ids: &[String],
) -> Result<Value, ValidationError> {
    Ok(json!({
        "visibility": "private",
        "preset": "private_chat",
        "room_alias_name": alias_localpart(alias, "order room alias")?,
        "name": format!("Morpheus order {order_id}"),
        "topic": format!("Morpheus marketplace order {order_id}"),
        "invite": invite_user_ids,
        "creation_content": {"m.federate": true},
    }))
}

pub fn matrix_create_room_url(
    homeserver_url: &str,
    appservice_token: &str,
    sender_user_id: &str,
) -> Result<reqwest::Url, ValidationError> {
    let base = homeserver_url.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/_matrix/client/v3/createRoom"))
        .map_err(|err| publisher_error(&format!("invalid Matrix createRoom URL: {err}")))?;
    url.query_pairs_mut()
        .append_pair("access_token", appservice_token)
        .append_pair("user_id", sender_user_id);
    Ok(url)
}

pub fn matrix_room_alias_url(
    homeserver_url: &str,
    alias: &str,
    appservice_token: &str,
    sender_user_id: &str,
) -> Result<reqwest::Url, ValidationError> {
    let base = homeserver_url.trim_end_matches('/');
    let encoded_alias = alias.replacen('#', "%23", 1);
    let mut url = reqwest::Url::parse(&format!(
        "{base}/_matrix/client/v3/directory/room/{encoded_alias}"
    ))
    .map_err(|err| publisher_error(&format!("invalid Matrix room alias URL: {err}")))?;
    url.query_pairs_mut()
        .append_pair("access_token", appservice_token)
        .append_pair("user_id", sender_user_id);
    Ok(url)
}

pub fn matrix_join_room_url(
    homeserver_url: &str,
    room_id: &str,
    appservice_token: &str,
    sender_user_id: &str,
) -> Result<reqwest::Url, ValidationError> {
    let base = homeserver_url.trim_end_matches('/');
    let mut url = reqwest::Url::parse(&format!("{base}/_matrix/client/v3/join/{room_id}"))
        .map_err(|err| publisher_error(&format!("invalid Matrix join URL: {err}")))?;
    url.query_pairs_mut()
        .append_pair("access_token", appservice_token)
        .append_pair("user_id", sender_user_id);
    Ok(url)
}

pub fn matrix_room_member_state_url(
    homeserver_url: &str,
    room_id: &str,
    user_id: &str,
    appservice_token: &str,
    sender_user_id: &str,
) -> Result<reqwest::Url, ValidationError> {
    let mut url = reqwest::Url::parse(homeserver_url.trim_end_matches('/'))
        .map_err(|err| publisher_error(&format!("invalid Matrix member state URL: {err}")))?;
    url.path_segments_mut()
        .map_err(|_| publisher_error("invalid Matrix member state URL base"))?
        .extend([
            "_matrix",
            "client",
            "v3",
            "rooms",
            room_id,
            "state",
            "m.room.member",
            user_id,
        ]);
    url.query_pairs_mut()
        .append_pair("access_token", appservice_token)
        .append_pair("user_id", sender_user_id);
    Ok(url)
}

pub fn matrix_join_room_body() -> Value {
    json!({})
}

fn room_id_from_alias(alias: &str) -> Result<String, ValidationError> {
    let localpart = alias_localpart(alias, "order room alias")?;
    let server_name = alias
        .split_once(':')
        .map(|(_, server)| server)
        .filter(|server| !server.is_empty())
        .ok_or_else(|| publisher_error("order room alias must look like #local:server"))?;
    Ok(format!("!{localpart}:{server_name}"))
}

fn publisher_error(message: &str) -> ValidationError {
    ValidationError::new(ValidationCode::PolicyViolation, message)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCatalogSource {
    pub instance_id: String,
    pub morpheus_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCatalogSyncError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCatalogSyncReport {
    pub source: RemoteCatalogSource,
    pub status: String,
    pub error: Option<RemoteCatalogSyncError>,
}

struct RemoteCatalogItems {
    sellers: Vec<CatalogSellerRecord>,
    products: Vec<CatalogProductRecord>,
    offers: Vec<CatalogOfferProjectionRecord>,
}

pub async fn sync_remote_catalog_once<S>(
    store: &S,
    source: &RemoteCatalogSource,
) -> Result<RemoteCatalogSyncReport, ValidationError>
where
    S: EventStore,
{
    match fetch_remote_catalog_items(source).await {
        Ok(items) => {
            apply_remote_catalog_items(store, source, items).await?;
            Ok(RemoteCatalogSyncReport {
                source: source.clone(),
                status: "live".into(),
                error: None,
            })
        }
        Err(err) => Ok(RemoteCatalogSyncReport {
            source: source.clone(),
            status: "cached".into(),
            error: Some(RemoteCatalogSyncError {
                code: "REMOTE_CATALOG_UNAVAILABLE".into(),
                message: format!(
                    "remote catalog sync for {} failed: {}",
                    source.instance_id, err.message
                ),
            }),
        }),
    }
}

async fn fetch_remote_catalog_items(
    source: &RemoteCatalogSource,
) -> Result<RemoteCatalogItems, ValidationError> {
    let client = reqwest::Client::new();
    let sellers = fetch_catalog_items::<CatalogSellerRecord>(
        &client,
        &source.morpheus_url,
        "/api/v1/catalog/sellers",
    )
    .await?;
    let products = fetch_catalog_items::<CatalogProductRecord>(
        &client,
        &source.morpheus_url,
        "/api/v1/catalog/products",
    )
    .await?;
    let offers = fetch_catalog_items::<CatalogOfferProjectionRecord>(
        &client,
        &source.morpheus_url,
        "/api/v1/catalog/offers",
    )
    .await?;
    Ok(RemoteCatalogItems {
        sellers,
        products,
        offers,
    })
}

async fn apply_remote_catalog_items<S>(
    store: &S,
    source: &RemoteCatalogSource,
    items: RemoteCatalogItems,
) -> Result<(), ValidationError>
where
    S: EventStore,
{
    let live_offer_ids = items
        .offers
        .iter()
        .map(|offer| offer.offer_id.clone())
        .collect::<HashSet<_>>();
    let existing_offer_ids = store
        .catalog_offers()
        .await?
        .into_iter()
        .filter(|offer| {
            parse_object_instance(&offer.offer_id)
                .map(|instance| instance == source.instance_id)
                .unwrap_or(false)
        })
        .map(|offer| offer.offer_id)
        .collect::<Vec<_>>();

    for seller in items.sellers {
        store
            .upsert_catalog_seller(
                &seller.seller_id,
                &seller.issuer_instance,
                &seller.status,
                seller.body,
            )
            .await?;
    }

    for product in items.products {
        store
            .upsert_catalog_product(
                &product.product_id,
                &product.seller_id,
                product.revision,
                product.body,
            )
            .await?;
    }

    for offer in items.offers {
        store.upsert_catalog_offer(offer).await?;
    }
    for offer_id in existing_offer_ids {
        if !live_offer_ids.contains(&offer_id) {
            store
                .tombstone_catalog_object(
                    &offer_id,
                    "offer",
                    json!({
                        "reason": "remote_catalog_missing",
                        "source": source.instance_id,
                    }),
                )
                .await?;
        }
    }
    Ok(())
}

async fn fetch_catalog_items<T>(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
) -> Result<Vec<T>, ValidationError>
where
    T: for<'de> Deserialize<'de>,
{
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| publisher_error(&format!("fetching remote catalog failed: {err}")))?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|err| publisher_error(&format!("reading remote catalog failed: {err}")))?;
    if !status.is_success() {
        return Err(publisher_error(&format!(
            "remote catalog returned {status}: {body}"
        )));
    }
    serde_json::from_value(body.get("items").cloned().unwrap_or_else(|| json!([])))
        .map_err(|err| publisher_error(&format!("decoding remote catalog failed: {err}")))
}

pub fn build_router<S>(config: ServerConfig, store: S) -> Router
where
    S: EventStore,
{
    build_router_with_publisher(config, store, InProcessMatrixPublisher)
}

pub fn build_router_with_publisher<S, P>(config: ServerConfig, store: S, publisher: P) -> Router
where
    S: EventStore,
    P: MatrixPublisher,
{
    let state = AppState {
        config,
        store,
        publisher,
    };
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/ui/admin", get(ui_admin))
        .route("/ui/seller", get(ui_seller::<S, P>))
        .route("/ui/buyer", get(ui_buyer::<S, P>))
        .route("/ui/assets/favicon.svg", get(ui_favicon_svg))
        .route("/ui/assets/app.css", get(ui_app_css))
        .route("/ui/assets/app.js", get(ui_app_js))
        .route("/ui/assets/app.bundle.js", get(ui_app_bundle_js))
        .route("/ui/assets/products/books.png", get(ui_product_books_png))
        .route("/ui/assets/products/cases.png", get(ui_product_cases_png))
        .route(
            "/ui/assets/products/sneakers.png",
            get(ui_product_sneakers_png),
        )
        .route(
            "/ui/assets/products/clothing.png",
            get(ui_product_clothing_png),
        )
        .route("/ui/assets/products/seed/{file}", get(ui_seed_product_jpg))
        .route("/admin/health", get(healthz))
        .route(
            "/_matrix/app/v1/transactions/{txn_id}",
            put(appservice_transaction::<S, P>),
        )
        .route("/admin/config", get(admin_config::<S, P>))
        .route("/admin/allowlist", get(admin_allowlist::<S, P>))
        .route(
            "/admin/projections/summary",
            get(admin_projection_summary::<S, P>),
        )
        .route("/admin/events", get(admin_events::<S, P>))
        .route(
            "/admin/catalog/rebuild",
            post(admin_catalog_rebuild::<S, P>),
        )
        .route(
            "/admin/evm-escrow/replay",
            post(admin_evm_escrow_replay::<S, P>),
        )
        .route(
            "/admin/evm-escrow/status",
            get(admin_evm_escrow_status::<S, P>),
        )
        .route(
            "/admin/rooms/bootstrap",
            post(admin_rooms_bootstrap::<S, P>),
        )
        .route(
            "/admin/orders/{order_id}/replay",
            post(admin_order_replay::<S, P>),
        )
        .route("/admin/orders/{order_id}", get(admin_order_show::<S, P>))
        .route("/api/v1/seller/announce", post(seller_announce::<S, P>))
        .route(
            "/api/v1/seller/products",
            post(seller_product_upsert::<S, P>),
        )
        .route("/api/v1/seller/offers", post(seller_offer_upsert::<S, P>))
        .route(
            "/api/v1/seller/offers/{offer_id}/withdraw",
            post(seller_offer_withdraw::<S, P>),
        )
        .route("/api/v1/seller/orders", get(seller_orders::<S, P>))
        .route(
            "/api/v1/seller/orders/{order_id}/accept",
            post(seller_order_accept::<S, P>),
        )
        .route(
            "/api/v1/seller/orders/{order_id}/reject",
            post(seller_order_reject::<S, P>),
        )
        .route(
            "/api/v1/seller/orders/{order_id}/payment-intent",
            post(seller_payment_intent::<S, P>),
        )
        .route(
            "/api/v1/seller/orders/{order_id}/evm-escrow/payment-intent",
            post(seller_evm_escrow_payment_intent::<S, P>),
        )
        .route(
            "/api/v1/seller/orders/{order_id}/payment-capture",
            post(seller_payment_capture::<S, P>),
        )
        .route(
            "/api/v1/seller/orders/{order_id}/entitlement-grant",
            post(seller_entitlement_grant::<S, P>),
        )
        .route(
            "/api/v1/seller/orders/{order_id}/complete",
            post(seller_order_complete::<S, P>),
        )
        .route("/api/v1/catalog/sellers", get(catalog_sellers::<S, P>))
        .route("/api/v1/catalog/products", get(catalog_products::<S, P>))
        .route("/api/v1/catalog/offers", get(catalog_offers::<S, P>))
        .route(
            "/api/v1/catalog/offers/{offer_id}",
            get(catalog_offer::<S, P>),
        )
        .route(
            "/api/v1/buyer/orders",
            post(buyer_order_create::<S, P>).get(buyer_orders::<S, P>),
        )
        .route(
            "/api/v1/buyer/orders/{order_id}",
            get(buyer_order_show::<S, P>),
        )
        .route(
            "/api/v1/buyer/orders/{order_id}/cancel",
            post(buyer_order_cancel::<S, P>),
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

async fn ui_admin() -> impl IntoResponse {
    Html(include_str!("../ui/admin.html"))
}

async fn ui_seller<S, P>(State(state): State<AppState<S, P>>) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    Html(render_ui_page(
        include_str!("../ui/seller.html"),
        &state.config,
    ))
}

async fn ui_buyer<S, P>(State(state): State<AppState<S, P>>) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    Html(render_ui_page(
        include_str!("../ui/buyer.html"),
        &state.config,
    ))
}

fn render_ui_page(template: &str, config: &ServerConfig) -> String {
    let ui_config = json!({
        "instance_id": config.instance_id,
        "matrix_server_name": config.matrix_server_name,
        "catalog_room_id": config.catalog_room_id,
    });
    let script = format!(
        r#"<script id="morpheus-ui-config" type="application/json">{}</script>"#,
        serde_json::to_string(&ui_config).unwrap_or_else(|_| "{}".into())
    );
    template.replacen("</head>", &format!("    {script}\n  </head>"), 1)
}

async fn ui_favicon_svg() -> impl IntoResponse {
    (
        [("content-type", "image/svg+xml")],
        include_str!("../ui/assets/favicon.svg"),
    )
}

async fn ui_app_css() -> impl IntoResponse {
    (
        [("content-type", "text/css")],
        include_str!("../ui/assets/app.css"),
    )
}

async fn ui_app_js() -> impl IntoResponse {
    (
        [("content-type", "application/javascript")],
        include_str!("../ui/assets/app.js"),
    )
}

async fn ui_app_bundle_js() -> impl IntoResponse {
    (
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("../ui/assets/app.bundle.js"),
    )
}

async fn ui_product_books_png() -> impl IntoResponse {
    (
        [("content-type", "image/png")],
        include_bytes!("../ui/assets/products/books.png").as_slice(),
    )
}

async fn ui_product_cases_png() -> impl IntoResponse {
    (
        [("content-type", "image/png")],
        include_bytes!("../ui/assets/products/cases.png").as_slice(),
    )
}

async fn ui_product_sneakers_png() -> impl IntoResponse {
    (
        [("content-type", "image/png")],
        include_bytes!("../ui/assets/products/sneakers.png").as_slice(),
    )
}

async fn ui_product_clothing_png() -> impl IntoResponse {
    (
        [("content-type", "image/png")],
        include_bytes!("../ui/assets/products/clothing.png").as_slice(),
    )
}

async fn ui_seed_product_jpg(Path(file): Path<String>) -> impl IntoResponse {
    match seed_product_image(&file) {
        Some(bytes) => ([("content-type", "image/jpeg")], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn seed_product_image(file: &str) -> Option<&'static [u8]> {
    match file {
        "booksprod0101.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0101.jpg").as_slice())
        }
        "booksprod0102.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0102.jpg").as_slice())
        }
        "booksprod0201.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0201.jpg").as_slice())
        }
        "booksprod0202.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0202.jpg").as_slice())
        }
        "booksprod0301.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0301.jpg").as_slice())
        }
        "booksprod0302.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0302.jpg").as_slice())
        }
        "booksprod0401.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0401.jpg").as_slice())
        }
        "booksprod0402.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0402.jpg").as_slice())
        }
        "booksprod0501.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0501.jpg").as_slice())
        }
        "booksprod0502.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/booksprod0502.jpg").as_slice())
        }
        "casesprod0101.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0101.jpg").as_slice())
        }
        "casesprod0102.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0102.jpg").as_slice())
        }
        "casesprod0201.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0201.jpg").as_slice())
        }
        "casesprod0202.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0202.jpg").as_slice())
        }
        "casesprod0301.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0301.jpg").as_slice())
        }
        "casesprod0302.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0302.jpg").as_slice())
        }
        "casesprod0401.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0401.jpg").as_slice())
        }
        "casesprod0402.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/casesprod0402.jpg").as_slice())
        }
        "fashionprod0101.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0101.jpg").as_slice())
        }
        "fashionprod0102.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0102.jpg").as_slice())
        }
        "fashionprod0201.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0201.jpg").as_slice())
        }
        "fashionprod0202.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0202.jpg").as_slice())
        }
        "fashionprod0301.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0301.jpg").as_slice())
        }
        "fashionprod0302.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0302.jpg").as_slice())
        }
        "fashionprod0401.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0401.jpg").as_slice())
        }
        "fashionprod0402.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0402.jpg").as_slice())
        }
        "fashionprod0501.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0501.jpg").as_slice())
        }
        "fashionprod0502.jpg" => {
            Some(include_bytes!("../ui/assets/products/seed/fashionprod0502.jpg").as_slice())
        }
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct AccessTokenQuery {
    access_token: Option<String>,
}

async fn appservice_transaction<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(txn_id): Path<String>,
    Query(query): Query<AccessTokenQuery>,
    Json(transaction): Json<AppServiceTransaction>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    let query_authorized =
        query.access_token.as_deref() == Some(state.config.homeserver_token.as_str());
    if !query_authorized && !bearer_authorized(&headers, &state.config.homeserver_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    if let Err(err) = ensure_invited_rooms_joined(&state, &transaction).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
        )
            .into_response();
    }

    match ingest_transaction(&state.store, txn_id, transaction).await {
        Ok(()) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(response) => response,
    }
}

async fn ensure_invited_rooms_joined<S, P>(
    state: &AppState<S, P>,
    transaction: &AppServiceTransaction,
) -> Result<(), ValidationError>
where
    S: EventStore,
    P: MatrixPublisher,
{
    let local_user_id = matrix_user_id(&state.config, &state.config.instance_id);
    let mut room_ids = BTreeSet::new();

    for event in &transaction.events {
        let is_invite = event.get("type").and_then(Value::as_str) == Some("m.room.member")
            && event.get("state_key").and_then(Value::as_str) == Some(local_user_id.as_str())
            && event.pointer("/content/membership").and_then(Value::as_str) == Some("invite");
        if !is_invite {
            continue;
        }
        if let Some(room_id) = event.get("room_id").and_then(Value::as_str) {
            room_ids.insert(room_id.to_string());
        }
    }

    for room_id in room_ids {
        state.publisher.ensure_room_joined(&room_id).await?;
    }

    Ok(())
}

pub async fn ingest_transaction<S>(
    store: &S,
    txn_id: String,
    transaction: AppServiceTransaction,
) -> Result<(), axum::response::Response>
where
    S: EventStore,
{
    let event_ids = match validate_transaction_event_ids(&transaction) {
        Ok(event_ids) => event_ids,
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": err.to_string(), "code": "INVALID_TRANSACTION" })),
            )
                .into_response());
        }
    };

    match store.appservice_transaction_event_ids(&txn_id).await {
        Ok(Some(previous)) if previous == event_ids => {
            return Ok(());
        }
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({
                    "error": "AppService transactions must be idempotent",
                    "code": ValidationCode::DuplicateEvent,
                })),
            )
                .into_response());
        }
        Ok(None) => {}
        Err(err) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.message, "code": err.code })),
            )
                .into_response());
        }
    }

    if let Err(err) = store
        .record_appservice_transaction(AppServiceTransactionRecord { txn_id, event_ids })
        .await
    {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": err.message, "code": err.code })),
        )
            .into_response());
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
                    context_validation::validate_event_context(store, &validated).await
                {
                    if let Err(store_err) = store
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
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": store_err.message, "code": store_err.code })),
                        )
                            .into_response());
                    }
                    let _ = store
                        .record_projection_error(ProjectionErrorRecord {
                            matrix_event_id: Some(validated.matrix_event_id),
                            code: err.code,
                            message: err.message.clone(),
                            details: err.details.clone(),
                        })
                        .await;
                    return Err((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": err.message, "code": err.code })),
                    )
                        .into_response());
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
                if let Err(err) = store.record_raw_event(record.clone()).await {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": err.message, "code": err.code })),
                    )
                        .into_response());
                }
                if let Err(err) = projection::persist_and_project(
                    store,
                    &validated,
                    protocol_version.as_str(),
                    created_at.as_str(),
                )
                .await
                {
                    return Err((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": err.message, "code": err.code })),
                    )
                        .into_response());
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
        if let Err(err) = store.record_raw_event(status).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": err.message, "code": err.code })),
            )
                .into_response());
        }
    }

    Ok(())
}

async fn admin_config<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
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

async fn admin_allowlist<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
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

async fn admin_projection_summary<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    projection_summary(&state.store).await
}

async fn admin_events<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    match state.store.projection_errors().await {
        Ok(errors) => Json(json!({ "events": errors })).into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn admin_catalog_rebuild<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
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

async fn admin_evm_escrow_replay<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }

    let evm = match state.config.evm_escrow.as_ref().filter(|evm| evm.enabled) {
        Some(evm) => evm,
        None => {
            return validation_error_response(
                "EVM_ESCROW_NOT_CONFIGURED",
                "evm escrow payment config is absent or disabled",
            );
        }
    };
    let rpc_url = match env::var(&evm.rpc_url_env) {
        Ok(rpc_url) => rpc_url,
        Err(_) => {
            return validation_error_response(
                "EVM_ESCROW_RPC_URL_MISSING",
                format!("missing EVM RPC URL env {}", evm.rpc_url_env),
            );
        }
    };
    let source = evm_rpc::EvmRpcClient::new(rpc_url);
    let watcher_publisher =
        evm_watcher::MatrixWatcherPublisher::new(state.config.clone(), state.publisher.clone());
    let result = match evm_watcher::scan_once(
        &state.store,
        &source,
        &watcher_publisher,
        evm_watcher::WatcherScanConfig {
            evm: evm.clone(),
            instance_id: state.config.instance_id.clone(),
        },
    )
    .await
    {
        Ok(result) => result,
        Err(err) => return store_error_response(err.message, err.code),
    };
    let chain_id = evm.chain_id as i64;
    let checkpoint = match state
        .store
        .evm_escrow_checkpoint(chain_id, &evm.escrow_contract)
        .await
    {
        Ok(checkpoint) => checkpoint,
        Err(err) => return store_error_response(err.message, err.code),
    };

    Json(json!({
        "status": "ok",
        "scanned": result.scanned,
        "accepted": result.accepted,
        "duplicates": result.duplicates,
        "rejected": result.rejected,
        "from_block": result.from_block,
        "to_block": result.to_block,
        "checkpoint": {
            "chain_id": chain_id,
            "escrow_contract": evm.escrow_contract,
            "latest_scanned_block": checkpoint,
        },
        "rpc_scan": { "enabled": true },
    }))
    .into_response()
}

async fn admin_evm_escrow_status<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    let Some(evm) = state.config.evm_escrow.as_ref().filter(|evm| evm.enabled) else {
        return Json(json!({"enabled": false})).into_response();
    };
    let chain_id = evm.chain_id as i64;
    let checkpoint = match state
        .store
        .evm_escrow_checkpoint(chain_id, &evm.escrow_contract)
        .await
    {
        Ok(checkpoint) => checkpoint,
        Err(err) => return store_error_response(err.message, err.code),
    };

    Json(json!({
        "enabled": true,
        "chain_id": evm.chain_id,
        "escrow_contract": evm.escrow_contract,
        "confirmations": evm.confirmations,
        "poll_interval_secs": evm.poll_interval_secs,
        "start_block": evm.start_block,
        "max_scan_blocks": evm.max_scan_blocks,
        "checkpoint": {
            "chain_id": chain_id,
            "escrow_contract": evm.escrow_contract,
            "latest_scanned_block": checkpoint,
        },
        "watcher": {
            "mode": "embedded",
            "rpc_url_env": evm.rpc_url_env,
        },
    }))
    .into_response()
}

async fn admin_rooms_bootstrap<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    (
        StatusCode::OK,
        Json(json!({
            "status": "ready",
            "catalog_room_id": state.config.catalog_room_id,
            "catalog_room_alias": state.config.catalog_room_alias,
            "order_room_alias_prefix": state.config.order_room_alias_prefix,
        })),
    )
        .into_response()
}

async fn admin_order_replay<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
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

async fn admin_order_show<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !admin_authorized(&headers, &state.config.admin_token) {
        return admin_unauthorized();
    }
    match enriched_order(&state.store, &order_id).await {
        Ok(Some(order)) => Json(json!({ "order": order })).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
        )
            .into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn seller_announce<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Json(request): Json<SellerAnnounceRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.seller_id,
    ) {
        return response;
    }
    publish_generated(
        &state,
        vec![marketplace_event(
            &state.config,
            "io.marketplace.actor.seller.announced",
            &state.config.catalog_room_id,
            &request.seller_id,
            json!({
                "seller_id": request.seller_id,
                "status": "active",
                "display_name": request.display_name,
                "legal_profile_ref": request.legal_profile_ref,
                "terms_ref": request.terms_ref,
                "terms_hash": request.terms_hash,
                "supported_payment_adapters": request.supported_payment_adapters,
                "supported_entitlement_types": request.supported_entitlement_types,
            }),
        )],
    )
    .await
}

async fn seller_product_upsert<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Json(request): Json<ProductUpsertRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.seller_id,
    ) {
        return response;
    }
    let media = request
        .image_src
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(|image_src| {
            json!([{
                "kind": "image",
                "uri": image_src,
                "role": "primary"
            }])
        })
        .unwrap_or_else(|| json!([]));
    publish_generated(
        &state,
        vec![marketplace_event(
            &state.config,
            "io.marketplace.product.upserted",
            &state.config.catalog_room_id,
            &request.seller_id,
            json!({
                "product_id": request.product_id,
                "seller_id": request.seller_id,
                "revision": request.revision,
                "status": "active",
                "kind": request.kind,
                "title": request.title,
                "description": request.description,
                "categories": request.categories,
                "tags": request.tags,
                "media": media,
                "image_src": request.image_src,
                "terms_hash": request.terms_hash,
            }),
        )],
    )
    .await
}

async fn seller_offer_upsert<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Json(request): Json<OfferUpsertRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.seller_id,
    ) {
        return response;
    }
    publish_generated(
        &state,
        vec![marketplace_event(
            &state.config,
            "io.marketplace.offer.upserted",
            &state.config.catalog_room_id,
            &request.seller_id,
            json!({
                "offer_id": request.offer_id,
                "product_id": request.product_id,
                "seller_id": request.seller_id,
                "revision": request.revision,
                "status": "active",
                "price": request.price,
                "payment_terms": {
                    "capture_policy": request.payment_capture_policy,
                    "adapter_policy": "seller_supported",
                },
                "seller_terms_hash": request.seller_terms_hash,
                "offer_terms_hash": request.offer_terms_hash,
                "entitlement": {"type": request.entitlement_type, "delivery": "external"},
                "availability": {"mode": request.availability_mode},
            }),
        )],
    )
    .await
}

async fn seller_offer_withdraw<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(offer_id): Path<String>,
    Json(request): Json<OfferWithdrawRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.seller_id,
    ) {
        return response;
    }
    publish_generated(
        &state,
        vec![marketplace_event(
            &state.config,
            "io.marketplace.offer.withdrawn",
            &state.config.catalog_room_id,
            &request.seller_id,
            json!({
                "offer_id": offer_id,
                "seller_id": request.seller_id,
                "revision": request.revision,
                "reason": request.reason.unwrap_or_else(|| "seller_withdrawn".into()),
            }),
        )],
    )
    .await
}

async fn seller_orders<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !bearer_authorized(&headers, &state.config.seller_token) {
        return role_unauthorized();
    }
    list_orders(&state.store).await
}

async fn seller_order_accept<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<OrderAcceptRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    order_event_response(
        &state,
        "io.marketplace.order.accepted",
        &order_id,
        &request.actor_id,
        json!({
            "order_id": order_id,
            "offer_revision": request.offer_revision,
            "seller_terms_hash": request.seller_terms_hash,
            "offer_terms_hash": request.offer_terms_hash,
            "payment_capture_policy": request.payment_capture_policy,
            "arbitration_policy_version": request.arbitration_policy_version,
        }),
    )
    .await
}

async fn seller_order_reject<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<OrderActionRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    seller_simple_order_event(
        state,
        headers,
        order_id,
        request,
        "io.marketplace.order.rejected",
    )
    .await
}

async fn seller_order_complete<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<OrderActionRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    match order_has_event(
        &state.store,
        &order_id,
        "io.marketplace.order.completed",
        None,
    )
    .await
    {
        Ok(true) => return accepted_noop_order_response(&state, &order_id).await,
        Ok(false) => {}
        Err(err) => return store_error_response(err.message, err.code),
    }
    let order = match state.store.order(&order_id).await {
        Ok(Some(order)) => order,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
            )
                .into_response();
        }
        Err(err) => return store_error_response(err.message, err.code),
    };
    if let Err(err) = state.publisher.ensure_room_joined(&order.room_id).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
        )
            .into_response();
    }
    let offer_revision = match state.store.catalog_offers().await {
        Ok(offers) => offers
            .into_iter()
            .find(|offer| offer.offer_id == order.offer_id)
            .map(|offer| offer.revision)
            .or_else(|| order.body.get("offer_revision").and_then(Value::as_i64))
            .unwrap_or(1),
        Err(err) => return store_error_response(err.message, err.code),
    };
    publish_generated(
        &state,
        vec![
            marketplace_event(
                &state.config,
                "io.marketplace.order.completed",
                &order.room_id,
                &request.actor_id,
                json!({ "order_id": order_id }),
            ),
            marketplace_event(
                &state.config,
                "io.marketplace.offer.withdrawn",
                &state.config.catalog_room_id,
                &request.actor_id,
                json!({
                    "offer_id": order.offer_id,
                    "seller_id": order.seller_id,
                    "revision": offer_revision,
                    "reason": "sold",
                }),
            ),
        ],
    )
    .await
}

async fn seller_payment_intent<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<PaymentIntentRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    match order_has_event(
        &state.store,
        &order_id,
        "io.marketplace.payment.intent.created",
        Some(("payment_id", &request.payment_id)),
    )
    .await
    {
        Ok(true) => return accepted_noop_order_response(&state, &order_id).await,
        Ok(false) => {}
        Err(err) => return store_error_response(err.message, err.code),
    }
    order_event_response(
        &state,
        "io.marketplace.payment.intent.created",
        &order_id,
        &request.actor_id,
        json!({
            "order_id": order_id,
            "payment_id": request.payment_id,
            "adapter": request.adapter,
            "amount": request.amount,
            "currency": request.currency,
            "capture_policy": request.capture_policy,
            "idempotency_key": request.idempotency_key,
            "provider_ref": request.provider_ref,
            "confirmation": request.confirmation,
            "expires_at": request.expires_at,
        }),
    )
    .await
}

async fn seller_evm_escrow_payment_intent<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<EvmEscrowPaymentIntentRequest>,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }

    let evm = match state.config.evm_escrow.as_ref().filter(|evm| evm.enabled) {
        Some(evm) => evm,
        None => {
            return validation_error_response(
                "EVM_ESCROW_NOT_CONFIGURED",
                "evm escrow payment config is absent or disabled",
            );
        }
    };
    let order = match state.store.order(&order_id).await {
        Ok(Some(order)) => order,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
            )
                .into_response();
        }
        Err(err) => return store_error_response(err.message, err.code),
    };
    if order.seller_id != request.actor_id {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "code": "ACTOR_FORBIDDEN",
                "error": "seller actor does not own this order",
            })),
        )
            .into_response();
    }
    if order.body.get("payment_adapter").and_then(Value::as_str) != Some("evm_escrow") {
        return validation_error_response(
            "ORDER_PAYMENT_ADAPTER_MISMATCH",
            "order payment_adapter is not evm_escrow",
        );
    }

    let price = match order.body.get("price") {
        Some(price) => price.clone(),
        None => return validation_error_response("ORDER_PRICE_MISSING", "order price is missing"),
    };
    let amount = match price.get("amount").and_then(Value::as_str) {
        Some(amount) => amount,
        None => {
            return validation_error_response(
                "ORDER_AMOUNT_MISSING",
                "order price.amount is missing",
            );
        }
    };
    let currency = match price.get("currency").and_then(Value::as_str) {
        Some(currency) => currency,
        None => {
            return validation_error_response(
                "ORDER_CURRENCY_MISSING",
                "order price.currency is missing",
            );
        }
    };
    let token = match evm
        .tokens
        .iter()
        .find(|token| token.currency.eq_ignore_ascii_case(currency))
    {
        Some(token) => token,
        None => {
            return validation_error_response(
                "EVM_ESCROW_TOKEN_UNSUPPORTED",
                "order currency is not configured for evm escrow",
            );
        }
    };
    let amount_units = match decimal_amount_units(amount, token.decimals) {
        Ok(amount_units) => amount_units,
        Err(message) => return validation_error_response("ORDER_AMOUNT_INVALID", message),
    };
    let offer_revision = order
        .body
        .get("offer_revision")
        .and_then(Value::as_i64)
        .unwrap_or(1);
    let payment_capture_policy = match order
        .body
        .get("payment_capture_policy")
        .and_then(Value::as_str)
    {
        Some(policy) => policy,
        None => {
            return validation_error_response(
                "ORDER_CAPTURE_POLICY_MISSING",
                "order payment_capture_policy is missing",
            );
        }
    };
    let arbiter_actor = match order.body.get("arbiter_actor").and_then(Value::as_str) {
        Some(arbiter_actor) => arbiter_actor,
        None => {
            return validation_error_response(
                "ORDER_ARBITER_MISSING",
                "order arbiter_actor is missing",
            );
        }
    };
    if let Some(response) = validate_evm_address("buyer_evm_address", &request.buyer_evm_address) {
        return response;
    }
    if let Some(response) = validate_evm_address("seller_evm_address", &request.seller_evm_address)
    {
        return response;
    }
    if let Some(response) =
        validate_evm_address("arbiter_evm_address", &request.arbiter_evm_address)
    {
        return response;
    }
    let input = evm_escrow::EvmEscrowIntentInput {
        protocol: "io.marketplace".into(),
        protocol_version: "0.1".into(),
        instance_id: state.config.instance_id.clone(),
        order_id: order.order_id.clone(),
        customer_id: order.customer_id.clone(),
        seller_id: order.seller_id.clone(),
        offer_id: order.offer_id.clone(),
        offer_revision,
        price: price.clone(),
        payment_adapter: "evm_escrow".into(),
        payment_capture_policy: payment_capture_policy.into(),
        chain_id: evm.chain_id,
        token_contract: token.contract.clone(),
        amount_units: amount_units.clone(),
        escrow_contract: evm.escrow_contract.clone(),
        seller_evm_address: request.seller_evm_address.clone(),
        buyer_evm_address: request.buyer_evm_address.clone(),
        arbiter_actor: arbiter_actor.into(),
        arbiter_evm_address: request.arbiter_evm_address.clone(),
    };
    let order_hash = match evm_escrow::compute_order_hash(&input) {
        Ok(order_hash) => order_hash,
        Err(err) => return store_error_response(err.message, err.code),
    };
    let provider_ref = format!("evm_escrow:{order_hash}");
    let idempotency_key = format!("evm_escrow:{}", request.payment_id);
    let expires_at = (Utc::now() + chrono::Duration::minutes(30))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let body = json!({
        "order_id": order_id,
        "payment_id": request.payment_id,
        "adapter": "evm_escrow",
        "amount": amount,
        "currency": currency,
        "capture_policy": payment_capture_policy,
        "idempotency_key": idempotency_key,
        "provider_ref": provider_ref,
        "confirmation": {
            "method": "evm_escrow_deposit",
            "uri": format!("https://{}/evm-escrow/{}", state.config.matrix_server_name, order_hash),
            "adapter": "evm_escrow",
            "chain_id": evm.chain_id,
            "token": token.contract.clone(),
            "token_currency": token.currency.clone(),
            "amount_units": amount_units,
            "escrow_contract": evm.escrow_contract.clone(),
            "order_hash": order_hash,
            "buyer_evm_address": request.buyer_evm_address,
            "seller_evm_address": request.seller_evm_address,
            "arbiter_actor": arbiter_actor,
            "arbiter_evm_address": request.arbiter_evm_address,
        },
        "expires_at": expires_at,
    });
    match existing_order_event(
        &state.store,
        &order_id,
        "io.marketplace.payment.intent.created",
        Some((
            "payment_id",
            body["payment_id"].as_str().unwrap_or_default(),
        )),
    )
    .await
    {
        Ok(Some(existing)) if evm_payment_intent_matches(&existing.body, &body) => {
            return accepted_noop_order_response(&state, &order_id).await;
        }
        Ok(Some(_)) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({
                    "code": "IDEMPOTENCY_CONFLICT",
                    "error": "payment_id already exists with different evm escrow intent terms",
                })),
            )
                .into_response();
        }
        Ok(None) => {}
        Err(err) => return store_error_response(err.message, err.code),
    }

    order_event_response(
        &state,
        "io.marketplace.payment.intent.created",
        &order_id,
        &request.actor_id,
        body,
    )
    .await
}

async fn seller_payment_capture<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<PaymentCaptureRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    let room_id = match state.store.order(&order_id).await {
        Ok(Some(order)) => order.room_id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
            )
                .into_response();
        }
        Err(err) => return store_error_response(err.message, err.code),
    };
    let existing_events = match state.store.order_events(&order_id).await {
        Ok(events) => events,
        Err(err) => return store_error_response(err.message, err.code),
    };
    let payment_id = request.payment_id.clone();
    let authorized_exists = existing_events.iter().any(|event| {
        order_event_matches(
            event,
            "io.marketplace.payment.authorized",
            Some(("payment_id", &payment_id)),
        )
    });
    let captured_exists = existing_events.iter().any(|event| {
        order_event_matches(
            event,
            "io.marketplace.payment.captured",
            Some(("payment_id", &payment_id)),
        )
    });
    if captured_exists {
        return accepted_noop_response(&room_id);
    }

    if let Err(err) = state.publisher.ensure_room_joined(&room_id).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
        )
            .into_response();
    }
    let mut events = Vec::new();
    if !authorized_exists {
        events.push(marketplace_event(
            &state.config,
            "io.marketplace.payment.authorized",
            &room_id,
            &request.actor_id,
            json!({
                "order_id": order_id,
                "payment_id": payment_id.clone(),
            }),
        ));
    }
    events.push(marketplace_event(
        &state.config,
        "io.marketplace.payment.captured",
        &room_id,
        &request.actor_id,
        json!({
            "order_id": order_id,
            "payment_id": payment_id,
            "adapter": request.adapter,
            "amount": request.amount,
            "currency": request.currency,
            "provider_ref": request.provider_ref,
            "evidence": request.evidence,
        }),
    ));
    publish_generated(&state, events).await
}

async fn order_has_event<S>(
    store: &S,
    order_id: &str,
    event_type: &str,
    body_field: Option<(&str, &str)>,
) -> Result<bool, ValidationError>
where
    S: EventStore,
{
    Ok(store
        .order_events(order_id)
        .await?
        .iter()
        .any(|event| order_event_matches(event, event_type, body_field)))
}

async fn existing_order_event<S>(
    store: &S,
    order_id: &str,
    event_type: &str,
    body_field: Option<(&str, &str)>,
) -> Result<Option<OrderEventRecord>, ValidationError>
where
    S: EventStore,
{
    Ok(store
        .order_events(order_id)
        .await?
        .into_iter()
        .find(|event| order_event_matches(event, event_type, body_field)))
}

fn order_event_matches(
    event: &OrderEventRecord,
    event_type: &str,
    body_field: Option<(&str, &str)>,
) -> bool {
    event.event_type == event_type
        && body_field.is_none_or(|(key, expected)| {
            event.body.get(key).and_then(Value::as_str) == Some(expected)
        })
}

async fn accepted_noop_order_response<S, P>(
    state: &AppState<S, P>,
    order_id: &str,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    match state.store.order(order_id).await {
        Ok(Some(order)) => accepted_noop_response(&order.room_id),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
        )
            .into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

fn accepted_noop_response(room_id: &str) -> axum::response::Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "accepted", "room_id": room_id, "event_ids": [] })),
    )
        .into_response()
}

fn validation_error_response(code: &str, error: impl Into<String>) -> axum::response::Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({ "code": code, "error": error.into() })),
    )
        .into_response()
}

fn decimal_amount_units(amount: &str, decimals: u8) -> Result<String, &'static str> {
    let (whole, fraction) = amount
        .split_once('.')
        .map_or((amount, ""), |(whole, fraction)| (whole, fraction));
    if whole.is_empty()
        || !whole.chars().all(|ch| ch.is_ascii_digit())
        || !fraction.chars().all(|ch| ch.is_ascii_digit())
        || fraction.len() > decimals as usize
    {
        return Err("order amount cannot be represented by configured token decimals");
    }

    let mut digits = String::with_capacity(whole.len() + decimals as usize);
    let trimmed_whole = whole.trim_start_matches('0');
    digits.push_str(if trimmed_whole.is_empty() {
        "0"
    } else {
        trimmed_whole
    });
    digits.push_str(fraction);
    for _ in fraction.len()..decimals as usize {
        digits.push('0');
    }
    let trimmed = digits.trim_start_matches('0');
    Ok(if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.into()
    })
}

fn validate_evm_address(field: &str, address: &str) -> Option<axum::response::Response> {
    if let Some(hex) = address.strip_prefix("0x") {
        if hex.len() == 40
            && hex.chars().all(|ch| ch.is_ascii_hexdigit())
            && hex.chars().any(|ch| ch != '0')
        {
            return None;
        }
    }
    Some(validation_error_response(
        "INVALID_EVM_ADDRESS",
        format!("{field} must be a nonzero 20-byte EVM address"),
    ))
}

fn evm_payment_intent_matches(existing: &Value, expected: &Value) -> bool {
    existing.get("adapter") == expected.get("adapter")
        && existing.get("amount") == expected.get("amount")
        && existing.get("currency") == expected.get("currency")
        && existing.get("capture_policy") == expected.get("capture_policy")
        && existing.get("provider_ref") == expected.get("provider_ref")
        && existing
            .get("confirmation")
            .and_then(|value| value.get("order_hash"))
            == expected
                .get("confirmation")
                .and_then(|value| value.get("order_hash"))
        && existing
            .get("confirmation")
            .and_then(|value| value.get("buyer_evm_address"))
            == expected
                .get("confirmation")
                .and_then(|value| value.get("buyer_evm_address"))
        && existing
            .get("confirmation")
            .and_then(|value| value.get("seller_evm_address"))
            == expected
                .get("confirmation")
                .and_then(|value| value.get("seller_evm_address"))
        && existing
            .get("confirmation")
            .and_then(|value| value.get("arbiter_evm_address"))
            == expected
                .get("confirmation")
                .and_then(|value| value.get("arbiter_evm_address"))
}

async fn seller_entitlement_grant<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<EntitlementGrantRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    order_event_response(
        &state,
        "io.marketplace.entitlement.granted",
        &order_id,
        &request.actor_id,
        json!({
            "order_id": order_id,
            "payment_id": request.payment_id,
            "entitlement_id": request.entitlement_id,
            "type": request.entitlement_type,
            "external_ref": request.external_ref,
            "evidence": request.evidence,
        }),
    )
    .await
}

async fn buyer_order_create<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Json(request): Json<BuyerOrderCreateRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.buyer_token,
        "customer",
        &state.config.instance_id,
        &request.customer_id,
    ) {
        return response;
    }
    if let Some(response) = match withdrawn_offer_response(&state.store, &request.offer_id).await {
        Ok(response) => response,
        Err(response) => return response,
    } {
        return response;
    }
    let room_id = match buyer_order_room_id(&state, &request).await {
        Ok(room_id) => room_id,
        Err((status, err)) => {
            return (
                status,
                Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
            )
                .into_response();
        }
    };
    let customer_bound = marketplace_event(
        &state.config,
        "io.marketplace.actor.customer.bound",
        &room_id,
        &request.customer_id,
        json!({
            "customer_id": request.customer_id,
            "status": "active",
            "display_name": request.customer_display_name,
            "instance_id": state.config.instance_id,
            "authorized_representatives": [format!("@{}:{}", state.config.appservice_sender_localpart, state.config.matrix_server_name)],
            "accepted_payment_adapters": ["mock"],
            "accepted_arbitration_policies": [request.arbitration_policy_id.clone()],
        }),
    );
    let order_created = marketplace_event(
        &state.config,
        "io.marketplace.order.created",
        &room_id,
        &request.customer_id,
        json!({
            "order_id": request.order_id,
            "room_id": room_id,
            "customer_id": request.customer_id,
            "seller_id": request.seller_id,
            "offer_id": request.offer_id,
            "offer_revision": request.offer_revision,
            "catalog_snapshot_id": request.catalog_snapshot_id,
            "quantity": 1,
            "price": request.price,
            "payment_adapter": request.payment_adapter,
            "payment_capture_policy": request.payment_capture_policy,
            "entitlement_type": request.entitlement_type,
            "seller_terms_hash": request.seller_terms_hash,
            "offer_terms_hash": request.offer_terms_hash,
            "arbiter_instance": request.arbiter_instance,
            "arbiter_actor": request.arbiter_actor,
            "arbitration_policy_id": request.arbitration_policy_id,
            "arbitration_policy_version": request.arbitration_policy_version,
            "arbitration_window": request.arbitration_window,
            "expires_at": request.expires_at,
        }),
    );
    publish_generated(&state, vec![customer_bound, order_created]).await
}

async fn withdrawn_offer_response<S>(
    store: &S,
    offer_id: &str,
) -> Result<Option<axum::response::Response>, axum::response::Response>
where
    S: EventStore,
{
    let tombstones = store
        .catalog_tombstones()
        .await
        .map_err(|err| store_error_response(err.message, err.code))?;
    Ok(tombstones
        .into_iter()
        .any(|tombstone| tombstone.object_type == "offer" && tombstone.object_id == offer_id)
        .then(|| {
            (
                StatusCode::CONFLICT,
                Json(json!({
                    "code": "OFFER_WITHDRAWN",
                    "error": "offer has been withdrawn",
                    "details": { "offer_id": offer_id },
                })),
            )
                .into_response()
        }))
}

async fn buyer_order_room_id<S, P>(
    state: &AppState<S, P>,
    request: &BuyerOrderCreateRequest,
) -> Result<String, (StatusCode, ValidationError)>
where
    S: EventStore,
    P: MatrixPublisher,
{
    let Some(prefix) = &state.config.order_room_alias_prefix else {
        return request
            .room_id
            .clone()
            .filter(|room_id| !room_id.is_empty())
            .ok_or_else(|| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    ValidationError::new(
                        ValidationCode::MissingRequiredField,
                        "room_id is required when order_room_alias_prefix is not configured",
                    ),
                )
            });
    };
    let alias = order_room_alias(prefix, &request.order_id, &state.config.matrix_server_name);
    let invite_user_ids = order_room_invite_user_ids(&state.config, request)
        .map_err(|err| (StatusCode::UNPROCESSABLE_ENTITY, err))?;
    state
        .publisher
        .ensure_order_room(&alias, &request.order_id, &invite_user_ids)
        .await
        .map_err(|err| (StatusCode::BAD_GATEWAY, err))
}

fn order_room_invite_user_ids(
    config: &ServerConfig,
    request: &BuyerOrderCreateRequest,
) -> Result<Vec<String>, ValidationError> {
    let seller = parse_actor_id(&request.seller_id)?;
    let local_sender = matrix_user_id(config, &config.instance_id);
    let mut invite_user_ids = vec![
        matrix_user_id(config, &seller.instance_id),
        matrix_user_id(config, &request.arbiter_instance),
    ];
    invite_user_ids.retain(|user_id| user_id != &local_sender);
    invite_user_ids.sort();
    invite_user_ids.dedup();
    Ok(invite_user_ids)
}

fn matrix_user_id(config: &ServerConfig, instance_id: &str) -> String {
    format!("@{}:{}", config.appservice_sender_localpart, instance_id)
}

async fn buyer_order_cancel<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(request): Json<OrderActionRequest>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.buyer_token,
        "customer",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    order_event_response(
        &state,
        "io.marketplace.order.cancelled",
        &order_id,
        &request.actor_id,
        json!({ "order_id": order_id }),
    )
    .await
}

async fn buyer_orders<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !bearer_authorized(&headers, &state.config.buyer_token) {
        return role_unauthorized();
    }
    list_orders(&state.store).await
}

async fn buyer_order_show<S, P>(
    State(state): State<AppState<S, P>>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    if !bearer_authorized(&headers, &state.config.buyer_token) {
        return role_unauthorized();
    }
    match state.store.order(&order_id).await {
        Ok(Some(order)) => Json(json!({ "order": order })).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
        )
            .into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn catalog_sellers<S, P>(State(state): State<AppState<S, P>>) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    match state.store.catalog_sellers().await {
        Ok(items) => Json(json!({ "items": items })).into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn catalog_products<S, P>(State(state): State<AppState<S, P>>) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    match state.store.catalog_products().await {
        Ok(items) => Json(json!({ "items": items })).into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn catalog_offers<S, P>(State(state): State<AppState<S, P>>) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    match valid_catalog_offers(&state.store).await {
        Ok(items) => Json(json!({ "items": items })).into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn catalog_offer<S, P>(
    State(state): State<AppState<S, P>>,
    Path(offer_id): Path<String>,
) -> impl IntoResponse
where
    S: EventStore,
    P: MatrixPublisher,
{
    match valid_catalog_offers(&state.store).await {
        Ok(items) => items
            .into_iter()
            .find(|offer| offer.offer_id == offer_id)
            .map(|offer| Json(json!({ "offer": offer })).into_response())
            .unwrap_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"code": "OFFER_NOT_FOUND", "error": "offer not found"})),
                )
                    .into_response()
            }),
        Err(err) => store_error_response(err.message, err.code),
    }
}

async fn valid_catalog_offers<S>(
    store: &S,
) -> Result<Vec<CatalogOfferProjectionRecord>, ValidationError>
where
    S: EventStore,
{
    let seller_ids = store
        .catalog_sellers()
        .await?
        .into_iter()
        .map(|seller| seller.seller_id)
        .collect::<BTreeSet<_>>();
    let product_ids = store
        .catalog_products()
        .await?
        .into_iter()
        .map(|product| product.product_id)
        .collect::<BTreeSet<_>>();
    let tombstones = store.catalog_tombstones().await?;
    let withdrawn_product_ids = tombstones
        .iter()
        .filter(|tombstone| tombstone.object_type == "product")
        .map(|tombstone| tombstone.object_id.as_str())
        .collect::<BTreeSet<_>>();
    let withdrawn_offer_ids = tombstones
        .iter()
        .filter(|tombstone| tombstone.object_type == "offer")
        .map(|tombstone| tombstone.object_id.as_str())
        .collect::<BTreeSet<_>>();

    Ok(store
        .catalog_offers()
        .await?
        .into_iter()
        .filter(|offer| {
            seller_ids.contains(&offer.seller_id)
                && product_ids.contains(&offer.product_id)
                && !withdrawn_product_ids.contains(offer.product_id.as_str())
                && !withdrawn_offer_ids.contains(offer.offer_id.as_str())
        })
        .collect())
}

async fn seller_simple_order_event<S, P>(
    state: AppState<S, P>,
    headers: HeaderMap,
    order_id: String,
    request: OrderActionRequest,
    event_type: &'static str,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    if let Some(response) = authorize_actor(
        &headers,
        &state.config.seller_token,
        "seller",
        &state.config.instance_id,
        &request.actor_id,
    ) {
        return response;
    }
    match order_has_event(&state.store, &order_id, event_type, None).await {
        Ok(true) => return accepted_noop_order_response(&state, &order_id).await,
        Ok(false) => {}
        Err(err) => return store_error_response(err.message, err.code),
    }
    order_event_response(
        &state,
        event_type,
        &order_id,
        &request.actor_id,
        json!({ "order_id": order_id }),
    )
    .await
}

async fn order_event_response<S, P>(
    state: &AppState<S, P>,
    event_type: &str,
    order_id: &str,
    actor_id: &str,
    body: Value,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    let room_id = match state.store.order(order_id).await {
        Ok(Some(order)) => order.room_id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"code": "ORDER_NOT_FOUND", "error": "order not found"})),
            )
                .into_response();
        }
        Err(err) => return store_error_response(err.message, err.code),
    };
    if let Err(err) = state.publisher.ensure_room_joined(&room_id).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
        )
            .into_response();
    }
    publish_generated(
        state,
        vec![marketplace_event(
            &state.config,
            event_type,
            &room_id,
            actor_id,
            body,
        )],
    )
    .await
}

async fn publish_generated<S, P>(
    state: &AppState<S, P>,
    events: Vec<Value>,
) -> axum::response::Response
where
    S: EventStore,
    P: MatrixPublisher,
{
    for event in &events {
        if let Err(err) = validate_event_envelope(event) {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
            )
                .into_response();
        }
    }
    let published = match state.publisher.publish(events).await {
        Ok(events) => events,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
            )
                .into_response();
        }
    };
    let event_ids = published
        .iter()
        .filter_map(|event| {
            event
                .get("event_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let txn_id = format!("api-{}", Ulid::new());
    let room_id = published
        .first()
        .and_then(|event| event.get("room_id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if state.publisher.ingest_after_publish() {
        match ingest_transaction(
            &state.store,
            txn_id,
            AppServiceTransaction {
                events: published.clone(),
            },
        )
        .await
        {
            Ok(()) => {}
            Err(response) => return response,
        }
        return (
            StatusCode::ACCEPTED,
            Json(json!({ "status": "accepted", "room_id": room_id, "event_ids": event_ids })),
        )
            .into_response();
    }
    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "submitted", "room_id": room_id, "event_ids": event_ids })),
    )
        .into_response()
}

fn marketplace_event(
    config: &ServerConfig,
    event_type: &str,
    room_id: &str,
    actor_id: &str,
    body: Value,
) -> Value {
    let local = Ulid::new().to_string();
    json!({
        "type": event_type,
        "room_id": room_id,
        "event_id": format!("$morpheus-{}:{}", local.to_ascii_lowercase(), config.matrix_server_name),
        "sender": format!("@{}:{}", config.appservice_sender_localpart, config.matrix_server_name),
        "origin_server_ts": Utc::now().timestamp_millis(),
        "content": {
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "protocol_event_id": format!("evt:{}:{}", config.instance_id, local),
            "created_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "issuer": {
                "instance_id": config.instance_id,
                "actor_id": actor_id,
                "matrix_user_id": format!("@{}:{}", config.appservice_sender_localpart, config.matrix_server_name),
            },
            "critical": [],
            "body": body,
        },
    })
}

pub fn watcher_payment_event(
    config: &ServerConfig,
    room_id: &str,
    event_type: &str,
    body: Value,
) -> Value {
    marketplace_event(
        config,
        event_type,
        room_id,
        &format!("arbiter:{}:EVMWATCHER", config.instance_id),
        body,
    )
}

fn authorize_actor(
    headers: &HeaderMap,
    token: &str,
    kind: &str,
    instance_id: &str,
    actor_id: &str,
) -> Option<axum::response::Response> {
    if !bearer_authorized(headers, token) {
        return Some(role_unauthorized());
    }
    match parse_actor_id(actor_id) {
        Ok(actor) if actor.kind == kind && actor.instance_id == instance_id => None,
        Ok(_) => Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "code": "ACTOR_FORBIDDEN",
                    "error": "actor is outside the authorized local role scope",
                })),
            )
                .into_response(),
        ),
        Err(err) => Some(
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
            )
                .into_response(),
        ),
    }
}

fn bearer_authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

fn role_unauthorized() -> axum::response::Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized", "code": "ROLE_UNAUTHORIZED" })),
    )
        .into_response()
}

fn admin_authorized(headers: &HeaderMap, token: &str) -> bool {
    bearer_authorized(headers, token)
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

async fn projection_summary<S>(store: &S) -> axum::response::Response
where
    S: EventStore,
{
    let catalog = match catalog_summary(store).await {
        Ok(catalog) => catalog,
        Err(response) => return response,
    };
    let orders = match store.orders().await {
        Ok(items) => items.len(),
        Err(err) => return store_error_response(err.message, err.code),
    };
    let payments = match store.payments().await {
        Ok(items) => items.len(),
        Err(err) => return store_error_response(err.message, err.code),
    };
    let entitlements = match store.entitlements().await {
        Ok(items) => items.len(),
        Err(err) => return store_error_response(err.message, err.code),
    };
    let disputes = match store.disputes().await {
        Ok(items) => items.len(),
        Err(err) => return store_error_response(err.message, err.code),
    };
    let arbitration_rulings = match store.arbitration_rulings().await {
        Ok(items) => items.len(),
        Err(err) => return store_error_response(err.message, err.code),
    };
    Json(json!({
        "catalog": catalog,
        "orders": orders,
        "payments": payments,
        "entitlements": entitlements,
        "disputes": disputes,
        "arbitration_rulings": arbitration_rulings,
    }))
    .into_response()
}

async fn list_orders<S>(store: &S) -> axum::response::Response
where
    S: EventStore,
{
    match (store.orders().await, store.payments().await) {
        (Ok(orders), Ok(payments)) => {
            let orders = enrich_orders_with_payments(orders, payments);
            Json(json!({ "orders": orders })).into_response()
        }
        (Err(err), _) => store_error_response(err.message, err.code),
        (_, Err(err)) => store_error_response(err.message, err.code),
    }
}

async fn enriched_order<S: EventStore>(
    store: &S,
    order_id: &str,
) -> Result<Option<Value>, ValidationError> {
    let Some(order) = store.order(order_id).await? else {
        return Ok(None);
    };
    let payments = store.payments().await?;
    Ok(enrich_orders_with_payments(vec![order], payments)
        .into_iter()
        .next())
}

fn enrich_orders_with_payments(
    orders: Vec<OrderProjectionRecord>,
    payments: Vec<PaymentProjectionRecord>,
) -> Vec<Value> {
    let mut payments_by_order = HashMap::new();
    for payment in payments {
        payments_by_order.insert(payment.order_id.clone(), payment);
    }
    orders
        .into_iter()
        .map(|order| {
            let mut value = json!(order);
            if let Some(payment) = payments_by_order.get(
                value
                    .get("order_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ) {
                value["payment"] = json!(payment);
            }
            value
        })
        .collect()
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
