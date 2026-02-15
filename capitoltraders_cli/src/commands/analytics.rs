//! The `analytics` subcommand: politician performance rankings and metrics.

use anyhow::{bail, Result};
use capitoltraders_lib::{
    analytics::{
        aggregate_politician_metrics, calculate_closed_trades, compute_trade_metrics,
        AnalyticsTrade, PoliticianMetrics,
    },
    validation, AnalyticsTradeRow, Db,
};
use chrono::{Datelike, Local, NaiveDate};
use clap::Args;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::output::{
    print_json, print_leaderboard_csv, print_leaderboard_markdown, print_leaderboard_table,
    print_leaderboard_xml, OutputFormat,
};

/// Arguments for the `analytics` subcommand.
///
/// Displays politician performance rankings from the local SQLite database.
/// Requires a synced and price-enriched database.
#[derive(Args)]
pub struct AnalyticsArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Time period filter: ytd, 1y, 2y, all (default: all)
    #[arg(long, default_value = "all")]
    pub period: String,

    /// Minimum closed trades for inclusion (default: 5)
    #[arg(long, default_value = "5")]
    pub min_trades: usize,

    /// Sort by metric: return, win-rate, alpha (default: return)
    #[arg(long, default_value = "return")]
    pub sort_by: String,

    /// Filter by party: democrat (d), republican (r)
    #[arg(long)]
    pub party: Option<String>,

    /// Filter by state (e.g., CA, TX)
    #[arg(long)]
    pub state: Option<String>,

    /// Number of results to show (default: 25)
    #[arg(long, default_value = "25")]
    pub top: usize,
}

/// Enriched leaderboard row for output (includes politician name, party, state).
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardRow {
    pub rank: usize,
    pub politician_name: String,
    pub party: String,
    pub state: String,
    pub total_trades: usize,
    pub win_rate: f64,
    pub avg_return: f64,
    pub avg_alpha: Option<f64>,
    pub avg_holding_days: Option<f64>,
    pub percentile: f64,
}

pub fn run(args: &AnalyticsArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Validate period filter
    let period_normalized = args.period.trim().to_lowercase();
    if !matches!(period_normalized.as_str(), "ytd" | "1y" | "2y" | "all") {
        bail!(
            "Invalid --period value: '{}'. Valid options: ytd, 1y, 2y, all",
            args.period
        );
    }

    // Validate sort_by filter
    let sort_by_normalized = args.sort_by.trim().to_lowercase();
    if !matches!(sort_by_normalized.as_str(), "return" | "win-rate" | "alpha") {
        bail!(
            "Invalid --sort-by value: '{}'. Valid options: return, win-rate, alpha",
            args.sort_by
        );
    }

    // Validate party filter if provided
    let party_filter = match args.party {
        Some(ref val) => Some(validation::validate_party(val.trim())?.to_string()),
        None => None,
    };

    // Validate state filter if provided
    let state_filter = match args.state {
        Some(ref val) => Some(validation::validate_state(val.trim())?.to_string()),
        None => None,
    };

    // Query all enriched trades
    let trade_rows = db.query_trades_for_analytics()?;

    if trade_rows.is_empty() {
        eprintln!("No enriched stock trades found.");
        eprintln!(
            "Hint: Run 'capitoltraders sync --db {}' then 'capitoltraders enrich-prices --db {}' first.",
            args.db.display(),
            args.db.display()
        );
        return Ok(());
    }

    // Convert DB rows to AnalyticsTrade
    let analytics_trades: Vec<AnalyticsTrade> = trade_rows
        .iter()
        .map(row_to_analytics_trade)
        .collect();

    // Run FIFO matching
    let closed_trades = calculate_closed_trades(analytics_trades);

    if closed_trades.is_empty() {
        eprintln!("No closed trades found (no matched buy-sell pairs).");
        eprintln!("Hint: Politicians need at least one sell transaction to generate closed trades.");
        return Ok(());
    }

    // Apply time period filter to closed trades (before computing metrics)
    let filtered_closed_trades = filter_closed_trades_by_period(&closed_trades, &period_normalized)?;

    if filtered_closed_trades.is_empty() {
        eprintln!("No closed trades found in the selected period '{}'.", args.period);
        return Ok(());
    }

    // Compute per-trade metrics
    let trade_metrics: Vec<_> = filtered_closed_trades
        .iter()
        .map(compute_trade_metrics)
        .collect();

    // Aggregate by politician
    let mut politician_metrics = aggregate_politician_metrics(&trade_metrics);

    // Load politician metadata for filtering and enrichment
    let politician_metadata = load_politician_metadata(&db)?;

    // Apply politician-level filters
    politician_metrics.retain(|pm| {
        // Min trades filter
        if pm.total_trades < args.min_trades {
            return false;
        }

        // Party filter
        if let Some(ref party) = party_filter {
            if let Some(meta) = politician_metadata.get(&pm.politician_id) {
                if &meta.party != party {
                    return false;
                }
            } else {
                return false; // No metadata = exclude
            }
        }

        // State filter
        if let Some(ref state) = state_filter {
            if let Some(meta) = politician_metadata.get(&pm.politician_id) {
                if &meta.state != state {
                    return false;
                }
            } else {
                return false; // No metadata = exclude
            }
        }

        true
    });

    if politician_metrics.is_empty() {
        eprintln!("No politicians match the given filters.");
        return Ok(());
    }

    // Re-compute percentile ranks after filtering
    recompute_percentile_ranks(&mut politician_metrics);

    // Sort by selected metric
    sort_by_metric(&mut politician_metrics, &sort_by_normalized);

    // Truncate to top N
    let total_politicians = politician_metrics.len();
    politician_metrics.truncate(args.top);

    // Build leaderboard rows
    let leaderboard_rows: Vec<LeaderboardRow> = politician_metrics
        .iter()
        .enumerate()
        .map(|(idx, pm)| {
            let meta = politician_metadata.get(&pm.politician_id).unwrap();
            LeaderboardRow {
                rank: idx + 1,
                politician_name: meta.name.clone(),
                party: meta.party.clone(),
                state: meta.state.clone(),
                total_trades: pm.total_trades,
                win_rate: pm.win_rate,
                avg_return: pm.avg_return,
                avg_alpha: pm
                    .avg_alpha_spy
                    .or(pm.avg_alpha_sector),
                avg_holding_days: pm.avg_holding_days.map(|d| d as f64),
                percentile: pm.percentile_rank,
            }
        })
        .collect();

    // Output leaderboard
    match format {
        OutputFormat::Table => print_leaderboard_table(&leaderboard_rows),
        OutputFormat::Json => print_json(&leaderboard_rows),
        OutputFormat::Csv => print_leaderboard_csv(&leaderboard_rows)?,
        OutputFormat::Markdown => print_leaderboard_markdown(&leaderboard_rows),
        OutputFormat::Xml => print_leaderboard_xml(&leaderboard_rows),
    }

    // Print summary to stderr
    eprintln!(
        "Showing {}/{} politicians ({} closed trades analyzed, period: {})",
        leaderboard_rows.len(),
        total_politicians,
        filtered_closed_trades.len(),
        args.period
    );

    Ok(())
}

/// Convert AnalyticsTradeRow to AnalyticsTrade.
fn row_to_analytics_trade(row: &AnalyticsTradeRow) -> AnalyticsTrade {
    // has_sector_benchmark: true if gics_sector.is_some() AND benchmark_price.is_some()
    let has_sector_benchmark = row.gics_sector.is_some() && row.benchmark_price.is_some();

    AnalyticsTrade {
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
}

/// Filter closed trades by time period based on sell_date.
fn filter_closed_trades_by_period(
    trades: &[capitoltraders_lib::analytics::ClosedTrade],
    period: &str,
) -> Result<Vec<capitoltraders_lib::analytics::ClosedTrade>> {
    let today = Local::now().naive_local().date();

    let cutoff_date = match period {
        "ytd" => {
            // Year-to-date: Jan 1 of current year
            NaiveDate::from_ymd_opt(today.year(), 1, 1)
                .ok_or_else(|| anyhow::anyhow!("Invalid date calculation for YTD"))?
        }
        "1y" => {
            // Last 365 days
            today - chrono::Duration::days(365)
        }
        "2y" => {
            // Last 730 days
            today - chrono::Duration::days(730)
        }
        "all" => {
            // No filter
            return Ok(trades.to_vec());
        }
        _ => unreachable!("period validated earlier"),
    };

    // Filter by sell_date >= cutoff_date
    let filtered: Vec<_> = trades
        .iter()
        .filter(|ct| {
            // Parse sell_date as NaiveDate
            if let Ok(sell_date) = NaiveDate::parse_from_str(&ct.sell_date, "%Y-%m-%d") {
                sell_date >= cutoff_date
            } else {
                // If parse fails, exclude the trade
                false
            }
        })
        .cloned()
        .collect();

    Ok(filtered)
}

/// Load politician metadata (id, name, party, state) into a HashMap.
struct PoliticianMetadata {
    name: String,
    party: String,
    state: String,
}

fn load_politician_metadata(db: &Db) -> Result<HashMap<String, PoliticianMetadata>> {
    let rows = db.conn().prepare(
        "SELECT politician_id, first_name, last_name, party, state_id FROM politicians",
    )?;

    let mut stmt = rows;
    let mapped_rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            PoliticianMetadata {
                name: format!("{} {}", row.get::<_, String>(1)?, row.get::<_, String>(2)?),
                party: row.get(3)?,
                state: row.get(4)?,
            },
        ))
    })?;

    let mut map = HashMap::new();
    for row in mapped_rows {
        let (id, meta) = row?;
        map.insert(id, meta);
    }

    Ok(map)
}

/// Re-compute percentile ranks after filtering (pool size changed).
fn recompute_percentile_ranks(metrics: &mut [PoliticianMetrics]) {
    let n = metrics.len();
    if n == 0 {
        return;
    }

    // Sort by avg_return descending (same as aggregate_politician_metrics)
    metrics.sort_by(|a, b| {
        b.avg_return
            .partial_cmp(&a.avg_return)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Recompute percentile: 1.0 - (index / (n-1))
    for (idx, pm) in metrics.iter_mut().enumerate() {
        if n == 1 {
            pm.percentile_rank = 1.0;
        } else {
            pm.percentile_rank = 1.0 - (idx as f64 / (n as f64 - 1.0));
        }
    }
}

/// Sort politician metrics by the selected metric.
fn sort_by_metric(metrics: &mut [PoliticianMetrics], sort_by: &str) {
    match sort_by {
        "return" => {
            // Already sorted by avg_return descending from aggregate_politician_metrics
            metrics.sort_by(|a, b| {
                b.avg_return
                    .partial_cmp(&a.avg_return)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "win-rate" => {
            metrics.sort_by(|a, b| {
                b.win_rate
                    .partial_cmp(&a.win_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "alpha" => {
            metrics.sort_by(|a, b| {
                let a_alpha = a.avg_alpha_spy.or(a.avg_alpha_sector).unwrap_or(f64::NEG_INFINITY);
                let b_alpha = b.avg_alpha_spy.or(b.avg_alpha_sector).unwrap_or(f64::NEG_INFINITY);
                b_alpha
                    .partial_cmp(&a_alpha)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        _ => unreachable!("sort_by validated earlier"),
    }
}
