//! CLI binary for querying congressional trading data from CapitolTrades.
//!
//! Provides four subcommands (`trades`, `politicians`, `issuers`, `sync`) with
//! extensive filtering, and supports output as table, JSON, CSV, Markdown, or XML.

mod commands;
mod output;
mod xml_output;

use anyhow::Result;
use capitoltraders_lib::ScrapeClient;
use clap::{Parser, Subcommand};

use crate::output::OutputFormat;

/// Top-level CLI structure parsed by clap.
#[derive(Parser)]
#[command(name = "capitoltraders")]
#[command(about = "Query congressional trading data from CapitolTrades")]
struct Cli {
    /// Output format: table, json, csv, md, xml
    #[arg(long, default_value = "table", global = true)]
    output: String,

    /// Override the scraping base URL (or set CAPITOLTRADES_BASE_URL)
    #[arg(long, global = true)]
    base_url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

/// Available subcommands.
#[derive(Subcommand)]
enum Commands {
    /// List recent trades
    Trades(Box<commands::trades::TradesArgs>),
    /// List politicians
    Politicians(commands::politicians::PoliticiansArgs),
    /// List or lookup issuers
    Issuers(commands::issuers::IssuersArgs),
    /// Sync data into a SQLite database
    Sync(commands::sync::SyncArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("capitoltraders=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let format = match cli.output.as_str() {
        "json" => OutputFormat::Json,
        "csv" => OutputFormat::Csv,
        "md" | "markdown" => OutputFormat::Markdown,
        "xml" => OutputFormat::Xml,
        _ => OutputFormat::Table,
    };

    let base_url = cli
        .base_url
        .clone()
        .or_else(|| std::env::var("CAPITOLTRADES_BASE_URL").ok());
    let scraper = match base_url.as_deref() {
        Some(url) => ScrapeClient::with_base_url(url)?,
        None => ScrapeClient::new()?,
    };

    match &cli.command {
        Commands::Trades(args) => {
            if let Some(ref db_path) = args.db {
                commands::trades::run_db(args.as_ref(), db_path, &format).await?
            } else {
                commands::trades::run(args.as_ref(), &scraper, &format).await?
            }
        }
        Commands::Politicians(args) => {
            if let Some(ref db_path) = args.db {
                commands::politicians::run_db(args, db_path, &format).await?
            } else {
                commands::politicians::run(args, &scraper, &format).await?
            }
        }
        Commands::Issuers(args) => {
            if let Some(ref db_path) = args.db {
                commands::issuers::run_db(args, db_path, &format)?
            } else {
                commands::issuers::run(args, &scraper, &format).await?
            }
        }
        Commands::Sync(args) => commands::sync::run(args, base_url.as_deref()).await?,
    }

    Ok(())
}
