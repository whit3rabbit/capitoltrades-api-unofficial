//! The `politicians` subcommand: lists and filters politicians who trade.

use std::path::PathBuf;

use anyhow::{bail, Result};
use capitoltraders_lib::analytics::{
    aggregate_politician_metrics, calculate_closed_trades, compute_trade_metrics, AnalyticsTrade,
    PoliticianMetrics,
};
use capitoltraders_lib::types::PoliticianDetail;
use capitoltraders_lib::validation;
use capitoltraders_lib::{Db, DbPoliticianFilter, DbPoliticianRow, ScrapeClient, ScrapedPoliticianCard};
use chrono::NaiveDate;
use clap::Args;
use serde::Serialize;
use std::collections::HashMap;

use crate::output::{
    print_enriched_politicians_csv, print_enriched_politicians_markdown,
    print_enriched_politicians_table, print_enriched_politicians_xml, print_json,
    print_politicians_csv, print_politicians_markdown, print_politicians_table,
    print_politicians_xml, OutputFormat,
};

/// Arguments for the `politicians` subcommand.
///
/// Supports filtering by party, name, state, committee, and issuer ID.
#[derive(Args)]
pub struct PoliticiansArgs {
    /// Filter by party (comma-separated): democrat (d), republican (r), other
    #[arg(long)]
    pub party: Option<String>,

    /// Search by politician name
    #[arg(long)]
    pub name: Option<String>,

    /// Search by name (hidden alias for --name, kept for backwards compatibility)
    #[arg(long, hide = true)]
    pub search: Option<String>,

    /// Filter by US state code (comma-separated, e.g. CA,TX,NY)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by committee (comma-separated, code or full name)
    #[arg(long)]
    pub committee: Option<String>,

    /// Filter by issuer ID (comma-separated, numeric)
    #[arg(long)]
    pub issuer_id: Option<String>,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: i64,

    /// Results per page
    #[arg(long, default_value = "12")]
    pub page_size: i64,

    /// Sort field: volume, name, issuers, trades, last-traded
    #[arg(long, default_value = "volume")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,

    /// Read politicians from local SQLite database (requires prior sync)
    #[arg(long)]
    pub db: Option<PathBuf>,
}

/// Enriched politician row with optional analytics summary fields.
///
/// Extends [`DbPoliticianRow`] with closed_trades, avg_return, win_rate, and percentile.
/// All analytics fields are Option types for backward compatibility.
#[derive(Serialize, Clone)]
pub struct EnrichedDbPoliticianRow {
    // Base DbPoliticianRow fields
    pub politician_id: String,
    pub name: String,
    pub party: String,
    pub state: String,
    pub chamber: String,
    pub committees: Vec<String>,
    pub trades: i64,
    pub volume: i64,
    // Analytics enrichment fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_trades: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_return: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentile: Option<f64>,
}

impl From<DbPoliticianRow> for EnrichedDbPoliticianRow {
    fn from(row: DbPoliticianRow) -> Self {
        Self {
            politician_id: row.politician_id,
            name: row.name,
            party: row.party,
            state: row.state,
            chamber: row.chamber,
            committees: row.committees,
            trades: row.trades,
            volume: row.volume,
            closed_trades: None,
            avg_return: None,
            win_rate: None,
            percentile: None,
        }
    }
}

/// Executes the politicians subcommand: validates inputs, scrapes results,
/// applies client-side filtering and sorting, then prints output.
pub async fn run(
    args: &PoliticiansArgs,
    scraper: &ScrapeClient,
    format: &OutputFormat,
) -> Result<()> {
    let page = validation::validate_page(args.page)?;
    let page_size = validation::validate_page_size(args.page_size)?;
    if page_size != 12 {
        eprintln!("Note: --page-size is ignored in scrape mode (fixed at 12).");
    }

    if args.committee.is_some() {
        bail!("--committee is not supported in scrape mode");
    }
    if args.issuer_id.is_some() {
        bail!("--issuer-id is not supported in scrape mode");
    }

    let resp = scraper.politicians_page(page).await?;
    let total_pages = resp.total_pages.unwrap_or(page);
    let total_count = resp.total_count;
    let mut cards = resp.data;

    if let Some(ref val) = args.party {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let p = validation::validate_party(item.trim())?;
            allowed.push(p.to_string());
        }
        cards.retain(|c| allowed.iter().any(|p| p == &c.party));
    }

    let search_input = args.name.as_ref().or(args.search.as_ref());
    if let Some(search) = search_input {
        let needle = validation::validate_search(search)?.to_lowercase();
        cards.retain(|c| c.name.to_lowercase().contains(&needle));
    }

    if let Some(ref val) = args.state {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_state(item.trim())?;
            allowed.push(validated);
        }
        cards.retain(|c| allowed.iter().any(|s| s == &c.state));
    }

    let mut records = Vec::with_capacity(cards.len());
    for card in cards {
        let detail = scraper
            .politician_detail(&card.politician_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "missing detail payload for politician {}",
                    card.politician_id
                )
            })?;
        records.push((card, detail));
    }

    match args.sort_by.as_str() {
        "name" => records.sort_by_key(|(_, detail)| detail.last_name.clone()),
        "issuers" => records.sort_by_key(|(card, _)| card.issuers),
        "trades" => records.sort_by_key(|(card, _)| card.trades),
        "last-traded" => records.sort_by_key(|(card, _)| parse_date_opt(&card.last_traded)),
        _ => records.sort_by_key(|(card, _)| card.volume),
    }
    if !args.asc {
        records.reverse();
    }

    let mut out: Vec<PoliticianDetail> = Vec::with_capacity(records.len());
    for (card, detail) in &records {
        out.push(scraped_politician_to_detail(card, detail)?);
    }

    match total_count {
        Some(count) => eprintln!(
            "Page {}/{} ({} total politicians)",
            page, total_pages, count
        ),
        None => eprintln!("Page {}/{} ({} politicians)", page, total_pages, out.len()),
    }

    match format {
        OutputFormat::Table => print_politicians_table(&out),
        OutputFormat::Json => print_json(&out),
        OutputFormat::Csv => print_politicians_csv(&out)?,
        OutputFormat::Markdown => print_politicians_markdown(&out),
        OutputFormat::Xml => print_politicians_xml(&out),
    }

    Ok(())
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

/// Executes the politicians subcommand against the local SQLite database.
///
/// Builds a [`DbPoliticianFilter`] from the subset of CLI flags supported on the
/// DB path, then calls [`Db::query_politicians`] and dispatches to output formatting.
pub async fn run_db(
    args: &PoliticiansArgs,
    db_path: &std::path::Path,
    format: &OutputFormat,
) -> Result<()> {
    // Bail on filters not supported by the DB query path
    let unsupported: &[(&str, bool)] = &[
        ("--committee", args.committee.is_some()),
        ("--issuer-id", args.issuer_id.is_some()),
    ];
    for (flag, present) in unsupported {
        if *present {
            bail!(
                "{} is not supported with --db. Supported filters: \
                 --party, --state, --name, --chamber",
                flag
            );
        }
    }

    let db = Db::open(db_path)?;

    // Build filter from supported args
    let mut filter = DbPoliticianFilter::default();

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

    let search_input = args.name.as_ref().or(args.search.as_ref());
    if let Some(search) = search_input {
        let validated = validation::validate_search(search)?;
        filter.name = Some(validated.to_string());
    }

    filter.limit = Some(args.page_size);

    let rows = db.query_politicians(&filter)?;
    eprintln!("{} politicians from database", rows.len());

    // Best-effort analytics enrichment: compute politician performance metrics
    let metrics_map: HashMap<String, PoliticianMetrics> = match load_politician_analytics(&db) {
        Ok(map) => map,
        Err(e) => {
            eprintln!(
                "Note: Analytics data unavailable ({}). Run 'enrich-prices' to enable performance metrics.",
                e
            );
            HashMap::new()
        }
    };

    // Enrich rows with analytics data
    let enriched_rows: Vec<EnrichedDbPoliticianRow> = rows
        .into_iter()
        .map(|row| enrich_politician_row(row, &metrics_map))
        .collect();

    match format {
        OutputFormat::Table => print_enriched_politicians_table(&enriched_rows),
        OutputFormat::Json => print_json(&enriched_rows),
        OutputFormat::Csv => print_enriched_politicians_csv(&enriched_rows)?,
        OutputFormat::Markdown => print_enriched_politicians_markdown(&enriched_rows),
        OutputFormat::Xml => print_enriched_politicians_xml(&enriched_rows),
    }

    Ok(())
}

/// Load politician analytics metrics from price-enriched trades.
///
/// Returns a HashMap keyed by politician_id with aggregated performance metrics.
/// Best-effort: returns error if price enrichment data is unavailable.
fn load_politician_analytics(db: &Db) -> Result<HashMap<String, PoliticianMetrics>> {
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
    let closed_trades = calculate_closed_trades(analytics_trades);

    // Compute metrics for each closed trade
    let all_metrics: Vec<_> = closed_trades.iter().map(compute_trade_metrics).collect();

    // Aggregate by politician
    let politician_metrics = aggregate_politician_metrics(&all_metrics);

    // Build HashMap keyed by politician_id
    let metrics_map: HashMap<String, PoliticianMetrics> = politician_metrics
        .into_iter()
        .map(|m| (m.politician_id.clone(), m))
        .collect();

    Ok(metrics_map)
}

/// Enrich a DbPoliticianRow with analytics performance metrics.
fn enrich_politician_row(
    row: DbPoliticianRow,
    metrics_map: &HashMap<String, PoliticianMetrics>,
) -> EnrichedDbPoliticianRow {
    let mut enriched = EnrichedDbPoliticianRow::from(row);

    if let Some(metrics) = metrics_map.get(&enriched.politician_id) {
        enriched.closed_trades = Some(metrics.total_trades);
        enriched.avg_return = Some(metrics.avg_return);
        enriched.win_rate = Some(metrics.win_rate);
        enriched.percentile = Some(metrics.percentile_rank * 100.0); // Convert to percentage
    }

    enriched
}

fn parse_date_opt(value: &Option<String>) -> Option<NaiveDate> {
    value
        .as_ref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

fn scraped_politician_to_detail(
    card: &ScrapedPoliticianCard,
    detail: &capitoltraders_lib::scrape::ScrapedPolitician,
) -> Result<PoliticianDetail> {
    let state = detail.state_id.to_ascii_uppercase();
    let full_name = format!("{} {}", detail.first_name, detail.last_name);

    let json = serde_json::json!({
        "_politicianId": card.politician_id,
        "_stateId": state,
        "party": detail.party,
        "partyOther": null,
        "district": null,
        "firstName": detail.first_name,
        "lastName": detail.last_name,
        "nickname": detail.nickname,
        "middleName": null,
        "fullName": full_name,
        "dob": detail.dob,
        "gender": detail.gender,
        "socialFacebook": null,
        "socialTwitter": null,
        "socialYoutube": null,
        "website": null,
        "chamber": detail.chamber,
        "committees": [],
        "stats": {
            "dateLastTraded": card.last_traded,
            "countTrades": card.trades,
            "countIssuers": card.issuers,
            "volume": card.volume
        }
    });

    let detail: PoliticianDetail = serde_json::from_value(json)?;
    Ok(detail)
}
