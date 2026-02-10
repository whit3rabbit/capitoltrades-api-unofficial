# Capitol Traders

## What This Is

A Rust CLI tool that tracks US congressional stock trades from CapitolTrades.com. It scrapes trade data, stores it in SQLite, and provides filtered querying with multiple output formats. The next milestone adds Yahoo Finance market data enrichment and per-politician portfolio tracking with profit/loss visibility.

## Core Value

Users can see what politicians are trading, what those positions are currently worth, and whether politicians are making or losing money on their trades.

## Requirements

### Validated

- [x] Scrape congressional trades from CapitolTrades BFF API -- existing
- [x] Store trades, politicians, issuers in SQLite with sync command -- existing
- [x] Enrich trades with detail page scraping (filing URL, asset type) -- existing
- [x] Enrich issuers with detail page scraping (sector, market cap) -- existing
- [x] Enrich politicians with committee data -- existing
- [x] Query trades with 24+ filters in scrape and DB modes -- existing
- [x] Query politicians with party/state/committee filters -- existing
- [x] Query issuers with search/sector/state filters -- existing
- [x] Output in table, JSON, CSV, Markdown, XML formats -- existing
- [x] Input validation for all filter parameters -- existing
- [x] Rate limiting and circuit breaker for upstream requests -- existing
- [x] Schema migration system (PRAGMA user_version) -- existing

### Active

- [ ] Yahoo Finance price enrichment for trade tickers
- [ ] Per-politician net position tracking (remaining shares per ticker)
- [ ] Trade-date historical price lookup
- [ ] Current price lookup (refreshed on enrichment run)
- [ ] P&L calculation per position (trade price vs current price)
- [ ] New `enrich-prices` subcommand for post-sync market data enrichment
- [ ] Schema migration to add price/portfolio columns to existing DB
- [ ] Option trade classification (call/put vs stock)
- [ ] Portfolio query output showing current holdings with values
- [ ] Graceful handling of missing tickers, delisted stocks, non-equity assets

### Out of Scope

- Option valuation (strike price, expiry, Greeks) -- deferred until data quality improves; Capitol Trades disclosures often lack contract details
- Real-time price streaming -- this is a batch enrichment tool, not a trading terminal
- Non-DB output format changes -- enrichment is DB-only; existing table/JSON/CSV/MD/XML formats unchanged for now
- Brokerage integration -- no buying/selling, just tracking
- Price alerts or notifications -- out of scope for CLI tool
- Historical price charts -- data stored but visualization not in scope

## Context

- Existing SQLite schema has 7 tables with enrichment tracking via `enriched_at` columns
- Trades have `issuer_ticker` field which maps to Yahoo Finance symbols
- Some trades are options (calls/puts) which need different handling than equities
- Capitol Trades reports trade value ranges (e.g., $1,001-$15,000), not exact amounts
- The enrichment pipeline pattern (Semaphore + JoinSet + mpsc) already exists for scrape enrichment and can be reused
- Need a Rust Yahoo Finance client -- no official SDK, need to evaluate available crates
- Yahoo Finance has rate limits; need to respect them with the existing retry/backoff infrastructure

## Constraints

- **Tech stack**: Rust workspace, must integrate with existing capitoltraders_lib and capitoltraders_cli crates
- **Database**: SQLite only, schema changes via versioned migrations (PRAGMA user_version)
- **API dependency**: Yahoo Finance is unofficial/undocumented; may break or rate-limit aggressively
- **Data quality**: Trade value ranges are imprecise; share counts not always disclosed; some tickers may not match Yahoo symbols
- **Enrichment pattern**: Must follow existing sentinel CASE upsert pattern to avoid overwriting good data

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| New subcommand vs extending sync --enrich | Separate concern: market data enrichment is conceptually different from scrape enrichment | New `enrich-prices` subcommand (REQ-I3) |
| Rust Yahoo Finance crate selection | yahoo_finance_api 4.1.0: mature (v4.x), compatible (reqwest 0.12, tokio 1.x), minimal deps, no auth needed | yahoo_finance_api 4.1.0 + time 0.3 (REQ-I2) |
| Portfolio as computed view vs stored table | Materialized `positions` table: avoids recalculating FIFO on every query, enables indexed filtering | Stored table with `enrich-prices` update (REQ-P1) |
| Trade value approximation strategy | Midpoint of dollar range / historical price = estimated shares; validated by checking estimate falls within range | Midpoint strategy (REQ-E4) |

---
*Last updated: 2026-02-09 after requirements definition*
