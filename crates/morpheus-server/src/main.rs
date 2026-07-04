use std::{env, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use morpheus_config::{AuthMode, MorpheusConfig, load_config};
use morpheus_server::{
    AuthServerConfig, OidcServerConfig, RemoteCatalogSource, ServerConfig, SynapseMatrixPublisher,
    build_router_with_publisher, ensure_catalog_room, sync_remote_catalog_once,
};
use morpheus_store::{PostgresEventStore, migrations};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::time::{Duration, sleep};

#[derive(Debug, Parser)]
#[command(name = "morpheus-server")]
#[command(about = "Morpheus Matrix marketplace Application Service")]
struct Cli {
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config(&cli.config)?;
    let auth = build_auth_runtime(&config)?;
    let remote_catalog_sources = config
        .allowlist
        .as_ref()
        .map(|allowlist| {
            allowlist
                .instances
                .iter()
                .filter(|entry| {
                    entry.status == "active"
                        && entry.instance_id != config.instance.instance_id
                        && entry
                            .capabilities
                            .iter()
                            .any(|capability| capability == "indexing")
                        && entry.morpheus_url.is_some()
                })
                .map(|entry| RemoteCatalogSource {
                    instance_id: entry.instance_id.clone(),
                    morpheus_url: entry.morpheus_url.clone().unwrap(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let pool = connect_postgres(&config.database.url).await?;
    sqlx::raw_sql(migrations::POSTGRES_0001)
        .execute(&pool)
        .await
        .context("running postgres migrations")?;

    let sender_user_id = format!(
        "@{}:{}",
        config.appservice.sender_localpart, config.instance.matrix_server_name
    );
    let catalog_room_id = if config.appservice.bootstrap_rooms {
        let alias = config
            .instance
            .catalog_room_alias
            .as_deref()
            .context("catalog_room_alias is required when bootstrap_rooms is true")?;
        ensure_catalog_room(
            &config.appservice.homeserver_url,
            &config.appservice.appservice_token,
            &sender_user_id,
            alias,
            &config.instance.instance_id,
        )
        .await
        .context("bootstrapping catalog room")?
    } else {
        config.instance.catalog_room_id
    };

    let publisher = SynapseMatrixPublisher::new(
        config.appservice.homeserver_url,
        config.appservice.appservice_token,
        sender_user_id,
    );
    let store = PostgresEventStore::new(pool);
    spawn_remote_catalog_indexer(store.clone(), remote_catalog_sources);
    let app = build_router_with_publisher(
        ServerConfig {
            instance_id: config.instance.instance_id,
            matrix_server_name: config.instance.matrix_server_name,
            catalog_room_id,
            catalog_room_alias: config.instance.catalog_room_alias,
            order_room_alias_prefix: config.instance.order_room_alias_prefix,
            appservice_sender_localpart: config.appservice.sender_localpart,
            homeserver_token: config.appservice.homeserver_token,
            auth,
        },
        store,
        publisher,
    );
    let listener = tokio::net::TcpListener::bind(&config.admin.bind)
        .await
        .with_context(|| format!("binding {}", config.admin.bind))?;
    axum::serve(listener, app)
        .await
        .context("serving morpheus-server")?;
    Ok(())
}

fn build_auth_runtime(config: &MorpheusConfig) -> Result<AuthServerConfig> {
    match config.auth.mode {
        AuthMode::StaticTokens => {
            let admin_token = env::var(&config.admin.bearer_token_env).with_context(|| {
                format!(
                    "missing admin bearer token env {}",
                    config.admin.bearer_token_env
                )
            })?;
            let seller_token = env::var(&config.auth.seller_token_env).with_context(|| {
                format!(
                    "missing seller bearer token env {}",
                    config.auth.seller_token_env
                )
            })?;
            let buyer_token = env::var(&config.auth.buyer_token_env).with_context(|| {
                format!(
                    "missing buyer bearer token env {}",
                    config.auth.buyer_token_env
                )
            })?;
            Ok(AuthServerConfig::static_tokens(
                &admin_token,
                &seller_token,
                &buyer_token,
            ))
        }
        AuthMode::Oidc => {
            let oidc = config
                .auth
                .oidc
                .as_ref()
                .context("auth oidc section is required in oidc mode")?;
            let client_secret = env::var(&oidc.client_secret_env).with_context(|| {
                format!("missing OIDC client secret env {}", oidc.client_secret_env)
            })?;
            let session_secret = env::var(&oidc.session_secret_env).with_context(|| {
                format!("missing session secret env {}", oidc.session_secret_env)
            })?;
            Ok(AuthServerConfig::oidc(
                OidcServerConfig {
                    issuer: oidc.issuer.clone(),
                    authorization_endpoint: oidc.authorization_endpoint.clone(),
                    token_endpoint: oidc.token_endpoint.clone(),
                    jwks_url: oidc.jwks_url.clone(),
                    client_id: oidc.client_id.clone(),
                    client_secret,
                    redirect_url: oidc.redirect_url.clone(),
                    session_secret,
                    role_claim: oidc.role_claim.clone(),
                    seller_actor_claim: oidc.seller_actor_claim.clone(),
                    buyer_actor_claim: oidc.buyer_actor_claim.clone(),
                    allow_insecure_test_tokens: oidc.allow_insecure_test_tokens,
                },
                optional_env(&config.admin.bearer_token_env),
                optional_env(&config.auth.seller_token_env),
                optional_env(&config.auth.buyer_token_env),
            ))
        }
    }
}

fn optional_env(name: &str) -> String {
    if name.is_empty() {
        String::new()
    } else {
        env::var(name).unwrap_or_default()
    }
}

fn spawn_remote_catalog_indexer(store: PostgresEventStore, sources: Vec<RemoteCatalogSource>) {
    if sources.is_empty() {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            for source in &sources {
                let _ = sync_remote_catalog_once(&store, source).await;
            }
        }
    });
}

async fn connect_postgres(database_url: &str) -> Result<PgPool> {
    let mut last_error = None;
    for _ in 0..30 {
        match PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(err) => {
                last_error = Some(err);
                sleep(Duration::from_secs(1)).await;
            }
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("postgres connection was not attempted")))
    .context("connecting to postgres database")
}
