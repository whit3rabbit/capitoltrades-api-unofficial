# Architecture Research: Trade Analytics Integration

**Domain:** Trade Analytics & Scoring for Congressional Stock Trading
**Researched:** 2026-02-14
**Confidence:** HIGH

## Executive Summary

Trade analytics features integrate into the existing 3-crate Rust workspace by following established patterns: domain logic in `capitoltraders_lib`, CLI dispatch in `capitoltraders_cli`, DB operations in `db.rs`. New analytics modules mirror the v1.1 price enrichment architecture: pure calculation functions in lib modules, enrichment pipeline with Semaphore + JoinSet + mpsc pattern, DB schema extension via migration, dedicated CLI commands with per-type output formatters.

Key integration approach: **extend existing enrichment pipeline** (add benchmark price fetching to Phase 2 of enrich-prices), **new scoring module** with pure calculation functions (parallels portfolio.rs pattern), **schema v6 migration** for analytics columns, **analytics CLI command** following portfolio.rs dispatch model.

## Existing Architecture (Context)

### Current Workspace Structure

```
capitoltraders/
├── capitoltrades_api/          # Vendored upstream HTTP client
│   └── types.rs                # Trade, Politician, Issuer types
├── capitoltraders_lib/         # Domain logic layer
│   ├── db.rs                   # Single-file SQLite operations (1000+ LOC)
│   ├── yahoo.rs                # YahooClient with DashMap cache
│   ├── pricing.rs              # TradeRange, estimate_shares (pure functions)
│   ├── portfolio.rs            # FIFO calculator (pure functions)
│   ├── analysis.rs             # Simple aggregations (trades_by_party, etc.)
│   ├── openfec/                # OpenFEC client + types module
│   ├── committee.rs            # CommitteeResolver
│   ├── employer_mapping.rs     # Employer-to-ticker matching
│   └── validation.rs           # 18 input validators
├── capitoltraders_cli/         # CLI dispatch layer
│   ├── commands/               # 11 subcommand modules
│   │   ├── enrich_prices.rs    # Two-phase enrichment pipeline (historical + current)
│   │   ├── portfolio.rs        # DB-only, calls calculate_positions()
│   │   ├── sync_donations.rs   # Concurrent pipeline with circuit breaker
│   │   └── ...
│   └── output.rs               # Per-type format functions (table/csv/md/xml/json)
└── schema/
    └── sqlite.sql              # Base schema (13 tables, schema v5)
```

### Established Patterns

**Domain Logic Location:**
- Pure calculation functions: lib modules (portfolio.rs, pricing.rs)
- External API clients: lib modules with Arc wrappers (yahoo.rs, openfec/)
- DB operations: single db.rs file with dedicated methods per feature

**Enrichment Pipeline Pattern (from enrich_prices.rs):**
```
1. Load unenriched rows from DB
2. Deduplicate by (ticker, date) or ticker
3. Spawn concurrent tasks with Semaphore rate limiting
4. Send results via mpsc channel
5. Single-threaded DB writes from channel receiver
6. Circuit breaker on consecutive failures
```

**CLI Command Pattern (from portfolio.rs):**
```
1. CLI args with validation
2. Open DB, validate filters
3. Query DB with filter struct
4. Dispatch to output formatters based on --output flag
5. Per-type format functions in output.rs
```

**DB Pattern:**
- PRAGMA user_version for schema migrations (currently v5)
- IF NOT EXISTS for idempotent migrations
- Sentinel CASE in upserts to prevent enrichment data loss
- JOIN issuers table for ticker access (ticker lives on issuers, not trades)
- Composite indexes for filter queries

## Analytics Components Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CLI Layer (capitoltraders_cli)                  │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                  │
│  │ enrich-prices│  │  analytics   │  │  portfolio   │                  │
│  │  (extended)  │  │   (new)      │  │  (existing)  │                  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                  │
│         │                 │                 │                           │
├─────────┴─────────────────┴─────────────────┴───────────────────────────┤
│                      Domain Logic (capitoltraders_lib)                  │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐ │
│  │ yahoo.rs │  │sector.rs │  │scoring.rs│  │portfolio │  │ pricing  │ │
│  │ (extend) │  │  (new)   │  │  (new)   │  │(existing)│  │(existing)│ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘ │
│       │             │             │             │             │        │
├───────┴─────────────┴─────────────┴─────────────┴─────────────┴────────┤
│                         Data Layer (db.rs + schema)                     │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  Schema v6: analytics columns on trades table                  │    │
│  │  - benchmark_price, sector_benchmark_price, alpha_score        │    │
│  │  - abnormal_return, sector_id, analytics_enriched_at           │    │
│  └────────────────────────────────────────────────────────────────┘    │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  New table: sector_benchmarks (ticker, sector_id, gics_code)   │    │
│  └────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘

External Dependencies:
┌──────────────┐    ┌──────────────┐
│ Yahoo Finance│    │  GICS Data   │
│  (SPY, XLK,  │    │  (static or  │
│   XLF, etc.) │    │   API)       │
└──────────────┘    └──────────────┘
```

### Component Responsibilities

| Component | Responsibility | Integration Pattern |
|-----------|----------------|-------------------|
| **capitoltraders_lib/sector.rs** | Sector classification (ticker → GICS sector), benchmark ticker selection (sector → XLK/XLF/etc.), sector mapping utilities | New module, parallels committee.rs structure |
| **capitoltraders_lib/scoring.rs** | Alpha score calculation (pure function), abnormal return calculation (trade return vs benchmark), composite scoring logic | New module, parallels portfolio.rs pure function pattern |
| **YahooClient (extended)** | Add get_benchmark_price_on_date() method, reuse existing cache structure, fetch SPY/sector ETF prices | Extend existing yahoo.rs, no architectural change |
| **Db::migrate_v6()** | Add analytics columns to trades table, create sector_benchmarks table, create indexes | Standard migration pattern in db.rs |
| **Db analytics methods** | get_unenriched_analytics_trades(), update_trade_analytics(), query_analytics_summary() | New methods in db.rs, parallel existing enrichment query methods |
| **enrich-prices (extended)** | Add Phase 3: benchmark price enrichment (after Phase 2 current prices), deduplicate by benchmark ticker + date | Extend existing command, preserve two-phase structure |
| **analytics command** | CLI dispatch for analytics queries, filter validation, output formatting | New command module, mirrors portfolio.rs structure |
| **output.rs (extended)** | Add print_analytics_table/csv/md/xml(), format alpha scores and returns with percentage precision | Extend existing output.rs, follow portfolio output pattern |

## Recommended Module Structure

### New Modules

```
capitoltraders_lib/src/
├── sector.rs                  # Sector classification and benchmark mapping
│   ├── SectorClassifier       # Ticker → GICS sector
│   ├── BenchmarkSelector      # Sector → benchmark ticker (SPY, XLK, etc.)
│   ├── static SECTOR_MAP      # HashMap<&str, GicsSector> for common tickers
│   └── GicsSector enum        # 11 GICS sectors
│
├── scoring.rs                 # Analytics scoring functions (pure)
│   ├── calculate_alpha_score()     # (trade_return, benchmark_return) → alpha
│   ├── calculate_abnormal_return() # (trade_value, current_value, benchmark_return) → abnormal
│   ├── calculate_composite_score() # Weighted combination of metrics
│   └── ScoreComponents        # Struct with alpha, abnormal_return, holding_period
│
└── lib.rs (updated)
    └── pub use sector::{SectorClassifier, GicsSector, BenchmarkSelector};
        pub use scoring::{calculate_alpha_score, calculate_abnormal_return, ScoreComponents};
```

```
capitoltraders_cli/src/commands/
├── enrich_prices.rs (extended)
│   └── Phase 3: benchmark_enrichment()  # After Phase 2, dedupe by benchmark ticker
│
├── analytics.rs (new)
│   ├── AnalyticsArgs         # Filters: politician, party, min-alpha, top-n
│   ├── run()                 # Query + output dispatch
│   └── validate_filters()    # Reuse validation module
│
└── mod.rs (updated)
    └── pub mod analytics;
```

```
capitoltraders_cli/src/
└── output.rs (extended)
    ├── print_analytics_table()    # Table with alpha/abnormal return columns
    ├── print_analytics_csv()      # CSV with sanitization
    ├── print_analytics_markdown() # Markdown table
    └── print_analytics_xml()      # XML via print_json bridge
```

### Structure Rationale

- **sector.rs as standalone module:** Sector classification is distinct concern from pricing/portfolio, may expand with additional sector-specific logic (industry groups, sub-industries)
- **scoring.rs mirrors portfolio.rs:** Pure calculation functions, no DB/network dependencies, enables unit testing with synthetic data
- **Extend enrich_prices.rs rather than new command:** Benchmark prices are enrichment data, not analytical output; fits existing two-phase pattern (historical, current, benchmarks)
- **analytics command separate from trades:** Analytics queries have different filter semantics (min-alpha, sector filters) and output schema (additional calculated columns)


## Architectural Patterns

### Pattern 1: Sector Classification with Static Mapping + Fallback

**What:** Use compile-time HashMap for common tickers (AAPL → Technology, JPM → Financials), fall back to external API (optional) or "Unknown" for missing tickers

**When to use:** When 80% of tickers can be classified statically, occasional misses acceptable

**Trade-offs:**
- **Pro:** Zero runtime cost for common cases, no external dependency for table stakes
- **Pro:** Predictable behavior, no API rate limiting concerns
- **Con:** Requires manual maintenance for new tickers
- **Con:** Cannot handle newly-IPO'd companies until code update

**Example:**
```rust
use std::collections::HashMap;
use lazy_static::lazy_static;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GicsSector {
    InformationTechnology,
    Financials,
    HealthCare,
    ConsumerDiscretionary,
    Industrials,
    // ... 6 more
    Unknown,
}

lazy_static! {
    static ref SECTOR_MAP: HashMap<&'static str, GicsSector> = {
        let mut m = HashMap::new();
        m.insert("AAPL", GicsSector::InformationTechnology);
        m.insert("MSFT", GicsSector::InformationTechnology);
        m.insert("JPM", GicsSector::Financials);
        m.insert("BAC", GicsSector::Financials);
        m.insert("JNJ", GicsSector::HealthCare);
        // ... top 200 traded tickers
        m
    };
}

pub fn classify_ticker(ticker: &str) -> GicsSector {
    SECTOR_MAP.get(ticker.trim().to_uppercase().as_str())
        .copied()
        .unwrap_or(GicsSector::Unknown)
}

pub fn benchmark_for_sector(sector: GicsSector) -> &'static str {
    match sector {
        GicsSector::InformationTechnology => "XLK",
        GicsSector::Financials => "XLF",
        GicsSector::HealthCare => "XLV",
        GicsSector::Unknown => "SPY",  // Default to market benchmark
        // ... rest of sectors
    }
}
```

### Pattern 2: Two-Phase Enrichment Extension

**What:** Extend existing enrich-prices pipeline with Phase 3 for benchmark prices, reuse YahooClient cache and Semaphore pattern

**When to use:** When new enrichment data depends on existing enriched data (benchmark prices need trade dates)

**Trade-offs:**
- **Pro:** Reuses existing concurrency infrastructure, rate limiting, circuit breaker
- **Pro:** Single command for all price enrichment, simpler UX
- **Con:** Longer-running command, cannot enrich benchmarks without first enriching trade prices
- **Con:** Batch failures affect multiple enrichment types

**Example:**
```rust
// In enrich_prices.rs, after Phase 2 current price enrichment:

// Phase 3: Benchmark price enrichment
let trades_for_benchmarks = db.get_trades_with_prices_but_no_benchmarks(args.batch_size)?;
let benchmark_date_map: HashMap<(String, NaiveDate), Vec<usize>> = HashMap::new();

for (idx, trade) in trades_for_benchmarks.iter().enumerate() {
    let sector = sector::classify_ticker(&trade.issuer_ticker);
    let benchmark_ticker = sector::benchmark_for_sector(sector);
    let date = NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d")?;
    benchmark_date_map
        .entry((benchmark_ticker.to_string(), date))
        .or_default()
        .push(idx);
}

// Spawn tasks for benchmark prices (same Semaphore + JoinSet + mpsc pattern)
// Update trades with benchmark_price column
```

### Pattern 3: Pure Scoring Functions with Struct Return

**What:** Scoring functions return dedicated struct with all calculated components, not just final score

**When to use:** When multiple derived metrics are needed for different analytical views

**Trade-offs:**
- **Pro:** Enables partial use (show abnormal return without alpha score)
- **Pro:** Easier to test individual calculation steps
- **Pro:** DB can store all components, allows recalculation of composite scores without re-enrichment
- **Con:** More columns in database (6 new columns vs 1)
- **Con:** Slightly more complex DB writes

**Example:**
```rust
#[derive(Debug, Clone, Copy)]
pub struct ScoreComponents {
    pub alpha_score: f64,           // Trade return - benchmark return
    pub abnormal_return: f64,       // Absolute dollar P&L vs benchmark expectation
    pub trade_return_pct: f64,      // Raw trade return percentage
    pub benchmark_return_pct: f64,  // Benchmark return over same period
    pub holding_period_days: i64,   // Trade date to current date
}

pub fn calculate_analytics(
    trade_date_price: f64,
    current_price: f64,
    benchmark_date_price: f64,
    benchmark_current_price: f64,
    estimated_value: f64,
    tx_date: NaiveDate,
) -> Option<ScoreComponents> {
    if trade_date_price <= 0.0 || benchmark_date_price <= 0.0 {
        return None;
    }

    let trade_return_pct = ((current_price - trade_date_price) / trade_date_price) * 100.0;
    let benchmark_return_pct = ((benchmark_current_price - benchmark_date_price) / benchmark_date_price) * 100.0;
    let alpha_score = trade_return_pct - benchmark_return_pct;

    let expected_value = estimated_value * (1.0 + benchmark_return_pct / 100.0);
    let actual_value = estimated_value * (1.0 + trade_return_pct / 100.0);
    let abnormal_return = actual_value - expected_value;

    let holding_period_days = (chrono::Utc::now().date_naive() - tx_date).num_days();

    Some(ScoreComponents {
        alpha_score,
        abnormal_return,
        trade_return_pct,
        benchmark_return_pct,
        holding_period_days,
    })
}
```

## Data Flow

### Analytics Enrichment Flow

```
[capitoltraders enrich-prices --db trades.db]
    ↓
Phase 1: Historical prices (ticker, date) → trade_date_price
    ↓
Phase 2: Current prices (ticker) → current_price
    ↓
Phase 3: Benchmark prices (benchmark_ticker, date) → benchmark_price
    ↓ (NEW)
[capitoltraders enrich-analytics --db trades.db]
    ↓
Calculate ScoreComponents for each trade
    ↓
UPDATE trades SET alpha_score=?, abnormal_return=?, analytics_enriched_at=?
```

### Analytics Query Flow

```
[capitoltraders analytics --db trades.db --min-alpha 5.0 --top 50]
    ↓
Validate filters (party, state, min-alpha, sector)
    ↓
db.query_analytics_trades(&filter)
    ↓
SELECT t.*, i.issuer_ticker, p.first_name, p.last_name, p.party
FROM trades t
JOIN issuers i ON t.issuer_id = i.issuer_id
JOIN politicians p ON t.politician_id = p.politician_id
WHERE analytics_enriched_at IS NOT NULL
  AND alpha_score >= ?
ORDER BY alpha_score DESC
LIMIT 50
    ↓
Build AnalyticsRow structs
    ↓
Dispatch to output formatter (table/csv/md/xml/json)
```

### Sector Classification Flow

```
[ticker from trade]
    ↓
sector::classify_ticker(ticker)
    ↓
SECTOR_MAP.get(ticker) → Some(GicsSector) or None
    ↓
sector::benchmark_for_sector(sector)
    ↓
"XLK" | "XLF" | "SPY" | ...
    ↓
YahooClient::get_price_on_date(benchmark_ticker, trade_date)
```

## Database Schema Extensions

### Schema v6 Migration

```sql
-- Add analytics columns to trades table
ALTER TABLE trades ADD COLUMN benchmark_price REAL;
ALTER TABLE trades ADD COLUMN sector_benchmark_price REAL;
ALTER TABLE trades ADD COLUMN alpha_score REAL;
ALTER TABLE trades ADD COLUMN abnormal_return REAL;
ALTER TABLE trades ADD COLUMN trade_return_pct REAL;
ALTER TABLE trades ADD COLUMN benchmark_return_pct REAL;
ALTER TABLE trades ADD COLUMN sector_id TEXT;
ALTER TABLE trades ADD COLUMN analytics_enriched_at TEXT;

-- Create sector_benchmarks reference table
CREATE TABLE IF NOT EXISTS sector_benchmarks (
    sector_id TEXT PRIMARY KEY,
    sector_name TEXT NOT NULL,
    benchmark_ticker TEXT NOT NULL,
    gics_code TEXT
);

-- Populate with 11 GICS sectors
INSERT INTO sector_benchmarks (sector_id, sector_name, benchmark_ticker, gics_code) VALUES
    ('tech', 'Information Technology', 'XLK', '45'),
    ('financials', 'Financials', 'XLF', '40'),
    ('healthcare', 'Health Care', 'XLV', '35'),
    ('consumer_disc', 'Consumer Discretionary', 'XLY', '25'),
    ('industrials', 'Industrials', 'XLI', '20'),
    ('materials', 'Materials', 'XLB', '15'),
    ('energy', 'Energy', 'XLE', '10'),
    ('utilities', 'Utilities', 'XLU', '55'),
    ('real_estate', 'Real Estate', 'XLRE', '60'),
    ('consumer_staples', 'Consumer Staples', 'XLP', '30'),
    ('comm_services', 'Communication Services', 'XLC', '50');

-- Indexes for analytics queries
CREATE INDEX IF NOT EXISTS idx_trades_alpha_score ON trades(alpha_score DESC);
CREATE INDEX IF NOT EXISTS idx_trades_abnormal_return ON trades(abnormal_return DESC);
CREATE INDEX IF NOT EXISTS idx_trades_analytics_enriched ON trades(analytics_enriched_at);
CREATE INDEX IF NOT EXISTS idx_trades_sector ON trades(sector_id);
```

### DB Method Extensions

```rust
// In db.rs

pub struct AnalyticsRow {
    pub tx_id: i64,
    pub politician_name: String,
    pub party: String,
    pub issuer_ticker: String,
    pub tx_date: String,
    pub alpha_score: f64,
    pub abnormal_return: f64,
    pub trade_return_pct: f64,
    pub benchmark_return_pct: f64,
}

pub struct AnalyticsFilter {
    pub politician_id: Option<String>,
    pub party: Option<String>,
    pub state: Option<String>,
    pub sector_id: Option<String>,
    pub min_alpha: Option<f64>,
    pub min_abnormal_return: Option<f64>,
}

impl Db {
    pub fn get_unenriched_analytics_trades(&self, limit: Option<i64>) -> Result<Vec<AnalyticsEnrichmentRow>, DbError> {
        // SELECT from trades WHERE price_enriched_at IS NOT NULL AND analytics_enriched_at IS NULL
    }

    pub fn update_trade_analytics(&self, tx_id: i64, components: &ScoreComponents, sector_id: &str) -> Result<(), DbError> {
        // UPDATE trades SET alpha_score=?, abnormal_return=?, sector_id=?, analytics_enriched_at=datetime('now')
    }

    pub fn query_analytics_trades(&self, filter: &AnalyticsFilter) -> Result<Vec<AnalyticsRow>, DbError> {
        // Dynamic WHERE clause builder (parallel to query_donations pattern)
    }
}
```

## Integration Points

### New Component Integration with Existing

| New Component | Integrates With | Integration Method | Notes |
|---------------|-----------------|-------------------|-------|
| sector.rs | issuers table | Read issuer_ticker via existing trades JOIN | Sector classification purely in-memory, no DB writes |
| scoring.rs | pricing.rs | Uses estimated_shares, estimated_value from pricing module | Pure function composition, no shared state |
| YahooClient::get_benchmark_price() | Existing cache DashMap | Reuse cache with (ticker, date) key | SPY/XLK prices cached same as AAPL/JPM |
| enrich-analytics command | enrich-prices command | Depends on price_enriched_at column | Sequential dependency, must run after enrich-prices |
| analytics CLI | portfolio CLI | Same filter validation pattern (party, state, politician) | Parallel command structure, different output schema |

### External Dependencies

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Yahoo Finance (benchmark ETFs) | Existing YahooClient | SPY, XLK, XLF, etc. are standard Yahoo tickers, no special handling |
| GICS Classification | Static HashMap in sector.rs | Top 200 tickers hardcoded, rest fall back to Unknown → SPY benchmark |

### Data Dependencies

```
Dependency Chain:
1. trades synced (sync command)
2. prices enriched (enrich-prices Phase 1+2) → trade_date_price, current_price
3. benchmarks enriched (enrich-prices Phase 3) → benchmark_price
4. analytics calculated (enrich-analytics command) → alpha_score, abnormal_return
5. analytics queried (analytics command)

OR (simpler):
1. trades synced
2. enrich-prices (all 3 phases in single run)
3. enrich-analytics (new command)
4. analytics query
```

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| 0-100k trades | Current single-file db.rs pattern scales fine, SQLite with indexes handles queries <100ms |
| 100k-1M trades | Add composite indexes (sector_id, alpha_score) for filtered top-N queries, consider VACUUM after enrichment |
| 1M+ trades | Consider partitioning trades table by year, or archiving old trades to separate DB file |

### Scaling Priorities

1. **First bottleneck:** Analytics enrichment run time (calculating scores for all trades)
   - **Mitigation:** Already concurrent via existing Semaphore pattern, no new bottleneck vs price enrichment
   - **Further optimization:** Batch UPDATE queries (100 trades per transaction) instead of per-trade updates

2. **Second bottleneck:** Analytics query performance with complex filters (sector + min_alpha + politician)
   - **Mitigation:** Composite indexes on (sector_id, alpha_score), (politician_id, alpha_score)
   - **Further optimization:** Materialized view or summary table for top-N per politician

3. **Benchmark price cache bloat:** 11 sector benchmarks × N unique trade dates = moderate cache growth
   - **Mitigation:** Existing DashMap TTL (300s) already handles this, cache cleared between runs
   - **Not a concern:** Benchmark tickers are reused across many trades (high cache hit rate)

## Anti-Patterns

### Anti-Pattern 1: Calculating Scores On-Demand Per Query

**What people do:** Calculate alpha_score and abnormal_return in SQL SELECT query using joins and expressions

**Why it's wrong:**
- Recalculates on every query, wastes CPU
- Prevents efficient indexing (can't index calculated columns in SQLite without materialized view)
- Makes queries slow and complex

**Do this instead:** Pre-calculate and store scores during enrichment, query pre-calculated values with simple WHERE filters

### Anti-Pattern 2: Separate Benchmark Sync Command

**What people do:** Create `capitoltraders sync-benchmarks` separate from `enrich-prices`

**Why it's wrong:**
- Benchmark prices are enrichment data, not raw sync data (raw data comes from capitoltrades.com)
- Creates UX confusion about command ordering
- Duplicates YahooClient instantiation and rate limiting logic

**Do this instead:** Extend enrich-prices with Phase 3 for benchmark prices, keeping all Yahoo Finance interaction in single command

### Anti-Pattern 3: External GICS API Dependency for All Classifications

**What people do:** Call external API (e.g., Financial Modeling Prep) for every ticker sector lookup

**Why it's wrong:**
- Rate limiting becomes critical bottleneck
- Requires API key management and cost
- 80% of trades are top 200 tickers (can be statically classified)
- Network failures block enrichment

**Do this instead:** Static HashMap for top tickers, external API as optional fallback (not Phase 1 requirement), Unknown sector defaults to SPY benchmark (always available)

### Anti-Pattern 4: Composite Score as Only Stored Metric

**What people do:** Calculate and store single composite_score column, discard alpha_score and abnormal_return components

**Why it's wrong:**
- Cannot query "show trades with alpha > 5%" if only composite exists
- Cannot A/B test different weighting formulas without re-enrichment
- Harder to debug anomalies (which component caused high score?)

**Do this instead:** Store all score components (alpha, abnormal return, individual returns), provide composite score as calculated field or view

## Build Order Recommendations

Based on data dependencies and architectural patterns:

### Phase 1: Sector Classification (No Dependencies)
- Create sector.rs module with GicsSector enum and SECTOR_MAP
- Implement classify_ticker() and benchmark_for_sector()
- Unit tests with known tickers (AAPL → Tech → XLK)
- **Deliverable:** Pure function module, no DB/network dependencies

### Phase 2: Schema v6 Migration (Depends on Phase 1 sector IDs)
- Add migrate_v6() to db.rs
- Create sector_benchmarks table with 11 GICS sectors
- Add analytics columns to trades table
- Create indexes for analytics queries
- **Deliverable:** Migration passing on fresh DB and v5 → v6 upgrade path

### Phase 3: Benchmark Price Enrichment (Depends on Phase 2 schema)
- Extend enrich-prices.rs with Phase 3 after Phase 2
- Add get_trades_with_prices_but_no_benchmarks() DB method
- Deduplicate by (benchmark_ticker, date)
- Reuse existing YahooClient and Semaphore pattern
- **Deliverable:** enrich-prices command enriches benchmark_price column

### Phase 4: Scoring Module (Depends on Phase 3 benchmark prices)
- Create scoring.rs with ScoreComponents struct
- Implement calculate_analytics() pure function
- Unit tests with synthetic price data
- **Deliverable:** Pure calculation module with 100% test coverage

### Phase 5: Analytics Enrichment Command (Depends on Phase 4 scoring)
- Create enrich-analytics.rs command module
- Implement get_unenriched_analytics_trades() and update_trade_analytics() DB methods
- Calculate ScoreComponents and UPDATE trades table
- **Deliverable:** Working enrichment command (no output yet)

### Phase 6: Analytics CLI & Output (Depends on Phase 5 enriched data)
- Create analytics.rs command module with AnalyticsArgs
- Implement query_analytics_trades() DB method with AnalyticsFilter
- Extend output.rs with print_analytics_* functions
- **Deliverable:** Full analytics query workflow (sync → enrich-prices → enrich-analytics → analytics)

**Rationale for ordering:**
- Sector classification is pure logic, no blockers, enables parallel work on schema
- Schema migration before enrichment avoids chicken-egg problem
- Benchmark enrichment before scoring (scoring needs benchmark prices)
- Scoring module before enrichment command (enrichment calls scoring functions)
- Analytics CLI last (depends on enriched data to test output)

## Sources

**Abnormal Returns Methodology:**
- [Abnormal Return - Corporate Finance Institute](https://corporatefinanceinstitute.com/resources/equities/abnormal-return/) - Definition and calculation methodology
- [Advanced ETF Analytics Methodology - Morningstar](https://morningstardirect.morningstar.com/clientcomm/Morningstar_Advanced_ETF_Analytics_Methodology_3.0.pdf) - Benchmark tracking and performance analysis

**Congressional Trading Analytics:**
- [Congress Trading - Quiver Quantitative](https://www.quiverquant.com/congresstrading/) - Existing analytics platform with weighted alpha
- [Politician Insider Trading Activity - Barchart.com](https://www.barchart.com/investing-ideas/politician-insider-trading) - Performance metrics display

**GICS Sector Classification:**
- [The Global Industry Classification Standard (GICS) - MSCI](https://www.msci.com/indexes/index-resources/gics) - Official GICS methodology
- [GICS Sector and Industry Map - State Street](https://www.ssga.com/us/en/institutional/capabilities/equities/sector-investing/gics-sector-and-industry-map) - 11 sectors mapping
- [Global Industry Classification Standard - Wikipedia](https://en.wikipedia.org/wiki/Global_Industry_Classification_Standard) - Structure overview

**Rust Analytics Libraries (Reference, Not Dependencies):**
- [RusTaLib - GitHub](https://github.com/rustic-ml/RusTaLib) - Technical analysis patterns in Rust
- [ta-rs - GitHub](https://github.com/greyblake/ta-rs) - Technical analysis library structure reference

**Benchmark ETFs:**
- [SPY ETF Stock Price & Overview - Stock Analysis](https://stockanalysis.com/etf/spy/) - S&P 500 benchmark
- [State Street SPDR Sector ETFs](https://www.ssga.com/us/en/institutional/capabilities/equities/sector-investing/gics-sector-and-industry-map) - XLK, XLF, XLV sector benchmarks

---
*Architecture research for: Trade Analytics & Scoring Integration*
*Researched: 2026-02-14*
*Confidence: HIGH (existing patterns well-established, analytics calculations standard in finance)*
