---
phase: 03-ticker-validation-trade-value-estimation
plan: 01
subsystem: pricing-calculation-and-db-access
tags: [price-enrichment, share-estimation, database, calculation]
dependency_graph:
  requires: [02-01-yahoo-finance-client]
  provides: [pricing-primitives, price-enrichment-db-layer]
  affects: [capitoltraders_lib]
tech_stack:
  added: [pricing-module]
  patterns: [range-parsing, share-estimation, join-queries, enrichment-tracking]
key_files:
  created:
    - capitoltraders_lib/src/pricing.rs
  modified:
    - capitoltraders_lib/src/lib.rs
    - capitoltraders_lib/src/db.rs
decisions: []
metrics:
  duration_minutes: 5
  tasks_completed: 2
  tests_added: 23
  files_created: 1
  files_modified: 2
  completed_date: 2026-02-11
---

# Phase 03 Plan 01: Pricing Calculation and DB Access Summary

**One-liner:** Dollar range parsing and share estimation primitives with DB access layer for price enrichment pipeline (no ticker validation or batch processing)

## What Was Built

Created the calculation layer and data access layer that Phase 4's enrichment pipeline will orchestrate:

1. **Pricing Module** (`capitoltraders_lib/src/pricing.rs`):
   - `TradeRange` struct with midpoint calculation
   - `parse_trade_range()` - extracts dollar ranges from size_range_low/high (returns None when either bound missing)
   - `ShareEstimate` struct for estimation results
   - `estimate_shares()` - calculates shares from range midpoint and historical price
   - Edge case handling: None bounds, zero/negative prices, inverted ranges, floating-point sanity checks
   - 13 comprehensive unit tests

2. **DB Price Enrichment Operations** (`capitoltraders_lib/src/db.rs`):
   - `PriceEnrichmentRow` struct - carries ticker, date, and range data for enrichment
   - `count_unenriched_prices()` - count trades needing price data
   - `get_unenriched_price_trades()` - fetch batch of trades for enrichment (with optional limit)
   - `update_trade_prices()` - atomically store price, shares, value, and enrichment timestamp
   - All three methods JOIN issuers table to access `issuer_ticker` (ticker lives on issuers, not trades)
   - Always sets `price_enriched_at` for resumability (even when price is None for invalid tickers)
   - 10 comprehensive unit tests

## Test Coverage

Added 23 tests (13 pricing + 10 DB):

**Pricing tests:**
- Normal case calculations (small/large ranges, various prices)
- Edge cases (None bounds, zero/negative price, inverted ranges)
- Validation checks (midpoint calculation, value within bounds)

**DB tests:**
- Empty DB handling
- Ticker exclusion (NULL ticker trades excluded from enrichment queue)
- Already-enriched exclusion (price_enriched_at IS NOT NULL)
- Limit parameter handling
- Range field propagation from ScrapedTradeDetail
- Storage verification (price, shares, value, timestamp)
- None storage (invalid ticker case still sets price_enriched_at)
- Resumability (enriched trades skipped on re-run)

All tests pass. Total test count: 332 (309 existing + 23 new).

## Key Implementation Details

**parse_trade_range behavior:**
- Requires BOTH size_range_low and size_range_high (no fallback to value column)
- Returns None when either bound missing - estimation requires both bounds for validation
- The `value` field on PriceEnrichmentRow exists for potential future use but is not consumed by parse_trade_range

**estimate_shares validation:**
- Guards against zero/negative prices (division by zero)
- Sanity check: estimated_value must fall within original range bounds
- Should virtually never fail with correct inputs (protects against NaN/Inf edge cases)
- Warning logged (eprintln) when validation fails - will be replaced with proper logging later

**DB JOIN pattern:**
- `issuer_ticker` lives on the `issuers` table, NOT the `trades` table
- All three DB methods JOIN issuers: `JOIN issuers i ON t.issuer_id = i.issuer_id`
- Forgetting this JOIN causes column-not-found errors

**Resumability:**
- `update_trade_prices()` always sets `price_enriched_at` to datetime('now')
- Even when price is None (invalid ticker), trade is marked as processed
- Prevents re-processing on subsequent runs (REQ-E3 from research)

## Deviations from Plan

None - plan executed exactly as written.

## Integration Points

**Upstream dependencies:**
- Phase 1 (01-01): Uses price columns added in migration v2 (trade_date_price, estimated_shares, estimated_value, price_enriched_at)
- Phase 2 (02-01): Phase 4 will use YahooClient to fetch prices (this plan only provides calculation primitives)

**Downstream consumers:**
- Phase 4 will use these primitives to build the enrichment pipeline:
  1. Call `db.get_unenriched_price_trades(limit)` to fetch batch
  2. For each row: call `yahoo.get_price_on_date(ticker, date)` to fetch price
  3. If price is Some: call `parse_trade_range()` then `estimate_shares()`
  4. Call `db.update_trade_prices()` to store results (or None for invalid tickers)

**What this plan does NOT do:**
- Does NOT validate tickers (Phase 4 will discover invalid tickers when YahooClient returns Ok(None))
- Does NOT run batch processing or enrichment pipeline (Phase 4 responsibility)
- Does NOT handle concurrency, rate limiting, or circuit breaking (Phase 4 concerns)

## Files Changed

**Created:**
- `capitoltraders_lib/src/pricing.rs` (207 lines) - range parsing and share estimation

**Modified:**
- `capitoltraders_lib/src/lib.rs` - added pricing module registration and exports, added PriceEnrichmentRow export
- `capitoltraders_lib/src/db.rs` - added PriceEnrichmentRow struct, 3 new methods, 10 tests (315 lines added)

## Commits

- `5f68afc` - feat(03-01): create pricing module with dollar range parsing and share estimation
- `a067919` - feat(03-01): add price enrichment DB operations and tests

## Self-Check

### Created Files Verification

```bash
[ -f "capitoltraders_lib/src/pricing.rs" ] && echo "FOUND: capitoltraders_lib/src/pricing.rs" || echo "MISSING: capitoltraders_lib/src/pricing.rs"
```

**Result:** FOUND: capitoltraders_lib/src/pricing.rs

### Commits Verification

```bash
git log --oneline --all | grep -q "5f68afc" && echo "FOUND: 5f68afc" || echo "MISSING: 5f68afc"
git log --oneline --all | grep -q "a067919" && echo "FOUND: a067919" || echo "MISSING: a067919"
```

**Result:**
- FOUND: 5f68afc
- FOUND: a067919

### Exports Verification

```bash
grep "pub use pricing" capitoltraders_lib/src/lib.rs
grep "PriceEnrichmentRow" capitoltraders_lib/src/lib.rs
```

**Result:**
- `pub use pricing::{estimate_shares, parse_trade_range, ShareEstimate, TradeRange};`
- `DbTradeRow, IssuerStatsRow, PoliticianStatsRow, PriceEnrichmentRow,`

### JOIN Verification

```bash
grep -A 5 "count_unenriched_prices" capitoltraders_lib/src/db.rs | grep "JOIN issuers"
grep -A 10 "get_unenriched_price_trades" capitoltraders_lib/src/db.rs | grep "JOIN issuers"
```

**Result:**
- count_unenriched_prices: `JOIN issuers i ON t.issuer_id = i.issuer_id`
- get_unenriched_price_trades: `JOIN issuers i ON t.issuer_id = i.issuer_id`

## Self-Check: PASSED

All files created, all commits exist, all exports present, all JOINs verified.

## Next Steps

Phase 4 (Price Enrichment Pipeline) will orchestrate these primitives:
1. Build concurrent enrichment pipeline using Semaphore + JoinSet + mpsc pattern from Phase 2
2. Integrate YahooClient for price lookups
3. Handle invalid tickers (YahooClient returns Ok(None))
4. Implement batch processing with configurable concurrency
5. Add circuit breaker for API failures
6. Create CLI subcommand `enrich-prices`
