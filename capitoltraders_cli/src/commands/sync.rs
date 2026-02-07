//! The `sync` subcommand: ingest CapitolTrades data into SQLite.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use clap::Args;
use capitoltraders_lib::{
    validation, Db, IssuerStatsRow, PoliticianStatsRow, ScrapeClient, ScrapedTrade,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::time::sleep;

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

    /// Page size for API pagination (1-100). Scrape mode uses a fixed page size.
    #[arg(long, default_value = "100")]
    pub page_size: i64,

    /// Fetch per-trade detail pages to capture filing URLs (slow)
    #[arg(long)]
    pub with_trade_details: bool,

    /// Delay between trade detail requests in milliseconds
    #[arg(long, default_value = "250")]
    pub details_delay_ms: u64,
}

pub async fn run(args: &SyncArgs, base_url: Option<&str>) -> Result<()> {
    let _page_size = validation::validate_page_size(args.page_size)?;
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

    if args.refresh_politicians || args.refresh_issuers {
        eprintln!("Note: refresh flags are ignored in scrape mode.");
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

    let scraper = match base_url.map(|s| s.to_string()).or_else(|| {
        std::env::var("CAPITOLTRADES_BASE_URL").ok()
    }) {
        Some(url) => ScrapeClient::with_base_url(&url)?,
        None => ScrapeClient::new()?,
    };

    let trade_result = sync_trades(
        &scraper,
        &mut db,
        if full { None } else { since_date },
        args.with_trade_details,
        args.details_delay_ms,
    )
    .await?;

    if let Some(max_pub_date) = trade_result.max_pub_date {
        db.set_meta("last_trade_pub_date", &max_pub_date.to_string())?;
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
    scraper: &ScrapeClient,
    db: &mut Db,
    since_date: Option<NaiveDate>,
    with_trade_details: bool,
    details_delay_ms: u64,
) -> Result<TradeSyncResult> {
    let mut page = 1;
    let mut total_ingested = 0;
    let mut max_pub_date: Option<NaiveDate> = None;
    let mut total_pages = None;

    let mut issuer_stats: HashMap<i64, IssuerAgg> = HashMap::new();
    let mut politician_stats: HashMap<String, PoliticianAgg> = HashMap::new();

    loop {
        let resp = scraper.trades_page(page).await?;
        total_pages = total_pages.or(resp.total_pages);
        let oldest_date = resp
            .data
            .last()
            .and_then(|trade| trade.pub_date.split('T').next())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let mut trades = resp.data;
        if let Some(since) = since_date {
            trades.retain(|trade| {
                trade
                    .pub_date
                    .split('T')
                    .next()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .map(|date| date >= since)
                    .unwrap_or(false)
            });
        }

        if !trades.is_empty() {
            if with_trade_details {
                for trade in &mut trades {
                    match scraper.trade_detail(trade.tx_id).await {
                        Ok(detail) => {
                            trade.filing_url = detail.filing_url;
                            trade.filing_id = detail.filing_id;
                        }
                        Err(err) => {
                            eprintln!("Failed to fetch trade {} detail: {}", trade.tx_id, err);
                        }
                    }
                    if details_delay_ms > 0 {
                        sleep(Duration::from_millis(details_delay_ms)).await;
                    }
                }
            }

            total_ingested += trades.len();
            for trade in &trades {
                if let Some(trade_date) = trade
                    .pub_date
                    .split('T')
                    .next()
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                {
                    max_pub_date = Some(match max_pub_date {
                        Some(current) => current.max(trade_date),
                        None => trade_date,
                    });
                }

                update_stats(&mut issuer_stats, &mut politician_stats, trade)?;
            }
            db.upsert_scraped_trades(&trades)?;
        }

        let total_pages_display = total_pages.unwrap_or(page);
        eprintln!(
            "Trades page {}/{} ({} items)",
            page,
            total_pages_display,
            trades.len()
        );

        if total_pages.map(|total| page >= total).unwrap_or(false) {
            break;
        }

        if let (Some(since), Some(oldest)) = (since_date, oldest_date) {
            if oldest < since {
                break;
            }
        }

        page += 1;
    }

    let politician_rows = build_politician_rows(politician_stats);
    let issuer_rows = build_issuer_rows(issuer_stats);
    db.upsert_politician_stats(&politician_rows)?;
    db.upsert_issuer_stats(&issuer_rows)?;

    Ok(TradeSyncResult {
        trade_count: total_ingested,
        max_pub_date,
    })
}

#[derive(Default)]
struct IssuerAgg {
    count_trades: i64,
    volume: i64,
    date_last_traded: Option<NaiveDate>,
    politicians: HashSet<String>,
}

#[derive(Default)]
struct PoliticianAgg {
    count_trades: i64,
    volume: i64,
    date_last_traded: Option<NaiveDate>,
    issuers: HashSet<i64>,
}

fn update_stats(
    issuer_stats: &mut HashMap<i64, IssuerAgg>,
    politician_stats: &mut HashMap<String, PoliticianAgg>,
    trade: &ScrapedTrade,
) -> Result<()> {
    let tx_date = NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d")
        .map_err(|e| anyhow!("invalid txDate {}: {}", trade.tx_date, e))?;

    let issuer_entry = issuer_stats
        .entry(trade.issuer_id)
        .or_insert_with(IssuerAgg::default);
    issuer_entry.count_trades += 1;
    issuer_entry.volume += trade.value;
    issuer_entry.politicians.insert(trade.politician_id.clone());
    issuer_entry.date_last_traded = Some(match issuer_entry.date_last_traded {
        Some(current) => current.max(tx_date),
        None => tx_date,
    });

    let pol_entry = politician_stats
        .entry(trade.politician_id.clone())
        .or_insert_with(PoliticianAgg::default);
    pol_entry.count_trades += 1;
    pol_entry.volume += trade.value;
    pol_entry.issuers.insert(trade.issuer_id);
    pol_entry.date_last_traded = Some(match pol_entry.date_last_traded {
        Some(current) => current.max(tx_date),
        None => tx_date,
    });

    Ok(())
}

fn build_politician_rows(
    stats: HashMap<String, PoliticianAgg>,
) -> Vec<PoliticianStatsRow> {
    stats
        .into_iter()
        .map(|(politician_id, agg)| PoliticianStatsRow {
            politician_id,
            date_last_traded: agg.date_last_traded.map(|d| d.to_string()),
            count_trades: agg.count_trades,
            count_issuers: agg.issuers.len() as i64,
            volume: agg.volume,
        })
        .collect()
}

fn build_issuer_rows(stats: HashMap<i64, IssuerAgg>) -> Vec<IssuerStatsRow> {
    stats
        .into_iter()
        .filter_map(|(issuer_id, agg)| {
            let date_last_traded = agg.date_last_traded?;
            Some(IssuerStatsRow {
                issuer_id,
                count_trades: agg.count_trades,
                count_politicians: agg.politicians.len() as i64,
                volume: agg.volume,
                date_last_traded: date_last_traded.to_string(),
            })
        })
        .collect()
}
