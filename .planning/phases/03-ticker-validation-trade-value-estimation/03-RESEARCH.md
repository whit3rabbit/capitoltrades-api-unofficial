# Phase 3: Ticker Validation & Trade Value Estimation - Research

**Researched:** 2026-02-10
**Domain:** Stock ticker validation, dollar range parsing, share estimation, batch enrichment pipeline
**Confidence:** HIGH

## Summary

This phase extends the existing enrichment pipeline (established in Phase 1-2) to validate tickers before price lookup, parse dollar range strings from Capitol Trades API, and estimate share counts. The codebase already has the necessary patterns: CircuitBreaker in sync.rs (consecutive failure tracking), Semaphore + JoinSet + mpsc channel for concurrent enrichment, YahooClient with graceful invalid ticker handling (returns Ok(None) for NoQuotes/NoResult/ApiError), and schema v2 with estimated_shares/estimated_value columns.

The technical challenges are well-understood: ticker validation via Yahoo Finance lookup (no dedicated validation endpoint exists), dollar range parsing requires simple string manipulation (strip "$", split on "-", parse to f64), and batch deduplication follows standard Rust patterns (HashSet or itertools dedup_by_key). The Capitol Trades API provides dollar ranges in the `value` field (midpoint i64), but we need to reconstruct bounds from database columns size_range_low/size_range_high or parse from original response if stored.

**Primary recommendation:** Reuse the existing enrich_trades pattern from sync.rs, add ticker deduplication before spawning tasks (collect unique tickers, fetch once per ticker), implement simple regex-free dollar parsing (strip/split/parse), store estimated_shares/estimated_value alongside trade_date_price in a single UPDATE, and validate estimation math (shares * price falls within range) before persisting.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio::sync::Semaphore | 1.43 | Concurrent task limiting | Already used in sync.rs enrich_trades/enrich_issuers |
| tokio::task::JoinSet | 1.43 | Async task collection | Existing pattern for batch enrichment |
| tokio::sync::mpsc | 1.43 | Single-threaded DB write channel | Prevents SQLite contention |
| dashmap::DashMap | 6.0 | Concurrent cache | Already in YahooClient for price caching |
| std::collections::HashSet | std | Deduplication | Standard library, zero-cost for unique ticker extraction |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| indicatif::ProgressBar | 0.17 | CLI progress feedback | For enrich command, consistent with sync.rs |
| rusqlite | (existing) | SQLite database | Query unenriched trades, batch UPDATE |
| chrono::NaiveDate | (existing) | Date parsing for trade_date | Already used in YahooClient conversion helpers |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Manual string parsing | rusty_money crate | Overkill: Capitol Trades uses simple "$N - $M" format, no locale complexity |
| regex for range parsing | String::split + parse | Regex adds compile-time overhead for trivial pattern |
| Custom circuit breaker | failsafe-rs crate | Existing CircuitBreaker in sync.rs is 20 lines, proven, no external dep |
| Vec dedup_by_key | HashSet collection | HashSet is O(1) lookup vs O(n) for dedup, better for 1000+ trades |

**Installation:**
No new dependencies required. All functionality uses existing crates (tokio, dashmap, rusqlite, chrono).

## Architecture Patterns

### Recommended Function Structure
```
capitoltraders_lib/src/
├── validation.rs        # Add validate_ticker() function
├── yahoo.rs            # YahooClient already handles invalid tickers gracefully
└── db.rs               # Add count_unenriched_prices(), get_unenriched_price_trades(), update_trade_price_and_shares()

capitoltraders_cli/src/commands/
├── sync.rs             # Reference implementation: enrich_trades()
└── enrich_prices.rs    # New: ticker-aware enrichment with estimation
```

### Pattern 1: Ticker Deduplication Before Batch
**What:** Collect unique tickers from unenriched trades before spawning tasks
**When to use:** When multiple trades share the same ticker (e.g., 1000 trades -> 200 unique tickers)
**Example:**
```rust
// Fetch all trades needing price enrichment
let trades = db.get_unenriched_price_trades(batch_size)?;

// Deduplicate by ticker
let unique_tickers: HashSet<String> = trades
    .iter()
    .filter_map(|t| t.ticker.as_ref())
    .cloned()
    .collect();

// Process each ticker once, cache results
for ticker in unique_tickers {
    // YahooClient.get_price_on_date() already caches internally
    // Multiple trades with same ticker will hit cache
}
```

### Pattern 2: Graceful Ticker Validation via Price Lookup
**What:** Use YahooClient.get_price_on_date() to validate ticker; Ok(None) = invalid
**When to use:** No dedicated ticker validation endpoint exists in Yahoo Finance
**Example:**
```rust
// From Phase 2 decision: Invalid ticker handling returns Ok(None)
let price = yahoo_client.get_price_on_date(&ticker, trade_date).await?;

match price {
    Some(p) => {
        // Valid ticker, price found
        estimated_shares = Some(range_midpoint / p);
        estimated_value = Some(estimated_shares.unwrap() * p);
    }
    None => {
        // Invalid/delisted ticker OR no data for that date
        // Store NULL in price columns, skip estimation
        trade_date_price = None;
        estimated_shares = None;
        estimated_value = None;
    }
}
```

### Pattern 3: Dollar Range Parsing (Simple String Manipulation)
**What:** Parse Capitol Trades dollar range strings like "$15,001 - $50,000"
**When to use:** When size_range_low and size_range_high are available in database
**Example:**
```rust
// Source: Database columns (preferred if available)
fn parse_range_from_db(low: Option<i64>, high: Option<i64>) -> Option<(f64, f64)> {
    match (low, high) {
        (Some(l), Some(h)) => Some((l as f64, h as f64)),
        _ => None,
    }
}

// Source: String parsing (fallback if needed)
fn parse_dollar_range(input: &str) -> Result<(f64, f64), String> {
    // Strip "$", split on "-", parse each side
    let cleaned = input.replace("$", "").replace(",", "");
    let parts: Vec<&str> = cleaned.split('-').map(|s| s.trim()).collect();

    if parts.len() != 2 {
        return Err("Invalid range format".to_string());
    }

    let low: f64 = parts[0].parse().map_err(|_| "Invalid number")?;
    let high: f64 = parts[1].parse().map_err(|_| "Invalid number")?;

    Ok((low, high))
}

fn midpoint(range: (f64, f64)) -> f64 {
    (range.0 + range.1) / 2.0
}
```

### Pattern 4: Share Estimation with Validation
**What:** Calculate estimated_shares = midpoint / price, validate result falls in range
**When to use:** After successful price fetch, before database UPDATE
**Example:**
```rust
fn estimate_shares(
    range_low: f64,
    range_high: f64,
    trade_date_price: f64,
) -> Result<(f64, f64), String> {
    let midpoint = (range_low + range_high) / 2.0;
    let estimated_shares = midpoint / trade_date_price;
    let estimated_value = estimated_shares * trade_date_price;

    // Validation: estimated value should fall within original range
    if estimated_value < range_low || estimated_value > range_high {
        return Err(format!(
            "Estimated value {} outside range [{}, {}]",
            estimated_value, range_low, range_high
        ));
    }

    Ok((estimated_shares, estimated_value))
}
```

### Pattern 5: Batch Enrichment with Circuit Breaker
**What:** Semaphore + JoinSet + mpsc channel + CircuitBreaker (from sync.rs)
**When to use:** For all enrichment operations (proven pattern)
**Example:**
```rust
// Source: capitoltraders_cli/src/commands/sync.rs:272-332
let semaphore = Arc::new(Semaphore::new(concurrency));
let (tx, mut rx) = mpsc::channel::<FetchResult<PriceResult>>(concurrency * 2);
let mut join_set = JoinSet::new();

for (ticker, trade_date) in ticker_date_pairs {
    let sem = Arc::clone(&semaphore);
    let sender = tx.clone();
    let yahoo_clone = yahoo_client.clone();

    join_set.spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        sleep(Duration::from_millis(300)).await; // Rate limit
        let result = yahoo_clone.get_price_on_date(&ticker, trade_date).await;
        let _ = sender.send(FetchResult { ticker, trade_date, result }).await;
    });
}
drop(tx);

let mut breaker = CircuitBreaker::new(max_failures);
while let Some(fetch) = rx.recv().await {
    match fetch.result {
        Ok(Some(price)) => {
            db.update_trade_price_and_shares(tx_id, price, shares, value)?;
            breaker.record_success();
        }
        Ok(None) => {
            // Invalid ticker, skip estimation
            breaker.record_success(); // Not a failure
        }
        Err(e) => {
            breaker.record_failure();
            if breaker.is_tripped() {
                join_set.abort_all();
                break;
            }
        }
    }
}
```

### Anti-Patterns to Avoid
- **Fetching price for same ticker multiple times:** YahooClient already caches, but deduplication reduces API calls
- **Using regex for simple string parsing:** `replace("$", "").split('-')` is faster and clearer than regex
- **Storing invalid tickers without marking:** Need flag or NULL in price_enriched_at to skip on re-run
- **Rounding estimated_shares to integer:** REQ-E4 originally said INTEGER, but Phase 1 decision used REAL for precision
- **Failing batch on single invalid ticker:** Ok(None) from YahooClient means skip gracefully, not abort

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Concurrent task limiting | Manual Arc<Mutex<usize>> counter | tokio::sync::Semaphore | Already proven in sync.rs, handles backpressure |
| Circuit breaking | Custom failure tracking | Existing CircuitBreaker in sync.rs | 20 lines, proven, tested, no external dep |
| Ticker deduplication | Manual loop with Vec contains | HashSet or itertools dedup_by_key | O(1) vs O(n) lookup, standard library |
| Dollar amount parsing | Currency parsing library | String replace + split + parse | Capitol Trades uses trivial "$N - $M" format |
| Date conversion | Manual timestamp math | Existing date_to_offset_datetime in yahoo.rs | Already handles chrono to time conversion |

**Key insight:** The existing codebase (sync.rs, yahoo.rs, db.rs) already has all necessary primitives. This phase is composition, not invention. The only new code is ticker validation logic (use price lookup), dollar range parsing (trivial string manipulation), and share estimation math (one division, one multiplication, one range check).

## Common Pitfalls

### Pitfall 1: Treating Ok(None) from YahooClient as Failure
**What goes wrong:** Circuit breaker trips prematurely, enrichment stops
**Why it happens:** Ok(None) looks like failure but is graceful handling of invalid/delisted tickers
**How to avoid:** Only call breaker.record_failure() on Err(_), not on Ok(None)
**Warning signs:** Circuit breaker trips with "N consecutive failures" but error logs show no actual errors

### Pitfall 2: Not Caching Ticker Validation Results
**What goes wrong:** Same invalid ticker validated multiple times across re-runs
**Why it happens:** No persistent marker that ticker was checked and found invalid
**How to avoid:** Store NULL in price_enriched_at when ticker is invalid (not just when price is unavailable)
**Warning signs:** Log shows same invalid ticker being retried on every enrich-prices run

### Pitfall 3: Assuming size_range_low/size_range_high Are Always Available
**What goes wrong:** Estimation fails for trades where range fields are NULL
**Why it happens:** Capitol Trades API sometimes omits these fields
**How to avoid:** Check both DB columns and fallback to parsing Trade.value if needed (may require storing original response)
**Warning signs:** Many trades skipped with "missing range data" even though Trade.value exists

### Pitfall 4: Validating Estimated Value Against Wrong Range
**What goes wrong:** Validation fails because comparing estimated_value to Trade.value instead of original range
**Why it happens:** Trade.value is midpoint, not the range bounds
**How to avoid:** Always validate against (size_range_low, size_range_high), not value
**Warning signs:** All estimations fail validation even for valid prices

### Pitfall 5: Forgetting to Handle Ticker Case Sensitivity
**What goes wrong:** "AAPL" and "aapl" treated as different tickers in deduplication
**Why it happens:** HashSet distinguishes by exact string match
**How to avoid:** Normalize tickers to uppercase before deduplication and before Yahoo lookup
**Warning signs:** Duplicate API calls for same ticker with different casing

### Pitfall 6: Blocking SQLite Updates in Async Loop
**What goes wrong:** SQLite connection blocks Tokio runtime, enrichment slows to serial
**Why it happens:** Calling db.update_trade_price_and_shares() directly in match arm
**How to avoid:** Use mpsc channel pattern from sync.rs - async tasks send results, single-threaded receiver writes to DB
**Warning signs:** Concurrency=5 performs same as concurrency=1

## Code Examples

Verified patterns from existing codebase:

### CircuitBreaker Pattern (Existing)
```rust
// Source: capitoltraders_cli/src/commands/sync.rs:196-217
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

### YahooClient Invalid Ticker Handling (Existing)
```rust
// Source: capitoltraders_lib/src/yahoo.rs:118-126
match self.connector.get_quote_history(ticker, start, end).await {
    Err(yahoo_finance_api::YahooError::NoQuotes)
    | Err(yahoo_finance_api::YahooError::NoResult)
    | Err(yahoo_finance_api::YahooError::ApiError(_)) => {
        // Invalid ticker or no data - cache None and return gracefully
        self.cache.insert(key, None);
        Ok(None)
    }
    Err(e) => Err(YahooError::Upstream(e)),
}
```

### Database Query Pattern for Unenriched Trades
```rust
// NEW: Add to capitoltraders_lib/src/db.rs
pub fn count_unenriched_prices(&self) -> Result<i64, DbError> {
    let count: i64 = self.conn.query_row(
        "SELECT COUNT(*) FROM trades
         WHERE issuer_ticker IS NOT NULL
         AND tx_date IS NOT NULL
         AND price_enriched_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

pub fn get_unenriched_price_trades(&self, limit: Option<i64>) -> Result<Vec<PriceEnrichmentRow>, DbError> {
    let sql = format!(
        "SELECT tx_id, issuer_ticker, tx_date, size_range_low, size_range_high
         FROM trades
         WHERE issuer_ticker IS NOT NULL
         AND tx_date IS NOT NULL
         AND price_enriched_at IS NULL
         ORDER BY tx_id {}",
        limit.map_or(String::new(), |l| format!("LIMIT {}", l))
    );
    // Map to struct with (tx_id, ticker, date, low, high)
}
```

### Database Update Pattern for Price and Shares
```rust
// NEW: Add to capitoltraders_lib/src/db.rs
pub fn update_trade_price_and_shares(
    &self,
    tx_id: i64,
    trade_date_price: Option<f64>,
    estimated_shares: Option<f64>,
    estimated_value: Option<f64>,
) -> Result<(), DbError> {
    self.conn.execute(
        "UPDATE trades
         SET trade_date_price = ?1,
             estimated_shares = ?2,
             estimated_value = ?3,
             price_enriched_at = datetime('now')
         WHERE tx_id = ?4",
        params![trade_date_price, estimated_shares, estimated_value, tx_id],
    )?;
    Ok(())
}
```

### Ticker Validation Function
```rust
// NEW: Add to capitoltraders_lib/src/validation.rs
pub fn validate_ticker(ticker: &str) -> Result<String, CapitolTradesError> {
    // Normalize to uppercase
    let normalized = ticker.trim().to_uppercase();

    // Basic format validation: 1-5 characters, uppercase letters and digits
    if normalized.is_empty() || normalized.len() > 5 {
        return Err(CapitolTradesError::InvalidInput(
            format!("Invalid ticker length: {}", ticker)
        ));
    }

    if !normalized.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err(CapitolTradesError::InvalidInput(
            format!("Invalid ticker format: {}", ticker)
        ));
    }

    Ok(normalized)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Block on HTTP calls | Semaphore + JoinSet + mpsc | Phase 2 (2026-02) | Concurrent enrichment without SQLite contention |
| Retry all failures | CircuitBreaker kill switch | Phase 2 (2026-02) | Stop early on API outages |
| Manual date conversion | date_to_offset_datetime helper | Phase 2 (2026-02) | Handles chrono/time conversion correctly |
| Store prices separately | Single UPDATE for price+shares | Phase 3 (TBD) | Atomic operation, fewer DB round-trips |

**Deprecated/outdated:**
- Manual retry loops: Use CircuitBreaker pattern instead
- regex for simple parsing: String methods are clearer and faster
- INTEGER for estimated_shares: Phase 1 decision changed to REAL for precision

## Open Questions

1. **Where does size_range_low/size_range_high come from?**
   - What we know: Schema has these columns, Trade API type has them as private fields
   - What's unclear: Are they always populated in the database after sync?
   - Recommendation: Check db.rs insert_trade() to see if these fields are persisted. If not, may need to store original Trade response or reconstruct from Trade.value assumptions.

2. **Should we validate tickers before or during enrichment?**
   - What we know: YahooClient.get_price_on_date() already handles invalid tickers gracefully
   - What's unclear: Is there value in pre-validating ticker format (regex) vs letting Yahoo Finance be the source of truth?
   - Recommendation: Skip pre-validation. Let YahooClient handle it. Format validation (validate_ticker) is useful for user input, not for DB-sourced tickers.

3. **How to handle edge case where estimated_value is outside range?**
   - What we know: REQ-E4 says "validate: estimated_shares * trade_date_price should fall within original range"
   - What's unclear: What action to take on validation failure? Skip the trade? Log warning? Store anyway?
   - Recommendation: Log warning, store price but NULL estimated_shares/estimated_value. This preserves price data while flagging estimation issue.

## Sources

### Primary (HIGH confidence)
- Existing codebase: capitoltraders_cli/src/commands/sync.rs (enrich_trades pattern, CircuitBreaker implementation)
- Existing codebase: capitoltraders_lib/src/yahoo.rs (YahooClient invalid ticker handling, caching)
- Existing codebase: capitoltraders_lib/src/db.rs (DbTradeRow with estimated_shares/estimated_value fields)
- Existing codebase: schema/sqlite.sql (trades table with price columns)
- Project documentation: .planning/REQUIREMENTS.md (REQ-E3, REQ-E4 specifications)
- Project documentation: .planning/STATE.md (REAL vs INTEGER decision for estimated_shares)

### Secondary (MEDIUM confidence)
- [Ticker symbol - Wikipedia](https://en.wikipedia.org/wiki/Ticker_symbol) - Historical context on 1-4 character symbols
- [NASDAQ Stock Symbol System Changes](https://www.nasdaqtrader.com/trader.aspx?id=StockSymChanges) - Symbol format specifications
- [NYSE Symbology Spec](https://www.nyse.com/publicdocs/nyse/data/NYSE_Symbology_Spec.pdf) - Official NYSE ticker format rules
- Rust documentation: std::collections::HashSet - Standard deduplication approach

### Tertiary (LOW confidence - marked for validation)
- [Top 5 Ways to Remove Duplicate Data in Rust](https://medium.com/@robssthe/top-5-ways-to-remove-duplicate-data-in-rust-strings-and-objects-6489e1929bb6) - General deduplication patterns
- [failsafe-rs](https://github.com/dmexe/failsafe-rs) - Circuit breaker library (not used, reference only)
- [rusty_money](https://docs.rs/rusty-money) - Currency parsing library (not needed for this use case)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in Cargo.toml, patterns proven in sync.rs
- Architecture: HIGH - Direct reuse of enrich_trades pattern, only additions are ticker dedup and share estimation
- Pitfalls: HIGH - Based on actual YahooClient behavior, CircuitBreaker edge cases from sync.rs, SQLite contention patterns

**Research date:** 2026-02-10
**Valid until:** 2026-03-10 (30 days - stable domain, Rust stdlib patterns don't change rapidly)
