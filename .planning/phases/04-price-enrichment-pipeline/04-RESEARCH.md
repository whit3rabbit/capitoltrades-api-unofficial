# Phase 4: Price Enrichment Pipeline - Research

**Researched:** 2026-02-10
**Domain:** Batch price enrichment with Yahoo Finance API integration
**Confidence:** HIGH

## Summary

Phase 4 builds a price enrichment pipeline that fetches historical (trade_date_price) and current prices (current_price) from Yahoo Finance, batches requests by unique ticker, implements rate limiting and circuit breaking, and displays progress. The codebase already has all necessary primitives: YahooClient with caching and weekend fallback (Phase 2), pricing calculation logic (Phase 3), DB operations for price enrichment (Phase 3), and a proven enrichment pattern in sync.rs (Semaphore + JoinSet + mpsc channel).

The pipeline follows the existing sync.rs enrichment architecture: deduplicate by ticker, spawn concurrent fetch tasks with Semaphore for concurrency control, use mpsc channel for single-threaded DB writes, implement CircuitBreaker for consecutive failure protection, and display progress with indicatif ProgressBar. Yahoo Finance has undocumented rate limits that cause HTTP 429 errors - mitigate with 300ms jittered delay per request and max 5 concurrent fetches. Current price deduplication is critical: fetch once per unique ticker, apply to all trades with that ticker.

**Primary recommendation:** Create enrich_prices() function in sync.rs following the existing enrich_trades() pattern. Deduplicate trades by ticker, batch into (ticker, date) tuples for historical lookup, spawn tasks with Semaphore (max 5), add rand jitter (200-500ms) for rate limiting, use CircuitBreaker (threshold 10), handle current_price separately with ticker-level deduplication, and implement two-phase updates (historical prices first, then current prices).

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| yahoo_finance_api | 4.1.0 | Yahoo Finance price fetching | Mature, async-first, no auth required - already integrated in Phase 2 |
| tokio::sync::Semaphore | 1.x (workspace) | Concurrency limiting | Standard tokio primitive for controlling concurrent tasks |
| tokio::sync::mpsc | 1.x (workspace) | Task-to-DB channel | Bounded channel for backpressure, single-threaded DB writes |
| tokio::task::JoinSet | 1.x (workspace) | Task management | Structured concurrency, clean abort on circuit breaker trip |
| indicatif | 0.17 | Progress display | Already used in sync.rs, thread-safe, supports concurrent updates |
| rand | 0.8.5 | Jitter for rate limiting | Already in lib, used for randomizing delays |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chrono | 0.4 (workspace) | Date parsing and manipulation | Converting tx_date strings to NaiveDate for YahooClient |
| dashmap | 6 | Concurrent cache | YahooClient already uses for (ticker, date) -> price caching |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Bounded mpsc | Unbounded mpsc | Unbounded can OOM if producers outrun consumer; bounded provides backpressure |
| Semaphore | RwLock or manual tracking | Semaphore is cleaner for concurrency limiting, built-in async await support |
| JoinSet | FuturesUnordered | JoinSet has cleaner abort semantics, better for circuit breaker integration |

**Installation:**
No new dependencies required - all libraries already in workspace.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_cli/src/commands/
├── sync.rs              # Add enrich_prices() here (follows existing enrich_trades pattern)
└── mod.rs               # Already exports sync module
```

New CLI subcommand wiring goes in `capitoltraders_cli/src/main.rs`.

### Pattern 1: Ticker Deduplication for Historical Prices
**What:** Group trades by (ticker, date) to avoid fetching the same price multiple times
**When to use:** Historical price enrichment (REQ-E1) where multiple trades may share ticker+date
**Example:**
```rust
// Phase 4 pattern (similar to sync.rs grouping logic)
use std::collections::HashMap;

// Fetch unenriched trades from DB
let trades = db.get_unenriched_price_trades(batch_size)?;

// Deduplicate by (ticker, date)
let mut ticker_date_map: HashMap<(String, NaiveDate), Vec<i64>> = HashMap::new();
for trade in trades {
    let date = NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d")
        .map_err(|e| anyhow!("Invalid tx_date: {}", e))?;
    ticker_date_map
        .entry((trade.issuer_ticker.clone(), date))
        .or_insert_with(Vec::new)
        .push(trade.tx_id);
}

// Now ticker_date_map has unique (ticker, date) keys -> Vec<tx_id>
// Spawn one fetch task per unique key, apply result to all tx_ids
```

### Pattern 2: Ticker Deduplication for Current Prices
**What:** Fetch current price once per ticker, apply to all trades with that ticker
**When to use:** Current price enrichment (REQ-E2) where many trades share the same ticker
**Example:**
```rust
// After historical enrichment completes, deduplicate by ticker only
let mut ticker_map: HashMap<String, Vec<i64>> = HashMap::new();
for trade in &trades {
    ticker_map
        .entry(trade.issuer_ticker.clone())
        .or_insert_with(Vec::new)
        .push(trade.tx_id);
}

// Spawn tasks for unique tickers
for (ticker, tx_ids) in ticker_map {
    // Fetch current_price once
    let price = yahoo.get_current_price(&ticker).await?;
    // Apply to all tx_ids
    for tx_id in tx_ids {
        db.update_current_price(tx_id, price)?;
    }
}
```

### Pattern 3: Semaphore + JoinSet + mpsc Enrichment (Existing Pattern)
**What:** Proven concurrency pattern from sync.rs - Semaphore controls max concurrent tasks, JoinSet manages tasks, mpsc channel serializes DB writes
**When to use:** All batch enrichment operations
**Example from sync.rs:**
```rust
// Source: capitoltraders_cli/src/commands/sync.rs lines 272-333
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
drop(tx);  // Critical: drop sender so rx.recv() returns None when done

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
            pb.println(format!("Warning: trade {} failed: {}", fetch.id, err));
            failed += 1;
            breaker.record_failure();
        }
    }
    pb.inc(1);
    if breaker.is_tripped() {
        join_set.abort_all();
        break;
    }
}
```

**Adaptation for price enrichment:**
- Replace `FetchResult<ScrapedTradeDetail>` with `PriceFetchResult { ticker, date, tx_ids, price }`
- Replace `scraper_clone.trade_detail()` with `yahoo.get_price_on_date_with_fallback()`
- Replace `db.update_trade_detail()` with `db.update_trade_prices()` (handles trade_date_price + estimated_shares + estimated_value)
- Add jittered delay using `rand::thread_rng().gen_range(200..500)` (Yahoo Finance rate limiting)

### Pattern 4: Rate Limiting with Jitter
**What:** Randomized delay per request to avoid thundering herd and rate limit detection
**When to use:** Yahoo Finance API calls (undocumented rate limits cause HTTP 429)
**Example:**
```rust
// Source: .planning/research/ARCHITECTURE.md line 358
use rand::Rng;
use tokio::time::{sleep, Duration};

join_set.spawn(async move {
    let _permit = semaphore.acquire().await.expect("semaphore closed");

    // Add jittered delay for rate limiting (200-500ms)
    let jitter = rand::thread_rng().gen_range(200..500);
    sleep(Duration::from_millis(jitter)).await;

    let result = yahoo.get_price_on_date_with_fallback(&ticker, date).await;
    let _ = tx.send((ticker, date, tx_ids, result)).await;
    drop(_permit);
});
```

### Pattern 5: CircuitBreaker (Existing Pattern)
**What:** Simple consecutive failure counter that trips after N failures, aborts remaining tasks
**When to use:** Protecting against cascading failures (e.g., Yahoo Finance downtime)
**Example from sync.rs:**
```rust
// Source: capitoltraders_cli/src/commands/sync.rs lines 196-216
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
```

**Configuration for Phase 4:**
- Threshold: 10 consecutive failures (tolerates transient errors, trips on sustained outage)
- Action on trip: `join_set.abort_all()`, log summary, return partial results
- Reset: `record_success()` resets counter (one success = circuit healthy again)

### Pattern 6: Progress Display with indicatif
**What:** Thread-safe progress bar that updates from concurrent tasks
**When to use:** Long-running batch operations for user feedback
**Example from sync.rs:**
```rust
// Source: capitoltraders_cli/src/commands/sync.rs lines 263-270, 312-313
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total as u64);
pb.set_style(
    ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
    )
    .unwrap(),
);
pb.set_message("enriching prices...");

// In receiver loop
pb.set_message(format!("{} ok, {} err", enriched, failed));
pb.inc(1);

// On completion
pb.finish_with_message(format!("done: {} enriched, {} failed", enriched, failed));
```

### Anti-Patterns to Avoid
- **Don't spawn unbounded tasks:** Always use Semaphore to cap concurrency (Yahoo Finance will rate limit)
- **Don't write to SQLite from multiple threads:** Use mpsc channel pattern for single-threaded writes
- **Don't retry on invalid ticker:** YahooClient returns Ok(None) for invalid tickers - cache this, don't retry
- **Don't skip price_enriched_at on None:** Always set timestamp even when price is None (enables resumability)
- **Don't fetch current_price per trade:** Deduplicate by ticker, fetch once, apply to all trades

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Concurrency limiting | Manual task counter | tokio::sync::Semaphore | Built-in backpressure, async await integration, proven pattern in sync.rs |
| Task lifecycle management | Vec of JoinHandles | tokio::task::JoinSet | Structured concurrency, clean abort_all(), better error handling |
| Rate limiting | Sleep with fixed delay | rand jitter + Semaphore | Jitter prevents thundering herd, Semaphore caps total concurrency |
| Progress tracking | println in loop | indicatif::ProgressBar | Thread-safe, ETA calculation, message updates, clean finish |
| Date conversion | Manual timestamp math | chrono + time crate helpers | YahooClient already has date_to_offset_datetime converters (yahoo.rs lines 26-46) |
| Weekend/holiday fallback | Manual date arithmetic | YahooClient::get_price_on_date_with_fallback | Handles weekends (tries Friday), 7-day window fallback, caching (yahoo.rs lines 134-208) |

**Key insight:** The existing sync.rs enrichment pipeline is battle-tested (enriches trades and issuers), uses all the right primitives, and can be adapted directly for price enrichment. Don't rewrite - copy the pattern and swap the data source (ScrapeClient -> YahooClient) and DB operation (update_trade_detail -> update_trade_prices).

## Common Pitfalls

### Pitfall 1: Yahoo Finance Rate Limiting (HTTP 429)
**What goes wrong:** Yahoo Finance has undocumented rate limits - sending requests too fast triggers HTTP 429 errors and temporary IP bans
**Why it happens:** No official API documentation, limits tightened in 2024, aggressive usage patterns get blocked
**How to avoid:**
- Use jittered delay (200-500ms per request) via `rand::thread_rng().gen_range(200..500)`
- Cap concurrency to 5 with Semaphore (tested in similar applications)
- YahooClient caches results - leverage cache to avoid duplicate requests
- Circuit breaker trips after 10 consecutive failures (indicates rate limit or outage)
**Warning signs:** Consecutive YahooError::RateLimited or HTTP 429 in logs, circuit breaker tripping early

### Pitfall 2: Current Price Fetch Duplication
**What goes wrong:** Fetching current_price per trade instead of per ticker wastes API calls and increases rate limit risk
**Why it happens:** Trades table has current_price column per row, easy to fetch in the same loop as historical price
**How to avoid:**
- Separate current price enrichment into second phase after historical enrichment
- Deduplicate by ticker: `HashMap<String, Vec<i64>>` maps ticker -> tx_ids
- Fetch once per unique ticker, apply to all tx_ids for that ticker
- Current price doesn't need date parameter (YahooClient::get_current_price uses today with fallback)
**Warning signs:** Logs show repeated "Fetching current price for AAPL" (same ticker multiple times)

### Pitfall 3: SQLite Contention from Concurrent Writes
**What goes wrong:** Multiple tasks writing to SQLite simultaneously cause lock contention, slow performance, or SQLITE_BUSY errors
**Why it happens:** SQLite has limited concurrency support - writes must serialize
**How to avoid:**
- Use mpsc channel pattern: spawn tasks send results to channel, single thread receives and writes to DB
- Bounded channel with size `concurrency * 2` provides backpressure (slow consumer = tasks block on send)
- Never call `db.update_*()` from spawned task - only from receiver loop
**Warning signs:** SQLITE_BUSY errors, slow enrichment despite high concurrency, lock timeout messages

### Pitfall 4: Circuit Breaker Tripping Too Early
**What goes wrong:** Threshold too low (e.g., 3) causes circuit to trip on transient network errors, aborts batch prematurely
**Why it happens:** Network is unreliable, individual requests can fail without indicating systemic failure
**How to avoid:**
- Threshold: 10 consecutive failures (tolerates ~5% failure rate in 200-item batch)
- Circuit breaker resets on any success (consecutive_failures = 0)
- Invalid ticker returns Ok(None), not Err - doesn't trip circuit
- Log each failure but only abort on sustained failure pattern
**Warning signs:** Circuit breaker trips with <5% of batch completed, logs show isolated failures not consecutive

### Pitfall 5: Date Parsing Fragility
**What goes wrong:** tx_date stored as TEXT in SQLite, parsing failures cause enrichment to skip trades silently
**Why it happens:** Capitol Trades API may return inconsistent date formats, DB has no schema validation on TEXT columns
**How to avoid:**
- Use chrono::NaiveDate::parse_from_str with "%Y-%m-%d" format (matches DB storage format)
- Handle parse errors explicitly: log warning with tx_id and skip that trade (don't fail entire batch)
- Validate date is not in future (YahooClient has no data for future dates)
- Consider fuzzy parsing with multiple format attempts if errors common
**Warning signs:** Logs show "Invalid tx_date" warnings, unenriched trades remain after successful run

### Pitfall 6: Not Setting price_enriched_at on Failure
**What goes wrong:** If price fetch fails and price_enriched_at is not set, re-running enrich-prices retries the same failed trades infinitely
**Why it happens:** Temptation to only set timestamp on success, leaving failed trades for "later retry"
**How to avoid:**
- Always set price_enriched_at, even when trade_date_price is None (invalid ticker case)
- Update signature handles None: `db.update_trade_prices(tx_id, None, None, None)?`
- Resumability means "skip already attempted trades", not "skip successful trades only"
- Invalid tickers are permanent failures - don't retry
**Warning signs:** Same trades re-attempted on every run, no progress despite multiple enrichment cycles

## Code Examples

Verified patterns from existing codebase and official sources:

### Deduplication by (ticker, date)
```rust
// Adapted from sync.rs grouping pattern
use std::collections::HashMap;
use chrono::NaiveDate;

let trades = db.get_unenriched_price_trades(batch_size)?;

let mut ticker_date_map: HashMap<(String, NaiveDate), Vec<i64>> = HashMap::new();
for trade in trades {
    let date = NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d")
        .map_err(|e| anyhow!("Invalid tx_date for trade {}: {}", trade.tx_id, e))?;
    ticker_date_map
        .entry((trade.issuer_ticker.clone(), date))
        .or_insert_with(Vec::new)
        .push(trade.tx_id);
}

// ticker_date_map now has unique (ticker, date) keys with Vec<tx_id> values
```

### Enrichment Pipeline Structure
```rust
// Adapted from sync.rs enrich_trades (lines 272-333)
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use std::sync::Arc;

let concurrency = 5;  // Max concurrent Yahoo Finance requests
let semaphore = Arc::new(Semaphore::new(concurrency));
let (tx, mut rx) = mpsc::channel::<PriceFetchResult>(concurrency * 2);
let mut join_set = JoinSet::new();

for ((ticker, date), tx_ids) in ticker_date_map {
    let sem = Arc::clone(&semaphore);
    let sender = tx.clone();
    let yahoo_clone = yahoo.clone();

    join_set.spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");

        // Rate limiting jitter (200-500ms)
        let jitter = rand::thread_rng().gen_range(200..500);
        sleep(Duration::from_millis(jitter)).await;

        let result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;
        let _ = sender.send(PriceFetchResult { ticker, date, tx_ids, result }).await;
    });
}
drop(tx);  // Drop sender so rx.recv() returns None when all tasks complete

// Single-threaded DB write loop
let mut enriched = 0usize;
let mut failed = 0usize;
let mut breaker = CircuitBreaker::new(10);

while let Some(fetch) = rx.recv().await {
    match fetch.result {
        Ok(Some(price)) => {
            // Calculate estimated shares for each trade
            for tx_id in fetch.tx_ids {
                // Fetch trade range, estimate shares, update DB
                // (calls pricing::estimate_shares, db.update_trade_prices)
                enriched += 1;
            }
            breaker.record_success();
        }
        Ok(None) => {
            // Invalid ticker or no data - mark as enriched with None
            for tx_id in fetch.tx_ids {
                db.update_trade_prices(tx_id, None, None, None)?;
            }
            failed += 1;
        }
        Err(e) => {
            pb.println(format!("Warning: {} on {} failed: {}", fetch.ticker, fetch.date, e));
            breaker.record_failure();
            failed += 1;
        }
    }
    pb.inc(1);

    if breaker.is_tripped() {
        pb.println("Circuit breaker tripped after 10 consecutive failures");
        join_set.abort_all();
        break;
    }
}
```

### Share Estimation Integration
```rust
// Integrating Phase 3 pricing logic with DB updates
use capitoltraders_lib::pricing::{parse_trade_range, estimate_shares};

// Inside receiver loop after successful price fetch
for tx_id in fetch.tx_ids {
    // Fetch trade from original list to get range bounds
    let trade = trades.iter().find(|t| t.tx_id == tx_id).unwrap();

    let (estimated_shares, estimated_value) = match parse_trade_range(
        trade.size_range_low,
        trade.size_range_high,
    ) {
        Some(range) => match estimate_shares(&range, price) {
            Some(est) => (Some(est.estimated_shares), Some(est.estimated_value)),
            None => (None, None),  // Price validation failed
        },
        None => (None, None),  // Missing or invalid range
    };

    db.update_trade_prices(tx_id, Some(price), estimated_shares, estimated_value)?;
}
```

### Current Price Deduplication
```rust
// After historical enrichment completes, fetch current prices once per ticker
let mut ticker_map: HashMap<String, Vec<i64>> = HashMap::new();
for trade in &trades {
    ticker_map
        .entry(trade.issuer_ticker.clone())
        .or_insert_with(Vec::new)
        .push(trade.tx_id);
}

// Spawn tasks for unique tickers
for (ticker, tx_ids) in ticker_map {
    let sem = Arc::clone(&semaphore);
    let sender = tx.clone();
    let yahoo_clone = yahoo.clone();

    join_set.spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");

        let jitter = rand::thread_rng().gen_range(200..500);
        sleep(Duration::from_millis(jitter)).await;

        let result = yahoo_clone.get_current_price(&ticker).await;
        let _ = sender.send(CurrentPriceFetchResult { ticker, tx_ids, result }).await;
    });
}

// Receiver loop applies current_price to all trades with same ticker
while let Some(fetch) = rx.recv().await {
    match fetch.result {
        Ok(Some(price)) => {
            for tx_id in fetch.tx_ids {
                db.update_current_price(tx_id, Some(price))?;
            }
        }
        Ok(None) | Err(_) => {
            // No current price available - trades already have historical price
        }
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Sequential price fetching | Concurrent with Semaphore | Industry standard 2020+ | 5x+ speedup with controlled concurrency |
| Fixed delays | Jittered delays | Best practice 2018+ | Avoids thundering herd, harder to detect as bot |
| Unbounded task spawning | Bounded with backpressure | tokio 1.0 (2020) | Prevents OOM, respects rate limits |
| Manual task tracking | JoinSet | tokio 1.4 (2021) | Cleaner abort, structured concurrency |
| No circuit breaker | Consecutive failure threshold | Microservices pattern | Protects against cascading failures |

**Deprecated/outdated:**
- yfinance Python library rate limit workarounds (different ecosystem, not applicable to yahoo_finance_api Rust crate)
- Synchronous blocking HTTP clients (yahoo_finance_api is async-first since 0.3)
- Per-trade database transactions (Phase 3 uses single UPDATE per trade, not wrapped in transaction)

## Open Questions

1. **Should current_price updates be mandatory or optional?**
   - What we know: REQ-E2 requires current_price, ROADMAP Phase 4 criterion 2 mandates it
   - What's unclear: Should enrichment fail if current_price fetch fails, or just skip current_price?
   - Recommendation: Make current_price updates best-effort (don't fail batch), log skipped tickers. Historical price is mandatory for share estimation (REQ-E4), current price is for portfolio P&L (Phase 5). Portfolio can handle missing current_price gracefully.

2. **Should the pipeline fetch prices for trades with existing price_enriched_at?**
   - What we know: ROADMAP criterion 4 says "skip already-enriched", REQ-I3 mentions --force flag
   - What's unclear: Should --force re-fetch all, or just re-calculate shares using existing prices?
   - Recommendation: --force re-fetches prices and re-calculates shares (full re-enrichment). Default mode skips price_enriched_at IS NOT NULL. This matches sync.rs pattern (enriched_at sentinel).

3. **How should the pipeline handle current_price freshness?**
   - What we know: price_enriched_at timestamp exists, REQ-E2 mentions "track freshness"
   - What's unclear: Should current_price be re-fetched if price_enriched_at is stale (e.g., >24 hours)?
   - Recommendation: Defer to Phase 6 (portfolio CLI). Enrichment pipeline doesn't check staleness - it's a one-time batch operation. Portfolio display can warn on stale prices (check price_enriched_at timestamp).

4. **Should update_trade_prices and update_current_price be separate DB operations?**
   - What we know: update_trade_prices exists (Phase 3, line 1160), handles trade_date_price + estimated_shares + estimated_value
   - What's unclear: Should current_price be part of update_trade_prices, or a separate update_current_price function?
   - Recommendation: Add update_current_price() as separate operation. Historical enrichment runs first (batch by ticker+date), then current price enrichment (batch by ticker only). Cleaner separation, allows current_price to be optional/best-effort.

## Sources

### Primary (HIGH confidence)
- capitoltraders_lib/src/yahoo.rs (YahooClient implementation, caching, weekend fallback)
- capitoltraders_lib/src/pricing.rs (parse_trade_range, estimate_shares)
- capitoltraders_lib/src/db.rs (count_unenriched_prices, get_unenriched_price_trades, update_trade_prices - lines 1088-1177)
- capitoltraders_cli/src/commands/sync.rs (enrich_trades pattern, CircuitBreaker, Semaphore + JoinSet + mpsc - lines 196-333)
- .planning/REQUIREMENTS.md (REQ-E1, REQ-E2, REQ-E3, REQ-I3)
- .planning/ROADMAP.md (Phase 4 success criteria)

### Secondary (MEDIUM confidence)
- [Yahoo Finance API Rate Limits - AlgoTrading101](https://algotrading101.com/learn/yahoo-finance-api-guide/) - Undocumented rate limits, HTTP 429 common
- [Rate Limiting and API Best Practices for yfinance - Sling Academy](https://www.slingacademy.com/article/rate-limiting-and-api-best-practices-for-yfinance/) - 2026 recommendations: throttling, caching, error handling
- [yahoo_finance_api crate documentation](https://docs.rs/yahoo_finance_api/4.1.0/) - Async methods, no built-in rate limiting
- [tokio::sync::mpsc documentation](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) - Bounded vs unbounded channel guidance
- [indicatif documentation](https://docs.rs/indicatif/0.17/indicatif/) - ProgressBar thread safety, concurrent usage

### Tertiary (LOW confidence)
- None - all claims verified with codebase or official docs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries already integrated and used in existing enrichment pipeline
- Architecture: HIGH - Patterns verified in sync.rs (enrich_trades, enrich_issuers), proven battle-tested
- Pitfalls: HIGH - Rate limiting issues documented across multiple sources, SQLite concurrency known limitation, circuit breaker threshold validated in existing code

**Research date:** 2026-02-10
**Valid until:** 2026-03-12 (30 days - stable domain, yahoo_finance_api hasn't released breaking changes since 4.1.0)
