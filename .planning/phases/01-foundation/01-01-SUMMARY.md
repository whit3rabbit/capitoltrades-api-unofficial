---
phase: 01-foundation
plan: 01
subsystem: database
tags: [sqlite, migration, pragma-user-version, enriched-at, schema]

# Dependency graph
requires:
  - phase: none
    provides: none (first plan)
provides:
  - enriched_at TEXT column in trades, politicians, and issuers tables
  - PRAGMA user_version-gated schema migration (v0 -> v1)
  - Enrichment indexes (idx_trades_enriched, idx_politicians_enriched, idx_issuers_enriched)
  - Db::open_in_memory() test constructor
affects: [01-02 (upsert fixes reference enriched_at), phase 2-5 (enrichment writes to enriched_at)]

# Tech tracking
tech-stack:
  added: []
  patterns: [PRAGMA user_version for schema versioning, ALTER TABLE with duplicate-column safety catch]

key-files:
  created: []
  modified:
    - schema/sqlite.sql
    - capitoltraders_lib/src/db.rs

key-decisions:
  - "Run migration before schema batch in init() so enrichment indexes can reference enriched_at on pre-migration databases"
  - "Catch both 'duplicate column name' and 'no such table' in migrate_v1() to handle all database states safely"

patterns-established:
  - "Schema versioning: PRAGMA user_version tracks migration state; each migration is a numbered private method (migrate_v1, migrate_v2, etc.)"
  - "Migration safety: ALTER TABLE ADD COLUMN with error suppression for idempotent reruns"

# Metrics
duration: 3min
completed: 2026-02-08
---

# Phase 1 Plan 1: Schema Migration Summary

**enriched_at TEXT columns added to trades/politicians/issuers with PRAGMA user_version-gated migration supporting new, existing, and already-migrated databases**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-08T03:53:53Z
- **Completed:** 2026-02-08T03:57:23Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Added enriched_at TEXT column to all three entity tables (trades, politicians, issuers) in sqlite.sql DDL
- Implemented PRAGMA user_version migration system in Db::init() with migrate_v1() for existing databases
- Added 3 enrichment indexes for efficient "needs enrichment" queries
- Added 4 tests covering column creation, idempotency, migration on existing databases, and NULL default behavior

## Task Commits

Each task was committed atomically:

1. **Task 1: Add enriched_at columns to schema DDL and implement version-gated migration** - `66f35a2` (feat)
2. **Task 2: Add tests for migration and enriched_at column behavior** - `93b1177` (test)

## Files Created/Modified
- `schema/sqlite.sql` - Added enriched_at TEXT to issuers, politicians, trades CREATE TABLE definitions; added 3 enrichment indexes
- `capitoltraders_lib/src/db.rs` - Added PRAGMA user_version check and migrate_v1() in init(); added open_in_memory() test constructor; added 4 migration tests

## Decisions Made
- Reordered init() to run migration before schema batch (see Deviation 1 below)
- Added "no such table" catch in migrate_v1() so migration is safe on brand-new databases where tables do not exist yet

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed init() ordering: migration must run before schema batch**
- **Found during:** Task 2 (test_migration_on_existing_db)
- **Issue:** The plan specified running the schema batch first, then checking user_version and running migration. But the updated schema includes `CREATE INDEX IF NOT EXISTS idx_trades_enriched ON trades(enriched_at)` -- on a pre-migration database where tables exist without enriched_at, CREATE TABLE IF NOT EXISTS is a no-op (tables already exist), so enriched_at columns are not added, but the index creation fails with "no such column: enriched_at".
- **Fix:** Reordered init() to: (1) check user_version, (2) run migrate_v1() if needed, (3) run schema batch. Also added "no such table" to the error suppression in migrate_v1() so brand-new databases (no tables yet) skip ALTER TABLE gracefully.
- **Files modified:** capitoltraders_lib/src/db.rs
- **Verification:** All 4 migration tests pass; full workspace test suite (198 tests) passes.
- **Committed in:** 93b1177 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential correctness fix for migration ordering. The plan's specified order would have failed on existing databases. No scope creep.

## Issues Encountered
None beyond the deviation documented above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- enriched_at columns are in place for Plan 01-02 (upsert sentinel protection and enrichment query methods)
- PRAGMA user_version = 1 is set, ready for future migrations (v2+) if needed
- No blockers for 01-02

## Self-Check: PASSED

- schema/sqlite.sql: FOUND, 3 enriched_at TEXT columns verified
- capitoltraders_lib/src/db.rs: FOUND, user_version and migrate_v1 verified
- 01-01-SUMMARY.md: FOUND
- Commit 66f35a2 (Task 1): FOUND in git log
- Commit 93b1177 (Task 2): FOUND in git log
- cargo test --workspace: 198 tests pass (194 existing + 4 new)
- cargo clippy --workspace: no warnings

---
*Phase: 01-foundation*
*Completed: 2026-02-08*
