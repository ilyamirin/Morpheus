use std::{fs, process::Command};

#[test]
fn validates_local_toml_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("local.toml");
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
entitlement_types = ["booking_slot"]

[appservice]
homeserver_url = "http://localhost:8008"
url = "http://morpheus-test:9000"
sender_localpart = "market"
namespace_prefix = "market_"
homeserver_token = "hs-token"
appservice_token = "as-token"

[database]
url = "sqlite::memory:"

[admin]
bind = "127.0.0.1:8080"
bearer_token_env = "MORPHEUS_ADMIN_TOKEN"

[auth]
seller_token_env = "MORPHEUS_SELLER_TOKEN"
buyer_token_env = "MORPHEUS_BUYER_TOKEN"

[[allowlist.instances]]
instance_id = "shop.example"
capabilities = ["catalog", "orders", "indexing"]
status = "active"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_morpheus"))
        .args(["config", "validate", "--config"])
        .arg(config)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn synapse_registration_uses_configured_appservice_url() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("local.toml");
    let out = dir.path().join("registration.yaml");
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
entitlement_types = ["booking_slot"]

[appservice]
homeserver_url = "http://localhost:8008"
url = "http://morpheus-test:9000"
sender_localpart = "market"
namespace_prefix = "market_"
homeserver_token = "hs-token"
appservice_token = "as-token"

[database]
url = "sqlite::memory:"

[admin]
bind = "127.0.0.1:8080"
bearer_token_env = "MORPHEUS_ADMIN_TOKEN"

[auth]
seller_token_env = "MORPHEUS_SELLER_TOKEN"
buyer_token_env = "MORPHEUS_BUYER_TOKEN"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_morpheus"))
        .args(["synapse", "registration", "--config"])
        .arg(config)
        .args(["--out"])
        .arg(&out)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(out)
            .unwrap()
            .contains("url: http://morpheus-test:9000")
    );
}

#[test]
fn migrates_sqlite_database_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("morpheus.sqlite");
    let database_url = format!("sqlite://{}", db.display());

    let output = Command::new(env!("CARGO_BIN_EXE_morpheus"))
        .args(["db", "migrate", "--database-url", &database_url])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "database migrated\n"
    );

    let bytes = fs::read(db).unwrap();
    assert!(
        bytes
            .windows(b"marketplace_events".len())
            .any(|window| window == b"marketplace_events"),
        "sqlite schema was not written to database file"
    );
}

#[test]
fn admin_health_uses_server_url_and_token() {
    let output = Command::new(env!("CARGO_BIN_EXE_morpheus"))
        .env("MORPHEUS_CLI_DRY_RUN_REQUEST", "1")
        .args([
            "--server-url",
            "http://127.0.0.1:18080",
            "--token",
            "admin-token",
            "admin",
            "health",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(request["method"], "GET");
    assert_eq!(request["path"], "/admin/health");
    assert_eq!(request["url"], "http://127.0.0.1:18080/admin/health");
    assert_eq!(request["authorization"], "Bearer admin-token");
}

#[test]
fn seller_offer_upsert_uses_seller_token_env_and_json_body() {
    let body = r#"{"seller_id":"seller:shop.example:01JSELLER","product_id":"prod:shop.example:01JPROD","offer_id":"offer:shop.example:01JOFFER","revision":1,"price":{"amount":"10.00","currency":"USD"},"payment_capture_policy":"before_entitlement","seller_terms_hash":"sha256:1111111111111111111111111111111111111111111111111111111111111111","offer_terms_hash":"sha256:2222222222222222222222222222222222222222222222222222222222222222","entitlement_type":"external_entitlement","availability_mode":"unlimited"}"#;
    let output = Command::new(env!("CARGO_BIN_EXE_morpheus"))
        .env("MORPHEUS_CLI_DRY_RUN_REQUEST", "1")
        .env("MORPHEUS_SELLER_TOKEN", "seller-token")
        .args([
            "--server-url",
            "http://127.0.0.1:18080",
            "seller",
            "offer",
            "upsert",
            "--json",
            body,
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(request["method"], "POST");
    assert_eq!(request["path"], "/api/v1/seller/offers");
    assert_eq!(request["authorization"], "Bearer seller-token");
    assert_eq!(request["body"]["offer_id"], "offer:shop.example:01JOFFER");
}

#[test]
fn buyer_order_create_token_flag_overrides_env() {
    let output = Command::new(env!("CARGO_BIN_EXE_morpheus"))
        .env("MORPHEUS_CLI_DRY_RUN_REQUEST", "1")
        .env("MORPHEUS_BUYER_TOKEN", "env-token")
        .args([
            "--server-url",
            "http://127.0.0.1:18080",
            "--token",
            "flag-token",
            "buyer",
            "order",
            "create",
            "--json",
            r#"{"customer_id":"customer:shop.example:01JCUST"}"#,
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(request["authorization"], "Bearer flag-token");
    assert_eq!(
        request["body"]["customer_id"],
        "customer:shop.example:01JCUST"
    );
}
