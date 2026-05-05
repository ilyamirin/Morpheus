use morpheus_matrix::{
    AppServiceTransaction, ApplicationServiceContext, event_ids, generate_synapse_registration,
    validate_application_service_sender, validate_transaction_event_ids,
};
use morpheus_protocol::ValidationCode;
use serde_json::json;

#[test]
fn appservice_sender_namespace_accepts_sender_and_exclusive_children() {
    let context = ApplicationServiceContext {
        instance_id: "shop.example".into(),
        server_name: "shop.example".into(),
        exclusive_user_localpart: "market".into(),
    };

    validate_application_service_sender("@market:shop.example", &context).unwrap();
    validate_application_service_sender("@market_seller:shop.example", &context).unwrap();

    let err = validate_application_service_sender("@seller:shop.example", &context).unwrap_err();
    assert_eq!(err.code, ValidationCode::UnauthorizedSender);
}

#[test]
fn synapse_registration_matches_configured_namespace() {
    let registration = generate_synapse_registration(
        "morpheus-shop",
        "http://morpheus-server:8080",
        "as-token",
        "hs-token",
        "market",
        "market",
    );

    assert_eq!(registration.id, "morpheus-shop");
    assert_eq!(registration.sender_localpart, "market");
    assert!(!registration.rate_limited);
    assert_eq!(registration.namespaces.users.len(), 1);
    assert!(registration.namespaces.users[0].exclusive);
    assert_eq!(registration.namespaces.users[0].regex, "@market.*");
    assert!(registration.namespaces.aliases.is_empty());
    assert!(registration.namespaces.rooms.is_empty());
}

#[test]
fn transaction_event_ids_preserve_wire_order() {
    let transaction = AppServiceTransaction {
        events: vec![
            json!({"event_id": "$one:shop.example"}),
            json!({"event_id": "$two:shop.example"}),
        ],
    };

    assert_eq!(
        event_ids(&transaction),
        vec!["$one:shop.example", "$two:shop.example"]
    );
    assert_eq!(
        validate_transaction_event_ids(&transaction).unwrap(),
        vec!["$one:shop.example", "$two:shop.example"]
    );
}

#[test]
fn transaction_event_ids_reject_missing_or_non_string_ids() {
    for event in [json!({"event_id": 42}), json!({})] {
        let transaction = AppServiceTransaction {
            events: vec![json!({"event_id": "$one:shop.example"}), event],
        };

        let err = validate_transaction_event_ids(&transaction).unwrap_err();
        assert!(
            err.to_string().contains("event 1"),
            "unexpected error: {err}"
        );
    }
}
