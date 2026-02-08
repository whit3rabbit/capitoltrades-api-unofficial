# Feature Landscape: Detail-Page Enrichment Pipeline

**Domain:** Web scraper enrichment for congressional stock trade tracker
**Researched:** 2026-02-07
**Mode:** Ecosystem (features dimension)

## Table Stakes

Features the tool must have for enrichment to be considered functional. Without these, the enrichment pipeline is broken or useless.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Extract all missing trade fields from detail pages | ~35K trades have "unknown" asset_type, empty committees/labels, NULL size brackets. Users filtering by asset type or committee get zero results. The data exists on the site -- not scraping it makes the tool incomplete. | Medium | Trade detail pages contain asset type, committees, labels, trade size, capital gains flag. The existing `trade_detail` scraper only extracts `filing_url` and `filing_id` -- it must be extended to parse these additional fields from the RSC payload. |
| Smart-sync: skip already-enriched rows | Enriching 35K+ trades at 250ms/request = ~2.4 hours minimum. Users should not re-fetch details for rows that already have complete data. | Medium | Query SQLite for trades where `asset_type = 'unknown'` or `size IS NULL` or `filing_url = ''`. Only fetch detail pages for those tx_ids. Track enrichment status per-row, not globally. |
| Configurable throttle for detail pages | Detail page requests are 1:1 per trade. At scale this is the dominant cost. Users need control over request rate to balance speed vs. politeness. | Low | Already partially implemented via `--details-delay-ms` (default 250ms). Should remain configurable. Consider increasing default to 500ms+ for bulk enrichment runs to reduce ban risk. |
| Idempotent upserts for enriched fields | Re-running enrichment must not corrupt existing good data. Must use `COALESCE` or conditional update logic so that a scraped NULL does not overwrite a previously populated value. | Low | The existing `ON CONFLICT DO UPDATE` patterns in db.rs already use `COALESCE` for some fields (issuer_ticker, sector). Must extend this pattern to asset_type, committees, labels, size fields. Never overwrite a known value with "unknown" or NULL. |
| Progress reporting during enrichment | Enrichment of 35K records takes hours. Users need to see where they are: "Enriching trade 1,234 / 35,000 (3.5%)". | Low | Print to stderr. Include ETA based on elapsed time per request. Already have some progress patterns in sync.rs for page-level reporting. |
| Issuer detail enrichment for performance data | Issuer listing pages return no performance data (market cap, trailing returns, EOD prices). The `issuer_detail` scraper exists and works. Need to call it systematically for issuers missing performance data. | Medium | ~2,500+ distinct issuers. At 500ms/request = ~20 minutes. Manageable. The scraper and DB upsert already exist (`issuer_detail`, `upsert_issuers`). Just needs orchestration in sync. |
| Surface enriched data in all output formats | Enrichment is pointless if CLI output still shows "unknown" for asset type and empty brackets for committees. Table/JSON/CSV/MD/XML output must reflect enriched fields. | Medium | The `TradeRow` struct in output.rs currently shows only 7 columns. Add asset_type and size at minimum. JSON output already serializes the full Trade struct, so it will work automatically. Table/CSV/Markdown need new column definitions. |

## Differentiators

Features that elevate the tool beyond basic scraping. Not expected but make it significantly more useful.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Checkpoint/resume for interrupted enrichment | If a 2-hour enrichment run dies at trade 15,000, it should resume from 15,001 not restart from scratch. Track last-enriched tx_id in `ingest_meta`. | Low | Store `last_enriched_tx_id` in `ingest_meta` table. On resume, query `WHERE tx_id > last_enriched AND (asset_type = 'unknown' OR ...)`. Simple but extremely valuable for large datasets. |
| Parallel enrichment with concurrency limit | Sequential fetching is slow. Fetch N detail pages concurrently (e.g., 3-5) with a shared rate limiter to saturate bandwidth without hammering the server. | High | Requires `tokio::sync::Semaphore` or `futures::stream::buffer_unordered`. Must handle errors per-task without aborting the batch. Significant refactor of the current sequential loop in sync_trades. Consider using a work queue pattern. |
| Selective enrichment by entity type | `--enrich trades` vs `--enrich issuers` vs `--enrich all`. Let users enrich only the data they need rather than running the full pipeline every time. | Low | Just flag parsing and conditional logic. Adds flexibility without complexity. |
| Enrichment statistics summary | After enrichment completes, print a summary: "Enriched 12,345 trades. Asset types resolved: 11,890. Committees populated: 8,234. Still missing: 455 trades." | Low | Simple counters during the enrichment loop. Helps users understand data completeness without running ad-hoc SQL queries. |
| Dry-run mode for enrichment | `--dry-run` that queries the DB for what would be enriched and reports counts without making any HTTP requests. "Would enrich 4,567 trades, 234 issuers." | Low | Just the SELECT query without the fetch/upsert loop. Very useful for CI/scheduling decisions. |
| Adaptive throttle based on server response | If the server returns 429 (Too Many Requests) or response times spike, automatically increase delay. If responses are fast and healthy, cautiously decrease delay. Respect Retry-After header. | Medium | The retry logic in `with_retry` already handles 429 and Retry-After. Adaptive throttle goes further: adjust the *baseline* delay between requests based on rolling response time average. Scrapy's AutoThrottle pattern is the model here. |
| Backfill command for historical data | `capitoltraders sync --backfill` that specifically targets old trades with missing data, working backwards from newest to oldest. Different from regular sync which processes newest-first. | Medium | Useful because the site may have added fields over time. Old trades ingested before enrichment existed need catching up. Query by `pub_date ASC` with missing-data filter. |
| Politician committee enrichment | Politician listing pages only show name/party/state/stats. Committee memberships are only on detail pages. Scraping `/politicians/{id}` populates the `politician_committees` join table. | Medium | ~600 politicians. At 500ms/request = ~5 minutes. The `politician_detail` scraper exists. Need to extend it to also parse committees from the politician detail page RSC payload (currently it only extracts basic `ScrapedPolitician` fields, not committees). |

## Anti-Features

Features to deliberately NOT build. Including these would add complexity, maintenance burden, or risk without proportional value.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Real-time / streaming enrichment | The data source updates daily (45-day STOCK Act disclosure window). Real-time enrichment adds WebSocket complexity for data that changes at most once per day. | Run enrichment as a batch job via cron (`sqlite-sync.yml` already runs daily). Daily batch is the right cadence for this data. |
| Scraping the full BFF API directly | The capitoltrades.com BFF API is unofficial, undocumented, and changes without notice. Building deep integration with it creates fragile coupling. | Stick with HTML/RSC payload scraping. The RSC approach is more stable than reverse-engineering API endpoints because the page contract changes less frequently than internal API contracts. |
| Caching detail pages locally (HTML cache) | Storing raw HTML for 35K+ detail pages wastes disk space and adds staleness concerns. The extracted data in SQLite is the authoritative cache. | Cache extracted data in SQLite (which already happens via upsert). If a value is populated and non-default, skip the detail page entirely. |
| Multi-threaded SQLite writes | SQLite does not support concurrent writers. Attempting parallel writes causes SQLITE_BUSY errors and corruption risk. | Keep all DB writes sequential within a transaction. Parallelize the HTTP fetching, but serialize the DB writes. The current pattern of batching upserts in a transaction is correct. |
| Auto-retry indefinitely on enrichment failure | If a detail page consistently fails (404, parse error), retrying forever wastes time and masks real problems. | Retry up to N times (already implemented), then log the failure and move on. Track failed tx_ids for manual review. The existing `with_retry` pattern with `max_retries: 3` is appropriate. |
| Enrichment via external APIs (SEC EDGAR, market data APIs) | Adding external data sources expands scope, introduces API key management, rate limit coordination across multiple providers, and data reconciliation complexity. | Stay focused on capitoltrades.com as the single source. The site already aggregates SEC EDGAR data. If users want market data enrichment, they can join the SQLite DB with external datasets themselves. |
| GUI or web dashboard for enrichment status | A terminal CLI tool should not become a web application. Adding a dashboard requires a web server, frontend framework, and deployment complexity. | Progress reporting to stderr is sufficient. For richer monitoring, users can query the SQLite DB directly (`SELECT COUNT(*) FROM trades WHERE asset_type = 'unknown'`). |

## Feature Dependencies

```
Smart-sync (skip enriched) --> Extract all missing trade fields
                           \-> Issuer detail enrichment
                           \-> Politician committee enrichment

Checkpoint/resume --> Smart-sync (must know what's already done)

Progress reporting --> Any enrichment operation (shows status)

Surface enriched data in output --> Extract all missing trade fields (data must exist first)

Parallel enrichment --> Smart-sync (need work queue of items to enrich)
                    \-> Configurable throttle (shared rate limiter)

Enrichment statistics --> Smart-sync (needs before/after counts)

Backfill command --> Smart-sync (same missing-data query, different ordering)

Dry-run mode --> Smart-sync (uses same query, skips execution)

Adaptive throttle --> Configurable throttle (extends existing delay logic)

Selective enrichment --> All enrichment operations (gates which entity types run)
```

## Priority ordering (what feeds the dependency chain):

```
1. Extract all missing trade fields (foundation -- everything depends on this)
2. Smart-sync: skip enriched rows (foundation -- makes enrichment practical at scale)
3. Idempotent upserts (safety -- prevents data corruption)
4. Progress reporting (UX -- required for any long-running operation)
5. Configurable throttle (already exists, just tune defaults)
6. Surface enriched data in output (value delivery -- users see the data)
7. Issuer detail enrichment (second entity type)
8. Checkpoint/resume (reliability -- prevents wasted work)
```

## MVP Recommendation

**Phase 1 -- Core enrichment (must ship together):**

1. Extend `trade_detail` scraper to extract asset_type, committees, labels, size, size_range_high, size_range_low, has_capital_gains from trade detail page RSC payload
2. Smart-sync query: `SELECT tx_id FROM trades WHERE asset_type = 'unknown' OR size IS NULL`
3. Idempotent upserts with `COALESCE` for all enrichable fields (never overwrite good data with defaults)
4. Progress reporting to stderr during enrichment
5. Surface asset_type in CLI table/CSV/Markdown output (add column to `TradeRow`)

**Phase 2 -- Reliability and entity expansion:**

1. Checkpoint/resume via `ingest_meta` tracking
2. Issuer detail enrichment (performance data, market cap, EOD prices)
3. Politician committee enrichment
4. Enrichment statistics summary
5. Dry-run mode

**Defer:**

- Parallel enrichment: High complexity, moderate gain. The bottleneck is server-side rate limiting, not client-side concurrency. Sequential with 250-500ms delay is fast enough for daily batch runs.
- Adaptive throttle: Nice-to-have but the fixed retry logic handles the common cases (429, timeouts). Only build this if ban rates become a problem.
- Backfill command: Smart-sync with `--full` already covers this case. A separate backfill verb is syntactic sugar, not new capability.

## Data Gap Inventory

Current state of fields that enrichment would populate. Based on codebase analysis of `scraped_trade_to_trade` (trades.rs) and `upsert_scraped_trades` (db.rs).

| Field | Current Value | Source for Real Value | Impact |
|-------|--------------|----------------------|--------|
| `asset_type` (assets table) | `"unknown"` hardcoded | Trade detail page RSC payload | HIGH -- blocks filtering by `--asset-type`, makes data appear incomplete |
| `committees` (trade_committees table) | Empty `[]` | Trade detail page RSC payload | HIGH -- blocks filtering by `--committee` |
| `labels` (trade_labels table) | Empty `[]` | Trade detail page RSC payload | MEDIUM -- blocks filtering by `--label` |
| `size` (trades table) | `NULL` | Trade detail page RSC payload | MEDIUM -- users cannot see trade size bracket |
| `size_range_high` (trades table) | `NULL` | Trade detail page RSC payload | LOW -- derived from size bracket |
| `size_range_low` (trades table) | `NULL` | Trade detail page RSC payload | LOW -- derived from size bracket |
| `has_capital_gains` (trades table) | `0` (false) | Trade detail page RSC payload | LOW -- informational only |
| `price` (trades table) | Often `NULL` (~6.5K) | Trade detail page RSC payload or issuer EOD prices | MEDIUM -- price at time of trade |
| `filing_url` (trades table) | Empty string `""` | Trade detail page (already implemented) | MEDIUM -- links to original SEC filing |
| `filing_id` (trades table) | `0` | Trade detail page (already implemented) | LOW -- internal reference |
| `performance` (issuer_performance) | Missing for listing-scraped issuers | Issuer detail page | MEDIUM -- market cap, trailing returns |
| `eod_prices` (issuer_eod_prices) | Missing for listing-scraped issuers | Issuer detail page | LOW -- historical price chart data |
| `politician committees` (politician_committees) | Empty for scraped politicians | Politician detail page | MEDIUM -- committee membership for analysis |

## Sources

- Codebase analysis: `capitoltraders_lib/src/scrape.rs`, `capitoltraders_lib/src/db.rs`, `capitoltraders_cli/src/commands/sync.rs`, `capitoltraders_cli/src/commands/trades.rs`
- [Incremental Web Scraping -- Stabler](https://stabler.tech/blog/how-to-perform-incremental-web-scraping)
- [Scrapy DeltaFetch for Incremental Crawls -- Zyte](https://www.zyte.com/blog/scrapy-tips-from-the-pros-july-2016/)
- [Idempotent Data Pipelines -- Airbyte](https://airbyte.com/data-engineering-resources/idempotency-in-data-pipelines)
- [Building Idempotent Data Pipelines -- Medium](https://medium.com/towards-data-engineering/building-idempotent-data-pipelines-a-practical-guide-to-reliability-at-scale-2afc1dcb7251)
- [Rate Limiting in Web Scraping -- ScrapeHero](https://www.scrapehero.com/rate-limiting-in-web-scraping/)
- [Scrapy AutoThrottle -- DEV](https://dev.to/ikram_khan/scrapy-autothrottle-rate-limiting-stop-getting-blocked-4kje)
- [Exponential Backoff for Rate Limits -- The Web Scraping Club](https://substack.thewebscraping.club/p/rate-limit-scraping-exponential-backoff)
- [Web Scraping and Enrichment Pipeline -- Medium](https://medium.com/@divyansh9144/a-look-at-web-data-scraping-and-enrichment-pipeline-2622de813750)
- [Quiver Quantitative Congress Trading](https://www.quiverquant.com/congresstrading/)
- [Capitol Trades](https://www.capitoltrades.com/)
