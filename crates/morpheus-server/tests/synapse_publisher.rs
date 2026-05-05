use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use morpheus_server::{
    MatrixPublisher, SynapseMatrixPublisher, catalog_alias_localpart,
    matrix_create_order_room_body, matrix_create_room_body, matrix_create_room_url,
    matrix_join_room_body, matrix_join_room_url, matrix_room_alias_url,
    matrix_room_member_state_url, matrix_send_body, matrix_send_url, order_room_alias,
};
use serde_json::json;

#[test]
fn synapse_send_url_uses_room_event_type_txn_token_and_user_id() {
    let url = matrix_send_url(
        "http://synapse-books:8008/",
        "!catalog:books.example",
        "io.marketplace.offer.upserted",
        "txn-1",
        "as-token",
        "@market:books.example",
    )
    .unwrap();

    assert_eq!(
        url.as_str(),
        "http://synapse-books:8008/_matrix/client/v3/rooms/!catalog:books.example/send/io.marketplace.offer.upserted/txn-1?access_token=as-token&user_id=%40market%3Abooks.example"
    );
}

#[test]
fn synapse_send_body_contains_only_matrix_event_content() {
    let event = json!({
        "type": "io.marketplace.offer.upserted",
        "room_id": "!catalog:books.example",
        "event_id": "$local:books.example",
        "sender": "@market:books.example",
        "content": {
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "body": {"offer_id": "offer:books.example:01JOFFER"}
        }
    });

    assert_eq!(
        matrix_send_body(&event).unwrap(),
        json!({
            "protocol": "io.marketplace",
            "protocol_version": "0.1",
            "body": {"offer_id": "offer:books.example:01JOFFER"}
        })
    );
}

#[test]
fn room_bootstrap_uses_catalog_alias_and_appservice_user() {
    assert_eq!(
        catalog_alias_localpart("#marketplace-catalog:books.example").unwrap(),
        "marketplace-catalog"
    );
    assert_eq!(
        matrix_create_room_url(
            "http://synapse-books:8008",
            "as-token",
            "@market:books.example",
        )
        .unwrap()
        .as_str(),
        "http://synapse-books:8008/_matrix/client/v3/createRoom?access_token=as-token&user_id=%40market%3Abooks.example"
    );
    assert_eq!(
        matrix_room_alias_url(
            "http://synapse-books:8008",
            "#marketplace-catalog:books.example",
            "as-token",
            "@market:books.example",
        )
        .unwrap()
        .as_str(),
        "http://synapse-books:8008/_matrix/client/v3/directory/room/%23marketplace-catalog:books.example?access_token=as-token&user_id=%40market%3Abooks.example"
    );
    assert_eq!(
        matrix_create_room_body("#marketplace-catalog:books.example", "books.example").unwrap(),
        json!({
            "visibility": "public",
            "preset": "public_chat",
            "room_alias_name": "marketplace-catalog",
            "name": "Morpheus catalog books.example",
            "topic": "Morpheus marketplace catalog for books.example",
            "creation_content": {"m.federate": true}
        })
    );
}

#[test]
fn order_room_alias_sanitizes_order_id() {
    assert_eq!(
        order_room_alias(
            "#marketplace-order-",
            "ord:books.example:UIORDER050501",
            "books.example",
        ),
        "#marketplace-order-ord-books-example-uiorder050501:books.example"
    );
}

#[test]
fn order_room_bootstrap_uses_private_alias_and_invites_participants() {
    assert_eq!(
        matrix_create_order_room_body(
            "#marketplace-order-ord-books-example-uiorder050501:books.example",
            "ord:books.example:UIORDER050501",
            &[
                "@market:fashion.example".to_string(),
                "@market:cases.example".to_string()
            ],
        )
        .unwrap(),
        json!({
            "visibility": "private",
            "preset": "private_chat",
            "room_alias_name": "marketplace-order-ord-books-example-uiorder050501",
            "name": "Morpheus order ord:books.example:UIORDER050501",
            "topic": "Morpheus marketplace order ord:books.example:UIORDER050501",
            "invite": ["@market:fashion.example", "@market:cases.example"],
            "creation_content": {"m.federate": true}
        })
    );
}

#[test]
fn room_join_helpers_use_appservice_user() {
    assert_eq!(
        matrix_join_room_url(
            "http://synapse-fashion:8008",
            "!orderroom:books.example",
            "fashion-as-token",
            "@market:fashion.example",
        )
        .unwrap()
        .as_str(),
        "http://synapse-fashion:8008/_matrix/client/v3/join/!orderroom:books.example?access_token=fashion-as-token&user_id=%40market%3Afashion.example"
    );
    assert_eq!(matrix_join_room_body(), json!({}));
    assert_eq!(
        matrix_room_member_state_url(
            "http://synapse-books:8008",
            "!orderroom:books.example",
            "@market:fashion.example",
            "books-as-token",
            "@market:books.example",
        )
        .unwrap()
        .as_str(),
        "http://synapse-books:8008/_matrix/client/v3/rooms/!orderroom:books.example/state/m.room.member/@market:fashion.example?access_token=books-as-token&user_id=%40market%3Abooks.example"
    );
}

#[tokio::test]
async fn ensure_order_room_does_not_retry_without_invites_on_create_failure() {
    async fn create_room(
        State(calls): State<Arc<AtomicUsize>>,
    ) -> (StatusCode, Json<serde_json::Value>) {
        calls.fetch_add(1, Ordering::SeqCst);
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "errcode": "M_UNKNOWN",
                "error": "Can't connect to server fashion.example"
            })),
        )
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/_matrix/client/v3/createRoom", post(create_room))
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let publisher = SynapseMatrixPublisher::new(
        format!("http://{addr}"),
        "as-token".into(),
        "@market:books.example".into(),
    );
    let err = publisher
        .ensure_order_room(
            "#marketplace-order-ord-books-example-01:books.example",
            "ord:books.example:01",
            &["@market:fashion.example".to_string()],
        )
        .await
        .unwrap_err();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(err.message.contains("createRoom returned 502 Bad Gateway"));
    assert!(err.message.contains("fashion.example"));
}
