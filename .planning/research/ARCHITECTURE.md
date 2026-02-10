# Architecture Research: Yahoo Finance Integration

**Domain:** Stock price enrichment and portfolio tracking for congressional trades
**Researched:** 2026-02-09
**Confidence:** HIGH

## Recommended Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            CLI Layer                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   trades    │  │ politicians │  │   issuers   │  │    sync     │    │
│  │   command   │  │   command   │  │   command   │  │   command   │    │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘    │
│         │                │                │                │            │
│         └────────────────┴────────────────┴────────────────┘            │
├─────────────────────────────────────────────────────────────────────────┤
│                         Library Layer                                    │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    capitoltraders_lib                            │    │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌──────────┐  │    │
│  │  │  Db        │  │ ScrapeClient│ │YahooClient │  │ Portfolio│  │    │
│  │  │ (SQLite)   │  │ (CT scrape) │ │ (new)      │  │ (new)    │  │    │
│  │  └────────────┘  └────────────┘  └────────────┘  └──────────┘  │    │
│  └─────────────────────────────────────────────────────────────────┘    │
├─────────────────────────────────────────────────────────────────────────┤
│                        External APIs                                     │
│  ┌─────────────────┐  ┌────────────────┐  ┌──────────────────┐         │
│  │ CapitolTrades   │  │ Yahoo Finance  │  │ SQLite Storage   │         │
│  │ (vendored API   │  │ (yfinance-rs)  │  │                  │         │
│  │  + scraper)     │  │                │  │                  │         │
│  └─────────────────┘  └────────────────┘  └──────────────────┘         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Component Boundaries

| Component | Responsibility | Lives In | Communicates With |
|-----------|---------------|----------|-------------------|
| YahooClient | Fetch historical prices and current quotes from Yahoo Finance | capitoltraders_lib/src/yahoo.rs (NEW) | Yahoo Finance API, Db |
| Portfolio Calculator | Compute holdings, unrealized PnL, cost basis (FIFO) | capitoltraders_lib/src/portfolio.rs (NEW) | Db, YahooClient |
| Db (price methods) | Store/query EOD prices and portfolio positions | capitoltraders_lib/src/db.rs (EXTEND) | YahooClient, Portfolio |
| enrich-prices command | CLI orchestration for price fetching | capitoltraders_cli/src/commands/enrich.rs (NEW) | YahooClient, Db |
| portfolio command | CLI display of positions and PnL | capitoltraders_cli/src/commands/portfolio.rs (NEW) | Portfolio, Db |

## Data Flow

### Price Enrichment Flow

```
User runs: `capitoltraders enrich-prices --db capitoltraders.db`
    ↓
CLI: Load Db, identify issuers needing price data
    ↓
CLI: Spawn concurrent YahooClient tasks (Semaphore + JoinSet + mpsc pattern)
    ↓
YahooClient: GET https://query2.finance.yahoo.com/v8/finance/chart/{ticker}
    ↓
YahooClient: Parse OHLCV candles into PriceRecord { date, ticker, close }
    ↓
mpsc channel: Send PriceRecord to single-threaded DB writer
    ↓
Db: INSERT INTO issuer_eod_prices (issuer_id, price_date, price) VALUES (...)
    ↓
CLI: Report success/failure, record last_enriched_date in ingest_meta
```

### Portfolio Calculation Flow

```
User runs: `capitoltraders portfolio --db capitoltraders.db --politician P000610`
    ↓
CLI: Load Db, query trades for politician
    ↓
Portfolio: Aggregate trades into lots with FIFO queue
    ↓
Portfolio: For each active holding, query latest price from issuer_eod_prices
    ↓
Portfolio: Calculate unrealized_pnl = (current_price - cost_basis) * shares
    ↓
Output: Display table with columns: ticker, shares, cost_basis, current_value, unrealized_pnl, pct_change
```

### Schema Evolution

**Existing tables (NO CHANGES):**
- `trades`: Already has tx_type (buy/sell), size, value, issuer_id
- `issuers`: Already has issuer_ticker (maps to Yahoo symbols)
- `issuer_eod_prices`: Already exists (issuer_id, price_date, price)

**New tables:**
```sql
-- Portfolio positions (materialized view, computed on-demand)
-- NOT stored in DB - calculated in-memory by Portfolio module
```

**New columns:**
```sql
-- ingest_meta entries (key-value pairs):
-- 'last_price_enrich_date': '2026-02-09' (tracks last EOD price fetch)
```

**Indexes (already exist):**
- `idx_eod_prices_date` on issuer_eod_prices(price_date)
- `idx_trades_politician` on trades(politician_id)
- `idx_trades_issuer` on trades(issuer_id)

## Architectural Patterns

### Pattern 1: Yahoo Finance Client as Lib Module

**What:** YahooClient wraps yfinance-rs crate, provides Capitol Traders-specific interface.

**When to use:** Yahoo Finance integration is tightly coupled to this project's needs (ticker mapping, error handling, rate limiting).

**Trade-offs:**
- **Pro:** Fast iteration, no separate crate to maintain, shares error types
- **Pro:** Access to private Db types for direct persistence
- **Con:** Increases capitoltraders_lib compilation time (but yfinance-rs is small)

**Decision:** Use module in lib crate, not separate crate. Yahoo integration is 200-300 LOC max, not worth split.

**Example:**
```rust
// capitoltraders_lib/src/yahoo.rs
pub struct YahooClient {
    inner: yfinance_rs::YahooConnector,
    cache: Arc<DashMap<String, CachedQuote>>,
}

impl YahooClient {
    pub async fn get_historical_prices(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<PriceRecord>, YahooError> {
        // Fetch from yfinance-rs, map to PriceRecord
    }

    pub async fn get_latest_quote(&self, ticker: &str) -> Result<Quote, YahooError> {
        // Check cache, fetch if stale
    }
}
```

### Pattern 2: Portfolio as Pure Calculation Module

**What:** Portfolio module computes positions from trades without storing state in DB.

**When to use:** Portfolio positions are derived data that can be recomputed on-demand.

**Trade-offs:**
- **Pro:** No schema complexity (positions, lots tables), no stale data
- **Pro:** Query-time flexibility (filter by date range, politician, etc.)
- **Con:** Slower for large datasets (but trades table is indexed)
- **Con:** FIFO logic must be correct (but well-tested pattern)

**Decision:** Use calculation-based approach. Project has ~100K trades max, FIFO computation is O(n log n) per politician.

**Example:**
```rust
// capitoltraders_lib/src/portfolio.rs
pub struct Position {
    pub ticker: String,
    pub shares: f64,
    pub cost_basis: f64,  // Total cost paid
    pub current_price: Option<f64>,
    pub unrealized_pnl: Option<f64>,
}

pub struct PortfolioCalculator<'a> {
    db: &'a Db,
}

impl<'a> PortfolioCalculator<'a> {
    pub fn compute_positions(
        &self,
        politician_id: &str,
        as_of_date: NaiveDate,
    ) -> Result<Vec<Position>, PortfolioError> {
        // 1. Query trades WHERE politician_id = ? AND tx_date <= as_of_date
        // 2. Group by issuer_id, sort by tx_date ASC
        // 3. For each issuer, apply FIFO: buy adds lot, sell consumes oldest lot
        // 4. Query latest price from issuer_eod_prices WHERE price_date <= as_of_date
        // 5. Calculate unrealized_pnl = (price - avg_cost) * shares
    }
}
```

### Pattern 3: Enrichment Pipeline Reuse

**What:** YahooClient price fetching uses same Semaphore + JoinSet + mpsc pattern as existing trade detail enrichment.

**When to use:** Concurrent HTTP requests with single-threaded DB writes.

**Trade-offs:**
- **Pro:** Proven pattern (already used in sync.rs for trade/issuer enrichment)
- **Pro:** Rate limiting via Semaphore, circuit breaker for HTTP failures
- **Con:** More complex than sequential fetch (but necessary for performance)

**Example:**
```rust
// capitoltraders_cli/src/commands/enrich.rs
async fn enrich_prices(
    yahoo: &YahooClient,
    db: &Db,
    batch_size: Option<i64>,
    concurrency: usize,
) -> Result<EnrichmentResult> {
    let tickers = db.get_issuers_needing_prices(batch_size)?;
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let (tx, mut rx) = mpsc::channel(concurrency * 2);
    let mut join_set = JoinSet::new();

    for ticker in tickers {
        let permit = semaphore.clone().acquire_owned().await?;
        let yahoo = yahoo.clone();
        let tx = tx.clone();
        join_set.spawn(async move {
            let result = yahoo.get_historical_prices(&ticker, start, end).await;
            let _ = tx.send((ticker, result)).await;
            drop(permit);
        });
    }
    drop(tx);

    // Single-threaded DB writes from mpsc receiver
    while let Some((ticker, result)) = rx.recv().await {
        match result {
            Ok(prices) => db.upsert_prices(&ticker, &prices)?,
            Err(e) => circuit_breaker.record_failure(),
        }
    }
}
```

### Pattern 4: Ticker Mapping and Fallbacks

**What:** CapitolTrades issuer_ticker may not match Yahoo Finance symbols. Provide manual override mechanism.

**When to use:** Ticker symbols change, issuers get acquired, stocks delist.

**Trade-offs:**
- **Pro:** Handles real-world ticker mismatches (e.g., "BRK.A" vs "BRK-A")
- **Con:** Manual maintenance required (but rare)

**Decision:** Store ticker overrides in ingest_meta table as JSON map.

**Example:**
```sql
-- ingest_meta table:
-- key: 'ticker_overrides'
-- value: '{"12345": "GOOGL", "67890": "BRK-A"}'
```

```rust
// capitoltraders_lib/src/yahoo.rs
impl YahooClient {
    fn resolve_ticker(&self, issuer_id: i64, issuer_ticker: &str) -> String {
        if let Some(override) = self.ticker_overrides.get(&issuer_id) {
            return override.clone();
        }
        issuer_ticker.replace('.', "-")  // BRK.A -> BRK-A
    }
}
```

## Anti-Patterns to Avoid

### Anti-Pattern 1: Storing Portfolio Positions in DB

**What:** Creating a `portfolio_positions` table that caches computed holdings.

**Why bad:**
- Stale data: Positions become outdated when new trades sync
- Schema complexity: Need triggers or scheduled jobs to keep in sync
- Query flexibility: Hard to compute "positions as of date X"

**Instead:** Compute positions on-demand from trades table. For 100K trades, FIFO aggregation takes <100ms.

### Anti-Pattern 2: Separate yfinance Crate

**What:** Creating a fourth workspace crate: `capitoltraders_yahoo`.

**Why bad:**
- Over-engineering: Yahoo integration is ~300 LOC, not worth separate crate
- Compilation penalty: Workspace crates must be compiled separately even if unchanged
- Circular dependencies: Would need capitoltraders_lib types (PriceRecord, YahooError)

**Instead:** Add yahoo.rs module to capitoltraders_lib. Module compiles with lib, shares types.

### Anti-Pattern 3: Real-Time Quote Streaming

**What:** Using yfinance-rs WebSocket streaming for live quotes.

**Why bad:**
- Out of scope: Congressional trades are disclosed 30-45 days late, real-time quotes are irrelevant
- Complexity: Requires long-lived connections, reconnection logic, state management
- Rate limits: Yahoo throttles WebSocket connections aggressively

**Instead:** Use EOD prices only. Fetch historical data once, update nightly via cron + enrich-prices.

### Anti-Pattern 4: Per-Trade Price Storage

**What:** Storing `price_at_tx_date` column in trades table.

**Why bad:**
- Redundant: EOD prices already in issuer_eod_prices, JOIN gives same result
- Stale: Price data becomes outdated if Yahoo adjusts splits/dividends
- Bloat: Trades table grows unnecessarily

**Instead:** JOIN trades with issuer_eod_prices at query time. Use SQL `WHERE price_date = tx_date`.

## Build Order

### Dependency Graph

```
capitoltrades_api (unchanged, already exists)
    ↓
capitoltraders_lib (EXTEND)
    ├─ db.rs (EXTEND: add price query methods, NO schema changes)
    ├─ yahoo.rs (NEW: YahooClient wrapper)
    ├─ portfolio.rs (NEW: Position calculator)
    └─ error.rs (EXTEND: add YahooError, PortfolioError variants)
    ↓
capitoltraders_cli (EXTEND)
    ├─ commands/enrich.rs (NEW: enrich-prices subcommand)
    └─ commands/portfolio.rs (NEW: portfolio subcommand)
```

### Build Sequence

1. **Phase 1: YahooClient + DB methods**
   - Add yfinance-rs to capitoltraders_lib/Cargo.toml
   - Implement capitoltraders_lib/src/yahoo.rs
   - Add Db::upsert_prices(), Db::get_latest_price() to db.rs
   - Write unit tests for YahooClient (use wiremock for HTTP)
   - Write integration tests for DB price methods

2. **Phase 2: enrich-prices command**
   - Implement capitoltraders_cli/src/commands/enrich.rs
   - Reuse enrichment pipeline pattern from sync.rs
   - Add --ticker flag for single-issuer test runs
   - Add --dry-run flag to show what would be enriched
   - Write integration test: enrich-prices → DB has prices

3. **Phase 3: Portfolio calculator**
   - Implement capitoltraders_lib/src/portfolio.rs
   - FIFO lot tracking logic
   - Position aggregation by ticker
   - Unit tests: FIFO edge cases (multiple buys/sells, partial sales)
   - Integration tests: compute_positions() against fixture DB

4. **Phase 4: portfolio command**
   - Implement capitoltraders_cli/src/commands/portfolio.rs
   - Output formats: table, JSON, CSV (reuse existing output.rs patterns)
   - Add --politician, --as-of-date, --ticker filters
   - Integration test: portfolio command → correct PnL

### What Depends on What

| Component | Depends On | Blocks |
|-----------|------------|--------|
| YahooClient | yfinance-rs (external crate) | enrich-prices command, Portfolio |
| Db price methods | YahooClient types (PriceRecord) | enrich-prices command, Portfolio |
| Portfolio calculator | Db price methods, trades table | portfolio command |
| enrich-prices command | YahooClient, Db | None (independent) |
| portfolio command | Portfolio calculator | None (independent) |

## Scalability Considerations

| Concern | Current (1K issuers) | At 10K issuers | At 100K issuers |
|---------|----------------------|----------------|-----------------|
| Price storage | 365 rows/issuer/year = 365K rows/year | 3.65M rows/year | 36.5M rows/year (SQLite limit ~281 trillion rows, no issue) |
| Enrichment time | ~3 min (1K tickers * 150ms/fetch, 3 concurrent) | ~30 min (batch overnight) | ~5 hours (batch weekly) |
| Portfolio calculation | <100ms (scan 100K trades, FIFO in-memory) | <500ms (1M trades, indexed scan) | ~2-3s (10M trades, consider caching) |
| Ticker lookup | O(1) DashMap cache, <1ms | O(1) DashMap cache, <1ms | O(1) DashMap cache, <1ms |

### Scaling Recommendations

- **1-10K issuers:** Run enrich-prices nightly via cron, no caching needed
- **10K-100K issuers:** Add batch_size flag, enrich in chunks, cache prices in DashMap
- **100K+ issuers:** Consider PostgreSQL for better concurrent writes, add materialized view for positions

## Sources

**Yahoo Finance Clients:**
- [yfinance-rs crate](https://docs.rs/yfinance-rs) - PRIMARY RECOMMENDATION: Ergonomic, async-first, feature-rich (HIGH confidence)
- [yahoo_finance_api crate](https://docs.rs/yahoo_finance_api) - Alternative: Mature, async, lighter weight (HIGH confidence)
- [crates.io Yahoo Finance libraries](https://crates.io/crates/yahoo_finance_api) (MEDIUM confidence)

**Portfolio Architecture:**
- [NautilusTrader portfolio module](https://lib.rs/finance) - Event-driven portfolio tracking, PnL calculations (MEDIUM confidence)
- [RustQuant portfolio implementation](https://github.com/avhz/RustQuant) - Portfolio as HashMap of Positions (MEDIUM confidence)
- [Redis securities portfolio data model](https://redis.io/blog/securities-portfolio-data-model/) - Lot tracking patterns (MEDIUM confidence)
- [Investment portfolio tracker database schema](https://databasesample.com/database/investment-portfolio-tracker-database) - Entity structure (LOW confidence - generic example)

**Rust Workspace Patterns:**
- [Cargo Workspaces (Rust Book)](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html) - When to split crates vs modules (HIGH confidence)
- [Rust at scale: packages, crates, and modules](https://mmapped.blog/posts/03-rust-packages-crates-modules) - Compilation tradeoffs (HIGH confidence)

**Financial Calculation Patterns:**
- [FIFO cost basis calculation](https://www.allstacksdeveloper.com/2022/09/fifo-stock-portfolio-google-sheets.html) - FIFO lot accounting (MEDIUM confidence)
- [Storing stock data efficiently](https://ericdraken.com/storing-stock-candle-data-efficiently/) - Time-series storage (LOW confidence - not SQLite-specific)

---
*Architecture research for: Yahoo Finance integration into existing Rust CLI with SQLite*
*Researched: 2026-02-09*
