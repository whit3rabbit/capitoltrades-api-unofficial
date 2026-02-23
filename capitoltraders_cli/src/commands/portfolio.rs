//! The `portfolio` subcommand: displays per-politician stock positions with P&L.

use anyhow::Result;
use capitoltraders_lib::committee_jurisdiction::load_committee_jurisdictions;
use capitoltraders_lib::portfolio::calculate_positions;
use capitoltraders_lib::{validation, Db, PortfolioFilter, PortfolioPosition};
use clap::Args;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::output::{
    print_enriched_portfolio_csv, print_enriched_portfolio_markdown,
    print_enriched_portfolio_table, print_enriched_portfolio_xml, print_json, OutputFormat,
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

    /// Show donation summary for the politician (requires synced donations)
    #[arg(long)]
    pub show_donations: bool,

    /// Show FIFO oversold position warnings (hidden by default)
    #[arg(long)]
    pub verbose: bool,
}

/// Enriched portfolio position with optional conflict detection fields.
///
/// Extends [`PortfolioPosition`] with gics_sector and in_committee_sector flag.
/// All conflict fields are Option types for backward compatibility.
#[derive(Serialize, Clone)]
pub struct EnrichedPortfolioPosition {
    // Base PortfolioPosition fields
    pub politician_id: String,
    pub ticker: String,
    pub shares_held: f64,
    pub cost_basis: f64,
    pub current_price: Option<f64>,
    pub current_value: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub unrealized_pnl_pct: Option<f64>,
    // Conflict enrichment fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gics_sector: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_committee_sector: Option<bool>,
}

impl From<PortfolioPosition> for EnrichedPortfolioPosition {
    fn from(pos: PortfolioPosition) -> Self {
        Self {
            politician_id: pos.politician_id,
            ticker: pos.ticker,
            shares_held: pos.shares_held,
            cost_basis: pos.cost_basis,
            current_price: pos.current_price,
            current_value: pos.current_value,
            unrealized_pnl: pos.unrealized_pnl,
            unrealized_pnl_pct: pos.unrealized_pnl_pct,
            gics_sector: None,
            in_committee_sector: None,
        }
    }
}

pub fn run(args: &PortfolioArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Compute FIFO positions from trades and persist to positions table
    let trades = db.query_trades_for_portfolio()?;
    if !trades.is_empty() {
        let positions = calculate_positions(trades, args.verbose);
        let count = db.upsert_positions(&positions)?;
        eprintln!("Computed {} FIFO positions from trade data", count);
    }

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

    let ticker = match args.ticker.as_ref() {
        Some(t) => {
            let input = t.trim().to_uppercase();
            // Resolve bare ticker to DB format (e.g., AAPL -> AAPL:US)
            match db.find_issuer_ticker(&input)? {
                Some(resolved) => Some(resolved),
                None => Some(input), // pass through as-is; query will return empty
            }
        }
        None => None,
    };

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

    // Best-effort conflict enrichment: load committee jurisdictions and sector data
    let enriched_positions = match enrich_portfolio_with_conflicts(&db, positions) {
        Ok(enriched) => enriched,
        Err(e) => {
            eprintln!(
                "Note: Conflict detection unavailable ({}). Displaying positions without conflict flags.",
                e
            );
            // Fall back to unenriched positions
            db.get_portfolio(&filter)?
                .into_iter()
                .map(EnrichedPortfolioPosition::from)
                .collect()
        }
    };

    // Count option trades for the note
    let option_count = db.count_option_trades(filter.politician_id.as_deref())?;

    match format {
        OutputFormat::Table => {
            print_enriched_portfolio_table(&enriched_positions);
            if option_count > 0 {
                eprintln!(
                    "\nNote: {} option trade(s) excluded (valuation deferred)",
                    option_count
                );
            }
        }
        OutputFormat::Json => print_json(&enriched_positions),
        OutputFormat::Csv => print_enriched_portfolio_csv(&enriched_positions)?,
        OutputFormat::Markdown => {
            print_enriched_portfolio_markdown(&enriched_positions);
            if option_count > 0 {
                eprintln!(
                    "\nNote: {} option trade(s) excluded (valuation deferred)",
                    option_count
                );
            }
        }
        OutputFormat::Xml => print_enriched_portfolio_xml(&enriched_positions),
    }

    // Donation summary (opt-in via --show-donations)
    if args.show_donations {
        if let Some(ref pid) = filter.politician_id {
            match db.get_donation_summary(pid) {
                Ok(Some(summary)) => {
                    eprintln!("\n--- Donation Summary ---");
                    eprintln!(
                        "Total received: ${:.0} ({} contributions)",
                        summary.total_amount, summary.donation_count
                    );
                    if !summary.top_sectors.is_empty() {
                        eprintln!("Top employer sectors (matched):");
                        for st in &summary.top_sectors {
                            eprintln!(
                                "  {:30} ${:>12.0} ({} employers)",
                                st.sector, st.total_amount, st.employer_count
                            );
                        }
                    }
                }
                Ok(None) => {
                    eprintln!("\nNo donation data available for this politician.");
                    eprintln!("Hint: Run 'capitoltraders sync-donations --db {} --politician ...' first.", args.db.display());
                }
                Err(e) => {
                    // Non-fatal: log warning but don't fail the portfolio command
                    eprintln!("\nWarning: Could not load donation summary: {}", e);
                }
            }
        } else {
            eprintln!("\nNote: --show-donations requires --politician filter to display donation summary.");
        }
    }

    Ok(())
}

/// Enrich portfolio positions with conflict detection data.
///
/// Best-effort: loads sector data from issuers table and checks if sectors
/// are under the politician's committee jurisdictions.
fn enrich_portfolio_with_conflicts(
    db: &Db,
    positions: Vec<PortfolioPosition>,
) -> Result<Vec<EnrichedPortfolioPosition>> {
    // Load committee jurisdictions
    let jurisdictions = load_committee_jurisdictions()?;

    // Build a set of unique tickers to query for sector data (bulk query to avoid N+1)
    let tickers: Vec<String> = positions
        .iter()
        .map(|p| p.ticker.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Query all sectors in one go
    let ticker_sectors: HashMap<String, Option<String>> = query_ticker_sectors(db, &tickers)?;

    // Group positions by politician_id to fetch committee data
    let politician_ids: Vec<String> = positions
        .iter()
        .map(|p| p.politician_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Build politician -> committees -> sectors mapping
    let politician_committee_sectors: HashMap<String, HashSet<String>> =
        build_politician_committee_sectors(db, &politician_ids, &jurisdictions)?;

    // Enrich each position
    let enriched: Vec<EnrichedPortfolioPosition> = positions
        .into_iter()
        .map(|pos| {
            let mut enriched = EnrichedPortfolioPosition::from(pos);

            // Look up sector for this ticker
            if let Some(sector_opt) = ticker_sectors.get(&enriched.ticker) {
                enriched.gics_sector = sector_opt.clone();

                // Check if sector is under politician's committee jurisdictions
                if let Some(ref sector) = sector_opt {
                    if let Some(committee_sectors) =
                        politician_committee_sectors.get(&enriched.politician_id)
                    {
                        enriched.in_committee_sector = Some(committee_sectors.contains(sector));
                    }
                }
            }

            enriched
        })
        .collect();

    Ok(enriched)
}

/// Query gics_sector for all tickers from the issuers table.
///
/// Uses individual queries for simplicity (small N, acceptable for CLI).
fn query_ticker_sectors(
    db: &Db,
    tickers: &[String],
) -> Result<HashMap<String, Option<String>>> {
    let conn = db.conn();
    let mut map = HashMap::new();

    for ticker in tickers {
        let mut stmt = conn.prepare("SELECT gics_sector FROM issuers WHERE issuer_ticker = ?1")?;
        let sector_result = stmt.query_row([ticker], |row| row.get::<_, Option<String>>(0));

        // Handle not found gracefully (ticker might not be in issuers table)
        let sector = sector_result.unwrap_or_default();

        map.insert(ticker.clone(), sector);
    }

    Ok(map)
}

/// Build a mapping of politician_id -> set of sectors under their committee jurisdictions.
fn build_politician_committee_sectors(
    db: &Db,
    politician_ids: &[String],
    jurisdictions: &[capitoltraders_lib::committee_jurisdiction::CommitteeJurisdiction],
) -> Result<HashMap<String, HashSet<String>>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();

    for politician_id in politician_ids {
        // Get committees for this politician
        let committees = db.get_politician_committee_names(politician_id)?;

        // Build set of sectors covered by those committees
        let sectors =
            capitoltraders_lib::committee_jurisdiction::get_committee_sectors(jurisdictions, &committees);

        map.insert(politician_id.clone(), sectors);
    }

    Ok(map)
}
