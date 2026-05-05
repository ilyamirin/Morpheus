use std::{fs, path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use morpheus_matrix::generate_synapse_registration;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::PgPoolOptions,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

#[derive(Debug, Parser)]
#[command(name = "morpheus")]
#[command(about = "Morpheus marketplace protocol server tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Synapse {
        #[command(subcommand)]
        command: SynapseCommand,
    },
    Conformance {
        #[command(subcommand)]
        command: ConformanceCommand,
    },
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Validate {
        #[arg(long)]
        config: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum SynapseCommand {
    Registration {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ConformanceCommand {
    Run,
}

#[derive(Debug, Subcommand)]
enum SnapshotCommand {
    Verify {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        sha256: String,
    },
}

#[derive(Debug, Subcommand)]
enum DbCommand {
    Migrate {
        #[arg(long)]
        database_url: String,
        #[arg(long, value_enum)]
        database_kind: Option<DatabaseKind>,
    },
}

#[derive(Debug, Subcommand)]
enum CatalogCommand {
    Rebuild,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DatabaseKind {
    Postgres,
    Sqlite,
}

#[derive(Debug, Deserialize)]
struct Config {
    instance: InstanceConfig,
    appservice: AppServiceConfig,
    database: DatabaseConfig,
    admin: AdminConfig,
    allowlist: Option<AllowlistConfig>,
}

#[derive(Debug, Deserialize)]
struct InstanceConfig {
    instance_id: String,
    matrix_server_name: String,
    application_service_id: String,
    catalog_room_id: String,
    protocol_versions: Vec<String>,
    payment_adapters: Vec<String>,
    entitlement_types: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AppServiceConfig {
    homeserver_url: String,
    sender_localpart: String,
    namespace_prefix: String,
    homeserver_token: String,
    appservice_token: String,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    url: String,
}

#[derive(Debug, Deserialize)]
struct AdminConfig {
    bind: String,
    bearer_token_env: String,
}

#[derive(Debug, Deserialize)]
struct AllowlistConfig {
    instances: Vec<AllowlistInstance>,
}

#[derive(Debug, Deserialize)]
struct AllowlistInstance {
    instance_id: String,
    capabilities: Vec<String>,
    status: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Config {
            command: ConfigCommand::Validate { config },
        } => {
            load_and_validate_config(&config)?;
            println!("config ok");
        }
        Commands::Synapse {
            command: SynapseCommand::Registration { config, out },
        } => {
            let config = load_and_validate_config(&config)?;
            let registration = generate_synapse_registration(
                &config.instance.application_service_id,
                "http://morpheus-server:8080",
                &config.appservice.appservice_token,
                &config.appservice.homeserver_token,
                &config.appservice.sender_localpart,
                &config.appservice.namespace_prefix,
            );
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(out, serde_yaml::to_string(&registration)?)?;
        }
        Commands::Conformance {
            command: ConformanceCommand::Run,
        } => {
            let results = morpheus_conformance::required_vectors().run_all();
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        Commands::Snapshot {
            command: SnapshotCommand::Verify { file, sha256 },
        } => {
            let bytes = fs::read(file)?;
            let digest = format!("sha256:{}", hex_string(Sha256::digest(bytes)));
            anyhow::ensure!(
                digest == sha256,
                "snapshot hash mismatch: expected {sha256}, got {digest}"
            );
            println!("snapshot ok");
        }
        Commands::Db {
            command:
                DbCommand::Migrate {
                    database_url,
                    database_kind,
                },
        } => {
            migrate_database(&database_url, database_kind).await?;
            println!("database migrated");
        }
        Commands::Catalog {
            command: CatalogCommand::Rebuild,
        } => {
            println!("{}", json!({ "status": "scheduled" }));
        }
    }
    Ok(())
}

async fn migrate_database(database_url: &str, database_kind: Option<DatabaseKind>) -> Result<()> {
    match database_kind
        .or_else(|| infer_database_kind(database_url))
        .with_context(|| format!("could not infer database kind from URL: {database_url}"))?
    {
        DatabaseKind::Postgres => {
            let pool = PgPoolOptions::new()
                .max_connections(1)
                .connect(database_url)
                .await
                .context("connecting to postgres database")?;
            sqlx::raw_sql(morpheus_store::migrations::POSTGRES_0001)
                .execute(&pool)
                .await
                .context("running postgres migrations")?;
        }
        DatabaseKind::Sqlite => {
            let options = SqliteConnectOptions::from_str(database_url)
                .context("parsing sqlite database URL")?
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .context("connecting to sqlite database")?;
            sqlx::raw_sql(morpheus_store::migrations::SQLITE_0001)
                .execute(&pool)
                .await
                .context("running sqlite migrations")?;
        }
    }
    Ok(())
}

fn infer_database_kind(database_url: &str) -> Option<DatabaseKind> {
    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        Some(DatabaseKind::Postgres)
    } else if database_url.starts_with("sqlite:") {
        Some(DatabaseKind::Sqlite)
    } else {
        None
    }
}

fn load_and_validate_config(path: &PathBuf) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let config: Config = toml::from_str(&text).context("parsing TOML config")?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &Config) -> Result<()> {
    anyhow::ensure!(
        !config.instance.instance_id.is_empty(),
        "instance_id is required"
    );
    anyhow::ensure!(
        !config.instance.matrix_server_name.is_empty(),
        "matrix_server_name is required"
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
    anyhow::ensure!(!config.database.url.is_empty(), "database url is required");
    anyhow::ensure!(!config.admin.bind.is_empty(), "admin bind is required");
    anyhow::ensure!(
        !config.admin.bearer_token_env.is_empty(),
        "admin bearer_token_env is required"
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

fn hex_string(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
