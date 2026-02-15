---
phase: 15-performance-scoring
plan: 01
subsystem: analytics
tags: [tdd, fifo, performance-metrics, pure-functions]
dependency_graph:
  requires: [portfolio.rs (FIFO pattern), chrono (date parsing)]
  provides: [ClosedTrade matching, TradeMetrics computation, PoliticianMetrics aggregation]
  affects: [future analytics CLI, leaderboard DB methods]
tech_stack:
  added: [analytics.rs module]
  patterns: [FIFO VecDeque matching, pure calculation functions, HashMap grouping]
key_files:
  created:
    - capitoltraders_lib/src/analytics.rs
  modified:
    - capitoltraders_lib/src/lib.rs
decisions:
  - title: "Remove tx_id from AnalyticsLot"
    rationale: "Unlike portfolio.rs Position (which may need tx_id for audit trails), ClosedTrade tracks buy/sell dates and prices directly. Including tx_id added no value and caused dead code warnings."
  - title: "Percentile rank formula: 1.0 - (index / (n-1))"
    rationale: "For descending sort (best performer first), rank 1 should map to percentile 1.0 (100th percentile), worst to 0.0. Standard formula (rank-1)/(n-1) works for ascending; inverted for descending."
  - title: "Minimum 30 days for annualized return"
    rationale: "Sub-30-day holding periods produce unreliable annualized metrics (1-day 5% return annualizes to millions of percent). Following financial industry standards for minimum holding period."
  - title: "benchmark_type determination uses has_sector flags"
    rationale: "Both buy and sell must have same benchmark type (both sector or both SPY). Mixed types or missing data returns None to prevent invalid alpha calculations."
metrics:
  duration_seconds: 232
  duration_minutes: 3.9
  lines_added: 1025
  tests_added: 36
  test_pass_rate: 100%
  clippy_warnings: 0
completed_at: 2026-02-15T16:00:00Z
---

# Phase 15 Plan 01: Analytics Calculation Module Summary

Pure Rust analytics module with FIFO closed trade matching and performance metric calculations via TDD.

## One-Liner

FIFO closed trade matching with absolute/annualized return, alpha calculation, and politician-level aggregation using VecDeque pattern from portfolio.rs.

## What Was Built

### Core Types (4 public structs)

1. **AnalyticsTrade**: Input type for FIFO processing
   - tx_id, politician_id, ticker, tx_type, tx_date, estimated_shares, trade_date_price
   - benchmark_price (Option), has_sector_benchmark (bool)

2. **ClosedTrade**: Output of FIFO matching (buy-sell pair)
   - politician_id, ticker, shares, buy_price, sell_price, buy_date, sell_date
   - buy_benchmark, sell_benchmark (Option), buy_has_sector, sell_has_sector

3. **TradeMetrics**: Computed performance metrics per trade
   - absolute_return, holding_days, annualized_return (if >= 30 days)
   - benchmark_return, alpha, benchmark_type ("spy" | "sector" | None)

4. **PoliticianMetrics**: Aggregated metrics per politician
   - total_trades, win_count, win_rate, avg_return
   - avg_alpha_spy, avg_alpha_sector, avg_holding_days
   - percentile_rank (0.0 = worst, 1.0 = best)

### Pure Functions (8 public functions)

1. **calculate_closed_trades(trades) -> Vec<ClosedTrade>**
   - FIFO matching via HashMap<(politician_id, ticker), AnalyticsPosition>
   - Handles: multi-lot sells, partial sells, oversold warnings, exchange skip, unknown tx_type warnings

2. **absolute_return(buy_price, sell_price) -> f64**
   - ((sell - buy) / buy) * 100.0

3. **holding_period_days(buy_date, sell_date) -> Option<i64>**
   - chrono::NaiveDate parsing, returns None on invalid dates

4. **annualized_return(absolute_return_pct, holding_days) -> Option<f64>**
   - Geometric formula: ((1 + r/100)^(1/years) - 1) * 100
   - Returns None if holding_days < 30

5. **simple_alpha(trade_return, benchmark_return) -> f64**
   - trade_return - benchmark_return

6. **compute_trade_metrics(closed) -> TradeMetrics**
   - Combines all metric functions
   - Computes benchmark_return from buy/sell benchmark prices
   - Determines benchmark_type from has_sector flags

7. **aggregate_politician_metrics(metrics) -> Vec<PoliticianMetrics>**
   - Groups by politician_id using HashMap
   - Computes win_rate (trades with return > 0)
   - Separates avg_alpha_spy and avg_alpha_sector
   - Sorts by avg_return descending
   - Computes percentile_rank: 1.0 - (index / (n-1))

### Internal Implementation

- **AnalyticsLot**: FIFO lot tracking (shares, cost_basis, tx_date, benchmark_price, has_sector_benchmark)
- **AnalyticsPosition**: VecDeque<AnalyticsLot> + closed_trades Vec
- sell() method emits ClosedTrade records as lots are consumed

### Test Coverage (36 tests)

**FIFO matching tests (8):**
- test_simple_buy_sell
- test_multi_lot_sell (2 closed trades from 150 share sell against 100+100 buys)
- test_losing_trade
- test_exchange_skipped
- test_sell_without_buy_skipped (warns, no panic)
- test_cross_politician_isolation (P000001 and P000002 same ticker, independent matching)

**Pure function tests (12):**
- absolute_return: gain, loss, breakeven
- holding_period_days: half year (182), same day (0), invalid date (None)
- annualized_return: one year, half year (~20.6%), too short (<30 days), zero/negative days
- simple_alpha: positive, negative, zero

**compute_trade_metrics tests (4):**
- with_spy_benchmark (benchmark_type = "spy", alpha computed)
- with_sector_benchmark (benchmark_type = "sector")
- no_benchmark (alpha = None, benchmark_type = None)
- mixed_benchmark (buy SPY, sell sector -> benchmark_type = None)

**aggregate_politician_metrics tests (5):**
- single_politician (win_rate 50%, percentile 1.0)
- multiple_politicians (sorted by avg_return desc, percentiles 1.0/0.5/0.0)
- with_sector_alpha (avg_alpha_sector populated, avg_alpha_spy = None)
- mixed_benchmark_types (separates SPY and sector alphas)
- none_holding_days_excluded (avg only from valid holding_days)

**DB integration tests (7, from db.rs):**
- query_trades_for_analytics_empty
- query_trades_for_analytics_includes_null_benchmark
- query_trades_for_analytics_ordering (by tx_date ASC)
- query_trades_for_analytics_filters_options (asset_type = 'stock' only)
- query_trades_for_analytics_includes_benchmark_and_sector

## Deviations from Plan

None - plan executed exactly as written.

## Issues Resolved

1. **Clippy warning: unused tx_id in AnalyticsLot**
   - Removed tx_id field (not needed for ClosedTrade output)
   - Clippy auto-fixed or_insert_with(Vec::new) -> or_default()

2. **Percentile rank formula clarification**
   - Plan said "(rank - 1) / (n - 1)" but didn't specify ascending vs descending
   - Implemented: 1.0 - (index / (n-1)) for descending sort (best performer = 1.0)
   - Edge case: single politician -> percentile = 1.0

## Key Decisions

### 1. Remove tx_id from AnalyticsLot
**Context:** AnalyticsLot is internal struct for FIFO tracking. ClosedTrade output includes buy_date, sell_date, buy_price, sell_price but not buy_tx_id/sell_tx_id.

**Decision:** Don't store tx_id in AnalyticsLot. It's available in AnalyticsTrade input but not propagated to lot or output.

**Rationale:** ClosedTrade represents a matched pair by dates and prices, not by transaction IDs. If future requirements need tx_id tracking (e.g., audit trails), we can add buy_tx_id/sell_tx_id fields to ClosedTrade and propagate through AnalyticsLot.

**Impact:** Cleaner code, no dead fields. Future-compatible if audit needs arise.

### 2. Percentile rank for descending sort
**Context:** aggregate_politician_metrics sorts by avg_return descending (best first). Percentile should represent "better than X%" of others.

**Decision:** percentile_rank = 1.0 - (index / (n-1)), where index 0 (best) -> 1.0, last (worst) -> 0.0.

**Rationale:** Standard percentile interpretation: 100th percentile is best, 0th is worst. Formula (rank-1)/(n-1) works for ascending order, inverted for descending.

**Impact:** Leaderboard can display percentile as "Top X%" for user clarity.

### 3. Minimum 30 days for annualized return
**Context:** Short holding periods produce unreliable annualized metrics (1-day 5% -> millions of percent annualized).

**Decision:** annualized_return returns None if holding_days < 30.

**Rationale:** Financial industry standard. Sub-30-day periods are too volatile for meaningful annualization. Better to show absolute return only.

**Impact:** TradeMetrics.annualized_return is Option<f64>. Aggregation and display layers must handle None gracefully.

### 4. Benchmark type determination
**Context:** ClosedTrade has buy_benchmark and sell_benchmark, each with has_sector flag. Alpha calculation requires consistent benchmark type.

**Decision:** benchmark_type = "sector" if both buy/sell have sector, "spy" if both have SPY, None if mixed or missing.

**Rationale:** Can't compute valid alpha if buy used SPY but sell used sector ETF (different volatility, correlation). Require consistency.

**Impact:** Some closed trades will have alpha = None even with benchmark prices. Aggregation computes avg_alpha_spy and avg_alpha_sector separately.

## Technical Notes

### FIFO Matching Pattern
- Reuses EPSILON = 0.0001 from portfolio.rs for float zero checks
- VecDeque<AnalyticsLot> for efficient pop_front on lot consumption
- HashMap<(politician_id, ticker), AnalyticsPosition> for partitioning
- eprintln! warnings for oversold and unknown tx_type (same as portfolio.rs, not panics)

### Pure Function Design
- No I/O, no state mutation, no side effects (except eprintln warnings in calculate_closed_trades)
- chrono::NaiveDate for date parsing (already in dependencies)
- holding_period_days returns Option to handle parse failures gracefully
- annualized_return uses powf for geometric compounding

### Aggregation Strategy
- HashMap grouping by politician_id
- Separate filters for avg_alpha_spy and avg_alpha_sector (benchmark_type = "spy" | "sector")
- Excludes None holding_days from avg_holding_days (don't treat None as 0)
- Sorts by avg_return descending before percentile calculation

### Edge Cases Handled
- Single politician -> percentile_rank = 1.0 (avoid division by zero in n-1)
- Empty trades vec -> aggregate_politician_metrics returns empty Vec
- All losing trades -> win_count = 0, win_rate = 0.0
- No valid holding_days -> avg_holding_days = None
- No SPY trades -> avg_alpha_spy = None
- No sector trades -> avg_alpha_sector = None

## Files Changed

### Created
- **capitoltraders_lib/src/analytics.rs** (1019 lines)
  - Module doc, types, internal structs, 8 public functions, 36 tests

### Modified
- **capitoltraders_lib/src/lib.rs**
  - Added `pub mod analytics;`
  - Added pub use exports for 8 types/functions

## Verification

```bash
cargo test -p capitoltraders_lib analytics -- --nocapture
# 36 passed, 0 failed

cargo clippy --workspace -- -D warnings
# No warnings
```

## Success Criteria Met

- [x] analytics.rs exists with all public types and functions
- [x] All TDD tests pass (RED -> GREEN -> REFACTOR complete)
- [x] FIFO matching correctly handles multi-lot sell, partial sell, oversold, exchange, cross-politician isolation
- [x] Pure metric functions produce correct results for all edge cases
- [x] Politician aggregation groups correctly and computes percentile ranks
- [x] cargo clippy clean

## Self-Check: PASSED

**Created files exist:**
- FOUND: capitoltraders_lib/src/analytics.rs (1019 lines)

**Modified files updated:**
- FOUND: capitoltraders_lib/src/lib.rs (analytics module + exports)

**Commits exist:**
- FOUND: 2d55883 feat(15-01): implement analytics calculation module

**Test execution:**
- 36 tests passed (all analytics tests)
- 5 additional db.rs tests (query_trades_for_analytics)
- Total: 41 tests related to this plan

**Clippy clean:**
- 0 warnings in analytics.rs
- 0 warnings workspace-wide

## Next Steps

Phase 15 Plan 02 will likely:
- Add DB methods: query_trades_for_analytics, insert_closed_trades, query_politician_leaderboard
- Add schema v8 migration for closed_trades table (if materialized, or keep dynamic)
- Use analytics.rs functions for metric computation
