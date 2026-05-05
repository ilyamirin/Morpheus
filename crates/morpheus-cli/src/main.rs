use std::{fs, path::PathBuf, process::Command, str::FromStr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use morpheus_config::load_config;
use morpheus_matrix::generate_synapse_registration;
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
    Demo {
        #[command(subcommand)]
        command: DemoCommand,
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

#[derive(Debug, Subcommand)]
enum DemoCommand {
    Seed {
        #[arg(long, value_enum)]
        scenario: DemoScenario,
        #[arg(long)]
        config_dir: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DatabaseKind {
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DemoScenario {
    ThreeRetailInstances,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Config {
            command: ConfigCommand::Validate { config },
        } => {
            load_config(&config)?;
            println!("config ok");
        }
        Commands::Synapse {
            command: SynapseCommand::Registration { config, out },
        } => {
            let config = load_config(&config)?;
            let registration = generate_synapse_registration(
                &config.instance.application_service_id,
                &config.appservice.url,
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
        Commands::Demo {
            command:
                DemoCommand::Seed {
                    scenario: DemoScenario::ThreeRetailInstances,
                    config_dir,
                },
        } => {
            run_demo_seed(config_dir)?;
        }
    }
    Ok(())
}

fn run_demo_seed(config_dir: PathBuf) -> Result<()> {
    let script = PathBuf::from("scripts/e2e/seed_three_retail.py");
    anyhow::ensure!(
        script.exists(),
        "demo seed script not found at {}",
        script.display()
    );
    let status = Command::new("python3")
        .arg(script)
        .arg("--config-dir")
        .arg(config_dir)
        .status()
        .context("running demo seed script")?;
    anyhow::ensure!(status.success(), "demo seed script failed with {status}");
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

fn hex_string(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
