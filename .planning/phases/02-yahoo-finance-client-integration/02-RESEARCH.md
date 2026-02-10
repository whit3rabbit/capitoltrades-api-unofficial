# Phase 02: Yahoo Finance Client Integration - Research

**Researched:** 2026-02-10
**Domain:** Rust financial data enrichment via Yahoo Finance API
**Confidence:** MEDIUM-HIGH

## Summary

Phase 2 integrates Yahoo Finance historical price fetching into the existing Capitol Traders system. The core challenge is bridging two different datetime crates (chrono used in our codebase, time 0.3 required by yahoo_finance_api) while maintaining clean error handling and respecting rate limits on an unofficial API.

The yahoo_finance_api 4.1.0 crate provides a lightweight async wrapper around Yahoo Finance's chart API. It returns Quote structs with adjclose (adjusted for splits/dividends) alongside raw OHLC data. The crate uses time::OffsetDateTime for timestamps, so conversion adapters are needed to work with our chrono::NaiveDate-based trade dates.

The enrichment pipeline should reuse the proven Semaphore + JoinSet + mpsc pattern from sync.rs, with added considerations for Yahoo Finance's undocumented rate limits (conservative approach: 200-500ms jitter between requests, circuit breaker for consecutive failures).

**Primary recommendation:** Implement YahooClient as a module in capitoltraders_lib (not a separate crate), wrap yahoo_finance_api with centralized time/chrono conversion, reuse existing enrichment pipeline patterns, and treat invalid tickers as None (not errors) to gracefully handle delisted stocks.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| yahoo_finance_api | 4.1.0 | Fetch historical/current prices from Yahoo Finance | Mature (4.x series), async-first, lightweight, compatible with reqwest 0.12, uses adjusted close prices |
| time | 0.3 | Datetime handling for Yahoo Finance API | Required by yahoo_finance_api, provides OffsetDateTime for Unix timestamp operations |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio::sync::Semaphore | (workspace 1.x) | Rate limiting concurrent API requests | Already used in sync.rs enrichment pipeline, controls concurrency |
| tokio::sync::mpsc | (workspace 1.x) | Channel for single-threaded DB writes | Already used in sync.rs, SQLite write constraint |
| tokio::task::JoinSet | (workspace 1.x) | Concurrent task spawning and abort | Already used in sync.rs, circuit breaker abort pattern |
| chrono | (workspace 0.4) | Project-wide date handling | Trade dates stored as NaiveDate, needs conversion to time::OffsetDateTime |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| yahoo_finance_api | yfinance-rs 0.3.x | More features (options, fundamentals) but heavier dependency tree, more complex API, overkill for historical price fetching |
| yahoo_finance_api | yahoo-finance 0.x | Older, less maintained, synchronous API (no async), incompatible with existing tokio runtime |
| time 0.3 | Direct Unix timestamp math | Error-prone timezone handling, loses type safety, manual leap-second considerations |

**Installation:**
```bash
# Add to capitoltraders_lib/Cargo.toml [dependencies]
yahoo_finance_api = "4.1.0"
time = { version = "0.3", features = ["macros"] }
```

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── yahoo.rs              # NEW: YahooClient wrapper, time/chrono conversion
├── error.rs              # EXTEND: add YahooError variant
└── db.rs                 # EXTEND: price query/update methods (schema exists)
```

### Pattern 1: Chrono-to-Time Conversion Adapter

**What:** Centralized bidirectional conversion between chrono::NaiveDate and time::OffsetDateTime

**When to use:** Yahoo Finance API requires time::OffsetDateTime, but our trade dates are chrono::NaiveDate

**Example:**
```rust
// Source: Combined from docs.rs/time and docs.rs/chrono
use chrono::NaiveDate;
use time::OffsetDateTime;

/// Convert NaiveDate to OffsetDateTime (UTC midnight on that date)
fn date_to_offset_datetime(date: NaiveDate) -> Result<OffsetDateTime, YahooError> {
    // NaiveDate -> NaiveDateTime (midnight UTC)
    let datetime = date.and_hms_opt(0, 0, 0)
        .ok_or_else(|| YahooError::InvalidDate(format!("Invalid date: {}", date)))?;

    // NaiveDateTime -> Unix timestamp
    let timestamp = datetime.and_utc().timestamp();

    // Unix timestamp -> OffsetDateTime
    OffsetDateTime::from_unix_timestamp(timestamp)
        .map_err(|_| YahooError::InvalidDate(format!("Timestamp out of range: {}", date)))
}

/// Convert OffsetDateTime back to NaiveDate (for roundtrip verification)
fn offset_datetime_to_date(dt: OffsetDateTime) -> NaiveDate {
    let timestamp = dt.unix_timestamp();
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .unwrap()
        .checked_add_days(chrono::Days::new((timestamp / 86400) as u64))
        .expect("Date out of range")
}
```

**Critical insight:** NaiveDate has no timezone, but Yahoo Finance API expects UTC. Always use midnight UTC (0:00:00) when converting trade dates to avoid timezone-based date shifts.

### Pattern 2: YahooClient Wrapper with Caching

**What:** Thin wrapper around yahoo_finance_api::YahooConnector with DashMap cache for deduplication

**When to use:** Multiple trades on same ticker/date should only fetch once

**Example:**
```rust
// Adapted from existing capitoltraders_lib cache pattern and yahoo_finance_api docs
use yahoo_finance_api::{YahooConnector, YahooError as UpstreamYahooError};
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct YahooClient {
    connector: YahooConnector,
    cache: Arc<DashMap<(String, NaiveDate), Option<f64>>>,  // (ticker, date) -> price
}

impl YahooClient {
    pub fn new() -> Result<Self, YahooError> {
        let connector = YahooConnector::new()
            .map_err(|e| YahooError::Upstream(e))?;
        Ok(Self {
            connector,
            cache: Arc::new(DashMap::new()),
        })
    }

    /// Fetch adjusted close price for a ticker on a specific date.
    /// Returns None if no data (weekend, holiday, delisted ticker).
    pub async fn get_price_on_date(
        &self,
        ticker: &str,
        date: NaiveDate,
    ) -> Result<Option<f64>, YahooError> {
        let cache_key = (ticker.to_string(), date);

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(*cached);
        }

        // Convert NaiveDate -> OffsetDateTime range (midnight to next day)
        let start = date_to_offset_datetime(date)?;
        let end = start + time::Duration::days(1);

        // Fetch from Yahoo Finance
        let response = self.connector
            .get_quote_history(ticker, start, end)
            .await
            .map_err(|e| match e {
                UpstreamYahooError::NoQuotes => return Ok(None),  // No data for this ticker/date
                UpstreamYahooError::TooManyRequests(_) => YahooError::RateLimited,
                _ => YahooError::Upstream(e),
            })?;

        // Extract quotes
        let quotes = response.quotes()
            .map_err(|e| YahooError::ParseFailed(e.to_string()))?;

        // Use adjusted close (accounts for splits/dividends)
        let price = quotes.first().map(|q| q.adjclose);

        // Cache result (including None for missing data)
        self.cache.insert(cache_key, price);

        Ok(price)
    }
}
```

### Pattern 3: Enrichment Pipeline Reuse

**What:** Reuse Semaphore + JoinSet + mpsc pattern from sync.rs for concurrent price fetching

**When to use:** Fetching prices for hundreds/thousands of unique (ticker, date) pairs

**Example:**
```rust
// Adapted from capitoltraders_cli/src/commands/sync.rs:272-320
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

async fn enrich_prices(
    yahoo: &YahooClient,
    db: &Db,
    batch_size: Option<i64>,
    concurrency: usize,
    max_failures: usize,
) -> Result<EnrichmentResult> {
    // 1. Query trades needing price enrichment
    let trades = db.get_trades_needing_prices(batch_size)?;

    // 2. Deduplicate by (ticker, date) to minimize API calls
    let mut lookups: HashMap<(String, NaiveDate), Vec<i64>> = HashMap::new();
    for trade in trades {
        lookups.entry((trade.issuer_ticker, trade.tx_date))
            .or_default()
            .push(trade.tx_id);
    }

    // 3. Concurrent fetch with rate limiting
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let (tx, mut rx) = mpsc::channel(concurrency * 2);
    let mut join_set = JoinSet::new();
    let mut breaker = CircuitBreaker::new(max_failures);

    for ((ticker, date), trade_ids) in lookups {
        if breaker.is_tripped() {
            break;
        }

        let sem = semaphore.clone();
        let yahoo = yahoo.clone();
        let tx = tx.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");

            // Jittered delay for rate limiting (200-500ms)
            let jitter = rand::thread_rng().gen_range(200..500);
            tokio::time::sleep(Duration::from_millis(jitter)).await;

            let result = yahoo.get_price_on_date(&ticker, date).await;
            let _ = tx.send((ticker, date, trade_ids, result)).await;
        });
    }
    drop(tx);

    // 4. Single-threaded DB writes from mpsc receiver
    let mut success = 0;
    let mut failed = 0;

    while let Some((ticker, date, trade_ids, result)) = rx.recv().await {
        match result {
            Ok(Some(price)) => {
                for tx_id in trade_ids {
                    db.update_trade_price(tx_id, price)?;
                }
                breaker.record_success();
                success += 1;
            }
            Ok(None) => {
                // No price data (weekend/holiday/delisted), not an error
                failed += 1;
            }
            Err(e) => {
                eprintln!("Failed to fetch {} on {}: {}", ticker, date, e);
                breaker.record_failure();
                failed += 1;
            }
        }
    }

    Ok(EnrichmentResult { success, failed })
}
```

### Anti-Patterns to Avoid

- **Don't create a separate yahoo_finance crate:** Integration code is project-specific (200-400 LOC max), keep as lib module
- **Don't use raw close prices:** Always use adjclose from Quote struct to account for splits/dividends
- **Don't treat missing data as errors:** None for weekends/holidays/delisted stocks is expected, not exceptional
- **Don't hardcode delays:** Use jittered random delays (200-500ms) to avoid thundering herd on rate limit recovery
- **Don't convert dates with local timezone:** Always use UTC midnight to avoid date shifts in different timezones

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Yahoo Finance HTTP API | Custom reqwest calls to query2.finance.yahoo.com | yahoo_finance_api crate | Handles authentication cookies, crumb tokens, JSON parsing, error types |
| Date range API queries | String formatting for periods ("1d", "1mo") | YahooConnector::get_quote_history(start, end) | Type-safe OffsetDateTime parameters, handles edge cases |
| Exponential backoff | Manual sleep(2^n) retry loops | Semaphore rate limiting + CircuitBreaker pattern | Proven in sync.rs, simpler than backoff crate for this use case |
| Ticker normalization | Per-call string manipulation | Centralized normalize_ticker() with override map | Edge cases (BRK.A vs BRK-A, GOOG vs GOOGL) need manual overrides |
| Unix timestamp conversion | Manual math (days * 86400) | time::OffsetDateTime::from_unix_timestamp() | Handles leap seconds, overflow checks, type safety |

**Key insight:** Yahoo Finance API has hidden complexity (cookies, crumbs, rate limits, error codes). The yahoo_finance_api crate handles this. Don't bypass it with raw reqwest calls.

## Common Pitfalls

### Pitfall 1: Weekend/Holiday Data Expectations

**What goes wrong:** Fetching price for Saturday trade date returns None, treated as error, circuit breaker trips

**Why it happens:** Stock markets are closed weekends/holidays, Yahoo Finance has no data for those dates

**How to avoid:**
- Return `Option<f64>` from get_price_on_date, None is valid
- Don't increment failure count for None results
- Consider fetching prior trading day (Friday for Saturday trades) via fallback logic

**Warning signs:** Circuit breaker trips early in enrichment, many "no quotes" errors for valid tickers

### Pitfall 2: Timezone-Based Date Shifts

**What goes wrong:** Trade date 2024-01-15 converted to OffsetDateTime in local timezone (PST = UTC-8), Yahoo Finance interprets as 2024-01-14

**Why it happens:** NaiveDate has no timezone, naive conversion to OffsetDateTime uses local offset

**How to avoid:**
- Always convert NaiveDate to midnight UTC: `date.and_hms_opt(0, 0, 0).unwrap().and_utc()`
- Never use `OffsetDateTime::now_local()` for date conversion
- Test with dates near timezone boundaries (2024-01-01 00:00:00 UTC)

**Warning signs:** Prices fetched are one day off from expected, especially near midnight

### Pitfall 3: Rate Limiting Without Jitter

**What goes wrong:** Concurrent tasks hit rate limit, all retry at same time, thundering herd, circuit breaker trips

**Why it happens:** Fixed delay (500ms) means all tasks wake simultaneously after rate limit cooldown

**How to avoid:**
- Use randomized jitter: `rand::thread_rng().gen_range(200..500)`
- Spread retries over time window to avoid synchronized bursts
- Monitor for HTTP 429 (TooManyRequests) and slow down globally (not just per-task)

**Warning signs:** Periodic bursts of 429 errors, circuit breaker trips after first burst

### Pitfall 4: Delisted Ticker Permanent Failures

**What goes wrong:** Stock delisted (e.g., SPAC merger), ticker invalid, yahoo_finance_api returns NoQuotes, treated as transient error, retries forever

**Why it happens:** No way to distinguish "temporarily unavailable" from "permanently invalid" ticker

**How to avoid:**
- Cache None results (invalid tickers) to prevent repeated lookups
- Consider ticker validation via search_ticker() before price fetch
- Manual override map for known delisted/renamed tickers in ingest_meta

**Warning signs:** Same ticker repeatedly fails with NoQuotes, enrichment never completes

### Pitfall 5: Adjusted Close Confusion

**What goes wrong:** Using `quote.close` instead of `quote.adjclose`, P&L calculations wrong after stock split

**Why it happens:** Close is raw price, doesn't account for 2:1 split (stock worth $100 becomes $50 overnight)

**How to avoid:**
- Always use `quote.adjclose` field
- Document why adjusted close matters (stock splits, dividends)
- Add test case: fetch price before/after known split (e.g., AAPL 2020-08-31 4:1 split)

**Warning signs:** Portfolio value drops 50% after split, cost basis doesn't match share count

## Code Examples

Verified patterns from official sources:

### Fetching Historical Quote

```rust
// Source: https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/
use yahoo_finance_api::{YahooConnector, YahooError};
use time::{macros::datetime, OffsetDateTime};

let provider = YahooConnector::new().unwrap();
let start = datetime!(2020-1-1 0:00:00.00 UTC);
let end = datetime!(2020-1-31 23:59:59.99 UTC);

let response = provider.get_quote_history("AAPL", start, end).await.unwrap();
let quotes = response.quotes().unwrap();

for quote in &quotes {
    println!(
        "Date: {:?}, Adjusted Close: {}",
        quote.timestamp,
        quote.adjclose
    );
}
```

### Chrono to Time Conversion

```rust
// Source: https://docs.rs/chrono/latest/chrono/naive/struct.NaiveDate.html
use chrono::NaiveDate;

// Create date from components
let date = NaiveDate::from_ymd_opt(2024, 1, 15)
    .ok_or_else(|| "Invalid date")?;

// Convert to datetime (midnight)
let datetime = date.and_hms_opt(0, 0, 0)
    .ok_or_else(|| "Invalid time")?;

// Convert to UTC timestamp
let timestamp = datetime.and_utc().timestamp();

// Convert to time::OffsetDateTime
let offset_dt = time::OffsetDateTime::from_unix_timestamp(timestamp)
    .map_err(|_| "Timestamp out of range")?;
```

### Error Handling Pattern

```rust
// Source: https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/enum.YahooError.html
use yahoo_finance_api::YahooError as UpstreamYahooError;

match yahoo_client.get_price_on_date("AAPL", date).await {
    Ok(Some(price)) => println!("Price: {}", price),
    Ok(None) => println!("No data (weekend/holiday/delisted)"),
    Err(YahooError::RateLimited) => {
        eprintln!("Rate limited, backing off");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    Err(YahooError::Upstream(UpstreamYahooError::NoQuotes)) => {
        println!("Ticker may be invalid or delisted");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Synchronous HTTP (yahoo-finance 0.x) | Async/await with tokio (yahoo_finance_api 4.x) | 2021 (v3.0) | Must use async fn, compatible with existing tokio runtime |
| time 0.2 | time 0.3 | 2023 | Breaking API changes, OffsetDateTime construction differs, incompatible macros |
| Raw close prices | Adjusted close (adjclose) | Always standard | Critical for correct historical returns, accounts for splits/dividends |
| Manual rate limiting | Semaphore-based concurrency control | N/A (pattern) | Simpler than exponential backoff for APIs with unknown limits |

**Deprecated/outdated:**
- yahoo-finance 0.x: Synchronous, not maintained, incompatible with tokio
- time 0.2: Incompatible with yahoo_finance_api 4.x which requires time 0.3

## Open Questions

1. **Weekend/Holiday Fallback Strategy**
   - What we know: Yahoo Finance returns no data for non-trading days
   - What's unclear: Should we automatically fetch prior trading day, or leave NULL?
   - Recommendation: Phase 2 stores NULL for weekends/holidays, Phase 3 adds fallback logic (fetch prior Friday for Saturday trades)

2. **Ticker Override Mechanism**
   - What we know: Some issuer_ticker values won't match Yahoo Finance symbols (BRK.A vs BRK-A)
   - What's unclear: Should overrides be in-code HashMap, DB table, or ingest_meta JSON?
   - Recommendation: Store in ingest_meta as JSON map `{"12345": "BRK-A"}`, queryable by issuer_id, allows runtime updates without code changes

3. **Current Price Update Frequency**
   - What we know: trades.current_price column exists for portfolio valuation
   - What's unclear: Should current_price update daily (cron job), on-demand (portfolio query), or manually (sync --refresh-prices)?
   - Recommendation: Phase 2 leaves current_price NULL, Phase 3 (price enrichment) decides update strategy

4. **Rate Limit Detection**
   - What we know: Yahoo Finance returns HTTP 429 for rate limits, but threshold is undocumented
   - What's unclear: What's the actual limit (per IP, per minute)? How long to back off?
   - Recommendation: Conservative approach (3 concurrent, 200-500ms jitter), monitor circuit breaker trips in production, adjust based on empirical data

## Sources

### Primary (HIGH confidence)
- [yahoo_finance_api crate docs](https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/) - API reference, YahooConnector methods, Quote struct
- [time crate OffsetDateTime docs](https://docs.rs/time/0.3/time/struct.OffsetDateTime.html) - Unix timestamp conversion, UTC handling
- [chrono NaiveDate docs](https://docs.rs/chrono/latest/chrono/naive/struct.NaiveDate.html) - Date construction, timestamp conversion
- [yahoo_finance_api YahooError enum](https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/enum.YahooError.html) - Error variants, when each triggers
- [yahoo_finance_api Quote struct](https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/struct.Quote.html) - Field definitions, adjclose vs close

### Secondary (MEDIUM confidence)
- [How to Implement Retry Logic with Exponential Backoff in Rust](https://oneuptime.com/blog/post/2026-01-07-rust-retry-exponential-backoff/view) - Backoff patterns (noted but using simpler Semaphore pattern)
- [How to Implement Exponential Backoff with Jitter in Rust](https://oneuptime.com/blog/post/2026-01-25-exponential-backoff-jitter-rust/view) - Jitter implementation
- [Timezone handling in Rust with Chrono-TZ](https://blog.logrocket.com/timezone-handling-in-rust-with-chrono-tz/) - Timezone pitfalls, NaiveDateTime behavior
- [Navigating the Yahoo Finance API Call Limit](https://apipark.com/technews/RZtyppGC.html) - Unofficial rate limit observations (2-3 req/sec safe)

### Tertiary (LOW confidence)
- [Why yfinance Keeps Getting Blocked](https://medium.com/@trading.dude/why-yfinance-keeps-getting-blocked-and-what-to-use-instead-92d84bb2cc01) - Python library rate limit issues (different implementation, but similar API)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - yahoo_finance_api 4.1.0 is mature, well-documented, actively maintained
- Architecture: HIGH - Patterns reused from proven sync.rs enrichment pipeline
- Time/chrono conversion: MEDIUM - Well-documented but error-prone, needs careful testing
- Rate limiting: MEDIUM - API limits undocumented, must rely on conservative empirical approach
- Pitfalls: HIGH - Weekend/holiday data, timezone shifts, adjusted close are well-known financial data issues

**Research date:** 2026-02-10
**Valid until:** 2026-03-10 (30 days, stable domain - Yahoo Finance API unchanged for years, time/chrono crates stable)
