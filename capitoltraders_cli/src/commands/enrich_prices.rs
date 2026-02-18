//! Price enrichment pipeline for fetching historical and current prices.
//!
//! Implements three-phase enrichment:
//! - Phase 1: Historical prices deduplicated by (ticker, date), Yahoo-first with Tiingo fallback
//! - Phase 2: Current prices deduplicated by ticker (Yahoo only)
//! - Phase 3: Benchmark prices (sector ETF or SPY) deduplicated by (ETF ticker, date)
//!
//! Uses Semaphore + JoinSet + mpsc pattern for concurrent fetching with rate limiting.

use anyhow::{anyhow, bail, Result};
use capitoltraders_lib::{pricing, ticker_alias, tiingo::TiingoClient, yahoo::YahooClient, Db};
use chrono::NaiveDate;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use std::collections::{HashMap, HashSet};
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

    /// Print enrichment diagnostics and exit (no Yahoo API calls)
    #[arg(long)]
    pub diagnose: bool,

    /// Reset trades that were attempted but got no price, then re-enrich them
    #[arg(long)]
    pub retry_failed: bool,
}

/// Message sent from fetch tasks to receiver for historical price enrichment.
struct HistoricalPriceResult {
    ticker: String,
    date: NaiveDate,
    trade_indices: Vec<usize>,
    result: Result<Option<f64>, capitoltraders_lib::yahoo::YahooError>,
    /// Which API provided the price ("yahoo", "tiingo", or "" if no data).
    source: String,
}

/// Message sent from fetch tasks to receiver for current price enrichment.
struct CurrentPriceResult {
    trade_indices: Vec<usize>,
    result: Result<Option<f64>, capitoltraders_lib::yahoo::YahooError>,
}

/// Message sent from fetch tasks to receiver for benchmark price enrichment.
struct BenchmarkPriceResult {
    trade_indices: Vec<i64>,  // tx_ids of trades to update
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

/// Map GICS sector name to benchmark ETF ticker.
///
/// Returns sector-specific ETF if issuer has GICS sector mapping,
/// otherwise SPY (S&P 500 market benchmark).
fn get_benchmark_ticker(gics_sector: Option<&str>) -> &'static str {
    match gics_sector {
        Some("Communication Services") => "XLC",
        Some("Consumer Discretionary") => "XLY",
        Some("Consumer Staples") => "XLP",
        Some("Energy") => "XLE",
        Some("Financials") => "XLF",
        Some("Health Care") => "XLV",
        Some("Industrials") => "XLI",
        Some("Information Technology") => "XLK",
        Some("Materials") => "XLB",
        Some("Real Estate") => "XLRE",
        Some("Utilities") => "XLU",
        _ => "SPY",
    }
}

/// Print enrichment diagnostics report to stderr.
fn print_diagnostics(db: &Db) -> Result<()> {
    let diag = db.get_enrichment_diagnostics()?;

    eprintln!("=== Enrichment Diagnostics ===");
    eprintln!();
    eprintln!("Overall breakdown:");
    eprintln!("  Total trades:           {:>6}", diag.total);
    eprintln!("  Has price:              {:>6} ({:.1}%)", diag.has_price, pct(diag.has_price, diag.total));
    eprintln!("  Attempted, no price:    {:>6} ({:.1}%)", diag.attempted_no_price, pct(diag.attempted_no_price, diag.total));
    eprintln!("  Never attempted:        {:>6} ({:.1}%)", diag.never_attempted, pct(diag.never_attempted, diag.total));

    if !diag.never_attempted_reasons.is_empty() {
        eprintln!();
        eprintln!("Why trades were never attempted:");
        for (reason, cnt) in &diag.never_attempted_reasons {
            eprintln!("  {:<25} {:>6}", reason, cnt);
        }
    }

    if !diag.top_failed_tickers.is_empty() {
        eprintln!();
        eprintln!("Top tickers attempted but got no price:");
        for (ticker, name, cnt) in &diag.top_failed_tickers {
            eprintln!("  {:<15} {:<40} {:>4}", ticker, name, cnt);
        }
    }

    if !diag.failed_suffix_distribution.is_empty() {
        eprintln!();
        eprintln!("Exchange suffix distribution on failed tickers:");
        for (suffix, cnt) in &diag.failed_suffix_distribution {
            eprintln!("  {:<10} {:>6}", suffix, cnt);
        }
    }

    if !diag.price_source_breakdown.is_empty() {
        eprintln!();
        eprintln!("Price source breakdown:");
        for (source, cnt) in &diag.price_source_breakdown {
            eprintln!("  {:<15} {:>6}", source, cnt);
        }
    }

    eprintln!();
    eprintln!("=== End Diagnostics ===");

    Ok(())
}

fn pct(part: i64, total: i64) -> f64 {
    if total == 0 { 0.0 } else { (part as f64 / total as f64) * 100.0 }
}

/// Run the price enrichment pipeline.
pub async fn run(args: &EnrichPricesArgs) -> Result<()> {
    let db = Db::open(&args.db)?;
    db.init().map_err(|e| anyhow!("Failed to initialize database: {}", e))?;

    // --diagnose: print diagnostics and exit
    if args.diagnose {
        print_diagnostics(&db)?;
        return Ok(());
    }

    if args.force {
        eprintln!("Note: --force flag is reserved for future use and currently has no effect");
    }

    // --retry-failed: reset trades that were attempted but got no price
    if args.retry_failed {
        let reset_count = db.reset_failed_price_enrichments()?;
        eprintln!("Reset {} trades that were attempted but got no price", reset_count);
    }

    // Load ticker aliases
    let aliases = ticker_alias::load_ticker_aliases()
        .map_err(|e| anyhow!("Failed to load ticker aliases: {}", e))?;
    let alias_count = aliases.len();
    if alias_count > 0 {
        eprintln!("Loaded {} ticker aliases", alias_count);
    }

    // Step 1: Setup
    let yahoo = Arc::new(
        YahooClient::new().map_err(|e| anyhow!("Failed to create Yahoo client: {}", e))?,
    );

    // Optional Tiingo fallback client (requires TIINGO_API_KEY in .env)
    let tiingo: Option<Arc<TiingoClient>> = match std::env::var("TIINGO_API_KEY") {
        Ok(key) if !key.is_empty() => {
            match TiingoClient::new(key) {
                Ok(client) => {
                    eprintln!("Tiingo fallback enabled (TIINGO_API_KEY found)");
                    Some(Arc::new(client))
                }
                Err(e) => {
                    eprintln!("Warning: Failed to create Tiingo client: {}", e);
                    None
                }
            }
        }
        _ => None,
    };

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

    // Step 2: Deduplicate by (normalized_ticker, date)
    let mut ticker_date_map: HashMap<(String, NaiveDate), Vec<usize>> = HashMap::new();
    let mut normalized_tickers: HashMap<String, String> = HashMap::new();
    let mut skipped_parse_errors = 0usize;
    let mut skipped_no_ticker = 0usize;
    let mut alias_skipped_unenrichable = 0usize;
    let mut alias_resolved = 0usize;

    for (idx, trade) in trades.iter().enumerate() {
        let yahoo_ticker = match pricing::resolve_yahoo_ticker(&trade.issuer_ticker, &aliases) {
            Some(t) => t,
            None => {
                // Check if this was an alias-based skip (known unenrichable) vs empty ticker
                if aliases.get(trade.issuer_ticker.as_str()) == Some(&None)
                    || aliases.get(trade.issuer_ticker.trim()) == Some(&None)
                {
                    alias_skipped_unenrichable += 1;
                } else {
                    skipped_no_ticker += 1;
                }
                continue;
            }
        };

        // Track alias usage for reporting
        if aliases.contains_key(trade.issuer_ticker.as_str())
            || aliases.contains_key(trade.issuer_ticker.trim())
        {
            alias_resolved += 1;
        }

        normalized_tickers
            .entry(trade.issuer_ticker.clone())
            .or_insert_with(|| yahoo_ticker.clone());
        match NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d") {
            Ok(date) => {
                ticker_date_map
                    .entry((yahoo_ticker, date))
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

    if skipped_no_ticker > 0 {
        eprintln!(
            "Skipped {} trades with empty or unparseable tickers",
            skipped_no_ticker
        );
    }
    if alias_skipped_unenrichable > 0 {
        eprintln!(
            "Skipped {} trades with known-unenrichable tickers (via aliases)",
            alias_skipped_unenrichable
        );
    }
    if alias_resolved > 0 {
        eprintln!(
            "Resolved {} trades via ticker aliases",
            alias_resolved
        );
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
        let tiingo_clone = tiingo.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            // Rate limiting with jittered delay
            let delay_ms = rand::thread_rng().gen_range(200..500);
            sleep(Duration::from_millis(delay_ms)).await;

            let yahoo_result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;

            // If Yahoo returned no data and Tiingo is available, try fallback
            let (result, source) = match &yahoo_result {
                Ok(Some(_)) => (yahoo_result, "yahoo".to_string()),
                Ok(None) if tiingo_clone.is_some() => {
                    let tiingo_client = tiingo_clone.as_ref().unwrap();
                    match tiingo_client.get_price_on_date(&ticker, date).await {
                        Ok(Some(price)) => (Ok(Some(price)), "tiingo".to_string()),
                        Ok(None) => (Ok(None), String::new()),
                        Err(_) => {
                            // Tiingo failed too, pass through Yahoo's Ok(None)
                            (Ok(None), String::new())
                        }
                    }
                }
                _ => {
                    // Yahoo returned Err or Ok(None) without Tiingo
                    (yahoo_result, String::new())
                }
            };

            let _ = sender
                .send(HistoricalPriceResult {
                    ticker,
                    date,
                    trade_indices: indices,
                    result,
                    source,
                })
                .await;
        });
    }
    drop(tx);

    let mut enriched = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut tiingo_resolved = 0usize;
    let mut breaker = CircuitBreaker::new(CIRCUIT_BREAKER_THRESHOLD);

    // Track tickers that return no data for end-of-run summary
    let mut no_data_tickers: HashSet<String> = HashSet::new();

    while let Some(fetch) = rx.recv().await {
        let source_opt = if fetch.source.is_empty() { None } else { Some(fetch.source.as_str()) };

        match fetch.result {
            Ok(Some(price)) => {
                if fetch.source == "tiingo" {
                    tiingo_resolved += fetch.trade_indices.len();
                    pb.println(format!("  Tiingo fallback: {} on {} = {:.2}", fetch.ticker, fetch.date, price));
                }
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
                                source_opt,
                            )?;
                            enriched += 1;
                            breaker.record_success();
                        } else {
                            // Estimate failed (division by zero or out of bounds)
                            db.update_trade_prices(trade.tx_id, Some(price), None, None, source_opt)?;
                            skipped += 1;
                        }
                    } else {
                        // Invalid range
                        db.update_trade_prices(trade.tx_id, Some(price), None, None, source_opt)?;
                        skipped += 1;
                    }
                }
            }
            Ok(None) => {
                // Log first occurrence of each failing ticker
                if no_data_tickers.insert(fetch.ticker.clone()) {
                    pb.println(format!("  No data: {} on {}", fetch.ticker, fetch.date));
                }
                for idx in &fetch.trade_indices {
                    let trade = &trades[*idx];
                    db.update_trade_prices(trade.tx_id, None, None, None, None)?;
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
                    db.update_trade_prices(trade.tx_id, None, None, None, None)?;
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

    // Print failing ticker summary
    if !no_data_tickers.is_empty() {
        eprintln!();
        eprintln!(
            "{} unique tickers returned no data from Yahoo:",
            no_data_tickers.len()
        );
        let mut sorted: Vec<&String> = no_data_tickers.iter().collect();
        sorted.sort();
        for ticker in sorted.iter().take(20) {
            eprintln!("  {}", ticker);
        }
        if no_data_tickers.len() > 20 {
            eprintln!("  ... and {} more", no_data_tickers.len() - 20);
        }
        eprintln!("Tip: Add aliases to seed_data/ticker_aliases.yml for renamed/known tickers");
        eprintln!();
    }

    // Step 4: Current price enrichment (Phase 2)
    let mut ticker_map: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, trade) in trades.iter().enumerate() {
        if let Some(yahoo_ticker) = normalized_tickers.get(&trade.issuer_ticker) {
            ticker_map
                .entry(yahoo_ticker.clone())
                .or_default()
                .push(idx);
        }
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

    // Step 4.5: Phase 3 -- Benchmark price enrichment
    let benchmark_trades = db.get_benchmark_unenriched_trades(args.batch_size)?;

    let (benchmark_enriched, benchmark_skipped, breaker3_tripped) = if benchmark_trades.is_empty() {
        eprintln!("No trades need benchmark enrichment");
        (0, 0, false)
    } else {
        // Build dedup map: (benchmark_ticker, date) -> Vec<tx_id>
        let mut benchmark_date_map: HashMap<(String, NaiveDate), Vec<i64>> = HashMap::new();
        for trade in &benchmark_trades {
            let date = match NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => continue,
            };
            let benchmark_ticker = get_benchmark_ticker(trade.gics_sector.as_deref());
            benchmark_date_map
                .entry((benchmark_ticker.to_string(), date))
                .or_default()
                .push(trade.tx_id);
        }

        let unique_pairs = benchmark_date_map.len();
        eprintln!(
            "Phase 3: Fetching benchmark prices for {} unique (ETF, date) pairs across {} trades",
            unique_pairs,
            benchmark_trades.len()
        );

        let pb3 = ProgressBar::new(unique_pairs as u64);
        pb3.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
            )
            .unwrap(),
        );
        pb3.set_message("fetching benchmark prices...");

        let semaphore3 = Arc::new(Semaphore::new(CONCURRENCY));
        let (tx3, mut rx3) = mpsc::channel::<BenchmarkPriceResult>(CONCURRENCY * 2);
        let mut join_set3 = JoinSet::new();

        for ((ticker, date), tx_ids) in benchmark_date_map {
            let sem = Arc::clone(&semaphore3);
            let sender = tx3.clone();
            let yahoo_clone = Arc::clone(&yahoo);

            join_set3.spawn(async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                let delay_ms = rand::thread_rng().gen_range(200..500);
                sleep(Duration::from_millis(delay_ms)).await;

                let result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;
                let _ = sender
                    .send(BenchmarkPriceResult {
                        trade_indices: tx_ids,
                        result,
                    })
                    .await;
            });
        }
        drop(tx3);

        let mut benchmark_enriched = 0usize;
        let mut benchmark_skipped = 0usize;
        let mut breaker3 = CircuitBreaker::new(CIRCUIT_BREAKER_THRESHOLD);

        while let Some(fetch) = rx3.recv().await {
            match fetch.result {
                Ok(Some(price)) => {
                    for tx_id in &fetch.trade_indices {
                        db.update_benchmark_price(*tx_id, Some(price))?;
                        benchmark_enriched += 1;
                    }
                    breaker3.record_success();
                }
                Ok(None) | Err(_) => {
                    // Mark as processed to avoid re-fetch
                    for tx_id in &fetch.trade_indices {
                        db.update_benchmark_price(*tx_id, None)?;
                        benchmark_skipped += 1;
                    }
                    breaker3.record_failure();
                }
            }
            pb3.set_message(format!("{} ok, {} skip", benchmark_enriched, benchmark_skipped));
            pb3.inc(1);

            if breaker3.is_tripped() {
                pb3.println(format!(
                    "Circuit breaker tripped after {} consecutive failures, stopping Phase 3",
                    CIRCUIT_BREAKER_THRESHOLD
                ));
                join_set3.abort_all();
                break;
            }
        }

        pb3.finish_with_message(format!(
            "Phase 3 done: {} enriched, {} skipped",
            benchmark_enriched, benchmark_skipped
        ));

        (benchmark_enriched, benchmark_skipped, breaker3.is_tripped())
    };

    // Step 5: Summary
    eprintln!();
    eprintln!(
        "Price enrichment complete: {} enriched, {} failed, {} skipped (historical)",
        enriched, failed, skipped + skipped_parse_errors
    );
    if tiingo_resolved > 0 {
        eprintln!(
            "  Tiingo fallback: {} trades resolved via Tiingo",
            tiingo_resolved
        );
    }
    eprintln!(
        "  Phase 2: {} current prices enriched, {} skipped",
        current_enriched, current_skipped
    );
    eprintln!(
        "  Phase 3: {} benchmark prices enriched, {} skipped",
        benchmark_enriched, benchmark_skipped
    );
    eprintln!(
        "  ({} total trades, {} unique ticker-date pairs, {} unique tickers)",
        total_trades, unique_pairs, unique_tickers
    );

    if breaker.is_tripped() || breaker3_tripped {
        eprintln!(
            "Warning: Circuit breaker tripped after {} consecutive failures -- some trades were not processed",
            CIRCUIT_BREAKER_THRESHOLD
        );
        bail!("Enrichment aborted due to circuit breaker");
    }

    Ok(())
}
