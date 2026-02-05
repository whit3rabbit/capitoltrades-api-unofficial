use anyhow::Result;
use clap::Args;
use capitoltraders_lib::types::Party;
use capitoltraders_lib::{CachedClient, PoliticianQuery, PoliticianSortBy, Query, SortDirection};

use crate::output::{print_json, print_politicians_table, OutputFormat};

#[derive(Args)]
pub struct PoliticiansArgs {
    /// Filter by party: democrat, republican, other
    #[arg(long)]
    pub party: Option<String>,

    /// Search by name
    #[arg(long)]
    pub search: Option<String>,

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

    if let Some(party) = &args.party {
        let p = match party.as_str() {
            "democrat" | "d" => Party::Democrat,
            "republican" | "r" => Party::Republican,
            _ => Party::Other,
        };
        query = query.with_party(&p);
    }

    if let Some(search) = &args.search {
        query = query.with_search(search);
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
    }

    Ok(())
}
