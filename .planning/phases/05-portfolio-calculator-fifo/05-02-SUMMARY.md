---
phase: 05-portfolio-calculator-fifo
plan: 02
subsystem: portfolio
tags: [database, fifo, position-tracking, unrealized-pnl, sql]
dependency_graph:
  requires:
    - phase: 05-01
      provides: FIFO portfolio calculator and Position types
  provides:
    - DB operations for querying trades for FIFO input
    - Bulk upsert of calculated positions to positions table
    - Portfolio query with unrealized P&L calculation
    - Option trade counting for reporting
  affects: [06-cli-portfolio-command]
tech_stack:
  added: []
  patterns: [dynamic-where-clauses, sql-subquery-for-latest-price, epsilon-filtering]
key_files:
  created: []
  modified:
    - capitoltraders_lib/src/db.rs
    - capitoltraders_lib/src/lib.rs
decisions:
  - "Use SQL subquery with ORDER BY DESC LIMIT 1 pattern for current_price lookup"
  - "Filter closed positions by default (shares_held > 0.0001) but allow include_closed flag"
  - "Compute unrealized P&L in Rust rather than SQL for clarity and type safety"
  - "Use separate query path for politician_id filter in count_option_trades to avoid lifetime issues"
metrics:
  duration: 5 min
  completed: 2026-02-11T03:13:00Z
  tasks: 2
  files: 2
  tests: 12
---

# Phase 05 Plan 02: Portfolio DB Operations Summary

**One-liner:** Database operations for FIFO portfolio calculator with stock-only filtering, position upserts, and unrealized P&L calculation via current price subqueries.

## What Was Built

Added four DB methods that bridge the FIFO portfolio calculator (from Plan 01) to SQLite storage, enabling portfolio calculation from persisted trades and materialization of positions with unrealized P&L metrics.

### Core DB Methods

1. **query_trades_for_portfolio() -> Vec<TradeFIFO>**
   - Queries trades with JOINs on issuers (for ticker) and assets (for type filtering)
   - Filters to stock-only trades (`asset_type = 'stock'`)
   - Excludes un-enriched trades (NULL estimated_shares or trade_date_price)
   - Orders chronologically (`tx_date ASC, tx_id ASC`) for deterministic FIFO processing
   - Returns TradeFIFO structs directly consumable by calculate_positions

2. **upsert_positions(HashMap<(String, String), Position>) -> usize**
   - Bulk upserts all positions (open and closed) to positions table
   - Uses ON CONFLICT(politician_id, issuer_ticker) for idempotent updates
   - Extracts shares_held, avg_cost_basis, and realized_pnl from Position
   - Returns count of upserted rows for progress reporting

3. **get_portfolio(PortfolioFilter) -> Vec<PortfolioPosition>**
   - Queries positions table with dynamic WHERE clauses (politician_id, ticker, party, state)
   - Subquery pattern: `(SELECT t2.current_price FROM trades t2 JOIN issuers i2 ... ORDER BY price_enriched_at DESC LIMIT 1)`
   - Computes unrealized_pnl = (current_price - cost_basis) * shares_held in Rust
   - Computes unrealized_pnl_pct = ((current_price - cost_basis) / cost_basis) * 100.0
   - Computes current_value = current_price * shares_held
   - Filters closed positions by default (shares_held > 0.0001), overridable with include_closed flag
   - Orders by position size (shares_held * cost_basis DESC) for largest positions first

4. **count_option_trades(Option<&str>) -> i64**
   - Counts trades where asset_type is NOT 'stock' and NOT 'unknown'
   - Optional politician_id filter for per-politician option trade reporting
   - Used to separate stock vs option trade statistics

### New Types

- **PortfolioPosition**: Display-layer struct with unrealized P&L fields (unrealized_pnl, unrealized_pnl_pct, current_price, current_value, price_date)
- **PortfolioFilter**: Dynamic filter struct for get_portfolio (politician_id, ticker, party, state, include_closed)

## Key Implementation Patterns

### SQL Subquery for Latest Price
```sql
(SELECT t2.current_price
 FROM trades t2
 JOIN issuers i2 ON t2.issuer_id = i2.issuer_id
 WHERE i2.issuer_ticker = p.issuer_ticker
   AND t2.current_price IS NOT NULL
 ORDER BY t2.price_enriched_at DESC
 LIMIT 1) as current_price
```
This pattern finds the most recent current_price for each ticker. The same subquery pattern is used for price_date.

### Dynamic WHERE Clause Building
Follows existing pattern from query_trades: build Vec of clauses, Vec of params, then join with AND. Allows flexible filtering without SQL injection.

### Unrealized P&L in Rust
Computed in row mapping rather than SQL for clarity and to avoid NULL handling complexity in database:
```rust
let unrealized_pnl = current_price.map(|price| (price - cost_basis) * shares_held);
```

## Test Coverage

12 comprehensive test cases (6 per task):

### Task 1 Tests (query_trades_for_portfolio + upsert_positions)
1. Empty DB returns empty vec
2. Filters option trades (only stock trades returned)
3. Ordering verification (tx_date ASC, tx_id ASC)
4. Skips unenriched trades (NULL estimated_shares)
5. Basic upsert (new position inserted)
6. Update existing (ON CONFLICT updates values)

### Task 2 Tests (get_portfolio + count_option_trades)
1. Empty positions table returns empty vec
2. With unrealized P&L (verifies calculation correctness)
3. Filters closed positions (shares_held = 0 excluded by default)
4. Filter by politician_id (dynamic WHERE clause)
5. Count option trades (excludes stock and unknown)
6. Missing current_price (unrealized_pnl = None when no price available)

All 12 new tests pass. Full workspace test suite: 306 tests (294 existing + 12 new).

## Integration Points

- **Input**: TradeFIFO from portfolio module (Plan 01)
- **Output**: Vec<TradeFIFO> for calculate_positions input
- **Storage**: positions table with shares_held, cost_basis, realized_pnl
- **Lookup**: current_price from trades table for unrealized P&L
- **Exports**: PortfolioPosition and PortfolioFilter added to capitoltraders_lib public API

## Task Commits

1. **Task 1: query_trades_for_portfolio and upsert_positions** - `bb45757`
   - query_trades_for_portfolio with stock filtering and chronological ordering
   - upsert_positions with ON CONFLICT update
   - 6 tests covering filtering, ordering, and upsert behavior

2. **Task 2: get_portfolio and count_option_trades** - `61a756a`
   - PortfolioPosition struct with unrealized P&L fields
   - get_portfolio with dynamic filters and subquery pattern
   - count_option_trades for option trade reporting
   - 6 tests covering filtering, P&L calculation, and option counting

## Deviations from Plan

None - plan executed exactly as written.

## Next Phase Readiness

Portfolio DB operations complete. Ready for Phase 05 Plan 03 (CLI portfolio command integration).

**Dependency chain:**
- Plan 01 (FIFO calculator) provides Position and calculate_positions
- Plan 02 (this plan) provides DB operations to/from calculator
- Plan 03 will wire CLI command to DB + calculator

---
*Phase: 05-portfolio-calculator-fifo*
*Completed: 2026-02-11*
