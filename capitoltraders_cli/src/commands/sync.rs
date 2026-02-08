//! The `sync` subcommand: ingest CapitolTrades data into SQLite.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use capitoltraders_lib::{
    validation, Db, IssuerStatsRow, PoliticianStatsRow, ScrapeClient, ScrapeError,
    ScrapedIssuerDetail, ScrapedTrade, ScrapedTradeDetail,
};
use chrono::NaiveDate;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
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
    #[arg(long, hide = true)]
    pub with_trade_details: bool,

    /// Enrich trade and issuer details after sync (fetches individual detail pages)
    #[arg(long)]
    pub enrich: bool,

    /// Show how many items would be enriched without fetching
    #[arg(long, requires = "enrich")]
    pub dry_run: bool,

    /// Maximum items to enrich per run (default: all)
    #[arg(long)]
    pub batch_size: Option<i64>,

    /// Delay between trade detail requests in milliseconds
    #[arg(long, default_value = "500")]
    pub details_delay_ms: u64,

    /// Number of concurrent detail page fetches (1-10)
    #[arg(long, default_value = "3")]
    pub concurrency: usize,

    /// Stop enrichment after N consecutive HTTP failures
    #[arg(long, default_value = "5")]
    pub max_failures: usize,
}

pub async fn run(args: &SyncArgs, base_url: Option<&str>) -> Result<()> {
    let _page_size = validation::validate_page_size(args.page_size)?;
    if args.concurrency < 1 || args.concurrency > 10 {
        return Err(anyhow!("--concurrency must be between 1 and 10"));
    }
    if args.max_failures < 1 {
        return Err(anyhow!("--max-failures must be at least 1"));
    }
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

    let scraper = match base_url
        .map(|s| s.to_string())
        .or_else(|| std::env::var("CAPITOLTRADES_BASE_URL").ok())
    {
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

    // Treat --with-trade-details as alias for --enrich
    let should_enrich = args.enrich || args.with_trade_details;
    if should_enrich {
        let result = enrich_trades(
            &scraper,
            &db,
            args.batch_size,
            args.details_delay_ms,
            args.dry_run,
            args.concurrency,
            args.max_failures,
        )
        .await?;
        eprintln!(
            "Enrichment: {}/{} trades processed ({} failed)",
            result.enriched, result.total, result.failed
        );

        let issuer_result = enrich_issuers(
            &scraper,
            &db,
            args.batch_size,
            args.details_delay_ms,
            args.dry_run,
            args.concurrency,
            args.max_failures,
        )
        .await?;
        eprintln!(
            "Issuer enrichment: {}/{} issuers processed ({} failed)",
            issuer_result.enriched, issuer_result.total, issuer_result.failed
        );
    }

    eprintln!("Syncing politician committee memberships...");
    let committee_count = enrich_politician_committees(
        &scraper,
        &db,
        args.details_delay_ms,
    )
    .await?;
    eprintln!(
        "Committee enrichment complete: {} memberships persisted",
        committee_count
    );

    Ok(())
}

struct EnrichmentResult {
    enriched: usize,
    #[allow(dead_code)]
    skipped: usize,
    failed: usize,
    total: usize,
}

/// Circuit breaker that trips after N consecutive failures.
/// Not a full circuit breaker with half-open state -- just a kill switch.
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize,
}

impl CircuitBreaker {
    fn new(threshold: usize) -> Self {
        Self {
            consecutive_failures: 0,
            threshold,
        }
    }
    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }
    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
    }
    fn is_tripped(&self) -> bool {
        self.consecutive_failures >= self.threshold
    }
}

struct FetchResult<T> {
    id: i64,
    result: std::result::Result<T, ScrapeError>,
}

async fn enrich_trades(
    scraper: &ScrapeClient,
    db: &Db,
    batch_size: Option<i64>,
    detail_delay_ms: u64,
    dry_run: bool,
    concurrency: usize,
    max_failures: usize,
) -> Result<EnrichmentResult> {
    if dry_run {
        let total = db.count_unenriched_trades()?;
        let selected = match batch_size {
            Some(n) => n.min(total),
            None => total,
        };
        eprintln!(
            "{} trades would be enriched ({} selected)",
            total, selected
        );
        return Ok(EnrichmentResult {
            enriched: 0,
            skipped: 0,
            failed: 0,
            total: total as usize,
        });
    }

    let queue = db.get_unenriched_trade_ids(batch_size)?;
    if queue.is_empty() {
        eprintln!("No trades need enrichment");
        return Ok(EnrichmentResult {
            enriched: 0,
            skipped: 0,
            failed: 0,
            total: 0,
        });
    }

    let total = queue.len();
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
        )
        .unwrap(),
    );
    pb.set_message("enriching trades...");

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let (tx, mut rx) = mpsc::channel::<FetchResult<ScrapedTradeDetail>>(concurrency * 2);
    let mut join_set = JoinSet::new();

    for tx_id in &queue {
        let sem = Arc::clone(&semaphore);
        let sender = tx.clone();
        let scraper_clone = scraper.clone();
        let id = *tx_id;
        let delay = detail_delay_ms;

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            if delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            }
            let result = scraper_clone.trade_detail(id).await;
            let _ = sender.send(FetchResult { id, result }).await;
        });
    }
    // Drop original sender so rx.recv() returns None when all spawned senders are dropped
    drop(tx);

    let mut enriched = 0usize;
    let mut failed = 0usize;
    let mut breaker = CircuitBreaker::new(max_failures);

    while let Some(fetch) = rx.recv().await {
        match fetch.result {
            Ok(ref detail) => {
                db.update_trade_detail(fetch.id, detail)?;
                enriched += 1;
                breaker.record_success();
            }
            Err(ref err) => {
                pb.println(format!("  Warning: trade {} failed: {}", fetch.id, err));
                failed += 1;
                breaker.record_failure();
            }
        }
        pb.set_message(format!("{} ok, {} err", enriched, failed));
        pb.inc(1);

        if breaker.is_tripped() {
            pb.println(format!(
                "Circuit breaker tripped after {} consecutive failures, stopping enrichment",
                max_failures
            ));
            join_set.abort_all();
            break;
        }
    }

    pb.finish_with_message(format!("done: {} enriched, {} failed", enriched, failed));

    Ok(EnrichmentResult {
        enriched,
        skipped: 0,
        failed,
        total,
    })
}

async fn enrich_issuers(
    scraper: &ScrapeClient,
    db: &Db,
    batch_size: Option<i64>,
    detail_delay_ms: u64,
    dry_run: bool,
    concurrency: usize,
    max_failures: usize,
) -> Result<EnrichmentResult> {
    if dry_run {
        let total = db.count_unenriched_issuers()?;
        let selected = match batch_size {
            Some(n) => n.min(total),
            None => total,
        };
        eprintln!(
            "{} issuers would be enriched ({} selected)",
            total, selected
        );
        return Ok(EnrichmentResult {
            enriched: 0,
            skipped: 0,
            failed: 0,
            total: total as usize,
        });
    }

    let queue = db.get_unenriched_issuer_ids(batch_size)?;
    if queue.is_empty() {
        eprintln!("No issuers need enrichment");
        return Ok(EnrichmentResult {
            enriched: 0,
            skipped: 0,
            failed: 0,
            total: 0,
        });
    }

    let total = queue.len();
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
        )
        .unwrap(),
    );
    pb.set_message("enriching issuers...");

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let (tx, mut rx) = mpsc::channel::<FetchResult<ScrapedIssuerDetail>>(concurrency * 2);
    let mut join_set = JoinSet::new();

    for issuer_id in &queue {
        let sem = Arc::clone(&semaphore);
        let sender = tx.clone();
        let scraper_clone = scraper.clone();
        let id = *issuer_id;
        let delay = detail_delay_ms;

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            if delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            }
            let result = scraper_clone.issuer_detail(id).await;
            let _ = sender.send(FetchResult { id, result }).await;
        });
    }
    // Drop original sender so rx.recv() returns None when all spawned senders are dropped
    drop(tx);

    let mut enriched = 0usize;
    let mut failed = 0usize;
    let mut breaker = CircuitBreaker::new(max_failures);

    while let Some(fetch) = rx.recv().await {
        match fetch.result {
            Ok(ref detail) => {
                db.update_issuer_detail(fetch.id, detail)?;
                enriched += 1;
                breaker.record_success();
            }
            Err(ref err) => {
                pb.println(format!("  Warning: issuer {} failed: {}", fetch.id, err));
                failed += 1;
                breaker.record_failure();
            }
        }
        pb.set_message(format!("{} ok, {} err", enriched, failed));
        pb.inc(1);

        if breaker.is_tripped() {
            pb.println(format!(
                "Circuit breaker tripped after {} consecutive failures, stopping enrichment",
                max_failures
            ));
            join_set.abort_all();
            break;
        }
    }

    pb.finish_with_message(format!("done: {} enriched, {} failed", enriched, failed));

    Ok(EnrichmentResult {
        enriched,
        skipped: 0,
        failed,
        total,
    })
}

async fn enrich_politician_committees(
    scraper: &ScrapeClient,
    db: &Db,
    throttle_ms: u64,
) -> Result<usize> {
    let mut memberships: Vec<(String, String)> = Vec::new();

    for &(code, name) in validation::COMMITTEE_MAP {
        let mut page = 1;
        let mut committee_member_count = 0;

        loop {
            let resp = scraper.politicians_by_committee(code, page).await?;
            for card in &resp.data {
                memberships.push((card.politician_id.clone(), code.to_string()));
                committee_member_count += 1;
            }

            let total_pages = resp.total_pages.unwrap_or(1);
            if page >= total_pages {
                break;
            }

            page += 1;

            if throttle_ms > 0 {
                sleep(Duration::from_millis(throttle_ms)).await;
            }
        }

        eprintln!("  {}: {} members", name, committee_member_count);

        if throttle_ms > 0 {
            sleep(Duration::from_millis(throttle_ms)).await;
        }
    }

    let inserted = db.replace_all_politician_committees(&memberships)?;
    db.mark_politicians_enriched()?;

    eprintln!(
        "Committee enrichment: {} memberships across {} committees",
        memberships.len(),
        validation::COMMITTEE_MAP.len()
    );

    Ok(inserted)
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

    let issuer_entry = issuer_stats.entry(trade.issuer_id).or_default();
    issuer_entry.count_trades += 1;
    issuer_entry.volume += trade.value;
    issuer_entry.politicians.insert(trade.politician_id.clone());
    issuer_entry.date_last_traded = Some(match issuer_entry.date_last_traded {
        Some(current) => current.max(tx_date),
        None => tx_date,
    });

    let pol_entry = politician_stats
        .entry(trade.politician_id.clone())
        .or_default();
    pol_entry.count_trades += 1;
    pol_entry.volume += trade.value;
    pol_entry.issuers.insert(trade.issuer_id);
    pol_entry.date_last_traded = Some(match pol_entry.date_last_traded {
        Some(current) => current.max(tx_date),
        None => tx_date,
    });

    Ok(())
}

fn build_politician_rows(stats: HashMap<String, PoliticianAgg>) -> Vec<PoliticianStatsRow> {
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
