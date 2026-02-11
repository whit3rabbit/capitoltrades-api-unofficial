//! Price enrichment pipeline for fetching historical and current prices from Yahoo Finance.
//!
//! Implements two-phase enrichment:
//! - Phase 1: Historical prices deduplicated by (ticker, date)
//! - Phase 2: Current prices deduplicated by ticker
//!
//! Uses Semaphore + JoinSet + mpsc pattern for concurrent fetching with rate limiting.

use anyhow::{anyhow, bail, Result};
use capitoltraders_lib::{pricing, yahoo::YahooClient, Db};
use chrono::NaiveDate;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use tokio::time::sleep;

/// Price enrichment CLI arguments.
#[derive(Args)]
pub struct EnrichPricesArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Maximum trades to process per run (default: all)
    #[arg(long)]
    pub batch_size: Option<i64>,

    /// Re-enrich already-enriched trades (reserved for future use)
    #[arg(long)]
    pub force: bool,
}

/// Message sent from fetch tasks to receiver for historical price enrichment.
struct HistoricalPriceResult {
    ticker: String,
    date: NaiveDate,
    trade_indices: Vec<usize>,
    result: Result<Option<f64>, capitoltraders_lib::yahoo::YahooError>,
}

/// Message sent from fetch tasks to receiver for current price enrichment.
struct CurrentPriceResult {
    trade_indices: Vec<usize>,
    result: Result<Option<f64>, capitoltraders_lib::yahoo::YahooError>,
}

/// Circuit breaker to stop processing after consecutive failures.
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

/// Run the price enrichment pipeline.
pub async fn run(args: &EnrichPricesArgs) -> Result<()> {
    if args.force {
        eprintln!("Note: --force flag is reserved for future use and currently has no effect");
    }

    // Step 1: Setup
    let db = Db::open(&args.db)?;
    let yahoo = Arc::new(
        YahooClient::new().map_err(|e| anyhow!("Failed to create Yahoo client: {}", e))?,
    );
    let trades = db.get_unenriched_price_trades(args.batch_size)?;

    if trades.is_empty() {
        eprintln!("No trades need price enrichment");
        return Ok(());
    }

    let total_trades = trades.len();
    eprintln!(
        "Starting price enrichment for {} trades",
        total_trades
    );

    // Step 2: Deduplicate by (ticker, date)
    let mut ticker_date_map: HashMap<(String, NaiveDate), Vec<usize>> = HashMap::new();
    let mut skipped_parse_errors = 0usize;

    for (idx, trade) in trades.iter().enumerate() {
        match NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d") {
            Ok(date) => {
                ticker_date_map
                    .entry((trade.issuer_ticker.clone(), date))
                    .or_default()
                    .push(idx);
            }
            Err(_) => {
                eprintln!(
                    "Warning: tx_id {} has invalid tx_date '{}', skipping",
                    trade.tx_id, trade.tx_date
                );
                skipped_parse_errors += 1;
            }
        }
    }

    let unique_pairs = ticker_date_map.len();
    eprintln!(
        "Phase 1: Fetching historical prices for {} unique (ticker, date) pairs",
        unique_pairs
    );

    // Step 3: Historical price enrichment (Phase 1)
    const CONCURRENCY: usize = 5;
    const CIRCUIT_BREAKER_THRESHOLD: usize = 10;

    let pb = ProgressBar::new(unique_pairs as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
        )
        .unwrap(),
    );
    pb.set_message("fetching historical prices...");

    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let (tx, mut rx) = mpsc::channel::<HistoricalPriceResult>(CONCURRENCY * 2);
    let mut join_set = JoinSet::new();

    for ((ticker, date), indices) in ticker_date_map {
        let sem = Arc::clone(&semaphore);
        let sender = tx.clone();
        let yahoo_clone = Arc::clone(&yahoo);

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            // Rate limiting with jittered delay
            let delay_ms = rand::thread_rng().gen_range(200..500);
            sleep(Duration::from_millis(delay_ms)).await;

            let result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;
            let _ = sender
                .send(HistoricalPriceResult {
                    ticker,
                    date,
                    trade_indices: indices,
                    result,
                })
                .await;
        });
    }
    drop(tx);

    let mut enriched = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut breaker = CircuitBreaker::new(CIRCUIT_BREAKER_THRESHOLD);

    while let Some(fetch) = rx.recv().await {
        match fetch.result {
            Ok(Some(price)) => {
                // Process all trades with this (ticker, date) pair
                for idx in &fetch.trade_indices {
                    let trade = &trades[*idx];
                    // Parse trade range and estimate shares
                    if let Some(range) =
                        pricing::parse_trade_range(trade.size_range_low, trade.size_range_high)
                    {
                        if let Some(estimate) = pricing::estimate_shares(&range, price) {
                            db.update_trade_prices(
                                trade.tx_id,
                                Some(price),
                                Some(estimate.estimated_shares),
                                Some(estimate.estimated_value),
                            )?;
                            enriched += 1;
                            breaker.record_success();
                        } else {
                            // Estimate failed (division by zero or out of bounds)
                            db.update_trade_prices(trade.tx_id, Some(price), None, None)?;
                            skipped += 1;
                        }
                    } else {
                        // Invalid range
                        db.update_trade_prices(trade.tx_id, Some(price), None, None)?;
                        skipped += 1;
                    }
                }
            }
            Ok(None) => {
                // Invalid ticker or no data for that date
                for idx in &fetch.trade_indices {
                    let trade = &trades[*idx];
                    db.update_trade_prices(trade.tx_id, None, None, None)?;
                    skipped += 1;
                }
            }
            Err(ref err) => {
                pb.println(format!(
                    "  Warning: {} on {} failed: {}",
                    fetch.ticker, fetch.date, err
                ));
                for idx in &fetch.trade_indices {
                    let trade = &trades[*idx];
                    // Mark as enriched with None to avoid re-processing
                    db.update_trade_prices(trade.tx_id, None, None, None)?;
                    failed += 1;
                }
                breaker.record_failure();
            }
        }
        pb.set_message(format!("{} ok, {} err, {} skip", enriched, failed, skipped));
        pb.inc(1);

        if breaker.is_tripped() {
            pb.println(format!(
                "Circuit breaker tripped after {} consecutive failures, stopping Phase 1",
                CIRCUIT_BREAKER_THRESHOLD
            ));
            join_set.abort_all();
            break;
        }
    }

    pb.finish_with_message(format!(
        "Phase 1 done: {} enriched, {} failed, {} skipped",
        enriched, failed, skipped
    ));

    // Step 4: Current price enrichment (Phase 2)
    let mut ticker_map: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, trade) in trades.iter().enumerate() {
        ticker_map
            .entry(trade.issuer_ticker.clone())
            .or_default()
            .push(idx);
    }

    let unique_tickers = ticker_map.len();
    eprintln!(
        "Phase 2: Fetching current prices for {} unique tickers",
        unique_tickers
    );

    let pb2 = ProgressBar::new(unique_tickers as u64);
    pb2.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
        )
        .unwrap(),
    );
    pb2.set_message("fetching current prices...");

    let semaphore2 = Arc::new(Semaphore::new(CONCURRENCY));
    let (tx2, mut rx2) = mpsc::channel::<CurrentPriceResult>(CONCURRENCY * 2);
    let mut join_set2 = JoinSet::new();

    for (ticker, indices) in ticker_map {
        let sem = Arc::clone(&semaphore2);
        let sender = tx2.clone();
        let yahoo_clone = Arc::clone(&yahoo);

        join_set2.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let delay_ms = rand::thread_rng().gen_range(200..500);
            sleep(Duration::from_millis(delay_ms)).await;

            let result = yahoo_clone.get_current_price(&ticker).await;
            let _ = sender
                .send(CurrentPriceResult {
                    trade_indices: indices,
                    result,
                })
                .await;
        });
    }
    drop(tx2);

    let mut current_enriched = 0usize;
    let mut current_skipped = 0usize;

    while let Some(fetch) = rx2.recv().await {
        match fetch.result {
            Ok(Some(price)) => {
                for idx in &fetch.trade_indices {
                    let trade = &trades[*idx];
                    db.update_current_price(trade.tx_id, Some(price))?;
                    current_enriched += 1;
                }
            }
            Ok(None) | Err(_) => {
                // Current price is best-effort, skip on failure
                current_skipped += fetch.trade_indices.len();
            }
        }
        pb2.set_message(format!("{} ok, {} skip", current_enriched, current_skipped));
        pb2.inc(1);
    }

    pb2.finish_with_message(format!(
        "Phase 2 done: {} enriched, {} skipped",
        current_enriched, current_skipped
    ));

    // Step 5: Summary
    eprintln!();
    eprintln!(
        "Price enrichment complete: {} enriched, {} failed, {} skipped",
        enriched, failed, skipped + skipped_parse_errors
    );
    eprintln!(
        "  ({} total trades, {} unique ticker-date pairs, {} unique tickers)",
        total_trades, unique_pairs, unique_tickers
    );

    if breaker.is_tripped() {
        eprintln!(
            "Warning: Circuit breaker tripped after {} consecutive failures -- some trades were not processed",
            CIRCUIT_BREAKER_THRESHOLD
        );
        bail!("Enrichment aborted due to circuit breaker");
    }

    Ok(())
}
