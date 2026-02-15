# Phase 14: Benchmark Price Enrichment - Research

**Researched:** 2026-02-15
**Domain:** Extending existing price enrichment pipeline to include benchmark prices
**Confidence:** HIGH

## Summary

Phase 14 extends the existing v1.1 price enrichment pipeline to fetch benchmark prices for the 12 ETFs stored in the sector_benchmarks table (SPY + 11 sector ETFs). This is a conservative, incremental enhancement rather than a greenfield implementation. The existing infrastructure provides nearly everything needed: YahooClient with weekend fallback, two-phase enrichment pattern, circuit breaker logic, and DashMap caching.

The core technical challenge is determining WHERE to store benchmark prices in the database schema. Three options exist: (1) add benchmark_price column to trades table, (2) create separate benchmark_prices table with (ticker, date, price) tuples, or (3) reuse existing issuer_eod_prices pattern. The recommendation is option 1 (trades.benchmark_price column) for Phase 14 simplicity, deferring normalized time-series storage to future performance scoring phases.

**Primary recommendation:** Add benchmark_price REAL column to trades table via schema v7 migration, extend enrich-prices Phase 1 to fetch benchmark prices after trade prices, use sector mapping to determine relevant benchmark (sector ETF or SPY fallback), apply existing circuit breaker threshold (10 consecutive failures), and leverage YahooClient cache for 12-ticker deduplication.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| yahoo_finance_api | 4.1.0 | Fetch ETF prices via YahooClient | Already used for trade price enrichment (v1.1) |
| rusqlite | 0.32.x | Schema v7 migration, add benchmark_price column | Project standard for all DB operations |
| tokio | 1.x | Async runtime for concurrent price fetching | Used throughout enrichment pipeline |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chrono | 0.4.x | Date handling for benchmark price lookups | Already used in yahoo.rs date conversions |
| rand | 0.8.5 | Jittered delay for rate limiting | Already used in enrich_prices.rs |
| indicatif | 0.17.x | Progress bar for benchmark enrichment | Already used in enrich_prices.rs |

### No New Dependencies Required
Phase 14 requires no new dependencies. All necessary infrastructure exists from v1.1 Phase 4 (enrich-prices command).

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── yahoo.rs                      # Existing: YahooClient, get_price_on_date_with_fallback
├── db.rs                         # Extend: migrate_v7(), update_benchmark_price()
├── sector_mapping.rs             # Existing: load_sector_mappings() for ticker-to-sector lookup
capitoltraders_cli/src/commands/
└── enrich_prices.rs              # Extend: Add Phase 3 after Phase 2 (current prices)
schema/
└── sqlite.sql                    # Extend: Add benchmark_price column to trades
```

### Pattern 1: Schema v7 Migration (Benchmark Price Column)
**What:** Add benchmark_price REAL column to trades table, indexed for analytics queries
**When to use:** During Db::init() when user_version < 7
**Example:**
```rust
fn migrate_v7(&self) -> Result<(), DbError> {
    // Add benchmark_price column to trades table
    match self.conn.execute(
        "ALTER TABLE trades ADD COLUMN benchmark_price REAL",
        []
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("duplicate column name") => {}
        Err(e) => return Err(e.into()),
    }

    // Create index for analytics queries filtering by benchmark_price IS NOT NULL
    self.conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_trades_benchmark_price ON trades(benchmark_price)",
        [],
    )?;

    Ok(())
}
```

### Pattern 2: Determine Benchmark Ticker for Trade
**What:** Given a trade's issuer_ticker and GICS sector, return the appropriate benchmark ETF ticker
**When to use:** During Phase 3 enrichment loop to decide which ETF to fetch
**Example:**
```rust
/// Determine benchmark ticker for a trade based on issuer sector.
///
/// Returns sector-specific ETF if issuer has GICS sector mapping, otherwise SPY.
fn get_benchmark_ticker_for_trade(issuer_ticker: &str, gics_sector: Option<&str>) -> &'static str {
    // Map GICS sector to ETF ticker
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
        _ => "SPY",  // Fallback to S&P 500 for unmapped or NULL sectors
    }
}
```

### Pattern 3: Extend enrich_prices.rs with Phase 3
**What:** Add benchmark price enrichment as third phase after current price enrichment
**When to use:** After Phase 2 completes in enrich_prices.rs
**Example:**
```rust
// Step 6: Benchmark price enrichment (Phase 3)
// Deduplicate by (benchmark_ticker, date) since multiple trades may share same benchmark
let mut benchmark_date_map: HashMap<(String, NaiveDate), Vec<usize>> = HashMap::new();

for (idx, trade) in trades.iter().enumerate() {
    let date = match NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => continue,  // Already logged in Phase 1
    };

    // Determine benchmark ticker based on sector
    let benchmark_ticker = get_benchmark_ticker_for_trade(
        &trade.issuer_ticker,
        trade.gics_sector.as_deref(),
    );

    benchmark_date_map
        .entry((benchmark_ticker.to_string(), date))
        .or_default()
        .push(idx);
}

let unique_benchmark_pairs = benchmark_date_map.len();
eprintln!(
    "Phase 3: Fetching benchmark prices for {} unique (ETF, date) pairs",
    unique_benchmark_pairs
);

// Fetch benchmark prices using existing Semaphore + JoinSet + mpsc pattern
// ... (same concurrent fetch structure as Phase 1) ...

while let Some(fetch) = rx3.recv().await {
    match fetch.result {
        Ok(Some(price)) => {
            for idx in &fetch.trade_indices {
                let trade = &trades[*idx];
                db.update_benchmark_price(trade.tx_id, Some(price))?;
                benchmark_enriched += 1;
            }
        }
        Ok(None) | Err(_) => {
            // Benchmark price fetch failed
            benchmark_skipped += fetch.trade_indices.len();
        }
    }
}
```

### Pattern 4: DB Method for Benchmark Price Update
**What:** Add update_benchmark_price() method to Db for storing benchmark prices
**When to use:** In Phase 3 enrichment loop when benchmark price is fetched
**Example:**
```rust
/// Update the benchmark_price for a trade.
///
/// Always sets the price (even if None) to avoid re-processing.
pub fn update_benchmark_price(&self, tx_id: i64, benchmark_price: Option<f64>) -> Result<(), DbError> {
    self.conn.execute(
        "UPDATE trades SET benchmark_price = ?1 WHERE tx_id = ?2",
        params![benchmark_price, tx_id],
    )?;
    Ok(())
}
```

### Pattern 5: Weekend Fallback for Benchmarks
**What:** Reuse existing YahooClient::get_price_on_date_with_fallback for benchmark ETFs
**When to use:** Phase 3 always uses fallback (same as Phase 1 trade prices)
**Example:**
```rust
// In spawned task for benchmark price fetch
let result = yahoo_clone
    .get_price_on_date_with_fallback(&benchmark_ticker, date)
    .await;
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Benchmark ticker lookup | Runtime API calls to get sector ETF | Static sector-to-ETF mapping (const match) | Sectors mapped in Phase 13, 11 GICS sectors are stable |
| Benchmark price caching | Separate cache for ETFs | YahooClient DashMap cache | Already deduplicates by (ticker, date), works for ETFs same as stocks |
| Weekend/holiday handling | Custom trading day calendar | get_price_on_date_with_fallback | 7-day lookback already handles market closures |
| Concurrent fetching | New async pattern | Existing Semaphore + JoinSet + mpsc | Proven pattern from Phase 1, supports concurrency control |
| Circuit breaker | New failure detection | Existing CircuitBreaker struct | Already tested threshold logic, reuse for Phase 3 |

**Key insight:** Phase 14 is a constrained extension, not a new system. The existing enrich_prices.rs pipeline already has all necessary patterns. The ONLY new code is: (1) sector-to-ETF mapping function, (2) Phase 3 loop structure, (3) update_benchmark_price() DB method, (4) schema v7 migration. Everything else is reuse.

## Common Pitfalls

### Pitfall 1: Fetching Same Benchmark Price Multiple Times
**What goes wrong:** 100 trades on same date all trigger separate Yahoo API calls for SPY on that date
**Why it happens:** Forgetting to deduplicate by (benchmark_ticker, date) before spawning tasks
**How to avoid:** Use HashMap deduplication pattern (same as Phase 1 ticker_date_map)
**Warning signs:** Phase 3 shows "fetching 5000 unique pairs" when only 12 tickers exist in benchmark table

### Pitfall 2: Overwriting Trade Prices with Benchmark Prices
**What goes wrong:** update_trade_prices() accidentally called instead of update_benchmark_price(), replacing trade_date_price
**Why it happens:** Copy-paste from Phase 1 code without changing method name
**How to avoid:** Create separate update_benchmark_price() method that only touches benchmark_price column
**Warning signs:** Tests show trade_date_price changing after Phase 3 runs

### Pitfall 3: Circuit Breaker Threshold Too Low for 12 Tickers
**What goes wrong:** Circuit breaker trips after 3 failures, but benchmark enrichment needs all 12 tickers
**Why it happens:** Using sync-donations threshold (5) instead of enrich-prices threshold (10)
**How to avoid:** Use CIRCUIT_BREAKER_THRESHOLD = 10 (same as Phase 1 trade price enrichment)
**Warning signs:** Phase 3 aborts early, benchmark_price remains NULL for 90%+ of trades

### Pitfall 4: Benchmark Ticker Mapping Missing GICS Sector
**What goes wrong:** get_benchmark_ticker_for_trade panics on Some("Technology") instead of Some("Information Technology")
**Why it happens:** Using abbreviated sector names instead of official GICS names from sector_benchmarks table
**How to avoid:** Match against GICS_SECTORS const array from sector_mapping.rs (exact capitalization)
**Warning signs:** Compiler warning "pattern unreachable" or runtime panic in Phase 3 loop

### Pitfall 5: Running Phase 3 Before Phase 13 Migration
**What goes wrong:** SQL error "no such column: gics_sector" when querying issuers table for sector
**Why it happens:** enrich-prices runs on database that hasn't executed schema v6 migration yet
**How to avoid:** Verify Phase 13 completion in acceptance test (get_sector_benchmarks returns 12 rows)
**Warning signs:** DB query error in get_benchmark_ticker_for_trade, Phase 3 crashes immediately

## Code Examples

Verified patterns based on existing project structure:

### Schema v7 Base Definition (schema/sqlite.sql)
```sql
-- Add to existing trades table in fresh DB schema:
CREATE TABLE IF NOT EXISTS trades (
    tx_id INTEGER PRIMARY KEY,
    politician_id TEXT NOT NULL,
    asset_id INTEGER NOT NULL,
    issuer_id INTEGER NOT NULL,
    pub_date TEXT NOT NULL,
    filing_date TEXT NOT NULL,
    tx_date TEXT NOT NULL,
    tx_type TEXT NOT NULL,
    tx_type_extended TEXT,
    has_capital_gains INTEGER NOT NULL,
    owner TEXT NOT NULL,
    chamber TEXT NOT NULL,
    price REAL,
    size INTEGER,
    size_range_high INTEGER,
    size_range_low INTEGER,
    value INTEGER NOT NULL,
    filing_id INTEGER NOT NULL,
    filing_url TEXT NOT NULL,
    reporting_gap INTEGER NOT NULL,
    comment TEXT,
    enriched_at TEXT,
    trade_date_price REAL,
    current_price REAL,
    price_enriched_at TEXT,
    estimated_shares REAL,
    estimated_value REAL,
    benchmark_price REAL  -- Phase 14 addition
);

-- Add to index section:
CREATE INDEX IF NOT EXISTS idx_trades_benchmark_price ON trades(benchmark_price);
```

### Migration Integration (db.rs::init)
```rust
// Add to Db::init() after migrate_v6:
if version < 7 {
    self.migrate_v7()?;
    self.conn.pragma_update(None, "user_version", 7)?;
}
```

### Benchmark Ticker Mapping Helper
```rust
// In capitoltraders_cli/src/commands/enrich_prices.rs
use capitoltraders_lib::sector_mapping::GICS_SECTORS;

/// Map GICS sector name to benchmark ETF ticker.
///
/// Returns SPY for NULL/unknown sectors (market-wide benchmark).
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
        _ => "SPY",  // Fallback for unmapped sectors
    }
}
```

### DB Method: update_benchmark_price
```rust
// In capitoltraders_lib/src/db.rs

/// Update the benchmark_price column for a single trade.
///
/// Always writes the value (even None) to avoid re-processing.
pub fn update_benchmark_price(
    &self,
    tx_id: i64,
    benchmark_price: Option<f64>,
) -> Result<(), DbError> {
    self.conn.execute(
        "UPDATE trades SET benchmark_price = ?1 WHERE tx_id = ?2",
        params![benchmark_price, tx_id],
    )?;
    Ok(())
}
```

### DB Method: get_trades_with_sectors (Extended Query)
```rust
// Extend existing get_unenriched_price_trades to include gics_sector from issuers JOIN

pub struct PriceEnrichmentRow {
    pub tx_id: i64,
    pub issuer_ticker: String,
    pub tx_date: String,
    pub size_range_low: Option<i64>,
    pub size_range_high: Option<i64>,
    pub gics_sector: Option<String>,  // Phase 14 addition
}

pub fn get_unenriched_price_trades(
    &self,
    limit: Option<i64>,
) -> Result<Vec<PriceEnrichmentRow>, DbError> {
    let query = if let Some(limit_val) = limit {
        format!(
            "SELECT t.tx_id, i.issuer_ticker, t.tx_date, t.size_range_low, t.size_range_high, i.gics_sector
             FROM trades t
             JOIN issuers i ON t.issuer_id = i.issuer_id
             WHERE t.price_enriched_at IS NULL
             LIMIT {}",
            limit_val
        )
    } else {
        "SELECT t.tx_id, i.issuer_ticker, t.tx_date, t.size_range_low, t.size_range_high, i.gics_sector
         FROM trades t
         JOIN issuers i ON t.issuer_id = i.issuer_id
         WHERE t.price_enriched_at IS NULL"
            .to_string()
    };

    let mut stmt = self.conn.prepare(&query)?;
    let rows = stmt.query_map([], |row| {
        Ok(PriceEnrichmentRow {
            tx_id: row.get(0)?,
            issuer_ticker: row.get(1)?,
            tx_date: row.get(2)?,
            size_range_low: row.get(3)?,
            size_range_high: row.get(4)?,
            gics_sector: row.get(5)?,  // Phase 14 addition
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
}
```

### Phase 3 Enrichment Loop (Minimal)
```rust
// After Phase 2 (current price enrichment) in enrich_prices.rs

// Step 6: Benchmark price enrichment (Phase 3)
let mut benchmark_date_map: HashMap<(String, NaiveDate), Vec<usize>> = HashMap::new();

for (idx, trade) in trades.iter().enumerate() {
    let date = match NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => continue,
    };

    let benchmark_ticker = get_benchmark_ticker(trade.gics_sector.as_deref());
    benchmark_date_map
        .entry((benchmark_ticker.to_string(), date))
        .or_default()
        .push(idx);
}

let unique_benchmark_pairs = benchmark_date_map.len();
eprintln!(
    "Phase 3: Fetching benchmark prices for {} unique (ETF, date) pairs",
    unique_benchmark_pairs
);

let pb3 = ProgressBar::new(unique_benchmark_pairs as u64);
pb3.set_style(/* same style as Phase 1 */);
pb3.set_message("fetching benchmark prices...");

let semaphore3 = Arc::new(Semaphore::new(CONCURRENCY));
let (tx3, mut rx3) = mpsc::channel::<BenchmarkPriceResult>(CONCURRENCY * 2);
let mut join_set3 = JoinSet::new();

for ((ticker, date), indices) in benchmark_date_map {
    let sem = Arc::clone(&semaphore3);
    let sender = tx3.clone();
    let yahoo_clone = Arc::clone(&yahoo);

    join_set3.spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        let delay_ms = rand::thread_rng().gen_range(200..500);
        sleep(Duration::from_millis(delay_ms)).await;

        let result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;
        let _ = sender.send(BenchmarkPriceResult {
            ticker,
            date,
            trade_indices: indices,
            result,
        }).await;
    });
}
drop(tx3);

let mut benchmark_enriched = 0usize;
let mut benchmark_skipped = 0usize;
let mut breaker3 = CircuitBreaker::new(CIRCUIT_BREAKER_THRESHOLD);

while let Some(fetch) = rx3.recv().await {
    match fetch.result {
        Ok(Some(price)) => {
            for idx in &fetch.trade_indices {
                let trade = &trades[*idx];
                db.update_benchmark_price(trade.tx_id, Some(price))?;
                benchmark_enriched += 1;
            }
            breaker3.record_success();
        }
        Ok(None) | Err(_) => {
            for idx in &fetch.trade_indices {
                let trade = &trades[*idx];
                db.update_benchmark_price(trade.tx_id, None)?;
                benchmark_skipped += 1;
            }
            breaker3.record_failure();
        }
    }
    pb3.set_message(format!("{} ok, {} skip", benchmark_enriched, benchmark_skipped));
    pb3.inc(1);

    if breaker3.is_tripped() {
        pb3.println("Circuit breaker tripped for benchmark prices, stopping Phase 3");
        join_set3.abort_all();
        break;
    }
}

pb3.finish_with_message(format!(
    "Phase 3 done: {} enriched, {} skipped",
    benchmark_enriched, benchmark_skipped
));
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No benchmark data | S&P 500 + sector ETF benchmarks | Phase 14 (2026-02) | Enables alpha calculation in Phase 15 |
| Single-pass enrichment | Three-phase (historical, current, benchmark) | Phase 14 (2026-02) | Deduplicates 12 ETFs efficiently |
| Trades table without benchmark_price | trades.benchmark_price REAL column | Schema v7 (2026-02) | Supports per-trade alpha queries |

**Current best practice (2026):**
- Store benchmark prices at trade granularity (not separate table) for Phase 14 simplicity
- Use sector mapping to determine relevant benchmark (sector ETF or SPY fallback)
- Reuse existing YahooClient cache (no separate ETF cache needed)
- Apply same circuit breaker threshold as trade price enrichment (10 consecutive failures)
- Three-phase enrichment: historical trade prices -> current trade prices -> benchmark prices

## Open Questions

1. **Should benchmark_price be nullable or have default value?**
   - What we know: Not all trades have valid dates, some ETF price lookups may fail
   - What's unclear: Whether NULL is acceptable or should use sentinel value (0.0)
   - Recommendation: Use NULL (same pattern as trade_date_price, current_price)

2. **Should Phase 3 run only for enriched trades or all trades?**
   - What we know: Phase 1/2 may skip trades with invalid tickers or date parse errors
   - What's unclear: Whether benchmark enrichment should be independent or dependent on Phase 1 success
   - Recommendation: Run Phase 3 for all trades (benchmark price independent of trade price availability)

3. **Should circuit breaker threshold differ for benchmark prices?**
   - What we know: Only 12 unique tickers (vs thousands for trade prices), higher success rate expected
   - What's unclear: Whether 10 consecutive failures is too high for 12-ticker dataset
   - Recommendation: Keep threshold at 10 (same as Phase 1) for consistency, can tune later if needed

## Sources

### Primary (HIGH confidence)
- [capitoltraders_cli/src/commands/enrich_prices.rs](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/commands/enrich_prices.rs) - Existing two-phase enrichment pipeline
- [capitoltraders_lib/src/yahoo.rs](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/yahoo.rs) - YahooClient with weekend fallback
- [capitoltraders_lib/src/db.rs](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/db.rs) - Schema migrations v1-v6, update_trade_prices pattern
- [Phase 13 Research](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/.planning/phases/13-data-foundation-sector-classification/13-RESEARCH.md) - GICS sectors, sector_benchmarks table, ETF mapping

### Secondary (MEDIUM confidence)
- [v1.3 Requirements](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/.planning/REQUIREMENTS.md) - FOUND-03 requirement definition
- [v1.3 Roadmap](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/.planning/ROADMAP.md) - Phase 14 success criteria

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in project from v1.1 Phase 4
- Architecture: HIGH - Follows proven v1.1 enrichment pattern, minimal new code required
- Schema design: MEDIUM - trades.benchmark_price is simplest solution, may refactor to time-series table in Phase 15+
- Pitfalls: HIGH - Derived from existing enrich_prices.rs patterns and common DB migration errors

**Research date:** 2026-02-15
**Valid until:** 2026-05-15 (3 months - v1.1 patterns stable, Phase 14 is incremental extension)
