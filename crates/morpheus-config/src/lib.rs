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
    #[serde(default)]
    pub policy: EvmEscrowPolicyConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmEscrowTokenConfig {
    pub symbol: String,
    pub contract: String,
    pub decimals: u8,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvmEscrowPolicyConfig {
    #[serde(default)]
    pub min_order_amount: Option<String>,
    #[serde(default)]
    pub max_order_amount: Option<String>,
    #[serde(default)]
    pub high_value_amount: Option<String>,
    #[serde(default)]
    pub deposit_timeout_secs: Option<u64>,
    #[serde(default)]
    pub fulfillment_timeout_secs: Option<u64>,
    #[serde(default)]
    pub buyer_review_timeout_secs: Option<u64>,
    #[serde(default)]
    pub dispute_timeout_secs: Option<u64>,
    #[serde(default)]
    pub estimated_fee_units: Option<String>,
    #[serde(default)]
    pub fee_token_symbol: Option<String>,
    #[serde(default)]
    pub risk_categories: Vec<String>,
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

fn is_decimal_amount(value: &str) -> bool {
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if parts.next().is_some() {
        return false;
    }
    if whole.is_empty() || !whole.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    match fraction {
        Some(fraction) => !fraction.is_empty() && fraction.chars().all(|ch| ch.is_ascii_digit()),
        None => true,
    }
}

fn is_uint_amount(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

fn compare_decimal_amounts(left: &str, right: &str) -> std::cmp::Ordering {
    let (left_whole, left_fraction) = left.split_once('.').unwrap_or((left, ""));
    let (right_whole, right_fraction) = right.split_once('.').unwrap_or((right, ""));
    let left_whole = left_whole.trim_start_matches('0');
    let right_whole = right_whole.trim_start_matches('0');
    let left_whole = if left_whole.is_empty() {
        "0"
    } else {
        left_whole
    };
    let right_whole = if right_whole.is_empty() {
        "0"
    } else {
        right_whole
    };

    left_whole
        .len()
        .cmp(&right_whole.len())
        .then_with(|| left_whole.cmp(right_whole))
        .then_with(|| {
            let max_fraction_len = left_fraction.len().max(right_fraction.len());
            let mut left_digits = left_fraction.bytes();
            let mut right_digits = right_fraction.bytes();
            for _ in 0..max_fraction_len {
                let left_digit = left_digits.next().unwrap_or(b'0');
                let right_digit = right_digits.next().unwrap_or(b'0');
                match left_digit.cmp(&right_digit) {
                    std::cmp::Ordering::Equal => {}
                    ordering => return ordering,
                }
            }
            std::cmp::Ordering::Equal
        })
}

fn decimal_amount_lte(left: &str, right: &str) -> bool {
    compare_decimal_amounts(left, right) != std::cmp::Ordering::Greater
}

fn ensure_positive_timeout(value: Option<u64>, name: &str) -> Result<()> {
    if let Some(value) = value {
        anyhow::ensure!(value > 0, "evm_escrow policy {name} must be positive");
    }
    Ok(())
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
        let policy = &evm.policy;
        for (name, value) in [
            ("min_order_amount", policy.min_order_amount.as_deref()),
            ("max_order_amount", policy.max_order_amount.as_deref()),
            ("high_value_amount", policy.high_value_amount.as_deref()),
        ] {
            if let Some(value) = value {
                anyhow::ensure!(
                    is_decimal_amount(value),
                    "evm_escrow policy {name} must be a decimal amount"
                );
            }
        }
        if let (Some(min), Some(max)) = (
            policy.min_order_amount.as_deref(),
            policy.max_order_amount.as_deref(),
        ) {
            anyhow::ensure!(
                decimal_amount_lte(min, max),
                "evm_escrow policy min_order_amount must be <= max_order_amount"
            );
        }
        if let (Some(min), Some(high_value)) = (
            policy.min_order_amount.as_deref(),
            policy.high_value_amount.as_deref(),
        ) {
            anyhow::ensure!(
                decimal_amount_lte(min, high_value),
                "evm_escrow policy high_value_amount must be >= min_order_amount"
            );
        }
        if let (Some(high_value), Some(max)) = (
            policy.high_value_amount.as_deref(),
            policy.max_order_amount.as_deref(),
        ) {
            anyhow::ensure!(
                decimal_amount_lte(high_value, max),
                "evm_escrow policy high_value_amount must be <= max_order_amount"
            );
        }
        if let Some(value) = &policy.estimated_fee_units {
            anyhow::ensure!(
                is_uint_amount(value),
                "evm_escrow policy estimated_fee_units must be an unsigned integer amount"
            );
        }
        match policy.fee_token_symbol.as_deref() {
            Some(symbol) if symbol.trim().is_empty() => {
                let message = if policy.estimated_fee_units.is_some() {
                    "evm_escrow policy fee_token_symbol is required when estimated_fee_units is set"
                } else {
                    "evm_escrow policy fee_token_symbol must not be empty"
                };
                anyhow::bail!(message);
            }
            Some(symbol) => {
                anyhow::ensure!(
                    symbol == symbol.trim(),
                    "evm_escrow policy fee_token_symbol must not contain surrounding whitespace"
                );
            }
            None => {}
        }
        if policy.estimated_fee_units.is_some() {
            anyhow::ensure!(
                policy.fee_token_symbol.is_some(),
                "evm_escrow policy fee_token_symbol is required when estimated_fee_units is set"
            );
        }
        ensure_positive_timeout(policy.deposit_timeout_secs, "deposit_timeout_secs")?;
        ensure_positive_timeout(policy.fulfillment_timeout_secs, "fulfillment_timeout_secs")?;
        ensure_positive_timeout(
            policy.buyer_review_timeout_secs,
            "buyer_review_timeout_secs",
        )?;
        ensure_positive_timeout(policy.dispute_timeout_secs, "dispute_timeout_secs")?;
        for category in &policy.risk_categories {
            anyhow::ensure!(
                !category.trim().is_empty(),
                "evm_escrow policy risk category must not be empty"
            );
            anyhow::ensure!(
                category == category.trim(),
                "evm_escrow policy risk category must not contain surrounding whitespace"
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
            policy: EvmEscrowPolicyConfig::default(),
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
    fn validates_old_evm_escrow_policy_config_defaults_when_missing() {
        let toml = r##"
[instance]
instance_id = "shop.example"
matrix_server_name = "shop.example"
application_service_id = "morpheus"
catalog_room_id = "!catalog:shop.example"
catalog_room_alias = "#marketplace-catalog:shop.example"
order_room_alias_prefix = "#marketplace-order-"
protocol_versions = ["0.1"]
payment_adapters = ["mock", "evm_escrow"]
entitlement_types = ["download"]

[appservice]
homeserver_url = "http://localhost:8008"
url = "http://localhost:8080"
sender_localpart = "market"
namespace_prefix = "_morpheus_"
homeserver_token = "hs-token"
appservice_token = "as-token"
bootstrap_rooms = true

[database]
url = "sqlite::memory:"

[admin]
bind = "127.0.0.1:8080"
bearer_token_env = "MORPHEUS_ADMIN_TOKEN"

[auth]
seller_token_env = "MORPHEUS_SELLER_TOKEN"
buyer_token_env = "MORPHEUS_BUYER_TOKEN"

[payments.evm_escrow]
enabled = true
chain_id = 31337
rpc_url_env = "MORPHEUS_EVM_RPC_URL"
escrow_contract = "0x0000000000000000000000000000000000000001"
default_token = "0x0000000000000000000000000000000000000002"
confirmations = 1
poll_interval_secs = 2
start_block = 0
max_scan_blocks = 100
rescan_depth = 3
deployments_path = "contracts/deployments/local.json"

[[payments.evm_escrow.tokens]]
symbol = "USDC"
contract = "0x0000000000000000000000000000000000000002"
decimals = 6
currency = "USDC"

[[allowlist.instances]]
instance_id = "shop.example"
capabilities = ["catalog"]
status = "active"
catalog_room_alias = "#marketplace-catalog:shop.example"
homeserver_url = "http://localhost:8008"
morpheus_url = "http://localhost:8080"
"##;

        let config: MorpheusConfig = toml::from_str(toml).unwrap();

        assert_eq!(
            config
                .payments
                .as_ref()
                .unwrap()
                .evm_escrow
                .as_ref()
                .unwrap()
                .policy,
            EvmEscrowPolicyConfig::default()
        );
        validate_config(&config).unwrap();
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
    fn validates_evm_escrow_policy_when_enabled() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.max_order_amount = Some("100.00".into());
        evm.policy.high_value_amount = Some("50.00".into());
        evm.policy.deposit_timeout_secs = Some(900);
        evm.policy.fulfillment_timeout_secs = Some(86_400);
        evm.policy.buyer_review_timeout_secs = Some(3_600);
        evm.policy.dispute_timeout_secs = Some(172_800);
        evm.policy.estimated_fee_units = Some("1000000000000000".into());
        evm.policy.fee_token_symbol = Some("ETH".into());
        evm.policy.risk_categories = vec!["electronics".into(), "preorder".into()];

        let config = config_with_evm_escrow(evm);

        validate_config(&config).unwrap();
    }

    #[test]
    fn validates_evm_escrow_policy_decimal_comparison_edges() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.min_order_amount = Some("2".into());
        evm.policy.max_order_amount = Some("10".into());
        evm.policy.high_value_amount = Some("10".into());

        validate_config(&config_with_evm_escrow(evm)).unwrap();

        let mut evm = valid_evm_escrow_config();
        evm.policy.min_order_amount = Some("1.2".into());
        evm.policy.max_order_amount = Some("1.20".into());
        evm.policy.high_value_amount = Some("1.20".into());

        validate_config(&config_with_evm_escrow(evm)).unwrap();
    }

    #[test]
    fn rejects_invalid_evm_escrow_policy_amounts() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.max_order_amount = Some("100.".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy max_order_amount must be a decimal amount"
        );
    }

    #[test]
    fn rejects_zero_evm_escrow_policy_timeouts() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.deposit_timeout_secs = Some(0);

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy deposit_timeout_secs must be positive"
        );
    }

    #[test]
    fn rejects_invalid_evm_escrow_policy_estimated_fee_units() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.estimated_fee_units = Some("1.5".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy estimated_fee_units must be an unsigned integer amount"
        );
    }

    #[test]
    fn rejects_missing_evm_escrow_policy_fee_token_symbol() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.estimated_fee_units = Some("1000000000000000".into());
        evm.policy.fee_token_symbol = None;

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy fee_token_symbol is required when estimated_fee_units is set"
        );
    }

    #[test]
    fn rejects_empty_evm_escrow_policy_fee_token_symbol() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.estimated_fee_units = Some("1000000000000000".into());
        evm.policy.fee_token_symbol = Some("".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy fee_token_symbol is required when estimated_fee_units is set"
        );
    }

    #[test]
    fn rejects_empty_evm_escrow_policy_fee_token_symbol_without_fee_units() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.fee_token_symbol = Some("".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(err, "evm_escrow policy fee_token_symbol must not be empty");
    }

    #[test]
    fn rejects_blank_evm_escrow_policy_fee_token_symbol() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.estimated_fee_units = Some("1000000000000000".into());
        evm.policy.fee_token_symbol = Some("   ".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy fee_token_symbol is required when estimated_fee_units is set"
        );
    }

    #[test]
    fn rejects_blank_evm_escrow_policy_fee_token_symbol_without_fee_units() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.fee_token_symbol = Some("   ".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(err, "evm_escrow policy fee_token_symbol must not be empty");
    }

    #[test]
    fn rejects_padded_evm_escrow_policy_fee_token_symbol() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.fee_token_symbol = Some(" ETH ".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy fee_token_symbol must not contain surrounding whitespace"
        );
    }

    #[test]
    fn rejects_empty_evm_escrow_policy_risk_categories() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.risk_categories = vec!["".into()];

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(err, "evm_escrow policy risk category must not be empty");
    }

    #[test]
    fn rejects_blank_evm_escrow_policy_risk_categories() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.risk_categories = vec!["   ".into()];

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(err, "evm_escrow policy risk category must not be empty");
    }

    #[test]
    fn rejects_padded_evm_escrow_policy_risk_categories() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.risk_categories = vec![" electronics ".into()];

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy risk category must not contain surrounding whitespace"
        );
    }

    #[test]
    fn rejects_inconsistent_evm_escrow_policy_min_max_amounts() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.min_order_amount = Some("100.01".into());
        evm.policy.max_order_amount = Some("100.00".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy min_order_amount must be <= max_order_amount"
        );
    }

    #[test]
    fn rejects_evm_escrow_policy_high_value_below_min() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.min_order_amount = Some("10.00".into());
        evm.policy.high_value_amount = Some("9.99".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy high_value_amount must be >= min_order_amount"
        );
    }

    #[test]
    fn rejects_evm_escrow_policy_high_value_above_max() {
        let mut evm = valid_evm_escrow_config();
        evm.policy.max_order_amount = Some("100.00".into());
        evm.policy.high_value_amount = Some("100.01".into());

        let config = config_with_evm_escrow(evm);

        let err = validate_config(&config).unwrap_err().to_string();
        assert_eq!(
            err,
            "evm_escrow policy high_value_amount must be <= max_order_amount"
        );
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
