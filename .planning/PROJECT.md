# Capitol Traders - Detail Page Enrichment

## What This Is

Capitol Traders is a Rust CLI tool that scrapes capitoltrades.com to track congressional stock trades. It syncs listing pages for trades, politicians, and issuers into SQLite, then enriches each record by fetching detail pages to populate asset types, filing details, trade sizing, pricing, committee memberships, performance metrics, and EOD price history. All enriched data is queryable via --db flag in 5 output formats.

## Core Value

Every synced record has complete data -- committees, labels, asset types, filing details, trade sizing, and pricing -- populated from detail pages, so downstream analysis is not working with incomplete or placeholder values.

## Requirements

### Validated

- Listing-page scraping for trades, politicians, issuers -- existing
- SQLite sync with incremental tracking (last_trade_pub_date) -- existing
- Multi-format CLI output (table, JSON, CSV, Markdown, XML) -- existing
- Detail-page scraper methods (trade_detail, politician_detail, issuer_detail) -- existing
- Input validation for all filter types -- existing
- Retry/backoff logic with configurable delays -- existing
- Schema files (JSON Schema, XSD, SQLite DDL) for output validation -- existing
- Schema migration with PRAGMA user_version gating -- v1.0
- Sentinel-protected upserts that preserve enriched data on re-sync -- v1.0
- Enrichment tracking via enriched_at timestamp columns -- v1.0
- Trade detail extraction (asset_type, sizing, price, filing details) -- v1.0
- Trade committees and labels extraction from RSC payloads -- v1.0
- Smart-skip enrichment for already-complete rows -- v1.0
- Batch checkpointing for crash-safe enrichment -- v1.0
- Dry-run mode for enrichment preview -- v1.0
- Politician committee extraction via listing page committee-filter iteration -- v1.0
- Issuer performance and EOD price extraction from detail pages -- v1.0
- Bounded concurrent enrichment with Semaphore (configurable 1-10) -- v1.0
- Progress bars (indicatif) for enrichment runs -- v1.0
- Circuit breaker for consecutive failure detection -- v1.0
- DB query path (--db flag) for trades, politicians, issuers with enriched columns in all 5 formats -- v1.0

### Active

(None -- next milestone requirements to be defined via /gsd:new-milestone)

### Out of Scope

- External APIs for data enrichment (Congress API, market data providers) -- all data comes from capitoltrades.com detail pages
- Changing the listing-page scraper behavior -- only extending detail-page scrapers
- Adding new CLI subcommands -- enrichment happens within existing trades/politicians/issuers/sync commands
- Real-time price feeds or live market data -- only what capitoltrades.com provides
- Headless browser rendering -- RSC payloads contain structured data
- BFF API fallback -- legacy API is unstable
- Full re-enrichment on every sync -- smart-skip required for performance
- Mobile or web UI -- CLI tool only

## Context

Shipped v1.0 with 13,589 LOC Rust across 3 workspace crates.
Tech stack: Rust, SQLite (rusqlite), reqwest, tokio, clap, tabled, indicatif.
294 tests passing, 0 clippy warnings.

Known tech debt:
- TRADE-05/TRADE-06 (committees/labels from trade RSC) implemented but unconfirmed on live site payloads
- Synthetic HTML fixtures may not match actual live RSC payload structure
- get_unenriched_politician_ids method exists but is unused (committee enrichment runs as full refresh)

v2 requirements candidates (from original REQUIREMENTS.md):
- SEL-01/SEL-02: Selective enrichment flags (--enrich-trades, --enrich-issuers, --enrich-all)
- MON-01: RSC payload canary test to detect Next.js format changes
- MON-02: Enrichment statistics report
- CLI-01: --from-db flag to read from SQLite instead of scraping
- CLI-02: Eliminate redundant detail fetching in trades command

## Constraints

- **Data source**: All enrichment comes from capitoltrades.com detail pages only -- no external APIs
- **Rate limiting**: Detail pages add significant request volume; throttle increased to 500ms for detail pages
- **Backward compatibility**: Existing CLI behavior must not break; enriched data is additive
- **Schema stability**: SQLite DDL defines all tables; migrations are version-gated via PRAGMA user_version

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Detail pages as sole data source | Self-contained tool, no external API keys | Good -- all fields populated from RSC payloads |
| Smart skip on populated rows | Check key fields before fetching detail; skip if populated | Good -- prevents redundant HTTP calls |
| 500ms throttle for detail pages | Detail pages are 1-per-record vs 1-per-12 for listings | Good -- respectful rate |
| Sentinel CASE upsert protection | Prevent re-sync from overwriting enriched data with defaults | Good -- data integrity maintained |
| PRAGMA user_version for migrations | Simple, no migration framework dependency | Good -- clean versioned migration |
| Committee-filter iteration for politicians | Detail pages lack committee data; listing page filter is only source | Good -- 48 requests covers all committees |
| Post-ingest enrichment pipeline | Enrich after sync_trades rather than inline | Good -- separates concerns, enables smart-skip |
| Bounded concurrency via Semaphore | Default 3, configurable 1-10 parallel requests | Good -- balances speed and server load |
| DB writes via mpsc channel | Single-threaded SQLite writes from concurrent tasks | Good -- avoids SQLite contention |
| CircuitBreaker as simple kill switch | Consecutive failure counter, not full half-open pattern | Good -- simple, effective for this use case |
| Unconditional committee enrichment | 48 requests is fast (~25s), no opt-in needed | Good -- always up to date |
| Per-entity DB query types | DbTradeRow, DbPoliticianRow, DbIssuerRow as read-side types | Good -- clean separation from scrape/API types |

---
*Last updated: 2026-02-09 after v1.0 milestone*
