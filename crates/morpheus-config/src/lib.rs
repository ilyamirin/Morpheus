use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorpheusConfig {
    pub instance: InstanceConfig,
    pub appservice: AppServiceConfig,
    pub database: DatabaseConfig,
    pub admin: AdminConfig,
    pub auth: AuthConfig,
    pub allowlist: Option<AllowlistConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub instance_id: String,
    pub matrix_server_name: String,
    pub application_service_id: String,
    pub catalog_room_id: String,
    pub protocol_versions: Vec<String>,
    pub payment_adapters: Vec<String>,
    pub entitlement_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServiceConfig {
    pub homeserver_url: String,
    pub url: String,
    pub sender_localpart: String,
    pub namespace_prefix: String,
    pub homeserver_token: String,
    pub appservice_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminConfig {
    pub bind: String,
    pub bearer_token_env: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthConfig {
    pub seller_token_env: String,
    pub buyer_token_env: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowlistConfig {
    pub instances: Vec<AllowlistInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowlistInstance {
    pub instance_id: String,
    pub capabilities: Vec<String>,
    pub status: String,
}

pub fn load_config(path: impl AsRef<Path>) -> Result<MorpheusConfig> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let config: MorpheusConfig = toml::from_str(&text).context("parsing TOML config")?;
    validate_config(&config)?;
    Ok(config)
}

pub fn validate_config(config: &MorpheusConfig) -> Result<()> {
    anyhow::ensure!(
        !config.instance.instance_id.is_empty(),
        "instance_id is required"
    );
    anyhow::ensure!(
        !config.instance.matrix_server_name.is_empty(),
        "matrix_server_name is required"
    );
    anyhow::ensure!(
        !config.instance.application_service_id.is_empty(),
        "application_service_id is required"
    );
    anyhow::ensure!(
        config.instance.catalog_room_id.starts_with('!'),
        "catalog_room_id must be a Matrix room id"
    );
    anyhow::ensure!(
        config
            .instance
            .protocol_versions
            .iter()
            .any(|version| version == "0.1"),
        "protocol_versions must include 0.1"
    );
    anyhow::ensure!(
        !config.instance.payment_adapters.is_empty(),
        "payment_adapters must not be empty"
    );
    anyhow::ensure!(
        !config.instance.entitlement_types.is_empty(),
        "entitlement_types must not be empty"
    );
    anyhow::ensure!(
        !config.appservice.homeserver_url.is_empty(),
        "homeserver_url is required"
    );
    anyhow::ensure!(
        !config.appservice.url.is_empty(),
        "appservice url is required"
    );
    anyhow::ensure!(
        !config.appservice.sender_localpart.is_empty(),
        "sender_localpart is required"
    );
    anyhow::ensure!(
        !config.appservice.namespace_prefix.is_empty(),
        "namespace_prefix is required"
    );
    anyhow::ensure!(
        !config.appservice.homeserver_token.is_empty(),
        "homeserver_token is required"
    );
    anyhow::ensure!(
        !config.appservice.appservice_token.is_empty(),
        "appservice_token is required"
    );
    anyhow::ensure!(!config.database.url.is_empty(), "database url is required");
    anyhow::ensure!(!config.admin.bind.is_empty(), "admin bind is required");
    anyhow::ensure!(
        !config.admin.bearer_token_env.is_empty(),
        "admin bearer_token_env is required"
    );
    anyhow::ensure!(
        !config.auth.seller_token_env.is_empty(),
        "auth seller_token_env is required"
    );
    anyhow::ensure!(
        !config.auth.buyer_token_env.is_empty(),
        "auth buyer_token_env is required"
    );
    if let Some(allowlist) = &config.allowlist {
        for entry in &allowlist.instances {
            anyhow::ensure!(
                !entry.instance_id.is_empty(),
                "allowlist instance_id is required"
            );
            anyhow::ensure!(
                !entry.capabilities.is_empty(),
                "allowlist capabilities are required"
            );
            anyhow::ensure!(
                entry.status == "active" || entry.status == "revoked",
                "allowlist status must be active or revoked"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> MorpheusConfig {
        MorpheusConfig {
            instance: InstanceConfig {
                instance_id: "shop.example".into(),
                matrix_server_name: "shop.example".into(),
                application_service_id: "io.marketplace.shop".into(),
                catalog_room_id: "!catalog:shop.example".into(),
                protocol_versions: vec!["0.1".into()],
                payment_adapters: vec!["mock".into()],
                entitlement_types: vec!["external_entitlement".into()],
            },
            appservice: AppServiceConfig {
                homeserver_url: "http://localhost:8008".into(),
                url: "http://morpheus-shop:8080".into(),
                sender_localpart: "market".into(),
                namespace_prefix: "market_".into(),
                homeserver_token: "hs-token".into(),
                appservice_token: "as-token".into(),
            },
            database: DatabaseConfig {
                url: "sqlite::memory:".into(),
            },
            admin: AdminConfig {
                bind: "127.0.0.1:8080".into(),
                bearer_token_env: "MORPHEUS_ADMIN_TOKEN".into(),
            },
            auth: AuthConfig {
                seller_token_env: "MORPHEUS_SELLER_TOKEN".into(),
                buyer_token_env: "MORPHEUS_BUYER_TOKEN".into(),
            },
            allowlist: Some(AllowlistConfig {
                instances: vec![AllowlistInstance {
                    instance_id: "shop.example".into(),
                    capabilities: vec!["catalog".into()],
                    status: "active".into(),
                }],
            }),
        }
    }

    #[test]
    fn validates_required_fields() {
        assert!(validate_config(&valid_config()).is_ok());

        let mut config = valid_config();
        config.appservice.url.clear();
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "appservice url is required"
        );

        let mut config = valid_config();
        config.auth.seller_token_env.clear();
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "auth seller_token_env is required"
        );
    }

    #[test]
    fn validates_allowlist_entries() {
        let mut config = valid_config();
        config.allowlist.as_mut().unwrap().instances[0]
            .capabilities
            .clear();
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "allowlist capabilities are required"
        );
    }
}
