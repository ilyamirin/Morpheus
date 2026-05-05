use std::{env, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use morpheus_config::load_config;
use morpheus_server::{ServerConfig, build_router};
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
    let admin_token = env::var(&config.admin.bearer_token_env).with_context(|| {
        format!(
            "missing admin bearer token env {}",
            config.admin.bearer_token_env
        )
    })?;
    let pool = connect_postgres(&config.database.url).await?;
    sqlx::raw_sql(migrations::POSTGRES_0001)
        .execute(&pool)
        .await
        .context("running postgres migrations")?;

    let store = PostgresEventStore::new(pool);
    let app = build_router(
        ServerConfig {
            homeserver_token: config.appservice.homeserver_token,
            admin_token,
        },
        store,
    );
    let listener = tokio::net::TcpListener::bind(&config.admin.bind)
        .await
        .with_context(|| format!("binding {}", config.admin.bind))?;
    axum::serve(listener, app)
        .await
        .context("serving morpheus-server")?;
    Ok(())
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
