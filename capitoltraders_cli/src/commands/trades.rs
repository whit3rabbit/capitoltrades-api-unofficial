//! The `trades` subcommand: lists congressional trades with extensive filtering.

use anyhow::{bail, Result};
use capitoltraders_lib::analytics::{
    calculate_closed_trades, compute_trade_metrics, AnalyticsTrade, TradeMetrics,
};
use capitoltraders_lib::types::Trade;
use capitoltraders_lib::validation;
use capitoltraders_lib::{Db, DbTradeFilter, DbTradeRow, ScrapeClient, ScrapedTrade};
use chrono::{NaiveDate, Utc};
use clap::Args;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

use crate::output::{
    print_enriched_trades_csv, print_enriched_trades_markdown, print_enriched_trades_table,
    print_enriched_trades_xml, print_json, print_trades_csv, print_trades_markdown,
    print_trades_table, print_trades_xml, OutputFormat,
};

/// Arguments for the `trades` subcommand.
///
/// Supports 24 filter flags, all of which accept comma-separated values where applicable.
/// Date filters use either relative days (`--days`, `--tx-days`) or absolute dates
/// (`--since`/`--until`, `--tx-since`/`--tx-until`), but not both simultaneously.
#[derive(Args)]
pub struct TradesArgs {
    /// Filter by issuer ID (numeric)
    #[arg(long)]
    pub issuer_id: Option<i64>,

    /// Search trades by politician name
    #[arg(long)]
    pub name: Option<String>,

    /// Search trades by issuer name/ticker (two-step lookup)
    #[arg(long)]
    pub issuer: Option<String>,

    /// Filter trades by politician name (two-step lookup)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by party (comma-separated): democrat (d), republican (r), other
    #[arg(long)]
    pub party: Option<String>,

    /// Filter by US state code (comma-separated, e.g. CA,TX,NY)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by committee (comma-separated, code or full name)
    #[arg(long)]
    pub committee: Option<String>,

    /// Filter trades from last N days (by publication date)
    #[arg(long, conflicts_with_all = ["since", "until"])]
    pub days: Option<i64>,

    /// Filter trades from last N days (by trade date)
    #[arg(long, conflicts_with_all = ["tx_since", "tx_until"])]
    pub tx_days: Option<i64>,

    /// Filter trades published on/after this date (YYYY-MM-DD)
    #[arg(long, conflicts_with = "days")]
    pub since: Option<String>,

    /// Filter trades published on/before this date (YYYY-MM-DD)
    #[arg(long, conflicts_with = "days")]
    pub until: Option<String>,

    /// Filter by transaction date on/after (YYYY-MM-DD)
    #[arg(long, conflicts_with = "tx_days")]
    pub tx_since: Option<String>,

    /// Filter by transaction date on/before (YYYY-MM-DD)
    #[arg(long, conflicts_with = "tx_days")]
    pub tx_until: Option<String>,

    /// Filter by trade size (1-10, comma-separated)
    #[arg(long)]
    pub trade_size: Option<String>,

    /// Filter by gender: female (f), male (m) -- comma-separated
    #[arg(long)]
    pub gender: Option<String>,

    /// Filter by market cap: mega,large,mid,small,micro,nano or 1-6 -- comma-separated
    #[arg(long)]
    pub market_cap: Option<String>,

    /// Filter by asset type: stock,etf,cryptocurrency,... -- comma-separated
    #[arg(long)]
    pub asset_type: Option<String>,

    /// Filter by label: faang,crypto,memestock,spac -- comma-separated
    #[arg(long)]
    pub label: Option<String>,

    /// Filter by sector: energy,financials,... -- comma-separated
    #[arg(long)]
    pub sector: Option<String>,

    /// Filter by transaction type: buy,sell,exchange,receive -- comma-separated
    #[arg(long)]
    pub tx_type: Option<String>,

    /// Filter by chamber: house (h), senate (s) -- comma-separated
    #[arg(long)]
    pub chamber: Option<String>,

    /// Filter by politician ID: P000197 format -- comma-separated
    #[arg(long)]
    pub politician_id: Option<String>,

    /// Filter by issuer state: 2-letter code (lowercase) -- comma-separated
    #[arg(long)]
    pub issuer_state: Option<String>,

    /// Filter by country: 2-letter ISO code (lowercase) -- comma-separated
    #[arg(long)]
    pub country: Option<String>,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: i64,

    /// Results per page
    #[arg(long, default_value = "12")]
    pub page_size: i64,

    /// Sort field: pub-date, trade-date, reporting-gap
    #[arg(long, default_value = "pub-date")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,

    /// Delay between trade detail requests in milliseconds
    #[arg(long, default_value = "250")]
    pub details_delay_ms: u64,

    /// Read trades from local SQLite database (requires prior sync)
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Show donation context for traded securities (requires synced donations and employer mappings)
    #[arg(long)]
    pub show_donor_context: bool,
}

/// Executes the trades subcommand: validates inputs, scrapes results,
/// applies client-side filtering and sorting, then prints output.
pub async fn run(args: &TradesArgs, scraper: &ScrapeClient, format: &OutputFormat) -> Result<()> {
    let page = validation::validate_page(args.page)?;
    let page_size = validation::validate_page_size(args.page_size)?;
    if page_size != 12 {
        eprintln!("Note: --page-size is ignored in scrape mode (fixed at 12).");
    }

    if args.show_donor_context {
        eprintln!("Note: --show-donor-context requires --db mode.");
    }

    if args.committee.is_some() {
        bail!("--committee is not supported in scrape mode");
    }
    if args.trade_size.is_some() {
        bail!("--trade-size is not supported in scrape mode");
    }
    if args.market_cap.is_some() {
        bail!("--market-cap is not supported in scrape mode");
    }
    if args.asset_type.is_some() {
        bail!("--asset-type is not supported in scrape mode");
    }
    if args.label.is_some() {
        bail!("--label is not supported in scrape mode");
    }

    let resp = scraper.trades_page(page).await?;
    let total_pages = resp.total_pages.unwrap_or(page);
    let total_count = resp.total_count;
    let mut trades = resp.data;
    let scraped_count = trades.len();

    if let Some(issuer_id) = args.issuer_id {
        trades.retain(|t| t.issuer_id == issuer_id);
    }

    if let Some(ref name) = args.name {
        let needle = validation::validate_search(name)?.to_lowercase();
        trades.retain(|t| {
            let full =
                format!("{} {}", t.politician.first_name, t.politician.last_name).to_lowercase();
            full.contains(&needle)
        });
    }

    if let Some(ref issuer) = args.issuer {
        let needle = validation::validate_search(issuer)?.to_lowercase();
        trades.retain(|t| {
            t.issuer.issuer_name.to_lowercase().contains(&needle)
                || t.issuer
                    .issuer_ticker
                    .as_ref()
                    .map(|s| s.to_lowercase().contains(&needle))
                    .unwrap_or(false)
        });
    }

    if let Some(ref politician) = args.politician {
        let needle = validation::validate_search(politician)?.to_lowercase();
        trades.retain(|t| {
            let full =
                format!("{} {}", t.politician.first_name, t.politician.last_name).to_lowercase();
            full.contains(&needle)
        });
    }

    if let Some(ref val) = args.party {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let p = validation::validate_party(item.trim())?;
            allowed.push(p.to_string());
        }
        trades.retain(|t| allowed.iter().any(|p| p == &t.politician.party));
    }

    if let Some(ref val) = args.state {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_state(item.trim())?;
            allowed.push(validated);
        }
        trades.retain(|t| {
            let state = t.politician.state_id.to_ascii_uppercase();
            allowed.iter().any(|s| s == &state)
        });
    }

    if let Some(ref val) = args.gender {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_gender(item.trim())?;
            allowed.push(validated.to_string());
        }
        trades.retain(|t| allowed.iter().any(|g| g == &t.politician.gender));
    }

    if let Some(ref val) = args.sector {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_sector(item.trim())?;
            allowed.push(validated.to_string());
        }
        trades.retain(|t| {
            t.issuer
                .sector
                .as_ref()
                .map(|s| allowed.iter().any(|v| v == s))
                .unwrap_or(false)
        });
    }

    if let Some(ref val) = args.tx_type {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_tx_type(item.trim())?;
            allowed.push(validated.to_string());
        }
        trades.retain(|t| allowed.iter().any(|v| v == &t.tx_type));
    }

    if let Some(ref val) = args.chamber {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_chamber(item.trim())?;
            allowed.push(validated.to_string());
        }
        trades.retain(|t| allowed.iter().any(|v| v == &t.chamber));
    }

    if let Some(ref val) = args.politician_id {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_politician_id(item.trim())?;
            allowed.push(validated);
        }
        trades.retain(|t| allowed.iter().any(|v| v == &t.politician_id));
    }

    if let Some(ref val) = args.issuer_state {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_issuer_state(item.trim())?;
            allowed.push(validated);
        }
        trades.retain(|t| {
            t.issuer
                .state_id
                .as_ref()
                .map(|s| allowed.iter().any(|v| v == s))
                .unwrap_or(false)
        });
    }

    if let Some(ref val) = args.country {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_country(item.trim())?;
            allowed.push(validated);
        }
        trades.retain(|t| {
            t.issuer
                .country
                .as_ref()
                .map(|s| allowed.iter().any(|v| v == s))
                .unwrap_or(false)
        });
    }

    // Parse absolute date filters
    let since_date = args
        .since
        .as_ref()
        .map(|s| validation::validate_date(s))
        .transpose()?;
    let until_date = args
        .until
        .as_ref()
        .map(|s| validation::validate_date(s))
        .transpose()?;
    let tx_since_date = args
        .tx_since
        .as_ref()
        .map(|s| validation::validate_date(s))
        .transpose()?;
    let tx_until_date = args
        .tx_until
        .as_ref()
        .map(|s| validation::validate_date(s))
        .transpose()?;

    if let (Some(s), Some(u)) = (since_date, until_date) {
        if s > u {
            bail!("--since ({}) must be on or before --until ({})", s, u);
        }
    }
    if let (Some(s), Some(u)) = (tx_since_date, tx_until_date) {
        if s > u {
            bail!("--tx-since ({}) must be on or before --tx-until ({})", s, u);
        }
    }

    let today = Utc::now().date_naive();
    let mut since_cutoff = since_date;
    let mut tx_since_cutoff = tx_since_date;

    if let Some(days) = args.days {
        let validated = validation::validate_days(days)?;
        since_cutoff = Some(today - chrono::Duration::days(validated));
    }

    if let Some(tx_days) = args.tx_days {
        let validated = validation::validate_days(tx_days)?;
        tx_since_cutoff = Some(today - chrono::Duration::days(validated));
    }

    let needs_filtering = since_cutoff.is_some()
        || until_date.is_some()
        || tx_since_cutoff.is_some()
        || tx_until_date.is_some();

    if needs_filtering {
        trades.retain(|t| {
            let pub_date = parse_date(&t.pub_date);
            let tx_date = NaiveDate::parse_from_str(&t.tx_date, "%Y-%m-%d").ok();
            if let Some(s) = since_cutoff {
                if pub_date.map(|d| d < s).unwrap_or(true) {
                    return false;
                }
            }
            if let Some(u) = until_date {
                if pub_date.map(|d| d > u).unwrap_or(true) {
                    return false;
                }
            }
            if let Some(s) = tx_since_cutoff {
                if tx_date.map(|d| d < s).unwrap_or(true) {
                    return false;
                }
            }
            if let Some(u) = tx_until_date {
                if tx_date.map(|d| d > u).unwrap_or(true) {
                    return false;
                }
            }
            true
        });
    }

    match args.sort_by.as_str() {
        "trade-date" => trades.sort_by_key(|t| t.tx_date.clone()),
        "reporting-gap" => trades.sort_by_key(|t| t.reporting_gap),
        _ => trades.sort_by_key(|t| t.pub_date.clone()),
    }
    if !args.asc {
        trades.reverse();
    }

    for trade in &mut trades {
        let detail = scraper.trade_detail(trade.tx_id).await?;
        trade.filing_url = detail.filing_url;
        trade.filing_id = detail.filing_id;
        if args.details_delay_ms > 0 {
            sleep(Duration::from_millis(args.details_delay_ms)).await;
        }
    }

    let mut out: Vec<Trade> = Vec::with_capacity(trades.len());
    for trade in &trades {
        out.push(scraped_trade_to_trade(trade)?);
    }

    if needs_filtering {
        eprintln!(
            "Page {}/{} ({} scraped, {} after filters)",
            page,
            total_pages,
            scraped_count,
            out.len()
        );
    } else {
        match total_count {
            Some(count) => {
                eprintln!("Page {}/{} ({} total trades)", page, total_pages, count)
            }
            None => eprintln!("Page {}/{} ({} trades)", page, total_pages, out.len()),
        }
    }

    match format {
        OutputFormat::Table => print_trades_table(&out),
        OutputFormat::Json => print_json(&out),
        OutputFormat::Csv => print_trades_csv(&out)?,
        OutputFormat::Markdown => print_trades_markdown(&out),
        OutputFormat::Xml => print_trades_xml(&out),
    }

    Ok(())
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    value
        .split('T')
        .next()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

/// Capitalize a validated party string to match DB storage format.
///
/// The validation module returns lowercase ("democrat", "republican", "other")
/// but the database stores the capitalized form from the API ("Democrat", etc.).
fn capitalize_party(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Executes the trades subcommand against the local SQLite database.
///
/// Builds a [`DbTradeFilter`] from the subset of CLI flags supported on the
/// DB path, then calls [`Db::query_trades`] and dispatches to output formatting.
pub async fn run_db(
    args: &TradesArgs,
    db_path: &std::path::Path,
    format: &OutputFormat,
) -> Result<()> {
    // Bail on filters not supported by the DB query path
    let unsupported: &[(&str, bool)] = &[
        ("--committee", args.committee.is_some()),
        ("--trade-size", args.trade_size.is_some()),
        ("--market-cap", args.market_cap.is_some()),
        ("--asset-type", args.asset_type.is_some()),
        ("--label", args.label.is_some()),
        ("--sector", args.sector.is_some()),
        ("--gender", args.gender.is_some()),
        ("--chamber", args.chamber.is_some()),
        ("--politician-id", args.politician_id.is_some()),
        ("--issuer-state", args.issuer_state.is_some()),
        ("--country", args.country.is_some()),
        ("--issuer-id", args.issuer_id.is_some()),
        ("--politician", args.politician.is_some()),
        ("--tx-days", args.tx_days.is_some()),
        ("--tx-since", args.tx_since.is_some()),
        ("--tx-until", args.tx_until.is_some()),
    ];
    for (flag, present) in unsupported {
        if *present {
            bail!(
                "{} is not yet supported with --db. Supported filters: \
                 --party, --state, --tx-type, --name, --issuer, --since, --until, --days",
                flag
            );
        }
    }

    let db = Db::open(db_path)?;

    // Build filter from supported args
    let mut filter = DbTradeFilter::default();

    if let Some(ref val) = args.party {
        let mut parts = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_party(item.trim())?;
            parts.push(capitalize_party(&validated.to_string()));
        }
        filter.party = Some(parts.join(","));
    }

    if let Some(ref val) = args.state {
        let mut parts = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_state(item.trim())?;
            parts.push(validated);
        }
        filter.state = Some(parts.join(","));
    }

    if let Some(ref val) = args.tx_type {
        let mut parts = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_tx_type(item.trim())?;
            parts.push(validated.to_string());
        }
        filter.tx_type = Some(parts.join(","));
    }

    if let Some(ref val) = args.name {
        let validated = validation::validate_search(val)?;
        filter.name = Some(validated.to_string());
    }

    if let Some(ref val) = args.issuer {
        let validated = validation::validate_search(val)?;
        filter.issuer = Some(validated.to_string());
    }

    // Handle date filters: --days converts to --since
    let today = Utc::now().date_naive();

    if let Some(days) = args.days {
        let validated = validation::validate_days(days)?;
        let since_date = today - chrono::Duration::days(validated);
        filter.since = Some(since_date.format("%Y-%m-%d").to_string());
    } else if let Some(ref val) = args.since {
        let d = validation::validate_date(val)?;
        filter.since = Some(d.format("%Y-%m-%d").to_string());
    }

    if let Some(ref val) = args.until {
        let d = validation::validate_date(val)?;
        filter.until = Some(d.format("%Y-%m-%d").to_string());
    }

    // Validate date range consistency
    if let (Some(ref s), Some(ref u)) = (&filter.since, &filter.until) {
        let since_d = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
        let until_d = NaiveDate::parse_from_str(u, "%Y-%m-%d")?;
        if since_d > until_d {
            bail!("--since ({}) must be on or before --until ({})", s, u);
        }
    }

    filter.limit = Some(args.page_size);

    let rows = db.query_trades(&filter)?;
    eprintln!("{} trades from database", rows.len());

    // Best-effort analytics enrichment: compute performance metrics for closed trades
    let metrics_map: HashMap<(String, String), TradeMetrics> = match load_analytics_metrics(&db) {
        Ok(map) => map,
        Err(e) => {
            eprintln!("Note: Analytics data unavailable ({}). Run 'enrich-prices' to enable performance metrics.", e);
            HashMap::new()
        }
    };

    // Enrich rows with analytics data (clone rows for donor context display)
    let enriched_rows: Vec<EnrichedDbTradeRow> = rows
        .iter()
        .cloned()
        .map(|row| enrich_trade_row(row, &metrics_map))
        .collect();

    match format {
        OutputFormat::Table => print_enriched_trades_table(&enriched_rows),
        OutputFormat::Json => print_json(&enriched_rows),
        OutputFormat::Csv => print_enriched_trades_csv(&enriched_rows)?,
        OutputFormat::Markdown => print_enriched_trades_markdown(&enriched_rows),
        OutputFormat::Xml => print_enriched_trades_xml(&enriched_rows),
    }

    // Donor context display (opt-in via --show-donor-context)
    if args.show_donor_context {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut any_context = false;

        for trade in &rows {
            if let Some(ref sector) = trade.issuer_sector {
                let key = (trade.politician_id.clone(), sector.clone());
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);

                let context = db.get_donor_context_for_sector(
                    &trade.politician_id,
                    sector,
                    5,  // top 5 employers
                )?;

                if !context.is_empty() {
                    if !any_context {
                        eprintln!("\n--- Donor Context ---");
                        any_context = true;
                    }
                    eprintln!(
                        "\n{} - {} sector:",
                        trade.politician_name, sector
                    );
                    for dc in &context {
                        eprintln!(
                            "  {:40} ${:>12.0} ({} donations)",
                            dc.employer, dc.total_amount, dc.donation_count
                        );
                    }
                }
            }
        }

        if !any_context && !rows.is_empty() {
            eprintln!("\nNo donor context available. Run 'map-employers load-seed' or 'map-employers export/import' to build employer mappings.");
        }
    }

    Ok(())
}

/// Enriched trade row with optional analytics performance metrics.
///
/// Extends [`DbTradeRow`] with absolute_return and alpha for closed trades.
/// All analytics fields are Option types for backward compatibility.
#[derive(Serialize, Clone)]
pub struct EnrichedDbTradeRow {
    // Base DbTradeRow fields
    pub tx_id: i64,
    pub pub_date: String,
    pub tx_date: String,
    pub tx_type: String,
    pub value: i64,
    pub price: Option<f64>,
    pub size: Option<i64>,
    pub filing_url: String,
    pub reporting_gap: i64,
    pub enriched_at: Option<String>,
    pub trade_date_price: Option<f64>,
    pub current_price: Option<f64>,
    pub price_enriched_at: Option<String>,
    pub estimated_shares: Option<f64>,
    pub estimated_value: Option<f64>,
    pub politician_name: String,
    pub party: String,
    pub state: String,
    pub chamber: String,
    pub issuer_name: String,
    pub issuer_ticker: String,
    pub asset_type: String,
    pub committees: Vec<String>,
    pub labels: Vec<String>,
    pub politician_id: String,
    pub issuer_sector: Option<String>,
    // Analytics enrichment fields (sell trades only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_return: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
}

impl From<DbTradeRow> for EnrichedDbTradeRow {
    fn from(row: DbTradeRow) -> Self {
        Self {
            tx_id: row.tx_id,
            pub_date: row.pub_date,
            tx_date: row.tx_date,
            tx_type: row.tx_type,
            value: row.value,
            price: row.price,
            size: row.size,
            filing_url: row.filing_url,
            reporting_gap: row.reporting_gap,
            enriched_at: row.enriched_at,
            trade_date_price: row.trade_date_price,
            current_price: row.current_price,
            price_enriched_at: row.price_enriched_at,
            estimated_shares: row.estimated_shares,
            estimated_value: row.estimated_value,
            politician_name: row.politician_name,
            party: row.party,
            state: row.state,
            chamber: row.chamber,
            issuer_name: row.issuer_name,
            issuer_ticker: row.issuer_ticker,
            asset_type: row.asset_type,
            committees: row.committees,
            labels: row.labels,
            politician_id: row.politician_id,
            issuer_sector: row.issuer_sector,
            absolute_return: None,
            alpha: None,
        }
    }
}

/// Load analytics metrics for all closed trades.
///
/// Returns a HashMap keyed by (politician_id, ticker) with the most recent
/// TradeMetrics for each sell trade. Best-effort: returns error if price
/// enrichment data is unavailable.
fn load_analytics_metrics(db: &Db) -> Result<HashMap<(String, String), TradeMetrics>> {
    // Query price-enriched trades for analytics
    let analytics_rows = db.query_trades_for_analytics()?;
    if analytics_rows.is_empty() {
        bail!("no price-enriched trades available");
    }

    // Convert to AnalyticsTrade
    let analytics_trades: Vec<AnalyticsTrade> = analytics_rows
        .into_iter()
        .map(|row| AnalyticsTrade {
            tx_id: row.tx_id,
            politician_id: row.politician_id,
            ticker: row.issuer_ticker,
            tx_type: row.tx_type,
            tx_date: row.tx_date,
            estimated_shares: row.estimated_shares,
            trade_date_price: row.trade_date_price,
            benchmark_price: row.benchmark_price,
            has_sector_benchmark: row.gics_sector.is_some(),
            gics_sector: row.gics_sector,
        })
        .collect();

    // Calculate closed trades using FIFO
    let closed_trades = calculate_closed_trades(analytics_trades, false);

    // Compute metrics for each closed trade
    let all_metrics: Vec<TradeMetrics> = closed_trades
        .iter()
        .map(compute_trade_metrics)
        .collect();

    // Build HashMap keyed by (politician_id, ticker)
    // For multiple closed trades of same (politician, ticker), keep most recent
    let mut metrics_map: HashMap<(String, String), TradeMetrics> = HashMap::new();
    for metric in all_metrics {
        let key = (metric.politician_id.clone(), metric.ticker.clone());
        metrics_map.insert(key, metric);
    }

    Ok(metrics_map)
}

/// Enrich a DbTradeRow with analytics performance metrics.
///
/// Sell trades are enriched with absolute_return and alpha from the metrics map.
/// Buy trades and trades without matching metrics remain unenriched (None fields).
fn enrich_trade_row(
    row: DbTradeRow,
    metrics_map: &HashMap<(String, String), TradeMetrics>,
) -> EnrichedDbTradeRow {
    let mut enriched = EnrichedDbTradeRow::from(row);

    // Only sell trades can have metrics (closed trades are recorded at sell time)
    if enriched.tx_type == "sell" {
        let key = (enriched.politician_id.clone(), enriched.issuer_ticker.clone());
        if let Some(metrics) = metrics_map.get(&key) {
            enriched.absolute_return = Some(metrics.absolute_return);
            enriched.alpha = metrics.alpha;
        }
    }

    enriched
}

fn scraped_trade_to_trade(trade: &ScrapedTrade) -> Result<Trade> {
    let filing_url = trade.filing_url.clone().unwrap_or_default();
    if filing_url.is_empty() {
        bail!("missing filing URL for trade {}", trade.tx_id);
    }

    let filing_date = trade
        .pub_date
        .split('T')
        .next()
        .unwrap_or(trade.pub_date.as_str());

    let politician_state = trade.politician.state_id.to_ascii_uppercase();

    let asset_ticker = trade.issuer.issuer_ticker.clone();
    let issuer_ticker = trade.issuer.issuer_ticker.clone();

    let json = serde_json::json!({
        "_txId": trade.tx_id,
        "_politicianId": trade.politician_id,
        "_assetId": trade.tx_id,
        "_issuerId": trade.issuer_id,
        "pubDate": trade.pub_date,
        "filingDate": filing_date,
        "txDate": trade.tx_date,
        "txType": trade.tx_type,
        "txTypeExtended": trade.tx_type_extended,
        "hasCapitalGains": false,
        "owner": trade.owner,
        "chamber": trade.chamber,
        "price": trade.price,
        "size": null,
        "sizeRangeHigh": null,
        "sizeRangeLow": null,
        "value": trade.value,
        "filingId": trade.filing_id.unwrap_or(0),
        "filingURL": filing_url,
        "reportingGap": trade.reporting_gap,
        "comment": trade.comment,
        "committees": [],
        "asset": {
            "assetType": "unknown",
            "assetTicker": asset_ticker,
            "instrument": null
        },
        "issuer": {
            "_stateId": trade.issuer.state_id,
            "c2iq": trade.issuer.c2iq,
            "country": trade.issuer.country,
            "issuerName": trade.issuer.issuer_name,
            "issuerTicker": issuer_ticker,
            "sector": trade.issuer.sector
        },
        "politician": {
            "_stateId": politician_state,
            "chamber": trade.politician.chamber,
            "dob": trade.politician.dob,
            "firstName": trade.politician.first_name,
            "gender": trade.politician.gender,
            "lastName": trade.politician.last_name,
            "nickname": trade.politician.nickname,
            "party": trade.politician.party
        },
        "labels": []
    });

    let trade: Trade = serde_json::from_value(json)?;
    Ok(trade)
}
