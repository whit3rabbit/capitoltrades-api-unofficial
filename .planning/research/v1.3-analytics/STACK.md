# Technology Stack: Congressional Trade Analytics

**Project:** Capitol Traders v1.3 Analytics & Scoring
**Researched:** 2026-02-14

## Recommended Stack

### Core Analytics Libraries
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| (no new dependencies) | - | Core metrics calculation | Existing Rust stdlib sufficient for aggregations, percentile calculations |
| serde | 1.0 (existing) | JSON/YAML data structures | Already used, handles sector mappings and config files |
| chrono | 0.4 (existing) | Date range filtering | Already used for trade dates, needed for YTD/1Y/3Y period calculations |

### Statistical Analysis (if needed)
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| statrs | 0.17 | Standard deviation, percentiles | Only needed if implementing true Sharpe ratio or volatility-based anomaly detection. Defer for v1.3. |
| linregress | 0.5 | Linear regression for Beta | Only needed for market correlation (Beta calculation). Defer for v1.3. |

### Data Storage (no changes)
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| rusqlite | 0.32 (existing) | SQLite interface | Already in use, schema v6 migration for benchmark prices and scores |
| SQLite | 3.x (system) | Persistence layer | Existing pattern, all analytics stored in DB |

### Price Data (no changes)
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| yahoo_finance_api | 4.1.0 (existing) | Benchmark & sector ETF prices | Already integrated in v1.1, reuse for S&P 500 and sector ETF price fetching |

### Supporting Libraries (existing)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| DashMap | 6.x (existing) | YahooClient caching | Already in use, no changes needed |
| tokio | 1.x (existing) | Async runtime | Concurrent benchmark price fetching |
| Semaphore | (tokio) | Concurrency control | Reuse enrichment pipeline pattern for benchmark sync |

## Sector Mapping Approach

### Option 1: Static YAML File (Recommended for v1.3)
```yaml
# data/sector_mappings.yaml
AAPL:
  sector: Technology
  sector_etf: XLK
JPM:
  sector: Financials
  sector_etf: XLF
```

**Pros:**
- No API dependency
- Fast lookup (parse once at startup)
- Version controlled
- Easy to extend

**Cons:**
- Manual maintenance for new tickers
- Stale data (sectors rarely change, but do change)

**Verdict:** Use this for v1.3. Already have `serde_yml 0.0.12` from v1.2.

### Option 2: Scrape from Issuer Detail Pages
Check if `sector` field is already populated in `issuers` table from v1.0 scraping. If yes, map to sector ETF via lookup table.

**Status:** Need to verify if `issuers.sector` contains GICS-compatible sector names. If not, fallback to Option 1.

### Option 3: Yahoo Finance API Lookup
Yahoo Finance `quote` endpoint returns `sector` and `industry` fields. Would require additional API calls during enrichment.

**Verdict:** Defer to v2.x. Adds complexity and API rate limit pressure.

## Committee-Sector Mapping

### Static YAML File
```yaml
# data/committee_sectors.yaml
"House Committee on Energy and Commerce":
  sectors:
    - Energy
    - Utilities
  etfs:
    - XLE
    - XLU

"House Committee on Financial Services":
  sectors:
    - Financials
  etfs:
    - XLF
```

**Rationale:** Committee names are stable, sector oversight is well-documented. Manual mapping ensures accuracy. Load once at CLI startup.

## Benchmark Price Storage

### Schema v6 Migration
Add `benchmark_prices` table:

```sql
CREATE TABLE IF NOT EXISTS benchmark_prices (
    ticker TEXT NOT NULL,          -- 'SPY' or 'XLK', etc.
    price_date TEXT NOT NULL,      -- 'YYYY-MM-DD'
    close_price REAL NOT NULL,
    PRIMARY KEY (ticker, price_date)
);

CREATE INDEX IF NOT EXISTS idx_benchmark_prices_ticker
ON benchmark_prices(ticker);

CREATE INDEX IF NOT EXISTS idx_benchmark_prices_date
ON benchmark_prices(price_date);
```

**Rationale:** Separate table avoids polluting `issuers` or `trades`. Daily granularity matches existing price enrichment pattern. Indexed for fast date-range lookups.

### Benchmark Tickers to Track
- `SPY` - S&P 500 ETF (primary benchmark)
- `XLB`, `XLC`, `XLE`, `XLF`, `XLI`, `XLK`, `XLP`, `XLRE`, `XLU`, `XLV`, `XLY` - 11 sector ETFs

Total: 12 tickers. At ~252 trading days/year, ~3,000 rows/year for all benchmarks.

## Analytics Calculation Approach

### Pure Functions (Rust stdlib only)
For v1.3, avoid external stats libraries. Implement directly:

**Percentile Rank:**
```rust
fn percentile_rank(value: f64, values: &[f64]) -> f64 {
    let count_below = values.iter().filter(|&&v| v < value).count();
    (count_below as f64 / values.len() as f64) * 100.0
}
```

**Win Rate (Batting Average):**
```rust
fn win_rate(trades: &[Trade]) -> f64 {
    let wins = trades.iter().filter(|t| t.realized_pnl > 0.0).count();
    (wins as f64 / trades.len() as f64) * 100.0
}
```

**HHI (Herfindahl-Hirschman Index):**
```rust
fn hhi(sector_weights: &[f64]) -> f64 {
    sector_weights.iter().map(|w| w * w * 10000.0).sum()
}
```

**Rolling Return:**
```rust
fn rolling_return(start_price: f64, end_price: f64) -> f64 {
    ((end_price / start_price) - 1.0) * 100.0
}
```

**Rationale:** Keep dependencies minimal. If performance becomes an issue, add `statrs` later.

## CLI Structure (no new dependencies)

Extend existing subcommands:
- `trades --score` - add scoring columns to output
- `portfolio --score` - add scoring columns to portfolio output
- `politicians --leaderboard` - new flag, sort by composite score
- `analytics` - new subcommand (or defer, output through existing commands)

**Rationale:** Reuse existing output formatting, filter validation, DB query patterns. No new CLI framework needed.

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Stats Library | None (stdlib) | statrs 0.17 | Overkill for percentiles and simple aggregations. Add later if volatility analysis needed. |
| Sector Data | Static YAML | Yahoo Finance API | Adds API dependency, rate limit pressure. Static file sufficient. |
| Benchmark Storage | benchmark_prices table | Reuse issuers table | Conceptually separate (benchmarks aren't politician-traded issuers). Clean separation. |
| Regression Lib | None | linregress 0.5 | Beta calculation deferred to v2.x. Not needed for v1.3 table stakes. |
| Config Format | YAML (serde_yml existing) | TOML or JSON | Already have serde_yml from v1.2. Reuse. |

## Installation

No new dependencies for v1.3 core features (table stakes + basic differentiators). If stats become needed later:

```bash
# Optional (defer to v2.x unless volatility analysis required):
cargo add statrs@0.17   # For standard deviation, advanced percentiles
cargo add linregress@0.5  # For Beta calculation via market regression
```

Current dependencies sufficient:
- `serde_yml` (existing v1.2) for sector/committee mappings
- `yahoo_finance_api` (existing v1.1) for benchmark prices
- `chrono` (existing) for date range filtering
- `rusqlite` (existing) for benchmark price storage

## Data Files

New static data files (version controlled):

```
data/
  sector_mappings.yaml      # ticker -> sector -> sector_etf
  committee_sectors.yaml    # committee name -> sectors + etfs
```

Load at CLI startup, validate structure, cache in memory for duration of command execution.

## Sources

- Existing codebase patterns (yahoo.rs, portfolio.rs, db.rs) reviewed for consistency
- No external library research needed (stdlib + existing deps sufficient)
- Sector ETF list from [State Street Select Sector SPDRs](https://www.ssga.com/us/en/intermediary/capabilities/equities/sector-investing/select-sector-etfs)
