use std::{env, fs, path::PathBuf, process::Command, str::FromStr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use morpheus_api::ErrorResponse;
use morpheus_config::load_config;
use morpheus_matrix::generate_synapse_registration;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::PgPoolOptions,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

#[derive(Debug, Parser)]
#[command(name = "morpheus")]
#[command(about = "Morpheus marketplace protocol server tools")]
struct Cli {
    #[arg(long, global = true, default_value = "http://127.0.0.1:8080")]
    server_url: String,
    #[arg(long, global = true)]
    token: Option<String>,
    #[arg(long, global = true)]
    pretty: bool,
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
    Admin {
        #[command(subcommand)]
        command: AdminCommand,
    },
    Seller {
        #[command(subcommand)]
        command: SellerCommand,
    },
    Buyer {
        #[command(subcommand)]
        command: BuyerCommand,
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
enum AdminCommand {
    Health,
    Config,
    Allowlist,
    Projections,
    Events,
    Rooms {
        #[command(subcommand)]
        command: AdminRoomsCommand,
    },
    Catalog {
        #[command(subcommand)]
        command: AdminCatalogCommand,
    },
    Order {
        #[command(subcommand)]
        command: AdminOrderCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AdminCatalogCommand {
    Rebuild,
}

#[derive(Debug, Subcommand)]
enum AdminRoomsCommand {
    Bootstrap,
}

#[derive(Debug, Subcommand)]
enum AdminOrderCommand {
    Replay { order_id: String },
}

#[derive(Debug, Subcommand)]
enum SellerCommand {
    Announce {
        #[arg(long)]
        json: String,
    },
    Product {
        #[command(subcommand)]
        command: SellerProductCommand,
    },
    Offer {
        #[command(subcommand)]
        command: SellerOfferCommand,
    },
    Orders {
        #[command(subcommand)]
        command: SellerOrdersCommand,
    },
    Order {
        #[command(subcommand)]
        command: SellerOrderCommand,
    },
    Payment {
        #[command(subcommand)]
        command: SellerPaymentCommand,
    },
    Entitlement {
        #[command(subcommand)]
        command: SellerEntitlementCommand,
    },
}

#[derive(Debug, Subcommand)]
enum SellerProductCommand {
    Upsert {
        #[arg(long)]
        json: String,
    },
}

#[derive(Debug, Subcommand)]
enum SellerOfferCommand {
    Upsert {
        #[arg(long)]
        json: String,
    },
    Withdraw {
        offer_id: String,
        #[arg(long)]
        json: String,
    },
}

#[derive(Debug, Subcommand)]
enum SellerOrdersCommand {
    List,
}

#[derive(Debug, Subcommand)]
enum SellerOrderCommand {
    Accept {
        order_id: String,
        #[arg(long)]
        json: String,
    },
    Reject {
        order_id: String,
        #[arg(long)]
        json: String,
    },
    Complete {
        order_id: String,
        #[arg(long)]
        json: String,
    },
}

#[derive(Debug, Subcommand)]
enum SellerPaymentCommand {
    Intent {
        order_id: String,
        #[arg(long)]
        json: String,
    },
    Capture {
        order_id: String,
        #[arg(long)]
        json: String,
    },
}

#[derive(Debug, Subcommand)]
enum SellerEntitlementCommand {
    Grant {
        order_id: String,
        #[arg(long)]
        json: String,
    },
}

#[derive(Debug, Subcommand)]
enum BuyerCommand {
    Catalog {
        #[command(subcommand)]
        command: BuyerCatalogCommand,
    },
    Order {
        #[command(subcommand)]
        command: BuyerOrderCommand,
    },
}

#[derive(Debug, Subcommand)]
enum BuyerCatalogCommand {
    Sellers,
    Products,
    Offers,
}

#[derive(Debug, Subcommand)]
enum BuyerOrderCommand {
    Create {
        #[arg(long)]
        json: String,
    },
    Cancel {
        order_id: String,
        #[arg(long)]
        json: String,
    },
    List,
    Show {
        order_id: String,
    },
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
    match &cli.command {
        Commands::Config {
            command: ConfigCommand::Validate { config },
        } => {
            load_config(config)?;
            println!("config ok");
        }
        Commands::Synapse {
            command: SynapseCommand::Registration { config, out },
        } => {
            let config = load_config(config)?;
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
                digest == *sha256,
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
            migrate_database(database_url, *database_kind).await?;
            println!("database migrated");
        }
        Commands::Catalog {
            command: CatalogCommand::Rebuild,
        } => {
            println!("{}", json!({ "status": "scheduled" }));
        }
        Commands::Admin { command } => {
            run_admin_command(&cli, command).await?;
        }
        Commands::Seller { command } => {
            run_seller_command(&cli, command).await?;
        }
        Commands::Buyer { command } => {
            run_buyer_command(&cli, command).await?;
        }
        Commands::Demo {
            command:
                DemoCommand::Seed {
                    scenario: DemoScenario::ThreeRetailInstances,
                    config_dir,
                },
        } => {
            run_demo_seed(config_dir.clone())?;
        }
    }
    Ok(())
}

async fn run_admin_command(cli: &Cli, command: &AdminCommand) -> Result<()> {
    match command {
        AdminCommand::Health => get(cli, "/admin/health", Role::Admin).await,
        AdminCommand::Config => get(cli, "/admin/config", Role::Admin).await,
        AdminCommand::Allowlist => get(cli, "/admin/allowlist", Role::Admin).await,
        AdminCommand::Projections => get(cli, "/admin/projections/summary", Role::Admin).await,
        AdminCommand::Events => get(cli, "/admin/events", Role::Admin).await,
        AdminCommand::Rooms {
            command: AdminRoomsCommand::Bootstrap,
        } => post(cli, "/admin/rooms/bootstrap", Role::Admin, json!({})).await,
        AdminCommand::Catalog {
            command: AdminCatalogCommand::Rebuild,
        } => post(cli, "/admin/catalog/rebuild", Role::Admin, json!({})).await,
        AdminCommand::Order {
            command: AdminOrderCommand::Replay { order_id },
        } => {
            post(
                cli,
                &format!("/admin/orders/{order_id}/replay"),
                Role::Admin,
                json!({}),
            )
            .await
        }
    }
}

async fn run_seller_command(cli: &Cli, command: &SellerCommand) -> Result<()> {
    match command {
        SellerCommand::Announce { json } => {
            post_json(cli, "/api/v1/seller/announce", Role::Seller, json).await
        }
        SellerCommand::Product {
            command: SellerProductCommand::Upsert { json },
        } => post_json(cli, "/api/v1/seller/products", Role::Seller, json).await,
        SellerCommand::Offer {
            command: SellerOfferCommand::Upsert { json },
        } => post_json(cli, "/api/v1/seller/offers", Role::Seller, json).await,
        SellerCommand::Offer {
            command: SellerOfferCommand::Withdraw { offer_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/offers/{offer_id}/withdraw"),
                Role::Seller,
                json,
            )
            .await
        }
        SellerCommand::Orders {
            command: SellerOrdersCommand::List,
        } => get(cli, "/api/v1/seller/orders", Role::Seller).await,
        SellerCommand::Order {
            command: SellerOrderCommand::Accept { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/orders/{order_id}/accept"),
                Role::Seller,
                json,
            )
            .await
        }
        SellerCommand::Order {
            command: SellerOrderCommand::Reject { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/orders/{order_id}/reject"),
                Role::Seller,
                json,
            )
            .await
        }
        SellerCommand::Order {
            command: SellerOrderCommand::Complete { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/orders/{order_id}/complete"),
                Role::Seller,
                json,
            )
            .await
        }
        SellerCommand::Payment {
            command: SellerPaymentCommand::Intent { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/orders/{order_id}/payment-intent"),
                Role::Seller,
                json,
            )
            .await
        }
        SellerCommand::Payment {
            command: SellerPaymentCommand::Capture { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/orders/{order_id}/payment-capture"),
                Role::Seller,
                json,
            )
            .await
        }
        SellerCommand::Entitlement {
            command: SellerEntitlementCommand::Grant { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/seller/orders/{order_id}/entitlement-grant"),
                Role::Seller,
                json,
            )
            .await
        }
    }
}

async fn run_buyer_command(cli: &Cli, command: &BuyerCommand) -> Result<()> {
    match command {
        BuyerCommand::Catalog {
            command: BuyerCatalogCommand::Sellers,
        } => get(cli, "/api/v1/catalog/sellers", Role::Buyer).await,
        BuyerCommand::Catalog {
            command: BuyerCatalogCommand::Products,
        } => get(cli, "/api/v1/catalog/products", Role::Buyer).await,
        BuyerCommand::Catalog {
            command: BuyerCatalogCommand::Offers,
        } => get(cli, "/api/v1/catalog/offers", Role::Buyer).await,
        BuyerCommand::Order {
            command: BuyerOrderCommand::Create { json },
        } => post_json(cli, "/api/v1/buyer/orders", Role::Buyer, json).await,
        BuyerCommand::Order {
            command: BuyerOrderCommand::Cancel { order_id, json },
        } => {
            post_json(
                cli,
                &format!("/api/v1/buyer/orders/{order_id}/cancel"),
                Role::Buyer,
                json,
            )
            .await
        }
        BuyerCommand::Order {
            command: BuyerOrderCommand::List,
        } => get(cli, "/api/v1/buyer/orders", Role::Buyer).await,
        BuyerCommand::Order {
            command: BuyerOrderCommand::Show { order_id },
        } => {
            get(
                cli,
                &format!("/api/v1/buyer/orders/{order_id}"),
                Role::Buyer,
            )
            .await
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Role {
    Admin,
    Seller,
    Buyer,
}

async fn get(cli: &Cli, path: &str, role: Role) -> Result<()> {
    request(cli, reqwest::Method::GET, path, role, None).await
}

async fn post_json(cli: &Cli, path: &str, role: Role, body: &str) -> Result<()> {
    let value: Value = serde_json::from_str(body).context("parsing --json request body")?;
    post(cli, path, role, value).await
}

async fn post(cli: &Cli, path: &str, role: Role, body: Value) -> Result<()> {
    request(cli, reqwest::Method::POST, path, role, Some(body)).await
}

async fn request(
    cli: &Cli,
    method: reqwest::Method,
    path: &str,
    role: Role,
    body: Option<Value>,
) -> Result<()> {
    let token = cli
        .token
        .clone()
        .or_else(|| env::var(default_token_env(role)).ok())
        .with_context(|| {
            format!(
                "missing token; pass --token or set {}",
                default_token_env(role)
            )
        })?;
    let url = format!("{}{}", cli.server_url.trim_end_matches('/'), path);
    if env::var("MORPHEUS_CLI_DRY_RUN_REQUEST").ok().as_deref() == Some("1") {
        print_json(
            &json!({
                "method": method.as_str(),
                "path": path,
                "url": url,
                "authorization": format!("Bearer {token}"),
                "body": body,
            }),
            cli.pretty,
        )?;
        return Ok(());
    }
    let client = reqwest::Client::new();
    let mut request = client.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.context("sending HTTP request")?;
    let status = response.status();
    let text = response.text().await.context("reading HTTP response")?;
    let value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "raw": text }));
    if !status.is_success() {
        if let Ok(error) = serde_json::from_str::<ErrorResponse>(&text) {
            anyhow::bail!("server returned {status}: {}: {}", error.code, error.error);
        }
        anyhow::bail!(
            "server returned {status}: {}",
            serde_json::to_string(&value)?
        );
    }
    print_json(&value, cli.pretty)?;
    Ok(())
}

fn default_token_env(role: Role) -> &'static str {
    match role {
        Role::Admin => "MORPHEUS_ADMIN_TOKEN",
        Role::Seller => "MORPHEUS_SELLER_TOKEN",
        Role::Buyer => "MORPHEUS_BUYER_TOKEN",
    }
}

fn print_json(value: &Value, pretty: bool) -> Result<()> {
    if pretty {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", serde_json::to_string(value)?);
    }
    Ok(())
}

fn run_demo_seed(config_dir: PathBuf) -> Result<()> {
    let script = PathBuf::from("scripts/e2e/seed_three_retail.py");
    let python = PathBuf::from(".venv/bin/python");
    anyhow::ensure!(
        script.exists(),
        "demo seed script not found at {}",
        script.display()
    );
    anyhow::ensure!(
        python.exists(),
        "python venv not found at {}; create it with `/opt/homebrew/bin/python3.12 -m venv .venv`",
        python.display()
    );
    let status = Command::new(&python)
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
