---
phase: 09-politician-to-committee-mapping-schema-v3
plan: 01
subsystem: database
tags: [sqlite, schema-migration, openfec, json-columns, committee-metadata]

# Dependency graph
requires:
  - phase: 08-openfec-api-client
    provides: OpenFEC Committee type definition for storage
  - phase: 07-fec-candidate-mapping
    provides: fec_mappings table structure for committee_ids column
provides:
  - Schema v4 migration with donations, fec_committees, donation_sync_meta tables
  - Committee metadata storage and retrieval operations
  - JSON column support for committee_ids on fec_mappings
affects: [10-committee-resolver-service, 11-donation-ingestion, 12-employer-matching]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "JSON column storage in SQLite for array data (committee_ids)"
    - "Schema v4 migration pattern following v1/v2/v3 conventions"
    - "Composite table creation in single migration (3 tables + 1 ALTER + 5 indexes)"

key-files:
  created: []
  modified:
    - capitoltraders_lib/src/db.rs
    - schema/sqlite.sql

key-decisions:
  - "No FOREIGN KEY from donations to fec_committees (donations may arrive before committee metadata)"
  - "No FOREIGN KEY from donation_sync_meta to politicians (reduces cascade overhead for metadata-only table)"
  - "Refactored upsert_committee to accept Committee struct to avoid clippy 8-parameter warning"
  - "JSON column merges across multiple FEC candidate IDs for same politician (deduplicates committee list)"

patterns-established:
  - "Schema version 4 includes all v1-v4 tables in base DDL for fresh databases"
  - "Existing migration tests updated to expect new version (avoid false negatives)"
  - "JSON serialization/deserialization via serde_json for TEXT columns"

# Metrics
duration: 5 min
completed: 2026-02-13
---

# Phase 09 Plan 01: Schema v4 Migration and Committee Storage Summary

**Schema v4 adds FEC committee metadata storage, donation tracking tables, and JSON column support for politician-to-committee mappings**

## Performance

- **Duration:** 5 min
- **Started:** 2026-02-13T01:22:35Z
- **Completed:** 2026-02-13T01:27:49Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Schema v4 migration with 3 new tables (fec_committees, donations, donation_sync_meta)
- Added committee_ids JSON column to fec_mappings table
- 5 new indexes for donation queries and committee lookup performance
- Committee DB operations (upsert, batch upsert, get, update)
- JSON column read/write with proper edge case handling (null, empty string, merging)
- 10 comprehensive tests covering migration paths and all committee operations

## Task Commits

Each task was committed atomically:

1. **Task 1: Schema v4 migration and base DDL update** - `a09c334` (feat)
2. **Task 2: Committee DB operations and JSON column support** - `29600e6` (feat)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - Added migrate_v4(), 5 committee DB methods, 10 tests
- `schema/sqlite.sql` - Added fec_committees, donations, donation_sync_meta tables, committee_ids column, 5 indexes

## Decisions Made
- No FOREIGN KEY from donations to fec_committees to allow donations before committee metadata is fetched
- No FOREIGN KEY from donation_sync_meta to politicians to reduce CASCADE overhead for metadata-only tracking
- Refactored upsert_committee to accept Committee struct instead of 8 parameters (clippy warning fix)
- JSON column merge across multiple FEC candidate IDs for same politician to deduplicate committee list

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Updated existing migration tests to expect version 4**
- **Found during:** Task 1 (after adding migrate_v4 to init())
- **Issue:** 7 existing tests were asserting user_version == 3, causing false negatives
- **Fix:** Updated all `assert_eq!(version, 3)` to `assert_eq!(version, 4)` and `assert_eq!(get_user_version(&db), 3)` to expect 4
- **Files modified:** capitoltraders_lib/src/db.rs (test module)
- **Verification:** All 406 existing tests pass without regression
- **Committed in:** a09c334 (Task 1 commit)

**2. [Rule 1 - Bug] Refactored upsert_committee to fix clippy warning**
- **Found during:** Task 2 (clippy check after implementation)
- **Issue:** clippy reported "this function has too many arguments (8/7)" for upsert_committee with individual parameters
- **Fix:** Changed signature from individual parameters to accepting `&Committee` struct, simplified upsert_committees to call upsert_committee per item
- **Files modified:** capitoltraders_lib/src/db.rs (upsert_committee, upsert_committees, 3 test functions)
- **Verification:** clippy clean, all tests pass
- **Committed in:** 29600e6 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both fixes were correctness requirements (test accuracy, code quality). No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Schema v4 complete and tested (fresh DB, upgrade from v3, idempotency)
- Committee storage operations ready for Phase 10 CommitteeResolver consumption
- JSON column support validated with round-trip tests
- Ready for Plan 02: Committee resolution service implementation

---
*Phase: 09-politician-to-committee-mapping-schema-v3*
*Completed: 2026-02-13*
