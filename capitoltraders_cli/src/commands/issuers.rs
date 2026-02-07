//! The `issuers` subcommand: lists and filters companies/funds traded by politicians.

use anyhow::{bail, Result};
use clap::Args;
use capitoltraders_lib::{ScrapeClient, ScrapedIssuerDetail, ScrapedIssuerList};
use capitoltraders_lib::validation;
use capitoltraders_lib::types::IssuerDetail;

use crate::output::{
    print_issuers_csv, print_issuers_markdown, print_issuers_table, print_issuers_xml, print_json,
    OutputFormat,
};

/// Arguments for the `issuers` subcommand.
///
/// Supports single-issuer lookup by ID or paginated listing with filters.
#[derive(Args)]
pub struct IssuersArgs {
    /// Get a single issuer by ID
    #[arg(long)]
    pub id: Option<i64>,

    /// Filter by sector (comma-separated, e.g. information-technology,financials)
    #[arg(long)]
    pub sector: Option<String>,

    /// Filter by market cap (comma-separated): mega, large, mid, small, micro, nano
    #[arg(long)]
    pub market_cap: Option<String>,

    /// Search by name
    #[arg(long)]
    pub search: Option<String>,

    /// Filter by US state code (comma-separated, e.g. CA,TX,NY)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by country (comma-separated, 2-letter ISO codes, e.g. us,ca)
    #[arg(long)]
    pub country: Option<String>,

    /// Filter by politician ID (comma-separated, e.g. P000197,P000123)
    #[arg(long)]
    pub politician_id: Option<String>,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: i64,

    /// Results per page
    #[arg(long, default_value = "12")]
    pub page_size: i64,

    /// Sort field: volume, politicians, trades, last-traded, mcap
    #[arg(long, default_value = "volume")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,
}

/// Executes the issuers subcommand: validates inputs, scrapes results,
/// applies client-side filtering and sorting, then prints output.
pub async fn run(args: &IssuersArgs, scraper: &ScrapeClient, format: &OutputFormat) -> Result<()> {
    if let Some(id) = args.id {
        let detail = scraper.issuer_detail(id).await?;
        let issuer = scraped_issuer_detail_to_detail(&detail)?;
        match format {
            OutputFormat::Table => print_issuers_table(&[issuer]),
            OutputFormat::Json => print_json(&issuer),
            OutputFormat::Csv => print_issuers_csv(&[issuer])?,
            OutputFormat::Markdown => print_issuers_markdown(&[issuer]),
            OutputFormat::Xml => print_issuers_xml(&[issuer]),
        }
        return Ok(());
    }

    let page = validation::validate_page(args.page)?;
    let page_size = validation::validate_page_size(args.page_size)?;
    if page_size != 12 {
        eprintln!("Note: --page-size is ignored in scrape mode (fixed at 12).");
    }

    if args.market_cap.is_some() {
        bail!("--market-cap is not supported in scrape mode");
    }
    if args.state.is_some() {
        bail!("--state is not supported in scrape mode");
    }
    if args.country.is_some() {
        bail!("--country is not supported in scrape mode");
    }
    if args.politician_id.is_some() {
        bail!("--politician-id is not supported in scrape mode");
    }
    if args.sort_by == "mcap" {
        bail!("--sort-by mcap is not supported in scrape mode");
    }

    let resp = scraper.issuers_page(page).await?;
    let total_pages = resp.total_pages.unwrap_or(page);
    let total_count = resp.total_count;
    let mut issuers = resp.data;

    if let Some(search) = &args.search {
        let needle = validation::validate_search(search)?.to_lowercase();
        issuers.retain(|i| {
            i.issuer_name.to_lowercase().contains(&needle)
                || i.issuer_ticker
                    .as_ref()
                    .map(|s| s.to_lowercase().contains(&needle))
                    .unwrap_or(false)
        });
    }

    if let Some(ref val) = args.sector {
        let mut allowed = Vec::new();
        for item in val.split(',') {
            let validated = validation::validate_sector(item.trim())?;
            allowed.push(validated.to_string());
        }
        issuers.retain(|i| {
            i.sector
                .as_ref()
                .map(|s| allowed.iter().any(|v| v == s))
                .unwrap_or(false)
        });
    }

    match args.sort_by.as_str() {
        "politicians" => issuers.sort_by_key(|i| i.stats.count_politicians),
        "trades" => issuers.sort_by_key(|i| i.stats.count_trades),
        "last-traded" => issuers.sort_by_key(|i| i.stats.date_last_traded.clone()),
        _ => issuers.sort_by_key(|i| i.stats.volume),
    }
    if !args.asc {
        issuers.reverse();
    }

    let mut out: Vec<IssuerDetail> = Vec::with_capacity(issuers.len());
    for issuer in &issuers {
        out.push(scraped_issuer_list_to_detail(issuer)?);
    }

    match total_count {
        Some(count) => eprintln!("Page {}/{} ({} total issuers)", page, total_pages, count),
        None => eprintln!("Page {}/{} ({} issuers)", page, total_pages, out.len()),
    }

    match format {
        OutputFormat::Table => print_issuers_table(&out),
        OutputFormat::Json => print_json(&out),
        OutputFormat::Csv => print_issuers_csv(&out)?,
        OutputFormat::Markdown => print_issuers_markdown(&out),
        OutputFormat::Xml => print_issuers_xml(&out),
    }

    Ok(())
}

fn scraped_issuer_list_to_detail(issuer: &ScrapedIssuerList) -> Result<IssuerDetail> {
    let json = serde_json::json!({
        "_issuerId": issuer.issuer_id,
        "_stateId": null,
        "c2iq": null,
        "country": null,
        "issuerName": issuer.issuer_name,
        "issuerTicker": issuer.issuer_ticker,
        "performance": null,
        "sector": issuer.sector,
        "stats": {
            "countTrades": issuer.stats.count_trades,
            "countPoliticians": issuer.stats.count_politicians,
            "volume": issuer.stats.volume,
            "dateLastTraded": issuer.stats.date_last_traded
        }
    });
    let detail: IssuerDetail = serde_json::from_value(json)?;
    Ok(detail)
}

fn scraped_issuer_detail_to_detail(detail: &ScrapedIssuerDetail) -> Result<IssuerDetail> {
    let performance = normalize_performance(detail.performance.clone());
    let json = serde_json::json!({
        "_issuerId": detail.issuer_id,
        "_stateId": detail.state_id,
        "c2iq": detail.c2iq,
        "country": detail.country,
        "issuerName": detail.issuer_name,
        "issuerTicker": detail.issuer_ticker,
        "performance": performance,
        "sector": detail.sector,
        "stats": {
            "countTrades": detail.stats.count_trades,
            "countPoliticians": detail.stats.count_politicians,
            "volume": detail.stats.volume,
            "dateLastTraded": detail.stats.date_last_traded
        }
    });
    let detail: IssuerDetail = serde_json::from_value(json)?;
    Ok(detail)
}

fn normalize_performance(value: Option<serde_json::Value>) -> serde_json::Value {
    let Some(value) = value else {
        return serde_json::Value::Null;
    };
    let Some(obj) = value.as_object() else {
        return serde_json::Value::Null;
    };
    let required = [
        "eodPrices",
        "mcap",
        "trailing1",
        "trailing1Change",
        "trailing7",
        "trailing7Change",
        "trailing30",
        "trailing30Change",
        "trailing90",
        "trailing90Change",
        "trailing365",
        "trailing365Change",
        "wtd",
        "wtdChange",
        "mtd",
        "mtdChange",
        "qtd",
        "qtdChange",
        "ytd",
        "ytdChange",
    ];
    if required
        .iter()
        .all(|key| obj.get(*key).map(|v| !v.is_null()).unwrap_or(false))
    {
        value
    } else {
        serde_json::Value::Null
    }
}
