# Roadmap: Yahoo Finance Price Enrichment

## Overview

This milestone extends Capitol Traders with Yahoo Finance market data integration and portfolio tracking. Starting with schema migration and Yahoo client integration, we'll enrich trades with historical and current prices, estimate trade values, calculate per-politician FIFO positions, and deliver unrealized/realized P&L. The journey concludes with a portfolio CLI command that shows current holdings with profit/loss visibility.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Schema Migration & Data Model** - Add price columns to trades table, create positions table
- [x] **Phase 2: Yahoo Finance Client Integration** - Integrate yahoo_finance_api crate with time/chrono conversion
- [x] **Phase 3: Ticker Validation & Trade Value Estimation** - Validate tickers, estimate shares from dollar ranges
- [ ] **Phase 4: Price Enrichment Pipeline** - Batch fetch historical and current prices with rate limiting
- [ ] **Phase 5: Portfolio Calculator (FIFO)** - Calculate net positions and realized/unrealized P&L
- [ ] **Phase 6: CLI Commands & Output** - Add enrich-prices and portfolio subcommands with output formatting

## Phase Details

### Phase 1: Schema Migration & Data Model
**Goal**: Database schema supports price storage and portfolio tracking
**Depends on**: Nothing (first phase)
**Requirements**: REQ-I1
**Success Criteria** (what must be TRUE):
  1. trades table has trade_date_price, current_price, price_enriched_at, estimated_shares, estimated_value columns
  2. positions table exists with politician_id, ticker, shares_held, cost_basis, last_updated columns
  3. Migration from v1 to v2 succeeds on existing DB without data loss
  4. DbTradeRow struct includes new price fields
  5. Migration is idempotent (re-running on migrated DB is safe)
**Plans:** 1 plan

Plans:
- [x] 01-01-PLAN.md -- Schema migration v2 (price columns + positions table + DbTradeRow update + tests)

### Phase 2: Yahoo Finance Client Integration
**Goal**: System can fetch historical and current prices from Yahoo Finance
**Depends on**: Phase 1
**Requirements**: REQ-I2
**Success Criteria** (what must be TRUE):
  1. YahooClient can fetch adjusted close price for any ticker on any historical date
  2. YahooClient can fetch current price for any ticker
  3. chrono::NaiveDate converts to time::OffsetDateTime and back without timezone issues
  4. Weekend/holiday dates return nearest prior trading day's price
  5. Invalid ticker symbols return None, not errors
**Plans:** 1 plan

Plans:
- [x] 02-01-PLAN.md -- YahooClient wrapper with time/chrono conversion, price fetching, weekend fallback, caching, and tests (TDD)

### Phase 3: Ticker Validation & Trade Value Estimation
**Goal**: Ticker symbols are validated and trade share counts are estimated
**Depends on**: Phase 2
**Requirements**: REQ-E3, REQ-E4
**Success Criteria** (what must be TRUE):
  1. Invalid/delisted tickers are detected before price lookup
  2. Dollar range strings parse into numeric bounds (e.g., "$15,001 - $50,000" -> 15001.0, 50000.0)
  3. Estimated shares calculation: midpoint / trade_date_price produces integer shares
  4. Estimated value validation: estimated_shares * trade_date_price falls within original range
  5. Trades with missing tickers or prices skip estimation without failing batch
**Plans:** 1 plan

Plans:
- [x] 03-01-PLAN.md -- Pricing module (dollar range parsing + share estimation) and DB operations (enrichment queries + price updates + tests)

### Phase 4: Price Enrichment Pipeline
**Goal**: Trades are enriched with historical and current prices via batch processing
**Depends on**: Phase 3
**Requirements**: REQ-E1, REQ-E2, REQ-I3
**Success Criteria** (what must be TRUE):
  1. enrich-prices command fetches trade_date_price for all unenriched trades
  2. enrich-prices command fetches current_price deduplicated by ticker
  3. Batch processing handles 200 unique tickers in under 2 minutes
  4. Re-running enrich-prices skips already-enriched trades (resumable)
  5. Circuit breaker trips after N consecutive failures, logs summary
  6. Rate limiting (300ms jittered delay, max 5 concurrent) prevents 429 errors
  7. Enrichment progress displays ticker count and success/fail/skip counts
**Plans:** 1 plan

Plans:
- [ ] 04-01-PLAN.md -- Price enrichment pipeline (historical + current price fetching, CLI subcommand wiring, rate limiting, circuit breaker)

### Phase 5: Portfolio Calculator (FIFO)
**Goal**: Per-politician net positions with realized and unrealized P&L calculated
**Depends on**: Phase 4
**Requirements**: REQ-P1, REQ-P2, REQ-P3, REQ-P4
**Success Criteria** (what must be TRUE):
  1. FIFO accounting calculates correct net position per politician per ticker
  2. Buy/sell/exchange/receive transaction types adjust positions correctly
  3. Unrealized P&L: (current_price - avg_cost_basis) * shares_held calculated per position
  4. Realized P&L: (sell_price - buy_cost_basis) * shares_sold calculated for closed positions
  5. Option trades classified separately and excluded from stock position calculations
  6. Positions never go negative (oversold positions logged as warnings)
  7. Portfolio calculator handles 100K trades in under 500ms
**Plans**: TBD

Plans:
- [ ] 05-01: [Plan details created during plan-phase]

### Phase 6: CLI Commands & Output
**Goal**: Users can enrich prices and view portfolios via CLI
**Depends on**: Phase 5
**Requirements**: REQ-I4
**Success Criteria** (what must be TRUE):
  1. capitoltraders enrich-prices --db <path> command exists and runs
  2. capitoltraders portfolio --db <path> command shows positions with P&L
  3. portfolio command filters by --politician, --party, --state, --ticker
  4. portfolio output includes: ticker, shares_held, avg_cost_basis, current_price, current_value, unrealized_pnl, unrealized_pnl_pct
  5. Option positions display separately with "valuation deferred" note
  6. All output formats (table, JSON, CSV, markdown, XML) work for portfolio command
  7. Enrichment command displays progress and summary (X/Y succeeded, Z failed, N skipped)
**Plans**: TBD

Plans:
- [ ] 06-01: [Plan details created during plan-phase]

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Schema Migration & Data Model | 1/1 | Complete | 2026-02-10 |
| 2. Yahoo Finance Client Integration | 1/1 | Complete | 2026-02-10 |
| 3. Ticker Validation & Trade Value Estimation | 1/1 | Complete | 2026-02-11 |
| 4. Price Enrichment Pipeline | 0/1 | Not started | - |
| 5. Portfolio Calculator (FIFO) | 0/? | Not started | - |
| 6. CLI Commands & Output | 0/? | Not started | - |

## Requirement Coverage

All v1 requirements mapped to phases:

**Enrichment:**
- REQ-E1 (Historical trade-date price) -> Phase 4
- REQ-E2 (Current price per ticker) -> Phase 4
- REQ-E3 (Ticker validation and batch processing) -> Phase 3
- REQ-E4 (Trade value estimation) -> Phase 3

**Portfolio:**
- REQ-P1 (Net position FIFO) -> Phase 5
- REQ-P2 (Unrealized P&L) -> Phase 5
- REQ-P3 (Realized P&L) -> Phase 5
- REQ-P4 (Option trade classification) -> Phase 5

**Infrastructure:**
- REQ-I1 (Schema migration v2) -> Phase 1
- REQ-I2 (Yahoo Finance client) -> Phase 2
- REQ-I3 (enrich-prices CLI) -> Phase 4
- REQ-I4 (portfolio CLI) -> Phase 6

**Coverage:** 12/12 requirements mapped (100%)

## Phase Ordering Rationale

**Phase 1 (Schema)** must be first - can't store prices without columns. Foundation for all enrichment.

**Phase 2 (Yahoo Client)** before enrichment - need API integration before calling it. Isolated integration layer.

**Phase 3 (Validation & Estimation)** before enrichment - ticker validation prevents wasted API calls, trade value estimation needed for share counts in portfolio.

**Phase 4 (Enrichment Pipeline)** implements the batch processing that populates price data. Integrates phases 1-3 into working enrichment.

**Phase 5 (Portfolio Calculator)** requires enriched price data from Phase 4. FIFO calculation needs historical prices for cost basis and current prices for unrealized P&L.

**Phase 6 (CLI Commands)** last - user-facing integration of all previous phases. Can't display portfolio until calculator works.

Dependencies honor requirement graph: I1 foundation, I2 enables E1/E2/E3, E3 enables E1/E2, E1 enables E4/P1/P3, E2 enables P2, P1 enables P2/P3, I3 integrates E1-E4, I4 integrates P1-P4.
