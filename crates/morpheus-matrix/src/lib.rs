use morpheus_protocol::ValidationError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppServiceTransaction {
    #[serde(default)]
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationServiceContext {
    pub instance_id: String,
    pub server_name: String,
    pub exclusive_user_localpart: String,
}

pub fn validate_application_service_sender(
    sender: &str,
    context: &ApplicationServiceContext,
) -> Result<(), ValidationError> {
    let expected = format!(
        "@{}:{}",
        context.exclusive_user_localpart, context.server_name
    );
    let prefix = format!("@{}_", context.exclusive_user_localpart);
    let suffix = format!(":{}", context.server_name);
    if sender == expected || (sender.starts_with(&prefix) && sender.ends_with(&suffix)) {
        Ok(())
    } else {
        Err(ValidationError::new(
            morpheus_protocol::ValidationCode::UnauthorizedSender,
            "Sender is outside marketplace Application Service namespace",
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseRegistration {
    pub id: String,
    pub url: String,
    pub as_token: String,
    pub hs_token: String,
    pub sender_localpart: String,
    pub namespaces: SynapseNamespaces,
    pub rate_limited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseNamespaces {
    pub users: Vec<SynapseNamespaceRule>,
    pub aliases: Vec<SynapseNamespaceRule>,
    pub rooms: Vec<SynapseNamespaceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynapseNamespaceRule {
    pub exclusive: bool,
    pub regex: String,
}

pub fn generate_synapse_registration(
    id: &str,
    url: &str,
    as_token: &str,
    hs_token: &str,
    sender_localpart: &str,
    namespace_prefix: &str,
) -> SynapseRegistration {
    SynapseRegistration {
        id: id.into(),
        url: url.into(),
        as_token: as_token.into(),
        hs_token: hs_token.into(),
        sender_localpart: sender_localpart.into(),
        namespaces: SynapseNamespaces {
            users: vec![SynapseNamespaceRule {
                exclusive: true,
                regex: format!("@{}.*", namespace_prefix),
            }],
            aliases: vec![],
            rooms: vec![],
        },
        rate_limited: false,
    }
}

#[derive(Debug, Error)]
pub enum MatrixError {
    #[error("invalid appservice transaction: {0}")]
    InvalidTransaction(String),
}
