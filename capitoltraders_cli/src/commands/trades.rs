use anyhow::Result;
use clap::Args;
use capitoltraders_lib::{CachedClient, Query, SortDirection, TradeQuery, TradeSortBy};

use crate::output::{print_json, print_trades_table, OutputFormat};

#[derive(Args)]
pub struct TradesArgs {
    /// Filter by issuer ID
    #[arg(long)]
    pub issuer_id: Option<i64>,

    /// Filter trades from last N days (by publication date)
    #[arg(long)]
    pub days: Option<i64>,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: i64,

    /// Results per page
    #[arg(long, default_value = "20")]
    pub page_size: i64,

    /// Sort field: pub-date, trade-date, reporting-gap
    #[arg(long, default_value = "pub-date")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,
}

pub async fn run(args: &TradesArgs, client: &CachedClient, format: &OutputFormat) -> Result<()> {
    let mut query = TradeQuery::default()
        .with_page(args.page)
        .with_page_size(args.page_size);

    if let Some(issuer_id) = args.issuer_id {
        query = query.with_issuer_id(issuer_id);
    }

    if let Some(days) = args.days {
        query = query.with_pub_date_relative(days);
    }

    let sort_by = match args.sort_by.as_str() {
        "trade-date" => TradeSortBy::TradeDate,
        "reporting-gap" => TradeSortBy::ReportingGap,
        _ => TradeSortBy::PublicationDate,
    };
    query = query.with_sort_by(sort_by);

    if args.asc {
        query = query.with_sort_direction(SortDirection::Asc);
    }

    let resp = client.get_trades(&query).await?;

    eprintln!(
        "Page {}/{} ({} total trades)",
        resp.meta.paging.page, resp.meta.paging.total_pages, resp.meta.paging.total_items
    );

    match format {
        OutputFormat::Table => print_trades_table(&resp.data),
        OutputFormat::Json => print_json(&resp.data),
    }

    Ok(())
}
