# Requirements - Yahoo Finance Price Enrichment (v1.0)

**Milestone:** Yahoo Finance Price Enrichment & Portfolio Tracking
**Created:** 2026-02-09

## Enrichment

- [ ] **REQ-E1**: Historical trade-date price lookup
  - Fetch adjusted close price from Yahoo Finance for each trade's `tx_date`
  - Handle weekends/holidays by using nearest prior trading day's close
  - Store as `trade_date_price REAL` in trades table
  - Use `tx_date` (actual trade date), not `pub_date` (disclosure date)
  - Skip trades with null/missing ticker symbols

- [ ] **REQ-E2**: Current price per ticker
  - Fetch latest adjusted close price for each unique ticker
  - Store as `current_price REAL` in trades table
  - Track freshness via `price_enriched_at TEXT` (ISO 8601) column
  - Deduplicate: fetch once per ticker, apply to all trades with that ticker

- [ ] **REQ-E3**: Ticker validation and batch processing
  - Validate ticker exists in Yahoo Finance before historical lookup
  - Mark invalid/delisted tickers as unenrichable (don't retry on future runs)
  - Batch by unique ticker (1000 trades -> ~200 tickers)
  - Resume after failure: skip already-enriched trades on re-run
  - Rate limit: sequential with 300ms jittered delay, max 3-5 concurrent
  - Circuit breaker: trip after N consecutive failures, resume with backoff
  - Log enrichment summary (success/fail/skip counts)

- [ ] **REQ-E4**: Trade value estimation
  - Parse Capitol Trades dollar range strings (e.g., "$15,001 - $50,000") into numeric bounds
  - Estimate share count: `shares = range_midpoint / trade_date_price`
  - Round to integer shares (no fractional for common stock)
  - Store as `estimated_shares INTEGER` and `estimated_value REAL` in trades table
  - Validate: `estimated_shares * trade_date_price` should fall within original range
  - Skip estimation when trade_date_price is unavailable

## Portfolio

- [ ] **REQ-P1**: Net position per politician per ticker (FIFO)
  - Calculate running balance of shares per politician per ticker
  - FIFO accounting: first shares bought are first shares sold
  - Handle transaction types: buy (add), sell (subtract), exchange, receive
  - Use `estimated_shares` from REQ-E4 for share counts
  - Store in `positions` table: politician_id, ticker, shares_held, cost_basis, last_updated
  - Positions should never go negative (log warning if data suggests it)

- [ ] **REQ-P2**: Unrealized P&L per position
  - Calculate: `(current_price - avg_cost_basis) * shares_held`
  - Requires current price (REQ-E2) and net position (REQ-P1)
  - Display as both dollar amount and percentage
  - Mark as stale if `price_enriched_at` is older than configurable threshold

- [ ] **REQ-P3**: Realized P&L from closed positions
  - Track cost basis of sold shares using FIFO matching
  - Calculate: `(sell_price - buy_cost_basis) * shares_sold` per matched lot
  - Accumulate across all closed lots for total realized P&L per position
  - Store in `realized_pnl` table or as columns on positions table

- [ ] **REQ-P4**: Option trade classification
  - Classify trades as stock vs option (call/put) using existing `asset_type` field
  - Options excluded from FIFO position calculation (different mechanics)
  - Display options separately in portfolio output with "valuation deferred" note
  - Track option trades count per politician for completeness metrics

## Infrastructure

- [ ] **REQ-I1**: Schema migration (v2)
  - Add columns to trades table: `trade_date_price`, `current_price`, `price_enriched_at`, `estimated_shares`, `estimated_value`
  - Create `positions` table for net position tracking
  - Use existing PRAGMA user_version migration pattern (increment to v2)
  - Auto-detect existing DB and add missing columns on first enrich-prices run

- [ ] **REQ-I2**: Yahoo Finance client integration
  - Add `yahoo_finance_api = "4.1.0"` and `time = "0.3"` dependencies to capitoltraders_lib
  - Create YahooClient wrapper with chrono-to-time conversion helpers
  - Reuse existing reqwest 0.12 / tokio runtime
  - No authentication required (unofficial API)

- [ ] **REQ-I3**: `enrich-prices` CLI subcommand
  - New subcommand: `capitoltraders enrich-prices --db <path>`
  - Required: `--db` flag (DB-only operation)
  - Optional: `--batch-size` (default 50), `--force` (re-enrich already-enriched)
  - Display progress: ticker count, success/fail/skip, elapsed time
  - Exit code 0 on success (even with partial failures), non-zero on total failure

- [ ] **REQ-I4**: `portfolio` CLI subcommand
  - New subcommand: `capitoltraders portfolio --db <path>`
  - Required: `--db` flag
  - Filters: `--politician`, `--party`, `--state`, `--ticker`
  - Show: ticker, shares held, avg cost basis, current price, unrealized P&L, P&L %
  - Separate section for option positions (classification only)
  - Respects `--output` global flag (table, JSON, CSV, markdown, XML)

## Acceptance Criteria

- Enrichment processes 200 unique tickers in under 2 minutes (sequential with rate limiting)
- Partial failures do not abort the batch; failed tickers are logged and skippable
- Re-running enrich-prices skips already-enriched trades (resumable)
- Portfolio positions calculated correctly for buy/sell sequences (FIFO verified)
- Option trades classified separately and excluded from stock position calculations
- Schema migration is backwards-compatible (existing DB works after upgrade)
- All new code has unit tests; enrichment pipeline has wiremock integration tests

## Dependencies

```
REQ-I1 (Schema) ── foundation for all other requirements
REQ-I2 (Yahoo Client) ── required by REQ-E1, REQ-E2, REQ-E3
REQ-E1 (Historical Price) ── required by REQ-E4, REQ-P1, REQ-P3
REQ-E2 (Current Price) ── required by REQ-P2
REQ-E3 (Ticker Validation) ── required by REQ-E1, REQ-E2
REQ-E4 (Trade Value) ── required by REQ-P1 (share counts)
REQ-P1 (Net Position) ── required by REQ-P2, REQ-P3
REQ-I3 (enrich-prices CLI) ── integrates REQ-E1 through REQ-E4
REQ-I4 (portfolio CLI) ── integrates REQ-P1 through REQ-P4
```

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| REQ-E1 | Phase 4 | Pending |
| REQ-E2 | Phase 4 | Pending |
| REQ-E3 | Phase 3 | Pending |
| REQ-E4 | Phase 3 | Pending |
| REQ-P1 | Phase 5 | Pending |
| REQ-P2 | Phase 5 | Pending |
| REQ-P3 | Phase 5 | Pending |
| REQ-P4 | Phase 5 | Pending |
| REQ-I1 | Phase 1 | Pending |
| REQ-I2 | Phase 2 | Pending |
| REQ-I3 | Phase 4 | Pending |
| REQ-I4 | Phase 6 | Pending |

**Coverage:** 12/12 requirements mapped (100%)

---
*Generated from research: STACK.md, FEATURES.md, ARCHITECTURE.md, PITFALLS.md*
