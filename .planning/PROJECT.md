# Capitol Traders

## What This Is

A Rust CLI tool that tracks US congressional stock trades from CapitolTrades.com. It scrapes trade data, stores it in SQLite, enriches trades with Yahoo Finance market prices, and provides filtered querying with multiple output formats. Includes per-politician portfolio tracking with FIFO cost basis and unrealized/realized P&L.

## Core Value

Users can see what politicians are trading, what those positions are currently worth, and whether politicians are making or losing money on their trades.

## Requirements

### Validated

- [x] Scrape congressional trades from CapitolTrades BFF API -- v1.0
- [x] Store trades, politicians, issuers in SQLite with sync command -- v1.0
- [x] Enrich trades with detail page scraping (filing URL, asset type) -- v1.0
- [x] Enrich issuers with detail page scraping (sector, market cap) -- v1.0
- [x] Enrich politicians with committee data -- v1.0
- [x] Query trades with 24+ filters in scrape and DB modes -- v1.0
- [x] Query politicians with party/state/committee filters -- v1.0
- [x] Query issuers with search/sector/state filters -- v1.0
- [x] Output in table, JSON, CSV, Markdown, XML formats -- v1.0
- [x] Input validation for all filter parameters -- v1.0
- [x] Rate limiting and circuit breaker for upstream requests -- v1.0
- [x] Schema migration system (PRAGMA user_version) -- v1.0
- [x] Yahoo Finance price enrichment for trade tickers -- v1.1
- [x] Per-politician net position tracking via FIFO accounting -- v1.1
- [x] Trade-date historical price lookup with weekend/holiday fallback -- v1.1
- [x] Current price lookup (refreshed on enrichment run) -- v1.1
- [x] Unrealized and realized P&L calculation per position -- v1.1
- [x] `enrich-prices` subcommand for post-sync market data enrichment -- v1.1
- [x] Schema migration v2 (price columns + positions table) -- v1.1
- [x] Option trade classification (excluded from FIFO, noted in output) -- v1.1
- [x] Portfolio query output showing current holdings with values -- v1.1
- [x] Dollar range parsing and share estimation from trade value ranges -- v1.1

### Active

(None -- define next milestone requirements via `/gsd:new-milestone`)

### Out of Scope

- Option valuation (strike price, expiry, Greeks) -- deferred until data quality improves; Capitol Trades disclosures often lack contract details
- Real-time price streaming -- this is a batch enrichment tool, not a trading terminal
- Brokerage integration -- no buying/selling, just tracking
- Price alerts or notifications -- out of scope for CLI tool
- Historical price charts -- data stored but visualization not in scope
- Price cache TTL expiration -- YahooClient DashMap cache grows unbounded; acceptable for batch enrichment sessions
- Staleness threshold for prices -- prices display as-is without configurable freshness warning

## Context

- Shipped v1.1 with 16,776 LOC Rust across 3 workspace crates
- Tech stack: Rust, SQLite (rusqlite), reqwest, tokio, clap, yahoo_finance_api
- 366 tests across workspace (all passing, no clippy warnings)
- 6 subcommands: trades, politicians, issuers, sync, enrich-prices, portfolio
- SQLite schema at v2 with 5 price columns on trades and positions table
- Enrichment pipeline uses Semaphore + JoinSet + mpsc pattern for concurrent fetching
- Price enrichment is two-phase: historical by (ticker, date), then current by ticker
- FIFO portfolio calculator uses VecDeque lot-based accounting
- Yahoo Finance is unofficial API with no auth; rate limited at 200-500ms jittered delay, max 5 concurrent

## Constraints

- **Tech stack**: Rust workspace, must integrate with existing capitoltraders_lib and capitoltraders_cli crates
- **Database**: SQLite only, schema changes via versioned migrations (PRAGMA user_version)
- **API dependency**: Yahoo Finance is unofficial/undocumented; may break or rate-limit aggressively
- **Data quality**: Trade value ranges are imprecise; share counts not always disclosed; some tickers may not match Yahoo symbols
- **Enrichment pattern**: Must follow existing sentinel CASE upsert pattern to avoid overwriting good data

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Vendored capitoltrades_api crate | Upstream Telegram bot project had different goals; needed control over types | Stable foundation, 15+ modifications documented |
| New subcommand vs extending sync --enrich | Separate concern: market data enrichment is conceptually different from scrape enrichment | New `enrich-prices` subcommand |
| yahoo_finance_api 4.1.0 crate | Mature (v4.x), compatible (reqwest 0.12, tokio 1.x), minimal deps, no auth needed | Working Yahoo Finance integration |
| Materialized positions table | Avoids recalculating FIFO on every query, enables indexed filtering | Stored table with `enrich-prices` update |
| Midpoint of dollar range / historical price | Best estimate given imprecise range data from Capitol Trades disclosures | Validated: estimated_value falls within original range |
| REAL for estimated_shares | Midpoint / price division rarely produces whole shares; REAL preserves precision | Fractional shares stored correctly |
| Arc<YahooClient> for task sharing | YahooConnector does not implement Clone | Shared across spawned tasks |
| Two-phase enrichment | Historical prices needed for share estimation; current prices for mark-to-market | Sequential: historical first, then current |
| VecDeque for FIFO lot queue | Efficient front/back operations for buy/sell matching | Correct FIFO behavior verified with 14 tests |
| Unrealized P&L at query time | Computed via current_price subquery; avoids storing stale P&L values | Fresh calculation on each portfolio query |
| Option trades note in table/markdown only | Data formats (JSON/CSV/XML) should be clean; notes are for human consumption | Clean data exports, informative CLI output |

---
*Last updated: 2026-02-11 after v1.1 milestone*
