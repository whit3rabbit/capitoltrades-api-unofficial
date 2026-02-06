use anyhow::Result;
use clap::Args;
use capitoltraders_lib::{CachedClient, PoliticianQuery, PoliticianSortBy, Query, SortDirection};
use capitoltraders_lib::validation;

use crate::output::{
    print_json, print_politicians_csv, print_politicians_markdown, print_politicians_table,
    print_politicians_xml, OutputFormat,
};

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
    #[arg(long, default_value = "20")]
    pub page_size: i64,

    /// Sort field: volume, name, issuers, trades, last-traded
    #[arg(long, default_value = "volume")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,
}

pub async fn run(
    args: &PoliticiansArgs,
    client: &CachedClient,
    format: &OutputFormat,
) -> Result<()> {
    let mut query = PoliticianQuery::default()
        .with_page(args.page)
        .with_page_size(args.page_size);

    if let Some(ref val) = args.party {
        for item in val.split(',') {
            let p = validation::validate_party(item.trim())?;
            query = query.with_party(&p);
        }
    }

    // --name takes precedence over --search (hidden alias)
    let search_input = args.name.as_ref().or(args.search.as_ref());
    if let Some(search) = search_input {
        let sanitized = validation::validate_search(search)?;
        query = query.with_search(&sanitized);
    }

    if let Some(ref val) = args.state {
        for item in val.split(',') {
            let validated = validation::validate_state(item.trim())?;
            query = query.with_state(&validated);
        }
    }

    if let Some(ref val) = args.committee {
        for item in val.split(',') {
            let validated = validation::validate_committee(item.trim())?;
            query = query.with_committee(&validated);
        }
    }

    if let Some(ref val) = args.issuer_id {
        for item in val.split(',') {
            let id: i64 = item.trim().parse().map_err(|_| {
                anyhow::anyhow!("invalid issuer ID '{}': must be numeric", item.trim())
            })?;
            query = query.with_issuer_id(id);
        }
    }

    let sort_by = match args.sort_by.as_str() {
        "name" => PoliticianSortBy::LastName,
        "issuers" => PoliticianSortBy::TradedIssuersCount,
        "trades" => PoliticianSortBy::TotalTrades,
        "last-traded" => PoliticianSortBy::DateLastTraded,
        _ => PoliticianSortBy::TradedVolume,
    };
    query = query.with_sort_by(sort_by);

    if args.asc {
        query = query.with_sort_direction(SortDirection::Asc);
    }

    let resp = client.get_politicians(&query).await?;

    eprintln!(
        "Page {}/{} ({} total politicians)",
        resp.meta.paging.page, resp.meta.paging.total_pages, resp.meta.paging.total_items
    );

    match format {
        OutputFormat::Table => print_politicians_table(&resp.data),
        OutputFormat::Json => print_json(&resp.data),
        OutputFormat::Csv => print_politicians_csv(&resp.data)?,
        OutputFormat::Markdown => print_politicians_markdown(&resp.data),
        OutputFormat::Xml => print_politicians_xml(&resp.data),
    }

    Ok(())
}
