//! The `portfolio` subcommand: displays per-politician stock positions with P&L.

use anyhow::Result;
use capitoltraders_lib::{validation, Db, PortfolioFilter};
use clap::Args;
use std::path::PathBuf;

use crate::output::{
    print_json, print_portfolio_csv, print_portfolio_markdown, print_portfolio_table,
    print_portfolio_xml, OutputFormat,
};

/// Arguments for the `portfolio` subcommand.
///
/// Displays stock positions with unrealized P&L from the local SQLite database.
/// Requires a synced and price-enriched database.
#[derive(Args)]
pub struct PortfolioArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Filter by politician ID (e.g., P000001)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by party: democrat (d), republican (r)
    #[arg(long)]
    pub party: Option<String>,

    /// Filter by state (e.g., CA, TX)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by ticker symbol (e.g., AAPL)
    #[arg(long)]
    pub ticker: Option<String>,

    /// Include closed positions (shares near zero)
    #[arg(long)]
    pub include_closed: bool,
}

pub fn run(args: &PortfolioArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Validate filters
    let party = match args.party {
        Some(ref val) => Some(validation::validate_party(val.trim())?.to_string()),
        None => None,
    };

    let state = match args.state {
        Some(ref val) => Some(validation::validate_state(val.trim())?.to_string()),
        None => None,
    };

    let politician_id = match args.politician {
        Some(ref val) => Some(validation::validate_politician_id(val.trim())?.to_string()),
        None => None,
    };

    let ticker = args.ticker.as_ref().map(|t| t.trim().to_uppercase());

    let filter = PortfolioFilter {
        politician_id,
        ticker,
        party,
        state,
        include_closed: args.include_closed,
    };

    let positions = db.get_portfolio(&filter)?;

    if positions.is_empty() {
        eprintln!("No portfolio positions found matching the given filters.");
        eprintln!("Hint: Run 'capitoltraders sync' then 'capitoltraders enrich-prices' first.");
        return Ok(());
    }

    // Count option trades for the note
    let option_count = db.count_option_trades(filter.politician_id.as_deref())?;

    match format {
        OutputFormat::Table => {
            print_portfolio_table(&positions);
            if option_count > 0 {
                eprintln!(
                    "\nNote: {} option trade(s) excluded (valuation deferred)",
                    option_count
                );
            }
        }
        OutputFormat::Json => print_json(&positions),
        OutputFormat::Csv => print_portfolio_csv(&positions)?,
        OutputFormat::Markdown => {
            print_portfolio_markdown(&positions);
            if option_count > 0 {
                eprintln!(
                    "\nNote: {} option trade(s) excluded (valuation deferred)",
                    option_count
                );
            }
        }
        OutputFormat::Xml => print_portfolio_xml(&positions),
    }

    Ok(())
}
