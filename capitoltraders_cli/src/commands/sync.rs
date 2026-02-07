//! The `sync` subcommand: ingest CapitolTrades data into SQLite.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use clap::Args;
use capitoltraders_lib::{
    validation, CachedClient, Db, IssuerQuery, IssuerSortBy, PoliticianQuery, PoliticianSortBy,
    Query, SortDirection, TradeQuery, TradeSortBy,
};

/// Arguments for the `sync` subcommand.
#[derive(Args)]
pub struct SyncArgs {
    /// SQLite database path
    #[arg(long, default_value = "capitoltraders.db")]
    pub db: PathBuf,

    /// Full refresh: fetch all trades, politicians, and issuers
    #[arg(long)]
    pub full: bool,

    /// Override the incremental cutoff date (YYYY-MM-DD, publication date)
    #[arg(long)]
    pub since: Option<String>,

    /// Refresh the full politician catalog (slow)
    #[arg(long)]
    pub refresh_politicians: bool,

    /// Refresh the full issuer catalog (slow)
    #[arg(long)]
    pub refresh_issuers: bool,

    /// Page size for API pagination (1-100)
    #[arg(long, default_value = "100")]
    pub page_size: i64,
}

pub async fn run(args: &SyncArgs, client: &CachedClient) -> Result<()> {
    let page_size = validation::validate_page_size(args.page_size)?;
    let mut db = Db::open(&args.db)?;
    db.init()?;

    let mut full = args.full;
    let mut since_date: Option<NaiveDate> = None;

    if !full {
        if let Some(ref since) = args.since {
            since_date = Some(validation::validate_date(since)?);
        } else if let Some(stored) = db.get_meta("last_trade_pub_date")? {
            since_date = Some(NaiveDate::parse_from_str(&stored, "%Y-%m-%d")?);
        } else if db.trade_count()? == 0 {
            full = true;
        }
    }

    if full {
        eprintln!("Starting full sync into {}", args.db.display());
    } else if let Some(date) = since_date {
        eprintln!(
            "Starting incremental sync into {} (since {})",
            args.db.display(),
            date
        );
    } else {
        eprintln!("Starting incremental sync into {}", args.db.display());
    }

    let trade_result =
        sync_trades(client, &mut db, page_size, if full { None } else { since_date })
        .await?;

    if let Some(max_pub_date) = trade_result.max_pub_date {
        db.set_meta("last_trade_pub_date", &max_pub_date.to_string())?;
    }

    if full || args.refresh_politicians {
        sync_politicians(client, &mut db, page_size).await?;
    }

    if full || args.refresh_issuers {
        sync_issuers(client, &mut db, page_size).await?;
    }

    eprintln!(
        "Sync complete: {} trades ingested",
        trade_result.trade_count
    );
    Ok(())
}

struct TradeSyncResult {
    trade_count: usize,
    max_pub_date: Option<NaiveDate>,
}

async fn sync_trades(
    client: &CachedClient,
    db: &mut Db,
    page_size: i64,
    since_date: Option<NaiveDate>,
) -> Result<TradeSyncResult> {
    let relative_days = if let Some(date) = since_date {
        match validation::date_to_relative_days(date) {
            Some(days) => Some(days + 1),
            None => return Err(anyhow!("--since date {} is in the future", date)),
        }
    } else {
        None
    };

    let mut page = 1;
    let mut total_ingested = 0;
    let mut max_pub_date: Option<NaiveDate> = None;

    loop {
        let mut query = TradeQuery::default()
            .with_page(page)
            .with_page_size(page_size)
            .with_sort_by(TradeSortBy::PublicationDate)
            .with_sort_direction(SortDirection::Desc);
        if let Some(days) = relative_days {
            query = query.with_pub_date_relative(days);
        }

        let resp = client.get_trades(&query).await?;
        let oldest_date = resp
            .data
            .last()
            .map(|trade| trade.pub_date.date_naive());

        let mut trades = resp.data;
        if let Some(since) = since_date {
            trades.retain(|trade| trade.pub_date.date_naive() >= since);
        }

        if !trades.is_empty() {
            total_ingested += trades.len();
            for trade in &trades {
                let trade_date = trade.pub_date.date_naive();
                max_pub_date = Some(match max_pub_date {
                    Some(current) => current.max(trade_date),
                    None => trade_date,
                });
            }
            db.upsert_trades(&trades)?;
        }

        let total_pages = resp.meta.paging.total_pages;
        eprintln!("Trades page {}/{} ({} items)", page, total_pages, trades.len());

        if page >= total_pages {
            break;
        }

        if let (Some(since), Some(oldest)) = (since_date, oldest_date) {
            if oldest < since {
                break;
            }
        }

        page += 1;
    }

    Ok(TradeSyncResult {
        trade_count: total_ingested,
        max_pub_date,
    })
}

async fn sync_politicians(client: &CachedClient, db: &mut Db, page_size: i64) -> Result<()> {
    let mut page = 1;
    loop {
        let query = PoliticianQuery::default()
            .with_page(page)
            .with_page_size(page_size)
            .with_sort_by(PoliticianSortBy::TradedVolume)
            .with_sort_direction(SortDirection::Desc);

        let resp = client.get_politicians(&query).await?;
        db.upsert_politicians(&resp.data)?;

        let total_pages = resp.meta.paging.total_pages;
        eprintln!(
            "Politicians page {}/{} ({} items)",
            page,
            total_pages,
            resp.data.len()
        );
        if page >= total_pages {
            break;
        }
        page += 1;
    }
    Ok(())
}

async fn sync_issuers(client: &CachedClient, db: &mut Db, page_size: i64) -> Result<()> {
    let mut page = 1;
    loop {
        let query = IssuerQuery::default()
            .with_page(page)
            .with_page_size(page_size)
            .with_sort_by(IssuerSortBy::TradedVolume)
            .with_sort_direction(SortDirection::Desc);

        let resp = client.get_issuers(&query).await?;
        db.upsert_issuers(&resp.data)?;

        let total_pages = resp.meta.paging.total_pages;
        eprintln!("Issuers page {}/{} ({} items)", page, total_pages, resp.data.len());
        if page >= total_pages {
            break;
        }
        page += 1;
    }
    Ok(())
}
