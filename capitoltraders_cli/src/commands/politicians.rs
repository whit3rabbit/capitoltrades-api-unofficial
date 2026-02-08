//! The `politicians` subcommand: lists and filters politicians who trade.

use std::path::PathBuf;

use anyhow::{bail, Result};
use capitoltraders_lib::types::PoliticianDetail;
use capitoltraders_lib::validation;
use capitoltraders_lib::{Db, DbPoliticianFilter, ScrapeClient, ScrapedPoliticianCard};
use chrono::NaiveDate;
use clap::Args;

use crate::output::{
    print_db_politicians_csv, print_db_politicians_markdown, print_db_politicians_table,
    print_db_politicians_xml, print_json, print_politicians_csv, print_politicians_markdown,
    print_politicians_table, print_politicians_xml, OutputFormat,
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

    match format {
        OutputFormat::Table => print_db_politicians_table(&rows),
        OutputFormat::Json => print_json(&rows),
        OutputFormat::Csv => print_db_politicians_csv(&rows)?,
        OutputFormat::Markdown => print_db_politicians_markdown(&rows),
        OutputFormat::Xml => print_db_politicians_xml(&rows),
    }

    Ok(())
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
