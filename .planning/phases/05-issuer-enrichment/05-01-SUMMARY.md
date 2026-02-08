---
phase: 05-issuer-enrichment
plan: 01
subsystem: database
tags: [sqlite, scraping, fixtures, enrichment, issuer, eod-prices, performance]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "SQLite schema with issuer_performance and issuer_eod_prices tables, enriched_at columns"
  - phase: 02-trade-extraction
    provides: "Synthetic fixture pattern, extract_rsc_payload, extract_json_object_after helpers"
provides:
  - "Synthetic issuer detail HTML fixtures (with-perf and no-perf variants)"
  - "update_issuer_detail() DB method for persisting performance + EOD data"
  - "count_unenriched_issuers() DB method for enrichment queue sizing"
  - "Fixture-based issuer_detail extraction tests (3 tests)"
  - "DB persistence tests for issuer enrichment (5 tests)"
affects: [05-issuer-enrichment, issuer-sync-pipeline, cli-issuer-output]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Issuer enrichment persistence via unchecked_transaction with COALESCE protection"
    - "Performance JSON parsed inline from serde_json::Value (not deserialized to typed struct)"
    - "EOD price DELETE+INSERT pattern for idempotent re-enrichment"

key-files:
  created:
    - "capitoltraders_lib/tests/fixtures/issuer_detail_with_performance.html"
    - "capitoltraders_lib/tests/fixtures/issuer_detail_no_performance.html"
  modified:
    - "capitoltraders_lib/src/scrape.rs"
    - "capitoltraders_lib/src/db.rs"

key-decisions:
  - "Performance JSON parsed inline from serde_json::Value rather than deserializing to DbPerformance struct -- update_issuer_detail takes ScrapedIssuerDetail which has Option<serde_json::Value> for performance, avoiding intermediate conversion"
  - "COALESCE on nullable issuer fields (state_id, c2iq, country, issuer_ticker, sector) but direct overwrite on issuer_name -- name always comes from the detail page and should be authoritative"
  - "Incomplete performance (missing required fields) treated same as null -- DELETE existing rows to avoid stale data"

patterns-established:
  - "Issuer enrichment COALESCE: nullable fields protected, issuer_name always overwritten, enriched_at set via datetime('now')"
  - "Performance validation inline: check all 20 required fields present and non-null before persisting"
  - "EOD price refresh: DELETE all for issuer_id then INSERT new entries in same transaction"

# Metrics
duration: 4min
completed: 2026-02-08
---

# Phase 5 Plan 1: Issuer Detail Fixtures and DB Persistence Summary

**Synthetic issuer detail fixtures with performance/EOD extraction tests, plus update_issuer_detail() and count_unenriched_issuers() DB methods**

## Performance

- **Duration:** 4 min
- **Started:** 2026-02-08T22:00:08Z
- **Completed:** 2026-02-08T22:04:00Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Created 2 synthetic HTML fixtures modeling issuer detail RSC payload structure (with and without performance data)
- Added 3 fixture-based extraction tests verifying issuer_detail() parsing for performance, no-performance, and EOD price array variants
- Implemented update_issuer_detail() with COALESCE protection, performance/EOD persistence, and enriched_at timestamping
- Implemented count_unenriched_issuers() for enrichment queue sizing
- Added 5 DB persistence tests covering with-performance, no-performance, field preservation, counting, and EOD replacement
- All 279 workspace tests pass with zero regressions and zero clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Create synthetic fixtures and fixture-based extraction tests** - `18d15a1` (feat)
2. **Task 2: Add update_issuer_detail and count_unenriched_issuers DB methods with tests** - `86b0ea2` (feat)

## Files Created/Modified
- `capitoltraders_lib/tests/fixtures/issuer_detail_with_performance.html` - Synthetic fixture with full performance data (AAPL, mcap 3.5T, 3 EOD prices)
- `capitoltraders_lib/tests/fixtures/issuer_detail_no_performance.html` - Synthetic fixture with performance: null (PrivateCo Holdings)
- `capitoltraders_lib/src/scrape.rs` - Added 3 fixture-based issuer_detail extraction tests
- `capitoltraders_lib/src/db.rs` - Added update_issuer_detail(), count_unenriched_issuers() methods and 5 tests

## Decisions Made
- Performance JSON parsed inline from serde_json::Value rather than deserializing to typed struct. The update_issuer_detail method receives ScrapedIssuerDetail which has Option<serde_json::Value> for performance, avoiding unnecessary intermediate conversion to DbPerformance.
- COALESCE on nullable issuer fields but direct overwrite on issuer_name. The detail page is authoritative for the name, while fields like state_id and sector should not be cleared if the detail page lacks them.
- Incomplete performance treated same as null. If any of the 20 required fields is missing or null, existing performance and EOD data is deleted to avoid stale partial data.

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None -- no external service configuration required.

## Next Phase Readiness
- update_issuer_detail() and count_unenriched_issuers() are ready for the sync pipeline (05-02)
- Fixtures provide structural reference for any live-site fixture capture
- The enrichment queue pattern (enriched_at IS NULL) is consistent with trades and politicians

---
*Phase: 05-issuer-enrichment*
*Completed: 2026-02-08*
