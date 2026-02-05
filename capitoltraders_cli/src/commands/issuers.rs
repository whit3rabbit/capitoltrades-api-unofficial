use anyhow::Result;
use clap::Args;
use capitoltraders_lib::types::{MarketCap, Sector};
use capitoltraders_lib::{CachedClient, IssuerQuery, IssuerSortBy, Query, SortDirection};
use capitoltraders_lib::validation;

use crate::output::{
    print_issuers_csv, print_issuers_markdown, print_issuers_table, print_json, OutputFormat,
};

#[derive(Args)]
pub struct IssuersArgs {
    /// Get a single issuer by ID
    #[arg(long)]
    pub id: Option<i64>,

    /// Filter by sector (e.g. information-technology, financials, health-care)
    #[arg(long)]
    pub sector: Option<String>,

    /// Filter by market cap: mega, large, mid, small, micro, nano
    #[arg(long)]
    pub market_cap: Option<String>,

    /// Search by name
    #[arg(long)]
    pub search: Option<String>,

    /// Filter by US state code (e.g. CA, TX, NY)
    #[arg(long)]
    pub state: Option<String>,

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
        }
        return Ok(());
    }

    let mut query = IssuerQuery::default()
        .with_page(args.page)
        .with_page_size(args.page_size);

    if let Some(search) = &args.search {
        query = query.with_search(search);
    }

    if let Some(ref state) = args.state {
        let validated = validation::validate_state(state)?;
        query = query.with_state(&validated);
    }

    if let Some(sector) = &args.sector {
        let s = match sector.as_str() {
            "communication-services" => Sector::CommunicationServices,
            "consumer-discretionary" => Sector::ConsumerDiscretionary,
            "consumer-staples" => Sector::ConsumerStaples,
            "energy" => Sector::Energy,
            "financials" => Sector::Financials,
            "health-care" => Sector::HealthCare,
            "industrials" => Sector::Industrials,
            "information-technology" => Sector::InformationTechnology,
            "materials" => Sector::Materials,
            "real-estate" => Sector::RealEstate,
            "utilities" => Sector::Utilities,
            _ => Sector::Other,
        };
        query = query.with_sector(s);
    }

    if let Some(mcap) = &args.market_cap {
        let m = match mcap.as_str() {
            "mega" => MarketCap::Mega,
            "large" => MarketCap::Large,
            "mid" => MarketCap::Mid,
            "small" => MarketCap::Small,
            "micro" => MarketCap::Micro,
            "nano" => MarketCap::Nano,
            _ => MarketCap::Mega,
        };
        query = query.with_market_cap(m);
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
    }

    Ok(())
}
