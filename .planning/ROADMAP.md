# Roadmap: Capitol Traders - Detail Page Enrichment

## Overview

This project extends the Capitol Traders scraper to populate missing data by fetching detail pages for trades, politicians, and issuers. The work progresses from fixing data-corruption bugs in the upsert layer, through extending trade detail scrapers, wiring enriched data into the sync pipeline and CLI output, then repeating for politicians and issuers, and finally adding bounded concurrency for production-scale runs.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Foundation** - Fix upsert data corruption, add enrichment tracking, schema migration
- [x] **Phase 2: Trade Extraction** - Extend trade_detail scraper to extract all missing fields from RSC payloads
- [x] **Phase 3: Trade Sync and Output** - Wire trade enrichment into sync pipeline with smart-skip, checkpointing, and CLI output
- [ ] **Phase 4: Politician Enrichment** - End-to-end politician detail extraction, sync, and CLI output
- [ ] **Phase 5: Issuer Enrichment** - End-to-end issuer detail extraction, sync, and CLI output
- [ ] **Phase 6: Concurrency and Reliability** - Bounded parallel fetching, progress bars, circuit breaker

## Phase Details

### Phase 1: Foundation
**Goal**: Enrichment infrastructure is safe and correct -- re-syncs never overwrite enriched data with defaults, enrichment state is tracked per row, and the database can be migrated from existing schema
**Depends on**: Nothing (first phase)
**Requirements**: FOUND-01, FOUND-02, FOUND-03, FOUND-04
**Success Criteria** (what must be TRUE):
  1. Running an incremental sync after a full enrichment run preserves all enriched field values (upsert COALESCE direction is correct)
  2. Each trade, politician, and issuer row has an enriched_at column that is NULL for un-enriched rows and contains a timestamp for enriched rows
  3. A Db query can return the list of trade/politician/issuer IDs that need enrichment (NULL enriched_at or default sentinel values)
  4. Opening an existing database (pre-migration) with the new code applies schema changes without data loss
**Plans**: 2 plans

Plans:
- [x] 01-01-PLAN.md -- Schema migration and enriched_at columns (FOUND-02, FOUND-04)
- [x] 01-02-PLAN.md -- Fix upsert sentinel protection and enrichment query methods (FOUND-01, FOUND-03)

### Phase 2: Trade Extraction
**Goal**: The trade_detail scraper extracts every field that listing pages leave as NULL or default, with test coverage against real HTML fixtures
**Depends on**: Phase 1
**Requirements**: TRADE-01, TRADE-02, TRADE-03, TRADE-04, TRADE-05, TRADE-06
**Success Criteria** (what must be TRUE):
  1. trade_detail() returns a populated asset_type for trades that listing pages defaulted to "unknown"
  2. trade_detail() returns size, size_range_high, and size_range_low values from the RSC payload
  3. trade_detail() returns filing_id and filing_url (where the detail page provides them)
  4. trade_detail() returns price data (where the detail page provides it)
  5. Committees and labels extraction from trade detail pages is attempted, with documented findings on data availability in the RSC payload
**Plans**: 2 plans

Plans:
- [x] 02-01-PLAN.md -- Capture HTML fixtures, extend ScrapedTradeDetail and extract_trade_detail, fixture-based tests (TRADE-01 through TRADE-06)
- [x] 02-02-PLAN.md -- Add Db::update_trade_detail() method with sentinel protection and comprehensive tests (TRADE-01 through TRADE-06 persistence)

### Phase 3: Trade Sync and Output
**Goal**: Users can run sync and get fully enriched trade data in the database, with smart-skip for efficiency, crash-safe checkpointing, and enriched fields visible in all CLI output formats
**Depends on**: Phase 2
**Requirements**: TRADE-07, TRADE-08, TRADE-09, TRADE-10, TRADE-11, PERF-04, OUT-01
**Success Criteria** (what must be TRUE):
  1. After sync with trade enrichment, the trade_committees and trade_labels join tables contain data extracted from detail pages
  2. Re-running sync skips trades that already have enriched_at set and all enrichable fields populated (smart-skip)
  3. If a sync run is interrupted mid-enrichment, restarting picks up where it left off rather than re-fetching already-enriched trades (batch checkpointing)
  4. Running `capitoltraders trades --output json` (and table/csv/md/xml) shows asset_type, committees, and labels from enriched data
  5. A dry-run mode reports how many trades would be enriched without making HTTP requests
**Plans**: 3 plans

Plans:
- [x] 03-01-PLAN.md -- Sync enrichment pipeline with --enrich, --dry-run, --batch-size, smart-skip, and 500ms throttle (TRADE-07 through TRADE-11, PERF-04)
- [x] 03-02-PLAN.md -- Database trade query with JOINed committees/labels and basic filters (OUT-01 data layer)
- [x] 03-03-PLAN.md -- CLI trades --db output extension for all formats with enriched columns (OUT-01 presentation)

### Phase 4: Politician Enrichment
**Goal**: Users get complete politician records with committee memberships populated from listing page committee-filter iteration (detail pages confirmed to lack committee data), visible in all CLI output formats
**Depends on**: Phase 1
**Requirements**: POL-01, POL-02, POL-03, OUT-02
**Success Criteria** (what must be TRUE):
  1. politician_detail() extracts committee memberships from the RSC payload (or documents that the data is unavailable and an alternative approach is needed)
  2. After sync, the politician_committees join table contains committee data for enriched politicians
  3. Politician enrichment runs automatically during sync without requiring an opt-in flag
  4. Running `capitoltraders politicians --output json` (and table/csv/md/xml) shows committee memberships for enriched politicians
**Plans**: 3 plans

Plans:
- [ ] 04-01-PLAN.md -- Committee membership scraping via listing page committee-filter iteration and DB persistence (POL-01, POL-02)
- [ ] 04-02-PLAN.md -- Sync pipeline integration for automatic committee enrichment (POL-03)
- [ ] 04-03-PLAN.md -- CLI politicians --db output with committee data in all formats (OUT-02)

### Phase 5: Issuer Enrichment
**Goal**: Users get complete issuer records with performance metrics and end-of-day price history populated from detail pages, visible in all CLI output formats
**Depends on**: Phase 1
**Requirements**: ISS-01, ISS-02, ISS-03, ISS-04, OUT-03
**Success Criteria** (what must be TRUE):
  1. issuer_detail() extracts performance data (market cap, trailing returns) from the RSC payload
  2. issuer_detail() extracts end-of-day price history from the RSC payload
  3. After sync, the issuer_performance and issuer_eod_prices tables contain data for enriched issuers
  4. Running `capitoltraders issuers --output json` (and table/csv/md/xml) shows performance and EOD price data for enriched issuers
**Plans**: TBD

Plans:
- [ ] 05-01: TBD
- [ ] 05-02: TBD

### Phase 6: Concurrency and Reliability
**Goal**: Enrichment runs complete in reasonable time (hours, not days) with bounded parallelism, user-visible progress, and automatic failure recovery
**Depends on**: Phase 3, Phase 4, Phase 5
**Requirements**: PERF-01, PERF-02, PERF-03
**Success Criteria** (what must be TRUE):
  1. Detail page fetches run with bounded concurrency (3-5 parallel requests via Semaphore) instead of sequentially
  2. During enrichment, a progress bar shows current position, total count, and estimated time remaining
  3. After N consecutive HTTP failures, the enrichment pauses or stops gracefully instead of burning through retries (circuit breaker)
**Plans**: TBD

Plans:
- [ ] 06-01: TBD
- [ ] 06-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3; phases 4 and 5 can run after 1 (parallel with 2/3); phase 6 after 3/4/5.

| Phase | Plans Complete | Status | Completed |
|-------|---------------|--------|-----------|
| 1. Foundation | 2/2 | Complete | 2026-02-08 |
| 2. Trade Extraction | 2/2 | Complete | 2026-02-08 |
| 3. Trade Sync and Output | 3/3 | Complete | 2026-02-08 |
| 4. Politician Enrichment | 0/3 | Planned | - |
| 5. Issuer Enrichment | 0/TBD | Not started | - |
| 6. Concurrency and Reliability | 0/TBD | Not started | - |
