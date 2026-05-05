use std::{fs, process::Command};

#[test]
fn server_binary_requires_admin_token_env_before_database_connect() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("server.toml");
    fs::write(
        &config,
        r#"
[instance]
instance_id = "shop.example"
matrix_server_name = "shop.example"
application_service_id = "io.marketplace.shop"
catalog_room_id = "!catalog:shop.example"
protocol_versions = ["0.1"]
payment_adapters = ["mock"]
entitlement_types = ["external_entitlement"]

[appservice]
homeserver_url = "http://localhost:8008"
url = "http://morpheus-shop:8080"
sender_localpart = "market"
namespace_prefix = "market_"
homeserver_token = "hs-token"
appservice_token = "as-token"

[database]
url = "postgres://morpheus:morpheus@127.0.0.1:1/morpheus"

[admin]
bind = "127.0.0.1:0"
bearer_token_env = "MORPHEUS_TEST_ADMIN_TOKEN_MISSING"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_morpheus-server"))
        .args(["--config"])
        .arg(config)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("missing admin bearer token env MORPHEUS_TEST_ADMIN_TOKEN_MISSING")
    );
}
