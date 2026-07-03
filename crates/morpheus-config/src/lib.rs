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
    #[serde(default)]
    pub payments: Option<PaymentsConfig>,
    pub allowlist: Option<AllowlistConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub instance_id: String,
    pub matrix_server_name: String,
    pub application_service_id: String,
    pub catalog_room_id: String,
    pub catalog_room_alias: Option<String>,
    pub order_room_alias_prefix: Option<String>,
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
    #[serde(default)]
    pub bootstrap_rooms: bool,
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
pub struct PaymentsConfig {
    #[serde(default)]
    pub evm_escrow: Option<EvmEscrowConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowConfig {
    pub enabled: bool,
    pub chain_id: u64,
    pub rpc_url_env: String,
    pub escrow_contract: String,
    pub default_token: String,
    pub confirmations: u64,
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub start_block: Option<u64>,
    #[serde(default)]
    pub max_scan_blocks: Option<u64>,
    #[serde(default)]
    pub rescan_depth: Option<u64>,
    #[serde(default)]
    pub deployments_path: Option<String>,
    pub tokens: Vec<EvmEscrowTokenConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowTokenConfig {
    pub symbol: String,
    pub contract: String,
    pub decimals: u8,
    pub currency: String,
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
    pub catalog_room_alias: Option<String>,
    pub homeserver_url: Option<String>,
    pub morpheus_url: Option<String>,
}

pub fn load_config(path: impl AsRef<Path>) -> Result<MorpheusConfig> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let config: MorpheusConfig = toml::from_str(&text).context("parsing TOML config")?;
    validate_config(&config)?;
    Ok(config)
}

fn is_evm_address(value: &str) -> bool {
    value.len() == 42
        && value.starts_with("0x")
        && value[2..].chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_nonzero_evm_address(value: &str) -> bool {
    is_evm_address(value) && !value[2..].chars().all(|ch| ch == '0')
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
    if let Some(alias) = &config.instance.catalog_room_alias {
        anyhow::ensure!(
            alias.starts_with('#') && alias.contains(':'),
            "catalog_room_alias must be a Matrix room alias"
        );
    }
    if config.appservice.bootstrap_rooms {
        anyhow::ensure!(
            config.instance.catalog_room_alias.is_some(),
            "catalog_room_alias is required when bootstrap_rooms is true"
        );
    }
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
    if let Some(evm) = config
        .payments
        .as_ref()
        .and_then(|payments| payments.evm_escrow.as_ref())
        .filter(|evm| evm.enabled)
    {
        anyhow::ensure!(
            config
                .instance
                .payment_adapters
                .iter()
                .any(|adapter| adapter == "evm_escrow"),
            "evm_escrow payment config requires instance.payment_adapters to include evm_escrow"
        );
        anyhow::ensure!(evm.chain_id > 0, "evm_escrow chain_id is required");
        anyhow::ensure!(
            !evm.rpc_url_env.is_empty(),
            "evm_escrow rpc_url_env is required"
        );
        anyhow::ensure!(
            is_nonzero_evm_address(&evm.escrow_contract),
            "evm_escrow escrow_contract must be an EVM address"
        );
        anyhow::ensure!(
            is_nonzero_evm_address(&evm.default_token),
            "evm_escrow default_token must be an EVM address"
        );
        anyhow::ensure!(
            evm.confirmations > 0,
            "evm_escrow confirmations must be positive"
        );
        anyhow::ensure!(
            evm.poll_interval_secs > 0,
            "evm_escrow poll_interval_secs must be positive"
        );
        if let Some(max_scan_blocks) = evm.max_scan_blocks {
            anyhow::ensure!(
                max_scan_blocks > 0,
                "evm_escrow max_scan_blocks must be positive"
            );
        }
        if let Some(rescan_depth) = evm.rescan_depth {
            anyhow::ensure!(rescan_depth > 0, "evm_escrow rescan_depth must be positive");
        }
        anyhow::ensure!(
            !evm.tokens.is_empty(),
            "evm_escrow tokens must not be empty"
        );
        anyhow::ensure!(
            evm.tokens
                .iter()
                .any(|token| token.contract.eq_ignore_ascii_case(&evm.default_token)),
            "evm_escrow default_token must be listed in tokens"
        );
        for token in &evm.tokens {
            anyhow::ensure!(
                !token.symbol.is_empty(),
                "evm_escrow token symbol is required"
            );
            anyhow::ensure!(
                is_nonzero_evm_address(&token.contract),
                "evm_escrow token contract must be an EVM address"
            );
            anyhow::ensure!(
                token.decimals <= 36,
                "evm_escrow token decimals must be <= 36"
            );
            anyhow::ensure!(
                !token.currency.is_empty(),
                "evm_escrow token currency is required"
            );
        }
    }
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
            if let Some(alias) = &entry.catalog_room_alias {
                anyhow::ensure!(
                    alias.starts_with('#') && alias.contains(':'),
                    "allowlist catalog_room_alias must be a Matrix room alias"
                );
            }
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
                catalog_room_alias: Some("#marketplace-catalog:shop.example".into()),
                order_room_alias_prefix: Some("#marketplace-order-".into()),
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
                bootstrap_rooms: true,
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
            payments: None,
            allowlist: Some(AllowlistConfig {
                instances: vec![AllowlistInstance {
                    instance_id: "shop.example".into(),
                    capabilities: vec!["catalog".into()],
                    status: "active".into(),
                    catalog_room_alias: Some("#marketplace-catalog:shop.example".into()),
                    homeserver_url: Some("http://localhost:8008".into()),
                    morpheus_url: Some("http://localhost:8080".into()),
                }],
            }),
        }
    }

    fn valid_evm_escrow_config() -> EvmEscrowConfig {
        EvmEscrowConfig {
            enabled: true,
            chain_id: 31337,
            rpc_url_env: "MORPHEUS_EVM_RPC_URL".into(),
            escrow_contract: "0x0000000000000000000000000000000000000001".into(),
            default_token: "0x0000000000000000000000000000000000000002".into(),
            confirmations: 1,
            poll_interval_secs: 2,
            start_block: Some(0),
            max_scan_blocks: Some(100),
            rescan_depth: Some(3),
            deployments_path: Some("contracts/deployments/local.json".into()),
            tokens: vec![EvmEscrowTokenConfig {
                symbol: "USDC".into(),
                contract: "0x0000000000000000000000000000000000000002".into(),
                decimals: 6,
                currency: "USDC".into(),
            }],
        }
    }

    fn config_with_evm_escrow(evm_escrow: EvmEscrowConfig) -> MorpheusConfig {
        let mut config = valid_config();
        config.instance.payment_adapters = vec!["mock".into(), "evm_escrow".into()];
        config.payments = Some(PaymentsConfig {
            evm_escrow: Some(evm_escrow),
        });
        config
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

        let mut config = valid_config();
        config.instance.catalog_room_alias = None;
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "catalog_room_alias is required when bootstrap_rooms is true"
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

    #[test]
    fn validates_evm_escrow_config_when_enabled() {
        let config = config_with_evm_escrow(valid_evm_escrow_config());

        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn validates_evm_escrow_default_token_case_insensitively() {
        let mut evm = valid_evm_escrow_config();
        evm.default_token = "0x000000000000000000000000000000000000000a".into();
        evm.tokens[0].contract = "0x000000000000000000000000000000000000000A".into();
        let config = config_with_evm_escrow(evm);

        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn validates_evm_escrow_scan_bounds_when_enabled() {
        let mut evm = valid_evm_escrow_config();
        evm.max_scan_blocks = Some(250);
        evm.start_block = Some(12);
        evm.rescan_depth = Some(3);
        let config = config_with_evm_escrow(evm);

        validate_config(&config).unwrap();
    }

    #[test]
    fn rejects_zero_evm_escrow_scan_bound() {
        let mut evm = valid_evm_escrow_config();
        evm.max_scan_blocks = Some(0);
        let config = config_with_evm_escrow(evm);

        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "evm_escrow max_scan_blocks must be positive",
        );
    }

    #[test]
    fn rejects_zero_evm_escrow_rescan_depth() {
        let mut evm = valid_evm_escrow_config();
        evm.rescan_depth = Some(0);
        let config = config_with_evm_escrow(evm);

        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "evm_escrow rescan_depth must be positive",
        );
    }

    #[test]
    fn rejects_zero_evm_escrow_addresses_when_enabled() {
        let zero_address = "0x0000000000000000000000000000000000000000";

        let mut evm = valid_evm_escrow_config();
        evm.escrow_contract = zero_address.into();
        let config = config_with_evm_escrow(evm);
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "evm_escrow escrow_contract must be an EVM address"
        );

        let mut evm = valid_evm_escrow_config();
        evm.default_token = zero_address.into();
        evm.tokens[0].contract = zero_address.into();
        let config = config_with_evm_escrow(evm);
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "evm_escrow default_token must be an EVM address"
        );

        let mut evm = valid_evm_escrow_config();
        evm.tokens.push(EvmEscrowTokenConfig {
            symbol: "ZERO".into(),
            contract: zero_address.into(),
            decimals: 18,
            currency: "ZERO".into(),
        });
        let config = config_with_evm_escrow(evm);
        assert_eq!(
            validate_config(&config).unwrap_err().to_string(),
            "evm_escrow token contract must be an EVM address"
        );
    }
}
