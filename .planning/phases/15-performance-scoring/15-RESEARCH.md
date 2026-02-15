# Phase 15: Performance Scoring & Leaderboards - Research

**Researched:** 2026-02-15
**Domain:** Financial performance metrics, leaderboard analytics, SQL aggregation
**Confidence:** HIGH

## Summary

Phase 15 adds performance scoring and leaderboard functionality to Capitol Traders. Users will be able to see individual trade performance metrics (absolute return, annualized return, holding period) and aggregate politician rankings based on win rate, S&P 500 alpha, and sector-relative performance. The implementation requires calculating performance metrics from enriched trade data (Phase 14 benchmark prices), aggregating by politician, and presenting sortable rankings via a new CLI analytics subcommand.

The core challenge is computing accurate performance metrics for closed trades (buy+sell pairs matched via FIFO) while handling incomplete data (missing benchmark prices, options trades). SQLite window functions (RANK, DENSE_RANK, PERCENT_RANK) provide native support for leaderboard calculations. The existing FIFO portfolio calculator (portfolio.rs) can be extended to track holding periods and realized returns per trade.

**Primary recommendation:** Build a two-layer architecture: (1) a pure Rust module (lib/src/analytics.rs) for computing per-trade and per-politician metrics from enriched data, (2) DB methods for querying closed trades and aggregating by politician with window functions for percentile ranking. Use existing patterns from portfolio.rs (FIFO matching) and donations.rs (aggregation queries with GROUP BY).

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.32 | SQLite DB access | Already used for all DB operations, supports window functions since SQLite 3.25+ |
| chrono | 0.4 | Date parsing/calculation | Already used for tx_date parsing, needed for holding period days calculation |
| Standard library | - | HashMap, VecDeque for FIFO | portfolio.rs already uses these for position tracking |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| None needed | - | - | All requirements satisfied by existing dependencies |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Custom Rust calculations | RustQuant crate | RustQuant provides portfolio optimization and advanced metrics (Sharpe ratio, Sortino) but adds complexity. Our needs (simple return %, annualized return, win rate) are straightforward arithmetic - custom implementation is simpler and avoids dependency bloat. |
| SQLite window functions | Application-layer sorting | SQLite PERCENT_RANK() is native, efficient, and indexed. Application sorting requires loading all politicians into memory. Use native SQL for ranking. |

**Installation:**
No new dependencies required. All calculations use existing stack.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── analytics.rs         # NEW: Performance calculation module
├── db.rs                # EXTEND: Add analytics query methods
├── portfolio.rs         # EXTEND: Add holding_period_days() to TradeFIFO
└── lib.rs               # EXTEND: pub use analytics types

capitoltraders_cli/src/commands/
├── analytics.rs         # NEW: CLI subcommand for leaderboards
└── mod.rs               # EXTEND: Add analytics module
```

### Pattern 1: Closed Trade Matching (FIFO-based)
**What:** Identify completed buy-sell pairs using the existing FIFO position calculator.
**When to use:** Calculating absolute return and holding period for individual trades.
**Example:**
```rust
// Source: Existing portfolio.rs pattern + extension for closed trade tracking
use std::collections::VecDeque;

pub struct ClosedTrade {
    pub buy_tx_id: i64,
    pub sell_tx_id: i64,
    pub ticker: String,
    pub politician_id: String,
    pub shares: f64,
    pub buy_price: f64,
    pub sell_price: f64,
    pub buy_date: NaiveDate,
    pub sell_date: NaiveDate,
}

pub struct TradeMatcherPosition {
    lots: VecDeque<Lot>,  // Reuse existing Lot from portfolio.rs
    closed_trades: Vec<ClosedTrade>,  // NEW: track completed pairs
}

impl TradeMatcherPosition {
    pub fn sell(&mut self, shares: f64, price: f64, tx_date: NaiveDate, tx_id: i64) {
        // Match against FIFO lots, emit ClosedTrade records
        while remaining > EPSILON {
            let lot = self.lots.pop_front().unwrap();
            let shares_to_sell = lot.shares.min(remaining);

            self.closed_trades.push(ClosedTrade {
                buy_tx_id: lot.tx_id,
                sell_tx_id: tx_id,
                shares: shares_to_sell,
                buy_price: lot.cost_basis,
                sell_price: price,
                buy_date: lot.tx_date,
                sell_date: tx_date,
                ticker: self.ticker.clone(),
                politician_id: self.politician_id.clone(),
            });

            remaining -= shares_to_sell;
        }
    }
}
```

### Pattern 2: Performance Metric Calculations
**What:** Pure functions computing financial metrics from closed trade data.
**When to use:** After FIFO matching produces ClosedTrade records.
**Example:**
```rust
// Source: Standard financial formulas verified against industry references
pub fn calculate_absolute_return(buy_price: f64, sell_price: f64) -> f64 {
    ((sell_price - buy_price) / buy_price) * 100.0
}

pub fn calculate_holding_period_days(buy_date: NaiveDate, sell_date: NaiveDate) -> i64 {
    (sell_date - buy_date).num_days()
}

pub fn calculate_annualized_return(absolute_return_pct: f64, holding_days: i64) -> Option<f64> {
    if holding_days <= 0 {
        return None;
    }
    let years = holding_days as f64 / 365.0;
    let multiplier = (1.0 + absolute_return_pct / 100.0).powf(1.0 / years) - 1.0;
    Some(multiplier * 100.0)
}

pub fn calculate_simple_alpha(trade_return_pct: f64, benchmark_return_pct: f64) -> f64 {
    trade_return_pct - benchmark_return_pct
}
```

### Pattern 3: Benchmark Return Calculation
**What:** Compute S&P 500 or sector ETF return over trade holding period using stored benchmark prices.
**When to use:** For alpha calculation (trade return vs benchmark return).
**Example:**
```rust
// Requires: benchmark_price_spy and benchmark_price_sector from Phase 14 schema v7
pub struct BenchmarkReturn {
    pub spy_return_pct: Option<f64>,      // S&P 500 return over holding period
    pub sector_return_pct: Option<f64>,   // Sector ETF return over holding period
}

pub fn calculate_benchmark_returns(
    buy_benchmark_spy: Option<f64>,
    sell_benchmark_spy: Option<f64>,
    buy_benchmark_sector: Option<f64>,
    sell_benchmark_sector: Option<f64>,
) -> BenchmarkReturn {
    let spy_return = match (buy_benchmark_spy, sell_benchmark_spy) {
        (Some(buy), Some(sell)) if buy > 0.0 => Some(((sell - buy) / buy) * 100.0),
        _ => None,
    };

    let sector_return = match (buy_benchmark_sector, sell_benchmark_sector) {
        (Some(buy), Some(sell)) if buy > 0.0 => Some(((sell - buy) / buy) * 100.0),
        _ => None,
    };

    BenchmarkReturn {
        spy_return_pct: spy_return,
        sector_return_pct: sector_return,
    }
}
```

### Pattern 4: Politician-Level Aggregation with Window Functions
**What:** Group closed trades by politician, calculate aggregate metrics (win rate, avg return, total trades), rank with percentiles.
**When to use:** Generating leaderboard view.
**Example:**
```rust
// SQL query using SQLite window functions for ranking
const LEADERBOARD_SQL: &str = "
    WITH politician_metrics AS (
        SELECT
            politician_id,
            COUNT(*) as total_trades,
            SUM(CASE WHEN absolute_return > 0 THEN 1 ELSE 0 END) * 100.0 / COUNT(*) as win_rate,
            AVG(absolute_return) as avg_return,
            AVG(spy_alpha) as avg_spy_alpha,
            AVG(sector_alpha) as avg_sector_alpha,
            AVG(holding_days) as avg_holding_days
        FROM closed_trades
        WHERE absolute_return IS NOT NULL
        GROUP BY politician_id
        HAVING total_trades >= ?1  -- minimum trade count filter
    )
    SELECT
        politician_id,
        total_trades,
        win_rate,
        avg_return,
        avg_spy_alpha,
        avg_sector_alpha,
        avg_holding_days,
        PERCENT_RANK() OVER (ORDER BY win_rate DESC) as win_rate_percentile,
        PERCENT_RANK() OVER (ORDER BY avg_spy_alpha DESC) as alpha_percentile,
        RANK() OVER (ORDER BY avg_return DESC) as return_rank
    FROM politician_metrics
    ORDER BY avg_return DESC  -- or user-selected sort column
";
```

### Pattern 5: Time Period Filtering
**What:** Filter closed trades by sell_date to support YTD, 1Y, 2Y, all-time views.
**When to use:** User selects time period filter in CLI.
**Example:**
```rust
pub enum TimePeriod {
    YTD,        // sell_date >= '2026-01-01'
    OneYear,    // sell_date >= date('now', '-1 year')
    TwoYear,    // sell_date >= date('now', '-2 years')
    AllTime,    // no filter
}

impl TimePeriod {
    pub fn to_sql_filter(&self) -> Option<String> {
        match self {
            TimePeriod::YTD => Some("sell_date >= '2026-01-01'".into()),  // hardcode current year
            TimePeriod::OneYear => Some("sell_date >= date('now', '-1 year')".into()),
            TimePeriod::TwoYear => Some("sell_date >= date('now', '-2 years')".into()),
            TimePeriod::AllTime => None,
        }
    }
}
```

### Anti-Patterns to Avoid
- **Don't compute metrics on open positions:** Unrealized P&L is volatile and misleading for performance scoring. Only use closed (buy+sell matched) trades.
- **Don't use naive date arithmetic for annualization:** `(sell_date - buy_date).num_days() / 365.0` is correct. Don't use month counts or year differences (Jan 1 2025 to Dec 31 2025 is 364 days, not 1 year).
- **Don't rank without minimum trade count filter:** A politician with 1 winning trade has 100% win rate but is not statistically meaningful. Require minimum N trades (suggest 10) for leaderboard inclusion.
- **Don't mix benchmark types in alpha calculation:** Use SPY alpha OR sector alpha, never average them. Sector alpha is more precise when available (issuer has gics_sector), SPY alpha is fallback.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Percentile ranking | Manual sorting + index calculation | SQLite PERCENT_RANK() window function | Native SQL window functions are indexed, handle ties correctly, and compute percentiles in O(n log n) with single pass. Custom implementation requires loading all rows into memory and sorting in application layer. |
| Date range calculations | String manipulation for 'YTD' | SQLite date() function with modifiers | SQLite date functions handle leap years, month boundaries, DST transitions. String manipulation is error-prone. |
| Annualized return formula | Custom exponential function | Standard formula: (1 + r)^(1/years) - 1 | Financial industry standard formula. Custom implementations risk rounding errors, edge cases (negative returns, sub-1-day periods). Use proven formula from authoritative sources. |
| Grouping trades by politician | HashMap loops in Rust | SQL GROUP BY with HAVING | SQL aggregation is declarative, indexed, and optimized by query planner. Application-layer grouping requires N full table scans. Use SQL for aggregation. |

**Key insight:** SQLite's query optimizer and window functions are designed for exactly this use case (aggregate metrics + ranking). Moving computation to application layer is slower, more complex, and loses index benefits. Keep aggregation in SQL, use Rust for domain logic (FIFO matching, metric formulas).

## Common Pitfalls

### Pitfall 1: Missing Benchmark Prices for Closed Trades
**What goes wrong:** User ran Phase 14 (benchmark enrichment) but some trades still have NULL benchmark_price. Alpha calculation fails or returns misleading results.
**Why it happens:** Phase 14 only enriches trades where issuer_ticker and tx_date are valid. Weekend trades, delisted tickers, or pre-enrichment data gaps leave NULLs.
**How to avoid:** In closed trade query, use LEFT JOIN pattern and explicit NULL handling. Document that alpha metrics show as NULL when benchmark data unavailable. Don't skip trades with missing alpha; show absolute return and mark alpha as "N/A".
**Warning signs:** Leaderboard shows politicians with 0 trades when filtering by alpha > X. Check for NULL benchmark_price filtering out all their trades.

### Pitfall 2: Incorrect FIFO Matching Across Politicians
**What goes wrong:** FIFO calculator matches buy/sell across different politicians (P000001 buys AAPL, P000002 sells AAPL, matcher thinks it's a closed trade).
**Why it happens:** Existing portfolio.rs uses HashMap key `(politician_id, ticker)` for position tracking. If analytics code forgets to partition by politician_id, it groups all AAPL trades together.
**How to avoid:** Always partition by (politician_id, ticker) when building position tracker. Never process trades across politician boundaries. Verify unit tests include multi-politician same-ticker scenarios.
**Warning signs:** Closed trade count higher than expected. Single politician shows negative shares_held in intermediate state.

### Pitfall 3: Annualized Return on Short Holding Periods
**What goes wrong:** Politician buys Monday, sells Tuesday (1 day hold). 5% return annualizes to (1.05)^365 - 1 = 48,000,000% return. Leaderboard dominated by short-term flips.
**Why it happens:** Annualized return formula amplifies short periods exponentially. 1% daily return compounds to 3,678% annually. Mathematically correct but misleading for ranking.
**How to avoid:** Add minimum holding period threshold (suggest 30 days) for annualized return metric. For trades < 30 days, show absolute return only, mark annualized as "N/A" or exclude from ranking. Document in CLI help text.
**Warning signs:** Leaderboard top 10 all have annualized returns > 1000%. Check avg_holding_days column - if < 7 days, disable annualized ranking.

### Pitfall 4: Options Trades in Performance Metrics
**What goes wrong:** Options have different valuation mechanics (strike price, expiration, premium). Using stock formulas produces garbage metrics.
**Why it happens:** Phase 1 price enrichment already excludes options (asset_type filter), but analytics code might not filter consistently.
**How to avoid:** Filter `WHERE a.asset_type = 'stock'` in closed trades query (same pattern as query_trades_for_portfolio). Explicitly exclude options. Count and report excluded option trades separately (like portfolio.rs does).
**Warning signs:** Negative absolute returns > 100% (implies sell_price < 0, impossible for stocks but possible for options).

### Pitfall 5: Time Period Filter Edge Cases (YTD)
**What goes wrong:** User runs leaderboard in January 2027, YTD filter still uses '2026-01-01'. No results shown.
**Why it happens:** Hardcoded year in YTD filter doesn't update when calendar year changes.
**How to avoid:** Use SQLite date('now', 'start of year') instead of hardcoded '2026-01-01'. Let SQLite compute current year dynamically. Update on every run.
**Warning signs:** YTD leaderboard empty in new calendar year while all-time shows data.

## Code Examples

Verified patterns from existing codebase and standard formulas:

### FIFO Closed Trade Matching
```rust
// Extends existing portfolio.rs Position pattern
// Source: capitoltraders_lib/src/portfolio.rs + closed trade extension

use std::collections::VecDeque;
use chrono::NaiveDate;

pub struct ClosedTradeLot {
    pub buy_tx_id: i64,
    pub sell_tx_id: i64,
    pub shares: f64,
    pub buy_price: f64,
    pub sell_price: f64,
    pub buy_date: NaiveDate,
    pub sell_date: NaiveDate,
}

pub struct AnalyticsPosition {
    pub politician_id: String,
    pub ticker: String,
    pub lots: VecDeque<Lot>,  // from portfolio.rs
    pub closed_trades: Vec<ClosedTradeLot>,
}

impl AnalyticsPosition {
    pub fn sell(&mut self, shares: f64, price: f64, date: NaiveDate, tx_id: i64) -> Result<(), String> {
        let mut remaining = shares;

        while remaining > EPSILON {
            let lot = match self.lots.front_mut() {
                Some(l) => l,
                None => return Err(format!("Oversold: {}", self.ticker)),
            };

            let shares_to_sell = lot.shares.min(remaining);

            // Record closed trade
            self.closed_trades.push(ClosedTradeLot {
                buy_tx_id: lot.tx_id,
                sell_tx_id: tx_id,
                shares: shares_to_sell,
                buy_price: lot.cost_basis,
                sell_price: price,
                buy_date: lot.tx_date.parse().unwrap(),  // stored as String in Lot
                sell_date: date,
            });

            lot.shares -= shares_to_sell;
            remaining -= shares_to_sell;

            if lot.shares < EPSILON {
                self.lots.pop_front();
            }
        }

        Ok(())
    }
}
```

### Performance Metric Functions
```rust
// Source: Standard financial formulas
// References: Wall Street Prep, Corporate Finance Institute
use chrono::NaiveDate;

/// Calculate absolute return percentage
pub fn absolute_return(buy_price: f64, sell_price: f64) -> f64 {
    ((sell_price - buy_price) / buy_price) * 100.0
}

/// Calculate holding period in days
pub fn holding_period_days(buy_date: NaiveDate, sell_date: NaiveDate) -> i64 {
    (sell_date - buy_date).num_days()
}

/// Calculate annualized return (geometric)
/// Returns None if holding period <= 0 or < 30 days (unreliable for annualization)
pub fn annualized_return(absolute_return_pct: f64, holding_days: i64) -> Option<f64> {
    if holding_days < 30 {
        return None;  // Too short for meaningful annualization
    }

    let years = holding_days as f64 / 365.0;
    let total_multiplier = 1.0 + (absolute_return_pct / 100.0);
    let annualized_multiplier = total_multiplier.powf(1.0 / years);

    Some((annualized_multiplier - 1.0) * 100.0)
}

/// Calculate simple alpha (excess return vs benchmark)
pub fn simple_alpha(trade_return_pct: f64, benchmark_return_pct: f64) -> f64 {
    trade_return_pct - benchmark_return_pct
}
```

### SQLite Window Functions for Ranking
```rust
// Source: SQLite official documentation + existing db.rs patterns
// URL: https://sqlite.org/windowfunctions.html

pub fn get_politician_leaderboard(
    &self,
    min_trades: i64,
    time_period: TimePeriod,
    sort_by: LeaderboardSort,
) -> Result<Vec<LeaderboardRow>, DbError> {
    let time_filter = match time_period.to_sql_filter() {
        Some(filter) => format!("AND {}", filter),
        None => String::new(),
    };

    let sql = format!(
        "WITH politician_metrics AS (
            SELECT
                ct.politician_id,
                p.first_name || ' ' || p.last_name as politician_name,
                p.party,
                p.state_id,
                COUNT(*) as total_trades,
                SUM(CASE WHEN ct.absolute_return > 0 THEN 1 ELSE 0 END) * 100.0 / COUNT(*) as win_rate,
                AVG(ct.absolute_return) as avg_return,
                AVG(ct.spy_alpha) as avg_spy_alpha,
                AVG(ct.sector_alpha) as avg_sector_alpha,
                AVG(ct.holding_days) as avg_holding_days
            FROM closed_trades ct
            JOIN politicians p ON ct.politician_id = p.politician_id
            WHERE ct.absolute_return IS NOT NULL
            {}
            GROUP BY ct.politician_id
            HAVING total_trades >= ?1
        )
        SELECT
            politician_id,
            politician_name,
            party,
            state_id,
            total_trades,
            win_rate,
            avg_return,
            avg_spy_alpha,
            avg_sector_alpha,
            avg_holding_days,
            PERCENT_RANK() OVER (ORDER BY win_rate DESC) as win_rate_percentile,
            PERCENT_RANK() OVER (ORDER BY avg_spy_alpha DESC) as alpha_percentile,
            RANK() OVER (ORDER BY {} DESC) as rank
        FROM politician_metrics
        ORDER BY {} DESC",
        time_filter,
        sort_by.to_sql_column(),
        sort_by.to_sql_column()
    );

    // Execute query, map rows to LeaderboardRow struct
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Application-layer percentile calculation | SQLite PERCENT_RANK() window function | SQLite 3.25.0 (2018-09-15) | Window functions added to SQLite in 2018. Modern best practice for ranking queries. Avoid loading all data into memory for sorting. |
| String-based date filtering | SQLite date() function with relative modifiers | Always available | Use `date('now', '-1 year')` not hardcoded dates. Query stays current without code changes. |
| Holding Period Return (HPR) only | Annualized HPR for comparability | Financial industry standard since 1980s | Annualized return normalizes performance across different holding periods. Required for fair comparison. |
| Simple return ranking | Alpha (benchmark-adjusted return) | Modern portfolio theory, 1960s+ | Alpha measures skill vs luck. A 10% return when market did 15% is worse than 5% when market did 2%. |

**Deprecated/outdated:**
- **Using total P&L for ranking:** Absolute dollar P&L is meaningless without account size context. A $1M gain on $100M portfolio (1%) is worse than $10K gain on $50K portfolio (20%). Use percentage returns always.
- **Ignoring holding period:** Comparing 5% return over 1 day vs 5% over 1 year is apples-to-oranges. Always annualize or filter by holding period range.
- **Manual percentile calculation:** `(rank - 1) / (total_count - 1)` is correct formula but SQLite PERCENT_RANK() is faster and handles NULLs/ties correctly.

## Open Questions

1. **Schema change for Phase 15: separate benchmark_price_spy and benchmark_price_sector columns?**
   - What we know: Phase 14 added single `benchmark_price` column. Requirements mention both SPY and sector benchmarks.
   - What's unclear: Does benchmark_price store SPY OR sector (depending on gics_sector presence), or do we need TWO columns to store both?
   - Recommendation: Check Phase 14 implementation. If benchmark_price stores sector-specific ETF (XLK for tech stocks) OR SPY (for non-mapped), we need schema v8 migration to add benchmark_price_spy separately. Alpha calculation requires both: sector_alpha = trade_return - sector_return, spy_alpha = trade_return - spy_return.

2. **Closed trades table: materialized view or dynamic query?**
   - What we know: FIFO matching is compute-intensive (O(n) per politician). Leaderboard queries need to scan all closed trades.
   - What's unclear: Should we store closed_trades as a table (updated by enrich-prices or separate command) or compute on-the-fly in analytics query?
   - Recommendation: Start with dynamic query (query_closed_trades method computes FIFO on-demand). If performance becomes issue (>5s for leaderboard), add `capitoltraders analytics refresh` command to populate closed_trades table. Premature optimization is root of evil.

3. **Win rate definition: all trades or profitable-only denominator?**
   - What we know: Standard definition is (winning_trades / total_trades) * 100.
   - What's unclear: Should we count break-even trades (exactly 0% return) as wins, losses, or exclude?
   - Recommendation: Use `> 0` for win (standard), `<= 0` for loss. Break-even is not a win. Document in CLI help text.

4. **Minimum holding period for annualized return display?**
   - What we know: <30 day periods produce unreliable annualized returns.
   - What's unclear: Should we hide annualized_return column, show NULL, show absolute return instead, or warn user?
   - Recommendation: Show `NULL` in annualized_return for <30 day holds. Add CLI flag `--min-holding-days 30` to filter leaderboard. Document that short-term trades excluded from annualized ranking.

## Sources

### Primary (HIGH confidence)
- [SQLite Window Functions Official Documentation](https://sqlite.org/windowfunctions.html) - PERCENT_RANK, RANK, DENSE_RANK syntax and behavior
- [SQLite PERCENT_RANK Tutorial](https://www.sqlitetutorial.net/sqlite-window-functions/sqlite-percent_rank/) - Practical examples of percentile calculation
- [Holding Period Return Formula - Wall Street Prep](https://www.wallstreetprep.com/knowledge/holding-period-return-hpr/) - Standard financial calculation formulas
- [Alpha Calculation - Corporate Finance Institute](https://corporatefinanceinstitute.com/resources/career-map/sell-side/capital-markets/alpha/) - Benchmark-adjusted return calculation

### Secondary (MEDIUM confidence)
- [Annualized Return Calculation - Nasdaq](https://www.nasdaq.com/articles/how-calculate-annualized-holding-period-return-2016-02-21) - Formula: (1+r)^(1/years) - 1
- [SQL Aggregate Functions and GROUP BY Guide](https://www.sheetinsights.com/2026/01/sql-aggregate-functions-and-group-by-power-up-your-reporting.html) - Performance optimization for grouping queries
- [Portfolio Performance Metrics - Agnifolio](https://agnifolio.com/blog/portfolio-performance-analytics-essential-metrics) - Industry standard metrics overview

### Tertiary (LOW confidence)
- [RustQuant GitHub](https://github.com/avhz/RustQuant) - Rust quantitative finance library (not needed for this phase, but reference for future advanced metrics)
- [Finalytics GitHub](https://github.com/Nnamdi-sys/finalytics) - Rust portfolio optimization library (overkill for current requirements)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - No new dependencies needed, all functionality available in rusqlite + chrono + stdlib
- Architecture: HIGH - Clear separation between analytics module (pure calculation) and DB layer (aggregation/ranking)
- Pitfalls: MEDIUM-HIGH - FIFO matching and annualization edge cases well-documented in financial literature, but options/benchmark NULL handling needs testing

**Research date:** 2026-02-15
**Valid until:** 60 days (stable domain - financial formulas don't change, SQLite window functions stable since 2018)
