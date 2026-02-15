//! The `conflicts` subcommand: committee trading scores and donation-trade correlations.

use anyhow::{bail, Result};
use capitoltraders_lib::{
    analytics::calculate_closed_trades,
    conflict::calculate_committee_trading_score,
    committee_jurisdiction::load_committee_jurisdictions,
    Db,
};
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;

use crate::output::{
    print_conflict_csv, print_conflict_markdown, print_conflict_table, print_conflict_xml,
    print_donation_correlation_csv, print_donation_correlation_markdown,
    print_donation_correlation_table, print_donation_correlation_xml, print_json, OutputFormat,
};

/// Arguments for the `conflicts` subcommand.
///
/// Displays committee trading scores and donation-trade correlations from the local SQLite database.
/// Requires a synced, FEC-synced, and employer-mapped database.
#[derive(Args)]
pub struct ConflictsArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Filter by politician name (partial match)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by committee name (exact match)
    #[arg(long)]
    pub committee: Option<String>,

    /// Minimum committee trading percentage (0-100, default: 0)
    #[arg(long, default_value = "0.0")]
    pub min_committee_pct: f64,

    /// Include donation-trade correlations in output
    #[arg(long)]
    pub include_donations: bool,

    /// Minimum employer mapping confidence for donations (0.0-1.0, default: 0.90)
    #[arg(long, default_value = "0.90")]
    pub min_confidence: f64,

    /// Number of results to show (default: 25)
    #[arg(long, default_value = "25")]
    pub top: usize,
}

/// Conflict row for output (committee trading scores).
#[derive(Debug, Clone, Serialize)]
pub struct ConflictRow {
    pub rank: usize,
    pub politician_name: String,
    pub committees: String,
    pub total_scored_trades: usize,
    pub committee_related_trades: usize,
    pub committee_trading_pct: f64,
}

/// Donation correlation row for output.
#[derive(Debug, Clone, Serialize)]
pub struct DonationCorrelationRow {
    pub politician_name: String,
    pub ticker: String,
    pub matching_donors: i64,
    pub total_donations: f64,
    pub donor_employers: String,
}

pub fn run(args: &ConflictsArgs, format: &OutputFormat) -> Result<()> {
    // Validate min_committee_pct range
    if args.min_committee_pct < 0.0 || args.min_committee_pct > 100.0 {
        bail!(
            "Invalid --min-committee-pct value: '{}'. Must be between 0 and 100",
            args.min_committee_pct
        );
    }

    // Validate min_confidence range
    if args.min_confidence < 0.0 || args.min_confidence > 1.0 {
        bail!(
            "Invalid --min-confidence value: '{}'. Must be between 0.0 and 1.0",
            args.min_confidence
        );
    }

    let db = Db::open(&args.db)?;

    // Load committee jurisdictions
    let committee_jurisdictions = load_committee_jurisdictions()?;

    // Query all enriched trades
    let trade_rows = db.query_trades_for_analytics()?;

    // Convert to AnalyticsTrade and run FIFO matching
    let analytics_trades: Vec<capitoltraders_lib::analytics::AnalyticsTrade> = trade_rows
        .iter()
        .map(|row| {
            let has_sector_benchmark = row.gics_sector.is_some() && row.benchmark_price.is_some();
            capitoltraders_lib::analytics::AnalyticsTrade {
                tx_id: row.tx_id,
                politician_id: row.politician_id.clone(),
                ticker: row.issuer_ticker.clone(),
                tx_type: row.tx_type.clone(),
                tx_date: row.tx_date.clone(),
                estimated_shares: row.estimated_shares,
                trade_date_price: row.trade_date_price,
                benchmark_price: row.benchmark_price,
                has_sector_benchmark,
                gics_sector: row.gics_sector.clone(),
            }
        })
        .collect();

    let closed_trades = calculate_closed_trades(analytics_trades);

    // Get all politicians with committees
    let politicians_with_committees = db.get_all_politicians_with_committees()?;

    // If --politician filter, resolve name to politician_id
    let politician_filter_id = if let Some(ref name) = args.politician {
        let matches = db.find_politician_by_name(name)?;
        if matches.is_empty() {
            bail!("No politician found matching name: '{}'", name);
        }
        if matches.len() > 1 {
            eprintln!(
                "Warning: Multiple politicians match '{}'. Using first match: {}",
                name, matches[0].1
            );
        }
        Some(matches[0].0.clone())
    } else {
        None
    };

    // Calculate committee trading scores for each politician
    let mut conflict_scores = Vec::new();

    for (politician_id, politician_name, committees) in politicians_with_committees {
        // Apply politician filter
        if let Some(ref filter_id) = politician_filter_id {
            if &politician_id != filter_id {
                continue;
            }
        }

        // Apply committee filter
        if let Some(ref committee_filter) = args.committee {
            if !committees.contains(committee_filter) {
                continue;
            }
        }

        // Filter closed trades for this politician
        let politician_trades: Vec<_> = closed_trades
            .iter()
            .filter(|t| t.politician_id == politician_id)
            .cloned()
            .collect();

        // Calculate score
        let score = calculate_committee_trading_score(
            &politician_trades,
            &committees,
            &committee_jurisdictions,
            politician_id.clone(),
            politician_name.clone(),
        );

        // Apply min_committee_pct threshold
        if score.committee_trading_pct >= args.min_committee_pct {
            conflict_scores.push(score);
        }
    }

    // Sort by committee_trading_pct descending
    conflict_scores.sort_by(|a, b| {
        b.committee_trading_pct
            .partial_cmp(&a.committee_trading_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to --top
    conflict_scores.truncate(args.top);

    // Convert to output rows
    let conflict_rows: Vec<ConflictRow> = conflict_scores
        .iter()
        .enumerate()
        .map(|(idx, score)| ConflictRow {
            rank: idx + 1,
            politician_name: score.politician_name.clone(),
            committees: score.committee_names.join(", "),
            total_scored_trades: score.total_scored_trades,
            committee_related_trades: score.committee_related_trades,
            committee_trading_pct: score.committee_trading_pct,
        })
        .collect();

    // Print disclaimer to stderr
    eprintln!(
        "\nNote: Based on current committee assignments. Historical committee membership not tracked. Trades with unknown sector excluded from scoring.\n"
    );

    // Output committee trading scores
    match format {
        OutputFormat::Table => print_conflict_table(&conflict_rows),
        OutputFormat::Json => print_json(&conflict_rows),
        OutputFormat::Csv => print_conflict_csv(&conflict_rows)?,
        OutputFormat::Markdown => print_conflict_markdown(&conflict_rows),
        OutputFormat::Xml => print_conflict_xml(&conflict_rows),
    }

    // Print summary to stderr
    let total_politicians = db.get_all_politicians_with_committees()?.len();
    eprintln!(
        "\nShowing {}/{} politicians with committee assignments ({} scored trades, min threshold: {:.1}%)\n",
        conflict_rows.len(),
        total_politicians,
        conflict_rows.iter().map(|r| r.total_scored_trades).sum::<usize>(),
        args.min_committee_pct
    );

    // If --include-donations, also query and output donation-trade correlations
    if args.include_donations {
        let correlations = db.query_donation_trade_correlations(args.min_confidence)?;

        // Convert to output rows
        let donation_rows: Vec<DonationCorrelationRow> = correlations
            .iter()
            .map(|c| DonationCorrelationRow {
                politician_name: c.politician_name.clone(),
                ticker: c.ticker.clone(),
                matching_donors: c.matching_donor_count,
                total_donations: c.total_donation_amount,
                donor_employers: c.donor_employers.clone(),
            })
            .collect();

        eprintln!("\n--- Donation-Trade Correlations (confidence >= {:.0}%) ---\n", args.min_confidence * 100.0);

        // Output donation correlations
        match format {
            OutputFormat::Table => print_donation_correlation_table(&donation_rows),
            OutputFormat::Json => print_json(&donation_rows),
            OutputFormat::Csv => print_donation_correlation_csv(&donation_rows)?,
            OutputFormat::Markdown => print_donation_correlation_markdown(&donation_rows),
            OutputFormat::Xml => print_donation_correlation_xml(&donation_rows),
        }

        eprintln!(
            "\nFound {} donation-trade correlations\n",
            donation_rows.len()
        );
    }

    Ok(())
}
