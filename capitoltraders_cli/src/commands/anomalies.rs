//! The `anomalies` subcommand: detect unusual trading patterns.

use anyhow::{bail, Result};
use capitoltraders_lib::{
    anomaly::{
        calculate_composite_anomaly_score, calculate_sector_concentration, detect_pre_move_trades,
        detect_unusual_volume, PortfolioPositionForHHI, TradeVolumeRecord, TradeWithFuturePrice,
    },
    Db,
};
use chrono::Local;
use clap::Args;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::output::{
    print_anomaly_csv, print_anomaly_markdown, print_anomaly_table, print_anomaly_xml,
    print_json, print_pre_move_csv, print_pre_move_markdown, print_pre_move_table,
    print_pre_move_xml, OutputFormat,
};

/// Arguments for the `anomalies` subcommand.
///
/// Detects unusual trading patterns including pre-move trades, volume spikes, and sector concentration.
#[derive(Args)]
pub struct AnomaliesArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Filter by politician name (partial match)
    #[arg(long)]
    pub politician: Option<String>,

    /// Minimum composite anomaly score (0.0-1.0, default: 0.0)
    #[arg(long, default_value = "0.0")]
    pub min_score: f64,

    /// Minimum confidence threshold (0.0-1.0, default: 0.0)
    #[arg(long, default_value = "0.0")]
    pub min_confidence: f64,

    /// Show detailed pre-move trade signals
    #[arg(long)]
    pub show_pre_move: bool,

    /// Number of results to show (default: 25)
    #[arg(long, default_value = "25")]
    pub top: usize,

    /// Sort by metric: score, volume, hhi, pre-move (default: score)
    #[arg(long, default_value = "score")]
    pub sort_by: String,
}

/// Anomaly row for output (composite scores per politician).
#[derive(Debug, Clone, Serialize)]
pub struct AnomalyRow {
    pub rank: usize,
    pub politician_name: String,
    pub pre_move_count: usize,
    pub volume_ratio: f64,
    pub hhi_score: f64,
    pub composite_score: f64,
    pub confidence: f64,
}

/// Pre-move signal row for detailed output.
#[derive(Debug, Clone, Serialize)]
pub struct PreMoveRow {
    pub politician_name: String,
    pub ticker: String,
    pub tx_date: String,
    pub tx_type: String,
    pub trade_price: f64,
    pub price_30d_later: f64,
    pub price_change_pct: f64,
}

pub fn run(args: &AnomaliesArgs, format: &OutputFormat) -> Result<()> {
    // Validate min_score range
    if !(0.0..=1.0).contains(&args.min_score) {
        bail!(
            "Invalid --min-score value: '{}'. Must be between 0.0 and 1.0",
            args.min_score
        );
    }

    // Validate min_confidence range
    if !(0.0..=1.0).contains(&args.min_confidence) {
        bail!(
            "Invalid --min-confidence value: '{}'. Must be between 0.0 and 1.0",
            args.min_confidence
        );
    }

    // Validate sort_by
    let valid_sort_options = ["score", "volume", "hhi", "pre-move"];
    if !valid_sort_options.contains(&args.sort_by.as_str()) {
        bail!(
            "Invalid --sort-by value: '{}'. Must be one of: {}",
            args.sort_by,
            valid_sort_options.join(", ")
        );
    }

    let db = Db::open(&args.db)?;

    // Optional politician filter
    let politician_filter = if let Some(ref name) = args.politician {
        let matches = db.find_politician_by_name(name)?;
        if matches.is_empty() {
            eprintln!("No politician found matching name: '{}'", name);
            return Ok(());
        }
        if matches.len() > 1 {
            eprintln!("Multiple politicians match '{}', please be more specific:", name);
            for (_, full_name) in matches {
                eprintln!("  - {}", full_name);
            }
            return Ok(());
        }
        Some(matches[0].0.clone())
    } else {
        None
    };

    // Query all three data sources
    let pre_move_candidates = db.query_pre_move_candidates()?;
    let volume_records = db.query_trade_volume_by_politician()?;
    let hhi_positions = db.query_portfolio_positions_for_hhi()?;

    // Check for empty data
    if pre_move_candidates.is_empty() && volume_records.is_empty() && hhi_positions.is_empty() {
        eprintln!("No data available for anomaly detection.");
        eprintln!("Hint: Run 'enrich-prices' to enable pre-move detection.");
        return Ok(());
    }

    // Convert DB rows to anomaly input types and run detection

    // 1. Pre-move detection
    let trades_with_future: Vec<TradeWithFuturePrice> = pre_move_candidates
        .iter()
        .map(|row| TradeWithFuturePrice {
            tx_id: row.tx_id,
            politician_id: row.politician_id.clone(),
            ticker: row.ticker.clone(),
            tx_date: row.tx_date.clone(),
            tx_type: row.tx_type.clone(),
            trade_price: row.trade_price,
            price_30d_later: row.price_30d_later,
        })
        .collect();

    let pre_move_signals = detect_pre_move_trades(&trades_with_future, 10.0);

    // Build pre_move count per politician
    let mut pre_move_counts: HashMap<String, usize> = HashMap::new();
    for signal in &pre_move_signals {
        *pre_move_counts.entry(signal.politician_id.clone()).or_insert(0) += 1;
    }

    // 2. Volume detection
    let mut volume_signals: HashMap<String, f64> = HashMap::new();
    let today = Local::now().naive_local().date();

    // Group volume records by politician_id
    let mut volume_by_politician: HashMap<String, Vec<TradeVolumeRecord>> = HashMap::new();
    for row in volume_records {
        volume_by_politician
            .entry(row.politician_id.clone())
            .or_default()
            .push(TradeVolumeRecord {
                politician_id: row.politician_id.clone(),
                tx_date: row.tx_date.clone(),
            });
    }

    for (politician_id, records) in volume_by_politician {
        let signal = detect_unusual_volume(&records, &politician_id, today, 90, 365);
        volume_signals.insert(politician_id, signal.volume_ratio);
    }

    // 3. HHI sector concentration
    let mut hhi_scores: HashMap<String, f64> = HashMap::new();

    // Group HHI positions by politician_id
    let mut positions_by_politician: HashMap<String, Vec<PortfolioPositionForHHI>> =
        HashMap::new();
    for row in hhi_positions {
        positions_by_politician
            .entry(row.politician_id.clone())
            .or_default()
            .push(PortfolioPositionForHHI {
                ticker: row.ticker.clone(),
                gics_sector: row.gics_sector.clone(),
                estimated_value: row.estimated_value,
            });
    }

    for (politician_id, positions) in positions_by_politician {
        let concentration = calculate_sector_concentration(&positions);
        hhi_scores.insert(politician_id, concentration.hhi_score);
    }

    // Collect unique politician IDs and names
    let mut politician_names: HashMap<String, String> = HashMap::new();

    for row in &pre_move_candidates {
        politician_names.insert(row.politician_id.clone(), row.politician_name.clone());
    }
    for row in db.query_trade_volume_by_politician()? {
        politician_names.insert(row.politician_id.clone(), row.politician_name.clone());
    }
    for row in db.query_portfolio_positions_for_hhi()? {
        politician_names.insert(row.politician_id.clone(), row.politician_name.clone());
    }

    // Calculate composite scores
    let mut anomaly_rows: Vec<AnomalyRow> = Vec::new();

    for (politician_id, politician_name) in &politician_names {
        // Apply politician filter
        if let Some(ref filter_id) = politician_filter {
            if politician_id != filter_id {
                continue;
            }
        }

        let pre_move_count = pre_move_counts.get(politician_id).copied().unwrap_or(0);
        let volume_ratio = volume_signals.get(politician_id).copied().unwrap_or(0.0);
        let hhi_score = hhi_scores.get(politician_id).copied().unwrap_or(0.0);

        let composite =
            calculate_composite_anomaly_score(pre_move_count, volume_ratio, hhi_score);

        // Apply filters
        if composite.composite < args.min_score {
            continue;
        }
        if composite.confidence < args.min_confidence {
            continue;
        }

        anomaly_rows.push(AnomalyRow {
            rank: 0, // will be set after sorting
            politician_name: politician_name.clone(),
            pre_move_count,
            volume_ratio,
            hhi_score,
            composite_score: composite.composite,
            confidence: composite.confidence,
        });
    }

    // Sort by selected metric
    match args.sort_by.as_str() {
        "score" => anomaly_rows.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "volume" => anomaly_rows.sort_by(|a, b| {
            b.volume_ratio
                .partial_cmp(&a.volume_ratio)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "hhi" => anomaly_rows.sort_by(|a, b| {
            b.hhi_score
                .partial_cmp(&a.hhi_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        "pre-move" => anomaly_rows.sort_by(|a, b| b.pre_move_count.cmp(&a.pre_move_count)),
        _ => {} // already validated
    }

    // Truncate to --top
    anomaly_rows.truncate(args.top);

    // Set rank numbers
    for (idx, row) in anomaly_rows.iter_mut().enumerate() {
        row.rank = idx + 1;
    }

    let total_before_filter = politician_names.len();

    // Output anomaly scores
    match format {
        OutputFormat::Table => print_anomaly_table(&anomaly_rows),
        OutputFormat::Json => print_json(&anomaly_rows),
        OutputFormat::Csv => print_anomaly_csv(&anomaly_rows)?,
        OutputFormat::Markdown => print_anomaly_markdown(&anomaly_rows),
        OutputFormat::Xml => print_anomaly_xml(&anomaly_rows),
    }

    // Print summary to stderr
    eprintln!(
        "\nShowing {}/{} politicians (min score: {:.2}, min confidence: {:.2})",
        anomaly_rows.len(),
        total_before_filter,
        args.min_score,
        args.min_confidence
    );

    // If --show-pre-move, output detailed pre-move signals
    if args.show_pre_move {
        eprintln!("\n--- Pre-Move Trade Signals ---\n");

        // Filter pre-move signals to matching politicians
        let shown_politician_names: std::collections::HashSet<String> = anomaly_rows
            .iter()
            .map(|r| r.politician_name.clone())
            .collect();

        // Build politician_id -> name mapping
        let id_to_name: HashMap<String, String> = pre_move_candidates
            .iter()
            .map(|row| (row.politician_id.clone(), row.politician_name.clone()))
            .collect();

        let pre_move_rows: Vec<PreMoveRow> = pre_move_signals
            .iter()
            .filter_map(|signal| {
                let name = id_to_name.get(&signal.politician_id)?;
                if shown_politician_names.contains(name) {
                    Some(PreMoveRow {
                        politician_name: name.clone(),
                        ticker: signal.ticker.clone(),
                        tx_date: signal.tx_date.clone(),
                        tx_type: signal.tx_type.clone(),
                        trade_price: signal.trade_price,
                        price_30d_later: signal.price_30d_later,
                        price_change_pct: signal.price_change_pct,
                    })
                } else {
                    None
                }
            })
            .collect();

        match format {
            OutputFormat::Table => print_pre_move_table(&pre_move_rows),
            OutputFormat::Json => print_json(&pre_move_rows),
            OutputFormat::Csv => print_pre_move_csv(&pre_move_rows)?,
            OutputFormat::Markdown => print_pre_move_markdown(&pre_move_rows),
            OutputFormat::Xml => print_pre_move_xml(&pre_move_rows),
        }
    }

    Ok(())
}
