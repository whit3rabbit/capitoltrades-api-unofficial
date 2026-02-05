mod commands;
mod output;

use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use capitoltraders_lib::cache::MemoryCache;
use capitoltraders_lib::CachedClient;

use crate::output::OutputFormat;

#[derive(Parser)]
#[command(name = "capitoltraders")]
#[command(about = "Query congressional trading data from CapitolTrades")]
struct Cli {
    /// Output format: table or json
    #[arg(long, default_value = "table", global = true)]
    output: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List recent trades
    Trades(Box<commands::trades::TradesArgs>),
    /// List politicians
    Politicians(commands::politicians::PoliticiansArgs),
    /// List or lookup issuers
    Issuers(commands::issuers::IssuersArgs),
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
        _ => OutputFormat::Table,
    };

    let cache = MemoryCache::new(Duration::from_secs(300));
    let client = CachedClient::new(cache);

    match &cli.command {
        Commands::Trades(args) => commands::trades::run(args.as_ref(), &client, &format).await?,
        Commands::Politicians(args) => commands::politicians::run(args, &client, &format).await?,
        Commands::Issuers(args) => commands::issuers::run(args, &client, &format).await?,
    }

    Ok(())
}
