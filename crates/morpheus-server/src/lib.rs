use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post, put},
};
use chrono::Utc;
use morpheus_api::{
    BuyerOrderCreateRequest, EntitlementGrantRequest, OfferUpsertRequest, OfferWithdrawRequest,
    OrderAcceptRequest, OrderActionRequest, PaymentCaptureRequest, PaymentIntentRequest,
    ProductUpsertRequest, SellerAnnounceRequest,
};
use morpheus_matrix::{AppServiceTransaction, validate_transaction_event_ids};
use morpheus_protocol::{ValidationCode, ValidationError, parse_actor_id, validate_event_envelope};
use morpheus_store::{
    AppServiceTransactionRecord, CatalogOfferProjectionRecord, CatalogProductRecord,
    CatalogSellerRecord, EventStore, ProjectionErrorRecord, RawMatrixEventRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ulid::Ulid;

mod context_validation;
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
        true
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCatalogSource {
    pub instance_id: String,
    pub morpheus_url: String,
}

pub async fn sync_remote_catalog_once<S>(
    store: &S,
    source: &RemoteCatalogSource,
) -> Result<(), ValidationError>
where
    S: EventStore,
{
    let client = reqwest::Client::new();
    let sellers = fetch_catalog_items::<CatalogSellerRecord>(
        &client,
        &source.morpheus_url,
        "/api/v1/catalog/sellers",
    )
    .await?;
    for seller in sellers {
        store
            .upsert_catalog_seller(
                &seller.seller_id,
                &seller.issuer_instance,
                &seller.status,
                seller.body,
            )
            .await?;
    }

    let products = fetch_catalog_items::<CatalogProductRecord>(
        &client,
        &source.morpheus_url,
        "/api/v1/catalog/products",
    )
    .await?;
    for product in products {
        store
            .upsert_catalog_product(
                &product.product_id,
                &product.seller_id,
                product.revision,
                product.body,
            )
            .await?;
    }

    let offers = fetch_catalog_items::<CatalogOfferProjectionRecord>(
        &client,
        &source.morpheus_url,
        "/api/v1/catalog/offers",
    )
    .await?;
    for offer in offers {
        store.upsert_catalog_offer(offer).await?;
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
        .route("/ui/seller", get(ui_seller))
        .route("/ui/buyer", get(ui_buyer))
        .route("/ui/assets/favicon.svg", get(ui_favicon_svg))
        .route("/ui/assets/app.css", get(ui_app_css))
        .route("/ui/assets/app.js", get(ui_app_js))
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
            "/admin/rooms/bootstrap",
            post(admin_rooms_bootstrap::<S, P>),
        )
        .route(
            "/admin/orders/{order_id}/replay",
            post(admin_order_replay::<S, P>),
        )
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

async fn ui_seller() -> impl IntoResponse {
    Html(include_str!("../ui/seller.html"))
}

async fn ui_buyer() -> impl IntoResponse {
    Html(include_str!("../ui/buyer.html"))
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
                "media": [],
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
    seller_simple_order_event(
        state,
        headers,
        order_id,
        request,
        "io.marketplace.order.completed",
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
    if let Err(err) = state.publisher.ensure_room_joined(&room_id).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "code": err.code, "error": err.message, "details": err.details })),
        )
            .into_response();
    }
    publish_generated(
        &state,
        vec![
            marketplace_event(
                &state.config,
                "io.marketplace.payment.authorized",
                &room_id,
                &request.actor_id,
                json!({
                    "order_id": order_id,
                    "payment_id": request.payment_id,
                }),
            ),
            marketplace_event(
                &state.config,
                "io.marketplace.payment.captured",
                &room_id,
                &request.actor_id,
                json!({
                    "order_id": order_id,
                    "payment_id": request.payment_id,
                    "adapter": request.adapter,
                    "amount": request.amount,
                    "currency": request.currency,
                    "provider_ref": request.provider_ref,
                    "evidence": request.evidence,
                }),
            ),
        ],
    )
    .await
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
    match state.store.catalog_offers().await {
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
    match state.store.catalog_offers().await {
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
    match store.orders().await {
        Ok(orders) => Json(json!({ "orders": orders })).into_response(),
        Err(err) => store_error_response(err.message, err.code),
    }
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
