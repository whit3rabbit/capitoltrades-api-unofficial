---
phase: 01-schema-migration-data-model
plan: 01
subsystem: database
tags: [sqlite, schema-migration, rusqlite, data-model]

# Dependency graph
requires:
  - phase: 00-foundation
    provides: Base SQLite schema with trades, politicians, issuers tables and v1 enriched_at migration
provides:
  - migrate_v2() function that adds 5 price columns to trades table
  - positions table with composite PK (politician_id, issuer_ticker) and CASCADE DELETE
  - DbTradeRow struct with trade_date_price, current_price, price_enriched_at, estimated_shares, estimated_value fields
  - query_trades() SELECT updated to return new price columns
  - V1_SCHEMA test constant for migration testing
  - 4 new migration and price field tests
affects: [02-yahoo-finance, 03-price-enrichment, 04-portfolio-calculation]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Schema versioning via user_version pragma with progressive migrations"
    - "Migration idempotency via 'duplicate column name' error catching"
    - "Fresh DB initialization includes all columns from all migrations in base schema"

key-files:
  created: []
  modified:
    - capitoltraders_lib/src/db.rs
    - schema/sqlite.sql
    - capitoltraders_cli/src/output_tests.rs

key-decisions:
  - "Use REAL (not INTEGER) for estimated_shares to preserve precision when midpoint/price division produces fractional shares"
  - "Add price columns to base schema.sql for fresh DBs while keeping ALTER TABLE migrations for existing DBs"
  - "Update existing migration tests to expect user_version 2 after fresh init"

patterns-established:
  - "Migration pattern: Check version, run migration if needed, bump version, then execute schema batch"
  - "Test pattern: V1_SCHEMA constant represents post-v1 state for testing v1-to-v2 migration"

# Metrics
duration: 5min
completed: 2026-02-10
---

# Phase 01 Plan 01: Schema Migration v2 Summary

**SQLite schema v2 with 5 price columns on trades and materialized positions table for FIFO portfolio tracking**

## Performance

- **Duration:** 5 min 37 sec
- **Started:** 2026-02-10T11:57:56Z
- **Completed:** 2026-02-10T12:03:33Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added v2 migration that adds 5 nullable price columns to trades table (trade_date_price, current_price, price_enriched_at, estimated_shares, estimated_value)
- Created positions table with composite PK (politician_id, issuer_ticker), REAL shares_held/cost_basis for precision, and CASCADE DELETE foreign key
- Updated DbTradeRow struct and query_trades() to include new price fields as Option<f64>/Option<String>
- Added 4 new tests covering v1-to-v2 migration, idempotency, and price field querying (populated and null cases)
- All 298 tests pass (up from 288 baseline, added 4 new + 6 upstream tests), no clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Schema migration v2 and positions table DDL** - `16b2139` (feat)
   - migrate_v2() method adds 5 price columns with duplicate column name catching
   - positions table DDL in schema.sql with 4 new indexes

2. **Task 2: Update DbTradeRow, query_trades, and all tests** - `ec66e43` (feat)
   - DbTradeRow struct extended with 5 new Option fields
   - query_trades() SELECT and row mapping updated (committees/labels now at indices 22/23)
   - V1_SCHEMA constant added for migration testing
   - 4 new tests: v1-to-v2 migration, v2 idempotency, price fields populated, price fields null

**Plan metadata:** (will be committed after SUMMARY.md and STATE.md updates)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - Added migrate_v2() method, updated DbTradeRow struct with 5 new price fields, updated query_trades() SELECT and row mapping, added V1_SCHEMA test constant and 4 new tests
- `schema/sqlite.sql` - Added positions table DDL with composite PK and 3 indexes, added idx_trades_price_enriched index, added 5 price columns to trades table for fresh DB initialization
- `capitoltraders_cli/src/output_tests.rs` - Updated sample_db_trade_row() to include new price fields as None

## Decisions Made
1. **REAL vs INTEGER for estimated_shares:** Used REAL (maps to Option<f64> in Rust) because midpoint/price division rarely produces whole shares. Plan explicitly called this out as anti-pattern to avoid.
2. **Price columns in base schema:** Added price columns to schema.sql trades table to ensure fresh DBs have them from the start. Migrations (migrate_v2) are for upgrading existing DBs only.
3. **Test version expectations:** Updated test_init_idempotent and test_migration_on_existing_db to expect user_version 2 after init, since fresh DBs now run both v1 and v2 migrations.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added price columns to schema.sql trades table**
- **Found during:** Task 2 (Test execution)
- **Issue:** Fresh DBs created via init() were running migrations (which silently skip on non-existent tables), then schema batch created trades table WITHOUT price columns. New indexes on price_enriched_at then failed because column didn't exist.
- **Fix:** Added trade_date_price, current_price, price_enriched_at, estimated_shares, estimated_value columns to CREATE TABLE trades in schema.sql. This ensures fresh DBs have all columns from the start. Migrations remain for upgrading existing DBs.
- **Files modified:** schema/sqlite.sql
- **Verification:** test_migration_v2_idempotent passed after fix, cargo test --workspace shows all 298 tests passing
- **Committed in:** ec66e43 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Fix was necessary for correctness - without it, fresh DBs would be missing price columns and tests would fail. This is the standard pattern: migrations for existing DBs, base schema includes all columns for fresh DBs.

## Issues Encountered

**Schema initialization order confusion:** Initially thought migrations would run on fresh DBs and add columns. Realized migrations silently skip on "no such table" errors, and schema batch creates base tables. Solution: base schema must include all columns from all migrations. Migrations only run on existing DBs.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

**Ready for Phase 02 (Yahoo Finance Integration):**
- trade_date_price and current_price columns exist and are queryable
- estimated_shares and estimated_value columns ready for enrichment pipeline
- positions table exists with correct schema for FIFO portfolio materialization

**Ready for Phase 03 (Price Enrichment):**
- price_enriched_at column for tracking enrichment timestamps
- DbTradeRow includes all price fields for enrichment pipeline queries

**No blockers.** Schema v2 migration tested with v0->v2, v1->v2, and fresh DB paths. Idempotency verified.

---
*Phase: 01-schema-migration-data-model*
*Completed: 2026-02-10*

## Self-Check: PASSED

All claims verified:
- Files modified exist: capitoltraders_lib/src/db.rs, schema/sqlite.sql, capitoltraders_cli/src/output_tests.rs
- Commits exist: 16b2139, ec66e43
- Test count: 298 tests running (baseline 288 + 4 new + 6 upstream = 298)

