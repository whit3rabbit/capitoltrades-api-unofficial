---
phase: 11-donations-cli-command
plan: 01
subsystem: database
tags: [rusqlite, donations, sql, aggregations, filtering]

# Dependency graph
requires:
  - phase: 10-donation-sync-pipeline
    provides: donations table, donation_sync_meta table, fec_committees table
provides:
  - DonationFilter and row types for all query modes
  - query_donations for individual donation records
  - query_donations_by_contributor for contributor aggregations
  - query_donations_by_employer for employer aggregations
  - query_donations_by_state for state aggregations
  - build_donation_where_clause helper for shared filter logic
affects: [11-02-donations-cli-command]

# Tech tracking
tech-stack:
  added: []
  patterns: [dynamic WHERE clause builder for donation queries, COALESCE for NULL handling in aggregations, shared filter helper to avoid code duplication]

key-files:
  created: []
  modified: [capitoltraders_lib/src/db.rs, capitoltraders_lib/src/lib.rs]

key-decisions:
  - "All donation queries join through donation_sync_meta (not direct politician_id on donations table)"
  - "NULL contributor names display as 'Unknown' via COALESCE in all queries"
  - "build_donation_where_clause shared helper avoids duplicating filter logic across 4 query methods"
  - "Aggregations use COUNT(DISTINCT contributor_name) for accurate contributor counts despite NULL values"

patterns-established:
  - "Dynamic WHERE clause construction with parameterized queries (?N notation)"
  - "COALESCE for NULL handling in both SELECT and GROUP BY clauses"
  - "Shared filter builder function for query methods with common filtering logic"

# Metrics
duration: 4min
completed: 2026-02-13
---

# Phase 11 Plan 01: Donation Query Foundation Summary

**Database layer with 4 donation query methods (individual + 3 aggregation modes) and 8 comprehensive unit tests covering filters, NULL handling, and sort order**

## Performance

- **Duration:** 4 minutes
- **Started:** 2026-02-13T20:30:11Z
- **Completed:** 2026-02-13T20:34:30Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- DonationFilter struct with 6 fields (politician_id, cycle, min_amount, employer, contributor_state, limit)
- DonationRow struct with 11 fields for individual donation display
- 3 aggregation row types (ContributorAggRow, EmployerAggRow, StateAggRow) with total/count/avg metrics
- 4 query methods with dynamic filtering and shared WHERE clause builder
- 8 unit tests with comprehensive coverage of filters, aggregations, NULL handling, and sort order verification

## Task Commits

Each task was committed atomically:

1. **Task 1: Donation filter and row types + individual query** - `e0b5a67` (feat)
2. **Task 2: Aggregation query methods and unit tests** - `c549633` (feat)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - Added DonationFilter, DonationRow, ContributorAggRow, EmployerAggRow, StateAggRow structs, 4 query methods (query_donations, query_donations_by_contributor, query_donations_by_employer, query_donations_by_state), build_donation_where_clause helper, and 8 unit tests
- `capitoltraders_lib/src/lib.rs` - Re-exported 5 donation types (DonationFilter, DonationRow, ContributorAggRow, EmployerAggRow, StateAggRow)

## Decisions Made

**All donation queries join through donation_sync_meta:**
- Donations table only has committee_id, not politician_id
- donation_sync_meta provides the (politician_id, committee_id) crosswalk
- All 4 query methods JOIN donations to donation_sync_meta to access politician data

**NULL handling via COALESCE:**
- contributor_name: NULL becomes 'Unknown'
- contributor_employer: NULL becomes 'Unknown' in employer aggregations
- contributor_state: NULL becomes 'Unknown' in state aggregations
- Applied in both SELECT and GROUP BY clauses for consistency

**Shared filter builder helper:**
- build_donation_where_clause extracts common filtering logic
- Returns (where_clause_string, params_vec) tuple
- Used by all 4 query methods to avoid duplication
- Follows existing pattern from query_trades (Vec<Box<dyn ToSql>> with ?N placeholders)

**Aggregation contributor counts:**
- COUNT(DISTINCT contributor_name) used for accurate counts
- NULL values excluded from distinct count automatically by SQL
- Verified in test_query_donations_by_state (CA has 1 distinct contributor despite 3 donations)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

**Missing last_synced field in test committee insert:**
- fec_committees.last_synced is NOT NULL
- Initial test setup omitted this field, causing constraint violation
- Fixed by adding last_synced = '2024-01-01T00:00:00Z' to INSERT
- All 9 tests passed after fix

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Ready for Phase 11 Plan 02 (donations CLI command implementation):
- All query methods available and tested
- Filter struct provides flexible filtering for CLI flags
- Aggregation modes ready for --group-by flag
- NULL handling verified and consistent
- 473 total tests passing (added 9 new donation query tests)

## Self-Check: PASSED

Files verified:
- FOUND: capitoltraders_lib/src/db.rs
- FOUND: capitoltraders_lib/src/lib.rs

Commits verified:
- FOUND: e0b5a67 (Task 1: Donation filter and row types + individual query)
- FOUND: c549633 (Task 2: Aggregation query methods and unit tests)

---
*Phase: 11-donations-cli-command*
*Completed: 2026-02-13*
