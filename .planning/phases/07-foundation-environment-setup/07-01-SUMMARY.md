---
phase: 07-foundation-environment-setup
plan: 01
subsystem: database, infra
tags: [dotenvy, serde_yml, rusqlite, sqlite, env-management, fec-mappings]

# Dependency graph
requires:
  - phase: 01-schema-migration-data-model
    provides: SQLite schema with v2 migrations (enriched_at and price columns)
provides:
  - dotenvy dependency for .env file loading at CLI startup
  - serde_yml dependency for future YAML parsing (Phase 8+)
  - .env.example template with OPENFEC_API_KEY placeholder
  - require_openfec_api_key() helper function with actionable error message
  - Schema v3 migration with fec_mappings table for politician-to-FEC-ID crosswalk
  - Composite PK (politician_id, fec_candidate_id) with bioguide_id and election cycle
  - Indexes on fec_candidate_id and bioguide_id for efficient lookups
affects: [08-fec-crosswalk-loading, 09-donation-ingestion, 10-portfolio-donations, 11-donation-queries, 12-donation-output]

# Tech tracking
tech-stack:
  added:
    - dotenvy 0.15 (workspace + CLI)
    - serde_yml 0.0.12 (workspace + lib)
  patterns:
    - .env loading silently at CLI startup (no panic if missing)
    - require_openfec_api_key() pattern for deferred API key validation
    - Schema versioning with user_version pragma (now at v3)
    - IF NOT EXISTS pattern for idempotent migrations

key-files:
  created:
    - .env.example - Template with OPENFEC_API_KEY placeholder
  modified:
    - Cargo.toml - Added dotenvy and serde_yml workspace dependencies
    - capitoltraders_cli/Cargo.toml - Added dotenvy dependency
    - capitoltraders_lib/Cargo.toml - Added serde_yml dependency
    - capitoltraders_cli/src/main.rs - Added .env loading and require_openfec_api_key()
    - schema/sqlite.sql - Added fec_mappings table and indexes
    - capitoltraders_lib/src/db.rs - Added migrate_v3(), V2_SCHEMA constant, and 4 new tests
    - .gitignore - Added !.env.example negation rule

key-decisions:
  - "dotenvy loads .env silently at startup (no panic if missing) to allow non-donation commands to work without API key"
  - "require_openfec_api_key() defers API key validation until donation commands actually need it"
  - "fec_mappings uses composite PK (politician_id, fec_candidate_id) to support multiple FEC IDs per politician across election cycles"
  - "Schema v3 migration follows IF NOT EXISTS pattern from v1/v2 for idempotency"

patterns-established:
  - "Deferred API key validation: helper function returns Result with actionable error message instead of panicking at startup"
  - "Schema migration versioning: check user_version, apply migration if needed, bump version, then apply base DDL"
  - "Test schema constants: OLD_SCHEMA, V1_SCHEMA, V2_SCHEMA for testing migration paths"

# Metrics
duration: 20min
completed: 2026-02-12
---

# Phase 7 Plan 1: Foundation & Environment Setup Summary

**dotenvy .env loading with OpenFEC API key template and schema v3 migration adding fec_mappings table for politician-to-FEC-ID crosswalk**

## Performance

- **Duration:** 20 min
- **Started:** 2026-02-12T01:19:51Z
- **Completed:** 2026-02-12T01:40:11Z
- **Tasks:** 2
- **Files modified:** 7
- **Files created:** 1

## Accomplishments
- .env file loading silently at CLI startup (missing .env is fine for non-donation commands)
- .env.example template tracked in git with OPENFEC_API_KEY placeholder
- require_openfec_api_key() helper provides actionable error with signup URL
- Schema v3 migration adds fec_mappings table with composite PK and indexes
- All 370 tests pass (366 existing + 4 new v3 migration tests)
- Zero clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Add workspace dependencies, .env loading, and .env.example** - `cd70b8f` (feat)
2. **Task 2: Schema v3 migration with fec_mappings table and tests** - `5ec5f2b` (feat)

## Files Created/Modified

### Created
- `.env.example` - Template with OPENFEC_API_KEY placeholder and api.data.gov signup instructions

### Modified
- `Cargo.toml` - Added dotenvy 0.15 and serde_yml 0.0.12 workspace dependencies
- `capitoltraders_cli/Cargo.toml` - Added dotenvy workspace dependency
- `capitoltraders_lib/Cargo.toml` - Added serde_yml workspace dependency
- `capitoltraders_cli/src/main.rs` - Added .env loading at startup, require_openfec_api_key() helper
- `schema/sqlite.sql` - Added fec_mappings table with composite PK and two indexes
- `capitoltraders_lib/src/db.rs` - Added migrate_v3(), updated init(), V2_SCHEMA constant, 4 new tests
- `.gitignore` - Added !.env.example negation to ensure template is tracked

## Decisions Made

1. **dotenvy silent loading:** Using `let _ = dotenvy::dotenv();` instead of attribute macro to avoid panicking on missing .env. Non-donation commands should work without API key.

2. **Deferred API key validation:** require_openfec_api_key() helper returns Result with actionable error message (signup URL, .env creation steps). Error is deferred until donation commands actually need the key in Phase 8+.

3. **Composite PK for fec_mappings:** Using (politician_id, fec_candidate_id) as PK to support multiple FEC candidate IDs per politician across different election cycles. bioguide_id is separate column for crosswalk flexibility.

4. **IF NOT EXISTS migration pattern:** migrate_v3() follows same pattern as v1/v2 using IF NOT EXISTS for idempotency. Fresh databases get fec_mappings from base DDL, existing v2 databases get it via migration.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all tasks executed as specified.

## User Setup Required

**External services require manual configuration.** See .env.example for:
- OPENFEC_API_KEY: Sign up at https://api.data.gov/signup/, check email for key
- Create .env file in project root with key value
- Verification: Run donation-related commands in Phase 8+ (will error with helpful message if key missing)

Note: .env is gitignored and will not be committed.

## Next Phase Readiness

- .env loading infrastructure complete
- Schema v3 with fec_mappings table ready for Phase 8 (FEC crosswalk loading)
- serde_yml dependency ready for YAML parsing
- require_openfec_api_key() helper ready for donation command integration
- All 370 tests passing, zero clippy warnings

**Ready for Phase 8 (FEC Crosswalk Loading).**

## Self-Check

Verifying all created files and commits exist:

- File: .env.example - FOUND
- File: schema/sqlite.sql (fec_mappings) - FOUND
- File: capitoltraders_lib/src/db.rs (migrate_v3) - FOUND
- Commit: cd70b8f (Task 1) - FOUND
- Commit: 5ec5f2b (Task 2) - FOUND
- Tests: 370 passing (63 + 9 + 244 + 3 + 8 + 7 + 36) - PASSED
- Clippy: zero warnings - PASSED

## Self-Check: PASSED

---
*Phase: 07-foundation-environment-setup*
*Completed: 2026-02-12*
