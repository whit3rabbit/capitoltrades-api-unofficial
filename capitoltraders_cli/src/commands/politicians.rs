use anyhow::Result;
use clap::Args;
use capitoltraders_lib::{CachedClient, PoliticianQuery, PoliticianSortBy, Query, SortDirection};
use capitoltraders_lib::validation;

use crate::output::{
    print_json, print_politicians_csv, print_politicians_markdown, print_politicians_table,
    OutputFormat,
};

#[derive(Args)]
pub struct PoliticiansArgs {
    /// Filter by party: democrat (d), republican (r), other
    #[arg(long)]
    pub party: Option<String>,

    /// Search by politician name
    #[arg(long)]
    pub name: Option<String>,

    /// Search by name (hidden alias for --name, kept for backwards compatibility)
    #[arg(long, hide = true)]
    pub search: Option<String>,

    /// Filter by US state code (e.g. CA, TX, NY)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by committee name (e.g. "Senate - Finance")
    #[arg(long)]
    pub committee: Option<String>,

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

    if let Some(ref party) = args.party {
        let p = validation::validate_party(party)?;
        query = query.with_party(&p);
    }

    // --name takes precedence over --search (hidden alias)
    let search_input = args.name.as_ref().or(args.search.as_ref());
    if let Some(search) = search_input {
        let sanitized = validation::validate_search(search)?;
        query = query.with_search(&sanitized);
    }

    if let Some(ref state) = args.state {
        let validated = validation::validate_state(state)?;
        query = query.with_state(&validated);
    }

    if let Some(ref committee) = args.committee {
        let validated = validation::validate_committee(committee)?;
        query = query.with_committee(&validated);
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
    }

    Ok(())
}
