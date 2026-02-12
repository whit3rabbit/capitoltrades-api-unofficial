---
phase: 07-foundation-environment-setup
plan: 02
subsystem: api, database, cli
tags: [fec-mapping, congress-legislators, yaml, serde_yml, reqwest, sqlite, sync-fec]

# Dependency graph
requires:
  - phase: 07-01
    provides: Schema v3 with fec_mappings table, serde_yml dependency
provides:
  - FEC mapping module with YAML types for congress-legislators dataset parsing
  - download_legislators() async function for fetching current + historical legislators
  - match_legislators_to_politicians() using (last_name, state) composite key matching
  - DB methods: upsert_fec_mappings, get_fec_ids_for_politician, get_politician_id_for_bioguide, get_politicians_for_fec_matching, count_fec_mappings
  - sync-fec CLI command for populating fec_mappings table
  - 15 new tests (9 matching logic + 6 DB operations)
affects: [08-openfec-api-client, 09-donation-ingestion]

# Tech tracking
tech-stack:
  added:
    - reqwest workspace dependency in CLI crate (for congress-legislators HTTP download)
  patterns:
    - (last_name, state) composite key matching for politician-to-legislator resolution
    - Case-insensitive name matching with collision detection (skip if multiple matches)
    - ON CONFLICT upsert pattern for idempotent FEC mapping sync

key-files:
  created:
    - capitoltraders_lib/src/fec_mapping.rs - YAML types, download, matching logic, 9 tests
    - capitoltraders_cli/src/commands/sync_fec.rs - sync-fec CLI command implementation
  modified:
    - capitoltraders_lib/src/lib.rs - Export fec_mapping module and types
    - capitoltraders_lib/src/db.rs - Add 5 FEC mapping DB methods + 6 tests
    - capitoltraders_cli/src/commands/mod.rs - Register sync_fec module
    - capitoltraders_cli/src/main.rs - Add SyncFec command variant and dispatch
    - capitoltraders_cli/Cargo.toml - Add reqwest workspace dependency

key-decisions:
  - "Use (last_name, state) composite key for matching instead of first_name matching to minimize false positives"
  - "Skip matches when multiple politicians have same (last_name, state) to avoid incorrect FEC ID assignment"
  - "Store bioguide_id in fec_mappings table even though it's not used for lookup (audit trail and future enhancement)"
  - "Download both current + historical legislators to maximize match coverage"
  - "Use tracing::warn! for collision detection instead of failing the entire sync"

patterns-established:
  - "Name-based entity matching: lowercase normalization, collision detection, graceful skipping"
  - "Congress-legislators dataset integration: combine current + historical YAML files into single Vec"
  - "Multi-FEC-ID handling: one FecMapping per FEC candidate ID, multiple rows per politician"

# Metrics
duration: 37min
completed: 2026-02-12
---

# Phase 7 Plan 2: FEC Mapping Module & Sync Summary

**Congress-legislators YAML parsing with (last_name, state) name matching, producing 5 DB operations and sync-fec CLI command with 15 new tests**

## Performance

- **Duration:** 37 min
- **Started:** 2026-02-12T01:58:10Z
- **Completed:** 2026-02-12T02:35:15Z
- **Tasks:** 2
- **Files modified:** 5
- **Files created:** 2

## Accomplishments
- FEC mapping module with YAML types (Legislator, LegislatorId, LegislatorName, Term) and FecMapping result type
- download_legislators() fetches both current + historical legislators from unitedstates/congress-legislators
- match_legislators_to_politicians() uses (last_name, state) composite key with collision detection
- 5 new DB methods for FEC mapping operations (upsert, lookup by politician, lookup by bioguide, count)
- sync-fec CLI command downloads YAML, matches politicians, persists to fec_mappings table
- 15 new tests: 9 matching logic edge cases + 6 DB operations tests
- All 385 tests pass (370 existing + 15 new), zero clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: FEC mapping module with YAML types, download, parse, and matching logic** - `9b37a43` (feat)
2. **Task 2: DB operations for FEC mappings and sync-fec CLI command** - `2088fb1` (feat)

## Files Created/Modified

### Created
- `capitoltraders_lib/src/fec_mapping.rs` - 438 lines with YAML types, FecMappingError, download_legislators(), match_legislators_to_politicians(), 9 unit tests
- `capitoltraders_cli/src/commands/sync_fec.rs` - sync-fec command with 5-step process (check politicians, download, match, persist, report)

### Modified
- `capitoltraders_lib/src/lib.rs` - Export fec_mapping module with FecMapping, FecMappingError, Legislator, and functions
- `capitoltraders_lib/src/db.rs` - Add 5 DB methods (get_politicians_for_fec_matching, upsert_fec_mappings, get_fec_ids_for_politician, get_politician_id_for_bioguide, count_fec_mappings) + 6 tests
- `capitoltraders_cli/src/commands/mod.rs` - Register sync_fec module
- `capitoltraders_cli/src/main.rs` - Add SyncFec command variant and dispatch to commands::sync_fec::run
- `capitoltraders_cli/Cargo.toml` - Add reqwest workspace dependency

## Decisions Made

1. **Composite key matching:** Use (last_name, state) instead of first_name matching. Last names are more stable across datasets (some legislators use nicknames vs official first names), and state narrows the match space significantly.

2. **Collision handling:** When multiple politicians match the same (last_name, state), skip the match entirely with a warning log. Better to miss a match than assign FEC IDs incorrectly. Real-world data is expected to have few/no collisions given small number of representatives per state.

3. **Bioguide ID storage:** Store bioguide_id in fec_mappings even though it's not used for primary lookups. Provides audit trail for how matches were made and enables future reverse lookups (bioguide -> politician_id).

4. **Both datasets:** Download both legislators-current.yaml and legislators-historical.yaml to maximize match coverage. CapitolTrades includes historical trade data, so we need FEC IDs for former legislators too.

5. **Warning not error:** Use tracing::warn! for collisions instead of failing the entire sync. One ambiguous match shouldn't prevent other valid matches from being stored.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all tasks executed as specified. Initial compilation error for missing reqwest in CLI crate was expected and resolved by adding the workspace dependency.

## User Setup Required

None - no external service configuration required for this plan. sync-fec command downloads public data from GitHub (no authentication required).

## Next Phase Readiness

- FEC mapping infrastructure complete and tested
- sync-fec command functional (compiles, shows in help, ready for integration testing)
- 15 new tests covering YAML parsing, name matching edge cases, DB operations, and idempotency
- All 385 tests passing, zero clippy warnings
- Phase 7 complete (2/2 plans done)

**Ready for Phase 8 (OpenFEC API Client).**

## Self-Check

Verifying all created files and commits exist:

- File: capitoltraders_lib/src/fec_mapping.rs - FOUND
- File: capitoltraders_cli/src/commands/sync_fec.rs - FOUND
- File: capitoltraders_lib/src/db.rs (FEC methods) - FOUND
- Commit: 9b37a43 (Task 1) - FOUND
- Commit: 2088fb1 (Task 2) - FOUND
- Tests: 385 passing (63 + 9 + 259 + 3 + 8 + 7 + 36) - PASSED
- Clippy: zero warnings - PASSED
- sync-fec in help: Listed in main help and has --db flag - PASSED

## Self-Check: PASSED

---
*Phase: 07-foundation-environment-setup*
*Completed: 2026-02-12*
