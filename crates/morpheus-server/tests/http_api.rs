use axum::body::Body;
use http::{Request, StatusCode};
use morpheus_server::{ServerConfig, build_router};
use morpheus_store::InMemoryEventStore;
use tower::ServiceExt;

#[tokio::test]
async fn transaction_endpoint_requires_synapse_token() {
    let app = build_router(
        ServerConfig {
            homeserver_token: "hs-token".into(),
            admin_token: "admin-token".into(),
        },
        InMemoryEventStore::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/_matrix/app/v1/transactions/txn-1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn transaction_endpoint_accepts_valid_token() {
    let app = build_router(
        ServerConfig {
            homeserver_token: "hs-token".into(),
            admin_token: "admin-token".into(),
        },
        InMemoryEventStore::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/_matrix/app/v1/transactions/txn-1?access_token=hs-token")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"events":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
