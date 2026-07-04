use axum::http::HeaderMap;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use ring::signature;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use url::Url;

pub const SESSION_COOKIE_NAME: &str = "morpheus_session";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthRole {
    Admin,
    Seller,
    Buyer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPrincipal {
    pub subject: String,
    pub display_name: Option<String>,
    pub roles: Vec<AuthRole>,
    pub seller_actor_ids: Vec<String>,
    pub buyer_actor_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSessionSeed {
    pub session_id: String,
    pub principal: AuthPrincipal,
}

#[derive(Debug, Clone)]
pub struct AuthServerConfig {
    pub mode: AuthServerMode,
    pub admin_token: String,
    pub seller_token: String,
    pub buyer_token: String,
    pub oidc: Option<OidcServerConfig>,
    state: Arc<Mutex<AuthState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthServerMode {
    StaticTokens,
    Oidc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcServerConfig {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_url: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub session_secret: String,
    pub role_claim: String,
    pub seller_actor_claim: String,
    pub buyer_actor_claim: String,
    pub allow_insecure_test_tokens: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingLogin {
    pub nonce: String,
    pub code_verifier: String,
    pub return_to: String,
}

#[derive(Debug, Default)]
struct AuthState {
    sessions: BTreeMap<String, AuthPrincipal>,
    pending_logins: BTreeMap<String, PendingLogin>,
    insecure_test_tokens: BTreeMap<String, String>,
}

impl AuthServerConfig {
    pub fn static_tokens(admin_token: &str, seller_token: &str, buyer_token: &str) -> Self {
        Self::static_tokens_with_sessions(admin_token, seller_token, buyer_token, Vec::new())
    }

    pub fn static_tokens_with_sessions(
        admin_token: &str,
        seller_token: &str,
        buyer_token: &str,
        sessions: Vec<AuthSessionSeed>,
    ) -> Self {
        Self {
            mode: AuthServerMode::StaticTokens,
            admin_token: admin_token.into(),
            seller_token: seller_token.into(),
            buyer_token: buyer_token.into(),
            oidc: None,
            state: Arc::new(Mutex::new(AuthState {
                sessions: sessions
                    .into_iter()
                    .map(|session| (session.session_id, session.principal))
                    .collect(),
                ..AuthState::default()
            })),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn oidc_test(
        issuer: &str,
        authorization_endpoint: &str,
        token_endpoint: &str,
        client_id: &str,
        client_secret: &str,
        redirect_url: &str,
        session_secret: &str,
        insecure_test_tokens: Vec<(String, String)>,
    ) -> Self {
        Self {
            mode: AuthServerMode::Oidc,
            admin_token: "admin-token".into(),
            seller_token: "seller-token".into(),
            buyer_token: "buyer-token".into(),
            oidc: Some(OidcServerConfig {
                issuer: issuer.into(),
                authorization_endpoint: authorization_endpoint.into(),
                token_endpoint: token_endpoint.into(),
                jwks_url: None,
                client_id: client_id.into(),
                client_secret: client_secret.into(),
                redirect_url: redirect_url.into(),
                session_secret: session_secret.into(),
                role_claim: "roles".into(),
                seller_actor_claim: "morpheus_sellers".into(),
                buyer_actor_claim: "morpheus_customers".into(),
                allow_insecure_test_tokens: true,
            }),
            state: Arc::new(Mutex::new(AuthState {
                insecure_test_tokens: insecure_test_tokens.into_iter().collect(),
                ..AuthState::default()
            })),
        }
    }

    pub fn oidc(
        oidc: OidcServerConfig,
        admin_token: String,
        seller_token: String,
        buyer_token: String,
    ) -> Self {
        Self {
            mode: AuthServerMode::Oidc,
            admin_token,
            seller_token,
            buyer_token,
            oidc: Some(oidc),
            state: Arc::new(Mutex::new(AuthState::default())),
        }
    }

    pub fn oidc_config(&self) -> Option<&OidcServerConfig> {
        self.oidc.as_ref()
    }

    pub fn add_insecure_test_token(&self, code: &str, id_token: &str) {
        self.state
            .lock()
            .unwrap()
            .insecure_test_tokens
            .insert(code.into(), id_token.into());
    }

    pub fn begin_oidc_login(&self, return_to: Option<&str>) -> Option<String> {
        let oidc = self.oidc_config()?;
        let state = random_token();
        let nonce = random_token();
        let code_verifier = random_token();
        let code_challenge = pkce_challenge(&code_verifier);
        let return_to = safe_return_to(return_to);
        self.state.lock().unwrap().pending_logins.insert(
            state.clone(),
            PendingLogin {
                nonce: nonce.clone(),
                code_verifier,
                return_to,
            },
        );
        let mut url = Url::parse(&oidc.authorization_endpoint).ok()?;
        url.query_pairs_mut()
            .append_pair("client_id", &oidc.client_id)
            .append_pair("redirect_uri", &oidc.redirect_url)
            .append_pair("response_type", "code")
            .append_pair("scope", "openid profile email")
            .append_pair("state", &state)
            .append_pair("nonce", &nonce)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &code_challenge);
        Some(url.to_string())
    }

    pub async fn complete_oidc_login(
        &self,
        state: &str,
        code: &str,
    ) -> Result<CompletedLogin, String> {
        let oidc = self
            .oidc_config()
            .ok_or_else(|| "oidc is not configured".to_string())?;
        let pending = self
            .state
            .lock()
            .unwrap()
            .pending_logins
            .remove(state)
            .ok_or_else(|| "unknown oidc state".to_string())?;
        let id_token = if oidc.allow_insecure_test_tokens {
            self.state
                .lock()
                .unwrap()
                .insecure_test_tokens
                .remove(code)
                .ok_or_else(|| "unknown oidc test code".to_string())?
        } else {
            exchange_oidc_code(oidc, &pending, code).await?
        };
        let principal = decode_id_token(oidc, &pending, &id_token).await?;
        let session_id = random_token();
        self.state
            .lock()
            .unwrap()
            .sessions
            .insert(session_id.clone(), principal);
        Ok(CompletedLogin {
            session_id,
            return_to: pending.return_to,
        })
    }

    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            AuthServerMode::StaticTokens => "static_tokens",
            AuthServerMode::Oidc => "oidc",
        }
    }

    pub fn principal_for_role(&self, headers: &HeaderMap, role: AuthRole) -> Option<AuthPrincipal> {
        self.session_principal(headers)
            .filter(|principal| principal.roles.contains(&role))
            .or_else(|| self.bearer_principal(headers, role))
    }

    pub fn session_principal(&self, headers: &HeaderMap) -> Option<AuthPrincipal> {
        let session_id = session_cookie(headers)?;
        self.state.lock().unwrap().sessions.get(session_id).cloned()
    }

    fn bearer_principal(&self, headers: &HeaderMap, role: AuthRole) -> Option<AuthPrincipal> {
        let expected = match role {
            AuthRole::Admin => &self.admin_token,
            AuthRole::Seller => &self.seller_token,
            AuthRole::Buyer => &self.buyer_token,
        };
        bearer_authorized(headers, expected).then(|| AuthPrincipal {
            subject: format!("legacy:{}", role.as_str()),
            display_name: None,
            roles: vec![role],
            seller_actor_ids: Vec::new(),
            buyer_actor_ids: Vec::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedLogin {
    pub session_id: String,
    pub return_to: String,
}

impl AuthRole {
    fn as_str(self) -> &'static str {
        match self {
            AuthRole::Admin => "admin",
            AuthRole::Seller => "seller",
            AuthRole::Buyer => "buyer",
        }
    }
}

pub fn bearer_authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
}

pub fn actor_binding_allows(principal: &AuthPrincipal, role: AuthRole, actor_id: &str) -> bool {
    let bindings = match role {
        AuthRole::Admin => return true,
        AuthRole::Seller => &principal.seller_actor_ids,
        AuthRole::Buyer => &principal.buyer_actor_ids,
    };
    bindings.is_empty() || bindings.iter().any(|bound| bound == actor_id)
}

fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|pair| {
            pair.strip_prefix(SESSION_COOKIE_NAME)
                .and_then(|value| value.strip_prefix('='))
        })
        .filter(|value| !value.is_empty())
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

fn safe_return_to(value: Option<&str>) -> String {
    value
        .filter(|return_to| {
            return_to.starts_with('/')
                && !return_to.starts_with("//")
                && !return_to.contains('\n')
                && !return_to.contains('\r')
        })
        .unwrap_or("/ui/buyer")
        .into()
}

pub fn encode_insecure_test_id_token(claims: Value) -> String {
    let header = json!({"alg": "none", "typ": "JWT"});
    format!(
        "{}.{}.",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap()),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    )
}

#[derive(Debug, Deserialize)]
struct OidcTokenResponse {
    id_token: String,
}

async fn exchange_oidc_code(
    oidc: &OidcServerConfig,
    pending: &PendingLogin,
    code: &str,
) -> Result<String, String> {
    let response = reqwest::Client::new()
        .post(&oidc.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", oidc.redirect_url.as_str()),
            ("client_id", oidc.client_id.as_str()),
            ("client_secret", oidc.client_secret.as_str()),
            ("code_verifier", pending.code_verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|err| format!("oidc token exchange failed: {err}"))?;
    let status = response.status();
    let body = response
        .json::<OidcTokenResponse>()
        .await
        .map_err(|err| format!("decoding oidc token response failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("oidc token endpoint returned {status}"));
    }
    Ok(body.id_token)
}

async fn decode_id_token(
    oidc: &OidcServerConfig,
    pending: &PendingLogin,
    id_token: &str,
) -> Result<AuthPrincipal, String> {
    let (header, claims) = decode_jwt(oidc, id_token).await?;
    if oidc.allow_insecure_test_tokens {
        if header.get("alg").and_then(Value::as_str) != Some("none") {
            return Err("test oidc token must use alg none".into());
        }
    } else {
        verify_rs256_jwt(oidc, id_token, &header).await?;
    }
    principal_from_claims(oidc, pending, &claims)
}

async fn decode_jwt(oidc: &OidcServerConfig, id_token: &str) -> Result<(Value, Value), String> {
    let mut parts = id_token.split('.');
    let header = decode_json_part(
        parts
            .next()
            .ok_or_else(|| "missing jwt header".to_string())?,
    )?;
    let claims = decode_json_part(
        parts
            .next()
            .ok_or_else(|| "missing jwt claims".to_string())?,
    )?;
    if parts.next().is_none() {
        return Err("missing jwt signature segment".into());
    }
    if !oidc.allow_insecure_test_tokens
        && header.get("alg").and_then(Value::as_str) != Some("RS256")
    {
        return Err("oidc id token must use RS256".into());
    }
    Ok((header, claims))
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    n: String,
    e: String,
}

async fn verify_rs256_jwt(
    oidc: &OidcServerConfig,
    id_token: &str,
    header: &Value,
) -> Result<(), String> {
    let jwks_url = oidc
        .jwks_url
        .as_deref()
        .ok_or_else(|| "oidc jwks_url is required for signed tokens".to_string())?;
    let kid = header
        .get("kid")
        .and_then(Value::as_str)
        .ok_or_else(|| "oidc id token kid is required".to_string())?;
    let jwks = reqwest::Client::new()
        .get(jwks_url)
        .send()
        .await
        .map_err(|err| format!("fetching oidc jwks failed: {err}"))?
        .json::<Jwks>()
        .await
        .map_err(|err| format!("decoding oidc jwks failed: {err}"))?;
    let jwk = jwks
        .keys
        .iter()
        .find(|key| key.kty == "RSA" && key.kid.as_deref() == Some(kid))
        .ok_or_else(|| "oidc jwks key not found".to_string())?;
    let n = URL_SAFE_NO_PAD
        .decode(&jwk.n)
        .map_err(|err| format!("decoding jwk modulus failed: {err}"))?;
    let e = URL_SAFE_NO_PAD
        .decode(&jwk.e)
        .map_err(|err| format!("decoding jwk exponent failed: {err}"))?;
    let mut parts = id_token.rsplitn(2, '.');
    let signature_part = parts
        .next()
        .ok_or_else(|| "missing jwt signature".to_string())?;
    let signing_input = parts
        .next()
        .ok_or_else(|| "missing jwt signing input".to_string())?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(signature_part)
        .map_err(|err| format!("decoding jwt signature failed: {err}"))?;
    signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &signature::RSA_PKCS1_2048_8192_SHA256,
            signing_input.as_bytes(),
            &signature_bytes,
        )
        .map_err(|_| "oidc id token signature verification failed".to_string())
}

fn principal_from_claims(
    oidc: &OidcServerConfig,
    pending: &PendingLogin,
    claims: &Value,
) -> Result<AuthPrincipal, String> {
    if claims.get("iss").and_then(Value::as_str) != Some(oidc.issuer.as_str()) {
        return Err("oidc issuer mismatch".into());
    }
    if !audience_matches(claims.get("aud"), &oidc.client_id) {
        return Err("oidc audience mismatch".into());
    }
    if claims.get("nonce").and_then(Value::as_str) != Some(pending.nonce.as_str()) {
        return Err("oidc nonce mismatch".into());
    }
    let now = chrono::Utc::now().timestamp();
    if claims
        .get("exp")
        .and_then(Value::as_i64)
        .is_some_and(|exp| exp <= now)
    {
        return Err("oidc token expired".into());
    }
    let subject = claims
        .get("sub")
        .and_then(Value::as_str)
        .filter(|subject| !subject.is_empty())
        .ok_or_else(|| "oidc subject is required".to_string())?;
    Ok(AuthPrincipal {
        subject: subject.into(),
        display_name: claims
            .get("name")
            .or_else(|| claims.get("email"))
            .and_then(Value::as_str)
            .map(str::to_string),
        roles: string_array(claims.get(&oidc.role_claim))
            .into_iter()
            .filter_map(|role| match role.as_str() {
                "admin" => Some(AuthRole::Admin),
                "seller" => Some(AuthRole::Seller),
                "buyer" => Some(AuthRole::Buyer),
                _ => None,
            })
            .collect(),
        seller_actor_ids: string_array(claims.get(&oidc.seller_actor_claim)),
        buyer_actor_ids: string_array(claims.get(&oidc.buyer_actor_claim)),
    })
}

fn decode_json_part(part: &str) -> Result<Value, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|err| format!("decoding jwt part failed: {err}"))?;
    serde_json::from_slice(&bytes).map_err(|err| format!("decoding jwt json failed: {err}"))
}

fn audience_matches(value: Option<&Value>, client_id: &str) -> bool {
    match value {
        Some(Value::String(aud)) => aud == client_id,
        Some(Value::Array(items)) => items.iter().any(|item| item.as_str() == Some(client_id)),
        _ => false,
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(item)) => vec![item.to_string()],
        _ => Vec::new(),
    }
}
