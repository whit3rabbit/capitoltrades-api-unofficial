# Requirements: Capitol Traders - Detail Page Enrichment

**Defined:** 2026-02-07
**Core Value:** Every synced record has complete data populated from detail pages, so downstream analysis works with real values instead of placeholders.

## v1 Requirements

### Foundation

- [ ] **FOUND-01**: Fix upsert COALESCE direction so re-syncs do not overwrite enriched data with defaults
- [ ] **FOUND-02**: Add `enriched_at` timestamp column to trades, politicians, and issuers tables
- [ ] **FOUND-03**: Create Db query methods to find rows needing enrichment (NULL/default key fields)
- [ ] **FOUND-04**: Add schema migration support (versioned ALTER TABLE for existing databases)

### Trade Enrichment

- [ ] **TRADE-01**: Extend trade_detail scraper to extract asset_type from RSC payload
- [ ] **TRADE-02**: Extend trade_detail scraper to extract size, size_range_high, size_range_low
- [ ] **TRADE-03**: Extend trade_detail scraper to extract price (where available)
- [ ] **TRADE-04**: Extend trade_detail scraper to extract filing_id and filing_url
- [ ] **TRADE-05**: Investigate and extract committees from trade detail RSC payload (if available)
- [ ] **TRADE-06**: Investigate and extract labels from trade detail RSC payload (if available)
- [ ] **TRADE-07**: Populate trade_committees join table during sync
- [ ] **TRADE-08**: Populate trade_labels join table during sync
- [ ] **TRADE-09**: Smart-sync: skip trade detail fetch when all enrichable fields are non-NULL/non-default
- [ ] **TRADE-10**: Batch commit checkpointing so enrichment can resume after crash
- [ ] **TRADE-11**: Dry-run mode to preview what would be enriched without fetching

### Politician Enrichment

- [ ] **POL-01**: Extend politician_detail scraper to extract committee memberships from RSC payload
- [ ] **POL-02**: Populate politician_committees join table during sync
- [ ] **POL-03**: Politician enrichment runs by default during sync (no opt-in flag)

### Issuer Enrichment

- [ ] **ISS-01**: Extend issuer_detail scraper to extract performance data from RSC payload
- [ ] **ISS-02**: Extend issuer_detail scraper to extract end-of-day price data
- [ ] **ISS-03**: Populate issuer_performance table during sync
- [ ] **ISS-04**: Populate issuer_eod_prices table during sync

### Concurrency / Performance

- [ ] **PERF-01**: Bounded concurrency for detail requests (3-5 parallel via tokio Semaphore)
- [ ] **PERF-02**: Progress bars for enrichment runs using indicatif
- [ ] **PERF-03**: Circuit breaker that stops after N consecutive failures
- [ ] **PERF-04**: Increased throttle delay for detail-page requests vs listing pages

### CLI Output

- [ ] **OUT-01**: Surface asset_type, committees, labels in trade output (all formats: table, JSON, CSV, MD, XML)
- [ ] **OUT-02**: Surface committee memberships in politician output (all formats)
- [ ] **OUT-03**: Surface performance and EOD price data in issuer output (all formats)

## v2 Requirements

### Selective Enrichment

- **SEL-01**: Add --enrich-trades, --enrich-issuers flags to control which entity types get enriched
- **SEL-02**: Add --enrich-all flag as shorthand for enriching all entity types

### Monitoring

- **MON-01**: RSC payload canary test to detect Next.js format changes
- **MON-02**: Enrichment statistics report (rows enriched, skipped, failed per entity type)

### CLI Enhancements

- **CLI-01**: Add --from-db flag to read from SQLite instead of scraping (use enriched data)
- **CLI-02**: Eliminate redundant detail fetching in trades command (currently fetches unconditionally)

## Out of Scope

| Feature | Reason |
|---------|--------|
| External API enrichment (Congress API, market data) | All data comes from capitoltrades.com detail pages |
| BFF API fallback | Legacy API is unstable and not maintained |
| Headless browser rendering | RSC payloads contain structured data; DOM parsing unnecessary |
| Real-time price feeds | Only what capitoltrades.com provides |
| Full re-enrichment on every sync | Smart-skip is required for performance |
| Mobile or web UI | CLI tool only |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUND-01 | Phase 1 | Pending |
| FOUND-02 | Phase 1 | Pending |
| FOUND-03 | Phase 1 | Pending |
| FOUND-04 | Phase 1 | Pending |
| TRADE-01 | Phase 2 | Pending |
| TRADE-02 | Phase 2 | Pending |
| TRADE-03 | Phase 2 | Pending |
| TRADE-04 | Phase 2 | Pending |
| TRADE-05 | Phase 2 | Pending |
| TRADE-06 | Phase 2 | Pending |
| TRADE-07 | Phase 3 | Pending |
| TRADE-08 | Phase 3 | Pending |
| TRADE-09 | Phase 3 | Pending |
| TRADE-10 | Phase 3 | Pending |
| TRADE-11 | Phase 3 | Pending |
| POL-01 | Phase 4 | Pending |
| POL-02 | Phase 4 | Pending |
| POL-03 | Phase 4 | Pending |
| ISS-01 | Phase 5 | Pending |
| ISS-02 | Phase 5 | Pending |
| ISS-03 | Phase 5 | Pending |
| ISS-04 | Phase 5 | Pending |
| PERF-01 | Phase 6 | Pending |
| PERF-02 | Phase 6 | Pending |
| PERF-03 | Phase 6 | Pending |
| PERF-04 | Phase 3 | Pending |
| OUT-01 | Phase 3 | Pending |
| OUT-02 | Phase 4 | Pending |
| OUT-03 | Phase 5 | Pending |

**Coverage:**
- v1 requirements: 29 total
- Mapped to phases: 29
- Unmapped: 0

---
*Requirements defined: 2026-02-07*
*Last updated: 2026-02-07 after roadmap creation*
