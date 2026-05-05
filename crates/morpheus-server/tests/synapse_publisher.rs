use morpheus_server::{
    catalog_alias_localpart, matrix_create_room_body, matrix_create_room_url,
    matrix_room_alias_url, matrix_send_body, matrix_send_url,
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
