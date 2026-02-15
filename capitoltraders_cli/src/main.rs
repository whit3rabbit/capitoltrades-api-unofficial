//! CLI binary for querying congressional trading data from CapitolTrades.
//!
//! Provides eight subcommands (`trades`, `politicians`, `issuers`, `sync`, `sync-fec`, `enrich-prices`, `portfolio`, `sync-donations`, `donations`) with
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
    /// Sync FEC candidate ID mappings from congress-legislators dataset
    SyncFec(commands::sync_fec::SyncFecArgs),
    /// Enrich trades with Yahoo Finance price data
    EnrichPrices(commands::enrich_prices::EnrichPricesArgs),
    /// View portfolio positions with P&L
    Portfolio(commands::portfolio::PortfolioArgs),
    /// Sync FEC donation data for politicians
    SyncDonations(commands::sync_donations::SyncDonationsArgs),
    /// Query synced FEC donation data
    Donations(commands::donations::DonationsArgs),
    /// Build employer-to-issuer mapping database
    MapEmployers(commands::map_employers::MapEmployersArgs),
    /// View politician performance rankings and analytics
    Analytics(commands::analytics::AnalyticsArgs),
    /// View committee trading scores and donation-trade correlations
    Conflicts(commands::conflicts::ConflictsArgs),
    /// Detect unusual trading patterns (pre-move trades, volume spikes, sector concentration)
    Anomalies(Box<commands::anomalies::AnomaliesArgs>),
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (silently ignore if missing)
    let _ = dotenvy::dotenv();

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
        Commands::SyncFec(args) => commands::sync_fec::run(args).await?,
        Commands::EnrichPrices(args) => commands::enrich_prices::run(args).await?,
        Commands::Portfolio(args) => commands::portfolio::run(args, &format)?,
        Commands::SyncDonations(args) => {
            let api_key = require_openfec_api_key()?;
            commands::sync_donations::run(args, api_key).await?
        }
        Commands::Donations(args) => commands::donations::run(args, &format)?,
        Commands::MapEmployers(args) => commands::map_employers::run(args)?,
        Commands::Analytics(args) => commands::analytics::run(args, &format)?,
        Commands::Conflicts(args) => commands::conflicts::run(args, &format)?,
        Commands::Anomalies(args) => commands::anomalies::run(args, &format)?,
    }

    Ok(())
}

/// Require the OpenFEC API key from environment, providing helpful error if missing.
pub fn require_openfec_api_key() -> Result<String> {
    std::env::var("OPENFEC_API_KEY").map_err(|_| {
        anyhow::anyhow!(
            "OpenFEC API key not found.\n\n\
             To use donation-related features, you need an API key from api.data.gov:\n\
             1. Sign up at https://api.data.gov/signup/\n\
             2. Check your email for the API key\n\
             3. Create a .env file in the project root:\n\
                echo 'OPENFEC_API_KEY=your_key_here' > .env\n\
             4. See .env.example for a template\n\n\
             Note: .env is gitignored and will not be committed."
        )
    })
}
