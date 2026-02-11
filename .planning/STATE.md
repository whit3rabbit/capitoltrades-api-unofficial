# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-09)

**Core value:** Users can see what politicians are trading, what those positions are currently worth, and whether politicians are making or losing money on their trades.
**Current focus:** Phase 5 complete - ready for Phase 6

## Current Position

Phase: 5 of 6 (Portfolio Calculator FIFO)
Plan: 2 of 2 complete
Status: Phase 5 complete - ready for Phase 6
Last activity: 2026-02-10 - Completed Phase 5 execution (FIFO calculator + DB integration)

Progress: [██████████] 100% (Phase 5 complete)

## Performance Metrics

**Velocity:**
- Total plans completed: 6
- Average duration: 4.5 min
- Total execution time: 0.45 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-schema-migration-data-model | 1 | 5 min | 5 min |
| 02-yahoo-finance-client-integration | 1 | 6 min | 6 min |
| 03-ticker-validation-trade-value-estimation | 1 | 5 min | 5 min |
| 04-price-enrichment-pipeline | 1 | 4 min | 4 min |
| 05-portfolio-calculator-fifo | 2 | 7 min | 3.5 min |

**Recent Trend:**
- 2026-02-10: 05-02 completed in 5 min (Portfolio DB operations)
- 2026-02-10: 05-01 completed in 2 min (FIFO portfolio calculator)
- 2026-02-11: 04-01 completed in 4 min (price enrichment pipeline)

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- New subcommand vs extending sync: New enrich-prices subcommand (separate concern from scrape enrichment)
- Yahoo Finance crate: yahoo_finance_api 4.1.0 (mature, compatible, focused)
- Portfolio storage: Materialized positions table (avoids FIFO recalculation on every query)
- Trade value strategy: Midpoint of range / historical price = estimated shares
- REAL vs INTEGER for estimated_shares: Use REAL for precision when midpoint/price division produces fractional shares (01-01)
- Price columns in base schema: Add price columns to schema.sql for fresh DBs, migrations only for existing DBs (01-01)
- Invalid ticker handling: Return Ok(None) instead of Err for invalid tickers (NoQuotes/NoResult/ApiError) - downstream code treats as non-fatal (02-01)
- YahooClient cache key: (String, NaiveDate) tuple - price data unique per ticker-date pair (02-01)
- Two-phase price enrichment: Historical prices by (ticker, date) first, then current prices by ticker - enables share estimation and mark-to-market (04-01)
- Arc<YahooClient> pattern: YahooConnector does not implement Clone, wrap in Arc for sharing across spawned tasks (04-01)
- Rate limiting strategy: 200-500ms jittered delay per request + concurrency limit 5 to avoid Yahoo Finance throttling (04-01)
- VecDeque for FIFO lot queue: efficient front/back operations for buy/sell matching (05-01)
- Oversold positions: Err return with warning log, no panics (05-01)
- Portfolio DB upsert: ON CONFLICT(politician_id, issuer_ticker) for idempotent writes (05-02)
- Unrealized P&L: computed at query time via current_price subquery from trades table (05-02)

### Pending Todos

None yet.

### Blockers/Concerns

None yet.

## Session Continuity

Last session: 2026-02-10 - Phase 5 execution complete
Stopped at: Phase 5 verified and complete
Resume file: .planning/phases/05-portfolio-calculator-fifo/05-portfolio-calculator-fifo-VERIFICATION.md
