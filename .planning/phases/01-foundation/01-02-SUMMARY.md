---
phase: 01-foundation
plan: 02
subsystem: database
tags: [sqlite, upsert, sentinel-protection, coalesce, enrichment-query, data-integrity]

# Dependency graph
requires:
  - phase: 01-foundation plan 01
    provides: enriched_at TEXT columns in trades/politicians/issuers tables, enrichment indexes
provides:
  - Sentinel-protected upsert SQL (CASE expressions) in upsert_trades and upsert_scraped_trades
  - COALESCE protection for nullable columns (price, size, size_range_high, size_range_low)
  - enriched_at preservation in all 4 upsert ON CONFLICT clauses
  - get_unenriched_trade_ids, get_unenriched_politician_ids, get_unenriched_issuer_ids query methods
affects: [phase 2-5 (enrichment pipeline relies on upsert safety and query methods), phase 3 (sync smart-skip uses get_unenriched_*)]

# Tech tracking
tech-stack:
  added: []
  patterns: [SQLite CASE expressions for sentinel value protection, COALESCE for nullable column preservation, enriched_at pinning in ON CONFLICT]

key-files:
  created: []
  modified:
    - capitoltraders_lib/src/db.rs

key-decisions:
  - "None - followed plan as specified"

patterns-established:
  - "Sentinel CASE pattern: WHEN excluded.field != sentinel THEN excluded.field ELSE table.field END -- prevents listing-page defaults from overwriting enriched values"
  - "enriched_at pinning: every upsert ON CONFLICT clause includes enriched_at = table.enriched_at to prevent timestamp clobbering"
  - "Enrichment queue pattern: SELECT id FROM table WHERE enriched_at IS NULL ORDER BY id [LIMIT n]"

# Metrics
duration: 3min
completed: 2026-02-08
---

# Phase 1 Plan 2: Upsert Sentinel Protection Summary

**CASE/COALESCE sentinel protection in 4 upsert functions preventing re-sync data corruption, plus enrichment queue query methods for trade/politician/issuer IDs**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-08T04:01:44Z
- **Completed:** 2026-02-08T04:05:13Z
- **Tasks:** 3
- **Files modified:** 1

## Accomplishments
- Fixed data corruption bug (FOUND-01): listing-page re-syncs no longer overwrite enriched field values with sentinel defaults (filing_id=0, filing_url='', asset_type='unknown', has_capital_gains=0)
- COALESCE protection for nullable columns (price, size, size_range_high, size_range_low) preserves non-NULL values when incoming data is NULL
- enriched_at timestamp preserved in all 4 upsert functions (upsert_trades, upsert_scraped_trades, upsert_politicians, upsert_issuers)
- 3 enrichment queue methods (FOUND-03) enable downstream phases to query which records need enrichment
- 11 new tests verify sentinel protection, COALESCE behavior, enriched_at preservation, and enrichment queries

## Task Commits

Each task was committed atomically:

1. **Task 1a: Fix sentinel protection and enriched_at in trade upsert functions** - `a659938` (fix)
2. **Task 1b: Add enriched_at preservation to upsert_politicians and upsert_issuers** - `a624b3a` (fix)
3. **Task 2: Add enrichment query methods and comprehensive tests** - `d42538f` (feat)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - CASE expressions for filing_id/filing_url/asset_type/has_capital_gains in upsert_trades and upsert_scraped_trades; COALESCE for price/size/size_range_high/size_range_low; enriched_at preservation in all 4 upsert functions; 3 get_unenriched_* query methods; 11 new tests with make_test_scraped_trade helper

## Decisions Made
None - followed plan as specified.

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 1 is now complete: all 4 success criteria verified (upsert safety, enriched_at columns, enrichment queries, migration safety)
- Phase 2 (Trade Extraction) can proceed: upsert layer is safe for enriched data, enrichment queue queries are available
- Phases 4 and 5 (Politician/Issuer Enrichment) can also proceed independently: they depend only on Phase 1
- No blockers

## Self-Check: PASSED

- capitoltraders_lib/src/db.rs: FOUND
- 01-02-SUMMARY.md: FOUND
- Commit a659938 (Task 1a): FOUND in git log
- Commit a624b3a (Task 1b): FOUND in git log
- Commit d42538f (Task 2): FOUND in git log
- enriched_at = trades.enriched_at: 2 occurrences (upsert_trades, upsert_scraped_trades)
- enriched_at = politicians.enriched_at: 3 occurrences (upsert_trades, upsert_scraped_trades, upsert_politicians)
- enriched_at = issuers.enriched_at: 3 occurrences (upsert_trades, upsert_scraped_trades, upsert_issuers)
- WHEN excluded.asset_type: 2 occurrences (both asset upserts)
- WHEN excluded.filing_id: 2 occurrences (both trade upserts)
- cargo test --workspace: 209 tests pass (198 existing + 11 new)
- cargo clippy --workspace: no warnings

---
*Phase: 01-foundation*
*Completed: 2026-02-08*
