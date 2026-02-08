---
phase: 02-trade-extraction
plan: 02
subsystem: database
tags: [sqlite, rusqlite, coalesce, sentinel-protection, enrichment]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "schema with enriched_at columns, sentinel CASE pattern, COALESCE pattern"
  - phase: 02-trade-extraction (plan 01)
    provides: "ScrapedTradeDetail struct with 10 fields"
provides:
  - "Db::update_trade_detail() method for persisting enrichment results"
  - "10 tests covering all update paths and edge cases"
affects: [03-trade-sync, 04-politician-enrichment, 05-issuer-enrichment]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "unchecked_transaction() for &self methods needing atomicity"
    - "asset_type upgrade pattern: only update from unknown to real type"
    - "join table refresh: delete+insert when new data available, skip when empty"

key-files:
  created: []
  modified:
    - "capitoltraders_lib/src/db.rs"

key-decisions:
  - "Used unchecked_transaction() instead of &mut self for API consistency with get_unenriched_*_ids methods"
  - "Asset type only upgrades from unknown -- second enrichment with different type does not overwrite"
  - "Join tables (committees, labels) only refreshed when incoming data is non-empty -- empty vec is a no-op, not a clear"

patterns-established:
  - "unchecked_transaction for &self: use when method does not need exclusive access but needs atomicity"
  - "asset_type one-way upgrade: WHERE asset_type = 'unknown' guard prevents overwrite of previously enriched values"

# Metrics
duration: 4min
completed: 2026-02-08
---

# Phase 2 Plan 2: Trade Detail DB Persistence Summary

**Db::update_trade_detail() method with COALESCE/CASE sentinel protection across trades, assets, and join tables**

## Performance

- **Duration:** 4 min
- **Started:** 2026-02-08T15:18:33Z
- **Completed:** 2026-02-08T15:22:45Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Db::update_trade_detail() persists all 10 ScrapedTradeDetail fields to 4 tables in a single transaction
- COALESCE pattern for nullable fields (price, size, size_range_high, size_range_low, has_capital_gains) prevents NULL from overwriting existing values
- CASE sentinel pattern for filing_id (sentinel: 0) and filing_url (sentinel: "") matches Phase 1 conventions
- Asset type upgrade is one-directional: only updates from "unknown" to a real type, never overwrites enriched values
- 10 new tests bring workspace total to 235

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Db::update_trade_detail() method** - `5586dfe` (feat)
2. **Task 2: Add comprehensive tests for update_trade_detail** - `dc3b712` (test)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - Added update_trade_detail() method (83 lines) and 10 tests (337 lines)

## Decisions Made
- Used `unchecked_transaction()` for atomicity with `&self` receiver, matching the pattern of existing get_unenriched_*_ids methods that also take `&self`. This avoids requiring `&mut self` which would be inconsistent with the enrichment query API.
- Asset type is a one-way upgrade: `WHERE asset_type = 'unknown'` prevents a second enrichment pass from overwriting a previously enriched value. This is intentional -- once we know it is "stock", a later extraction yielding "etf" should not silently replace it.
- Empty committees/labels vectors are treated as "no data" (no-op), not "clear all". This prevents a failed extraction from wiping previously extracted data.

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness
- Phase 2 (Trade Extraction) is complete: both plans delivered
- Plan 02-01 provides ScrapedTradeDetail extraction from RSC payloads
- Plan 02-02 provides Db::update_trade_detail() to persist those extractions
- Ready for Phase 3 (Trade Sync) which will orchestrate: fetch page -> upsert trades -> get unenriched IDs -> scrape detail -> update_trade_detail
- Blockers: TRADE-05 (committees) and TRADE-06 (labels) extraction is tested against synthetic fixtures only; may need adjustment when live RSC payloads are available

---
*Phase: 02-trade-extraction*
*Completed: 2026-02-08*
