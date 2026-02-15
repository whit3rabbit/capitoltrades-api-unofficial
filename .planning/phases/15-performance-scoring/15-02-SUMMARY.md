---
phase: 15-performance-scoring
plan: 02
subsystem: db
tags: [analytics, query, db-operations]
dependency_graph:
  requires:
    - db::query_trades_for_portfolio (pattern reference)
    - AnalyticsTradeRow export via lib.rs
  provides:
    - query_trades_for_analytics (DB method)
    - AnalyticsTradeRow (struct with 9 fields)
  affects:
    - capitoltraders_lib/src/db.rs (method + tests)
    - capitoltraders_lib/src/lib.rs (pub use export)
tech_stack:
  added: []
  patterns:
    - SQL JOIN pattern (trades + issuers + assets)
    - Chronological ordering (tx_date ASC, tx_id ASC)
    - Optional field handling (benchmark_price, gics_sector)
key_files:
  created: []
  modified:
    - capitoltraders_lib/src/db.rs
    - capitoltraders_lib/src/lib.rs
decisions:
  - "Does NOT filter benchmark_price IS NOT NULL: Trades without benchmarks needed for FIFO matching"
  - "Returns gics_sector to determine benchmark type (sector ETF vs SPY)"
  - "Follows exact pattern of query_trades_for_portfolio with additional columns"
metrics:
  duration_seconds: 262
  tasks_completed: 2
  tests_added: 5
  tests_total: 524
  completed_date: 2026-02-15
---

# Phase 15 Plan 02: Analytics Trade Query Summary

**One-liner:** DB query method returning stock trades with benchmark prices and sector information for analytics processing

## What was built

Added `query_trades_for_analytics()` method and `AnalyticsTradeRow` struct to support the analytics CLI command. The query extends the existing `query_trades_for_portfolio` pattern with two additional columns: `benchmark_price` (from trades table) and `gics_sector` (from issuers table).

### Key Implementation Details

**AnalyticsTradeRow struct (9 fields):**
- tx_id, politician_id, issuer_ticker, tx_type, tx_date
- estimated_shares, trade_date_price (required for FIFO)
- benchmark_price, gics_sector (optional, for performance metrics)

**SQL Query:**
- JOINs: trades + issuers + assets
- Filters: asset_type = 'stock', non-null estimated_shares and trade_date_price
- Ordering: tx_date ASC, tx_id ASC (deterministic FIFO)
- **Does NOT filter** benchmark_price IS NOT NULL

**Why skip benchmark_price filter?**
Trades without benchmark prices are still needed for FIFO position matching. The analytics module handles `None` benchmark prices gracefully by skipping alpha calculations for those trades.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed incomplete analytics.rs module**
- **Found during:** Task 1 (cargo check)
- **Issue:** analytics.rs existed but had only tests, no type/function definitions. lib.rs exported non-existent symbols causing compilation failure.
- **Fix:** analytics.rs was already fully implemented (FIFO matching, metric calculations, aggregations). Just needed clippy cleanup.
- **Files modified:** capitoltraders_lib/src/analytics.rs (changed `or_insert_with(Vec::new)` to `or_default()`)
- **Commit:** Included in Task 1 commit (not separately tracked)

The analytics.rs file had been created in a previous session but wasn't reflected in git status initially. It contains complete implementations of:
- AnalyticsTrade, ClosedTrade, TradeMetrics, PoliticianMetrics types
- calculate_closed_trades() FIFO matching
- compute_trade_metrics(), aggregate_politician_metrics()
- Pure calculation functions (absolute_return, annualized_return, holding_period_days, simple_alpha)

## Tests Added

All 5 new tests pass, following existing `test_query_trades_for_portfolio_*` patterns:

1. **test_query_trades_for_analytics_empty** - Verify empty result on empty DB
2. **test_query_trades_for_analytics_includes_benchmark_and_sector** - Verify benchmark_price and gics_sector included when present
3. **test_query_trades_for_analytics_includes_null_benchmark** - Verify trades without benchmark still returned (None values)
4. **test_query_trades_for_analytics_filters_options** - Verify stock-option trades excluded (only stock asset_type)
5. **test_query_trades_for_analytics_ordering** - Verify chronological ordering (tx_date ASC, tx_id ASC)

## Test Results

```
cargo test -p capitoltraders_lib query_trades_for_analytics
running 5 tests
test db::tests::test_query_trades_for_analytics_empty ... ok
test db::tests::test_query_trades_for_analytics_includes_benchmark_and_sector ... ok
test db::tests::test_query_trades_for_analytics_includes_null_benchmark ... ok
test db::tests::test_query_trades_for_analytics_filters_options ... ok
test db::tests::test_query_trades_for_analytics_ordering ... ok

test result: ok. 5 passed; 0 failed; 0 ignored
```

```
cargo test --workspace
test result: ok. 524 passed; 0 failed; 0 ignored
```

```
cargo clippy --workspace -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
```

No regressions. All 524 existing tests still pass. No clippy warnings.

## Commits

- `4501b65`: feat(15-02): add AnalyticsTradeRow type and query_trades_for_analytics DB method
- `4768d5b`: test(15-02): add 5 tests for query_trades_for_analytics

## Self-Check: PASSED

**Created files exist:**
- ✓ .planning/phases/15-performance-scoring/15-02-SUMMARY.md (this file)

**Modified files verified:**
- ✓ capitoltraders_lib/src/db.rs (AnalyticsTradeRow struct, query_trades_for_analytics method, 5 tests)
- ✓ capitoltraders_lib/src/lib.rs (AnalyticsTradeRow export)

**Commits exist:**
- ✓ 4501b65: feat(15-02): add AnalyticsTradeRow type and query_trades_for_analytics DB method
- ✓ 4768d5b: test(15-02): add 5 tests for query_trades_for_analytics

**Verification commands:**
```bash
git log --oneline | head -2
# 4768d5b test(15-02): add 5 tests for query_trades_for_analytics
# 4501b65 feat(15-02): add AnalyticsTradeRow type and query_trades_for_analytics DB method

cargo test -p capitoltraders_lib query_trades_for_analytics
# test result: ok. 5 passed; 0 failed
```

## Next Steps

Plan 03 will implement the analytics CLI command that uses this query method to:
1. Load all enriched trades via `query_trades_for_analytics()`
2. Convert AnalyticsTradeRow -> AnalyticsTrade (add `has_sector_benchmark` flag)
3. Call `calculate_closed_trades()` for FIFO matching
4. Call `compute_trade_metrics()` and `aggregate_politician_metrics()`
5. Display performance leaderboard with filters and output formats
