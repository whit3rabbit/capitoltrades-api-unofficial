# Capitol Traders - Detail Page Enrichment

## What This Is

Capitol Traders is a Rust CLI tool that scrapes capitoltrades.com to track congressional stock trades. The current scraper only hits listing pages, which leaves five SQLite tables empty and several per-record fields defaulted to NULL or placeholder values. This project extends the existing detail-page scrapers to capture the missing data and surfaces the enriched fields in CLI output.

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

### Active

- [ ] Extend trade_detail scraper to extract committees and labels
- [ ] Extend trade_detail scraper to extract asset type (currently defaults to "unknown" for 35,266 records)
- [ ] Extend trade_detail scraper to extract filing details (filing_id, filing_url -- currently 0 and empty)
- [ ] Extend trade_detail scraper to extract trade sizing (size, size_range_high, size_range_low -- currently NULL)
- [ ] Extend trade_detail scraper to extract price (NULL for ~18.5% of trades)
- [ ] Extend politician_detail scraper to extract committee memberships
- [ ] Extend issuer_detail scraper to extract performance data
- [ ] Extend issuer_detail scraper to extract end-of-day prices
- [ ] Populate trade_committees join table during sync
- [ ] Populate trade_labels join table during sync
- [ ] Populate politician_committees join table during sync
- [ ] Populate issuer_performance table during sync
- [ ] Populate issuer_eod_prices table during sync
- [ ] Smart detail fetching: on sync (full or incremental), check each existing row -- if key fields are NULL/default, fetch detail page; if all fields populated, skip
- [ ] Increase throttle delay when hitting detail pages (vs listing pages)
- [ ] Surface committees, labels, performance data in CLI display output (all formats)

### Out of Scope

- External APIs for data enrichment (Congress API, market data providers) -- all data comes from capitoltrades.com detail pages
- Changing the listing-page scraper behavior -- only extending detail-page scrapers
- Adding new CLI subcommands -- enrichment happens within existing trades/politicians/issuers/sync commands
- Real-time price feeds or live market data -- only what capitoltrades.com provides

## Context

- The scraper already has `trade_detail()`, `politician_detail()`, and `issuer_detail()` methods in `capitoltraders_lib/src/scrape.rs`
- The SQLite schema already defines the five target tables (`trade_committees`, `trade_labels`, `politician_committees`, `issuer_performance`, `issuer_eod_prices`) in `schema/sqlite.sql`
- The `db.rs` upsert functions already reference these tables but the data coming in is empty
- Current data quality issues from a synced database:
  - Asset Type: 35,266 records defaulted to "unknown"
  - Filing Details: filing_id = 0, filing_url = empty
  - Trade Sizing: size, size_range_high, size_range_low = NULL
  - Pricing: price = NULL for ~18.5% (6,523 records)
- The `--with-trade-details` flag already exists for sync but produces incomplete results
- Listing pages return 12 results per page; detail pages are individual requests per record

## Constraints

- **Data source**: All enrichment comes from capitoltrades.com detail pages only -- no external APIs
- **Rate limiting**: Detail pages add significant request volume; throttle must be increased vs listing pages to avoid being blocked
- **Backward compatibility**: Existing CLI behavior must not break; enriched data is additive
- **Schema stability**: SQLite DDL already defines the target tables; schema changes should be avoided if possible

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Detail pages as sole data source | User wants self-contained tool, no external API keys or dependencies | -- Pending |
| Always fetch details during sync | Simplifies logic; smart skip for already-complete rows avoids redundant requests | -- Pending |
| Increase throttle for detail pages | Detail pages are 1-per-record vs 1-per-12 for listings; need to be respectful of the source | -- Pending |
| Smart skip on populated rows | Check key fields before fetching detail; if all non-NULL, skip the request | -- Pending |

---
*Last updated: 2026-02-07 after initialization*
