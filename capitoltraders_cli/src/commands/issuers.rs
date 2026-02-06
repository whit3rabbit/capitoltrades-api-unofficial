use anyhow::Result;
use clap::Args;
use capitoltraders_lib::{CachedClient, IssuerQuery, IssuerSortBy, Query, SortDirection};
use capitoltraders_lib::validation;

use crate::output::{
    print_issuers_csv, print_issuers_markdown, print_issuers_table, print_issuers_xml, print_json,
    OutputFormat,
};

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
    #[arg(long, default_value = "20")]
    pub page_size: i64,

    /// Sort field: volume, politicians, trades, last-traded, mcap
    #[arg(long, default_value = "volume")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,
}

pub async fn run(args: &IssuersArgs, client: &CachedClient, format: &OutputFormat) -> Result<()> {
    if let Some(id) = args.id {
        let resp = client.get_issuer(id).await?;
        match format {
            OutputFormat::Table => print_issuers_table(&[resp.data]),
            OutputFormat::Json => print_json(&resp.data),
            OutputFormat::Csv => print_issuers_csv(&[resp.data])?,
            OutputFormat::Markdown => print_issuers_markdown(&[resp.data]),
            OutputFormat::Xml => print_issuers_xml(&[resp.data]),
        }
        return Ok(());
    }

    let mut query = IssuerQuery::default()
        .with_page(args.page)
        .with_page_size(args.page_size);

    if let Some(search) = &args.search {
        let validated = validation::validate_search(search)?;
        query = query.with_search(&validated);
    }

    if let Some(ref val) = args.state {
        for item in val.split(',') {
            let validated = validation::validate_state(item.trim())?;
            query = query.with_state(&validated);
        }
    }

    if let Some(ref val) = args.sector {
        for item in val.split(',') {
            let validated = validation::validate_sector(item.trim())?;
            query = query.with_sector(validated);
        }
    }

    if let Some(ref val) = args.market_cap {
        for item in val.split(',') {
            let validated = validation::validate_market_cap(item.trim())?;
            query = query.with_market_cap(validated);
        }
    }

    if let Some(ref val) = args.country {
        for item in val.split(',') {
            let validated = validation::validate_country(item.trim())?;
            query = query.with_country(&validated);
        }
    }

    if let Some(ref val) = args.politician_id {
        for item in val.split(',') {
            let validated = validation::validate_politician_id(item.trim())?;
            query = query.with_politician_id(validated);
        }
    }

    let sort_by = match args.sort_by.as_str() {
        "politicians" => IssuerSortBy::PoliticiansCount,
        "trades" => IssuerSortBy::TotalTrades,
        "last-traded" => IssuerSortBy::DateLastTraded,
        "mcap" => IssuerSortBy::MarketCap,
        _ => IssuerSortBy::TradedVolume,
    };
    query = query.with_sort_by(sort_by);

    if args.asc {
        query = query.with_sort_direction(SortDirection::Asc);
    }

    let resp = client.get_issuers(&query).await?;

    eprintln!(
        "Page {}/{} ({} total issuers)",
        resp.meta.paging.page, resp.meta.paging.total_pages, resp.meta.paging.total_items
    );

    match format {
        OutputFormat::Table => print_issuers_table(&resp.data),
        OutputFormat::Json => print_json(&resp.data),
        OutputFormat::Csv => print_issuers_csv(&resp.data)?,
        OutputFormat::Markdown => print_issuers_markdown(&resp.data),
        OutputFormat::Xml => print_issuers_xml(&resp.data),
    }

    Ok(())
}
