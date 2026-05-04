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
sender_localpart = "market"
namespace_prefix = "market_"
homeserver_token = "hs-token"
appservice_token = "as-token"

[database]
url = "sqlite::memory:"

[admin]
bind = "127.0.0.1:8080"
bearer_token_env = "MORPHEUS_ADMIN_TOKEN"

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
