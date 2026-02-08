---
phase: 04-politician-enrichment
plan: 03
subsystem: cli, database
tags: [sqlite, cli, politician, committee, output, table, json, csv, xml, markdown]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "SQLite schema with politician_committees table, politician_stats table"
  - phase: 04-politician-enrichment
    plan: 01
    provides: "replace_all_politician_committees() for committee membership persistence"
provides:
  - "DbPoliticianRow struct with committee membership data from LEFT JOIN"
  - "DbPoliticianFilter struct for party, state, name, chamber filtering"
  - "Db::query_politicians() method with GROUP_CONCAT committee aggregation"
  - "--db flag on politicians command for SQLite-backed output"
  - "All 5 output formats (table, json, csv, md, xml) for DB politicians with committee data"
  - "db_politicians_to_xml() function reusing items_to_xml generic"
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "DB command path for politicians: --db flag routes to run_db() bypassing scraper"
    - "DbPoliticianRow as canonical read-side politician type (vs PoliticianDetail for API)"
    - "capitalize_party() copied to politicians.rs for DB filter capitalization"

key-files:
  created: []
  modified:
    - "capitoltraders_lib/src/db.rs"
    - "capitoltraders_lib/src/lib.rs"
    - "capitoltraders_cli/src/commands/politicians.rs"
    - "capitoltraders_cli/src/main.rs"
    - "capitoltraders_cli/src/output.rs"
    - "capitoltraders_cli/src/xml_output.rs"
    - "capitoltraders_cli/src/output_tests.rs"

key-decisions:
  - "Copied capitalize_party() to politicians.rs rather than factoring out shared utility -- same 6-line function, not worth coupling"
  - "Unsupported filters (--committee, --issuer-id) bail with explicit supported-filter list on DB path"
  - "query_politicians uses same dynamic filter pattern as query_trades (Vec of Box dyn ToSql, param_idx)"

patterns-established:
  - "DB politician command path: --db flag, run_db(), DbPoliticianFilter, query_politicians()"
  - "DbPoliticianOutputRow with Committees column for all 5 output formats"

# Metrics
duration: 4min
completed: 2026-02-08
---

# Phase 4 Plan 3: CLI Politicians --db with Committee-Aware Output Summary

**query_politicians() with LEFT JOIN on politician_committees, --db flag routing, and all 5 output formats displaying committee memberships**

## Performance

- **Duration:** 4 min
- **Started:** 2026-02-08T21:18:11Z
- **Completed:** 2026-02-08T21:22:11Z
- **Tasks:** 2
- **Files modified:** 7 (db.rs, lib.rs, politicians.rs, main.rs, output.rs, xml_output.rs, output_tests.rs)

## Accomplishments
- Added DbPoliticianRow, DbPoliticianFilter, and query_politicians() with LEFT JOIN on politician_committees and politician_stats
- Added --db flag to politicians command with run_db() function for DB-backed output
- Added DbPoliticianOutputRow and print_db_politicians_* functions for all 5 formats (table, json, csv, md, xml)
- Unsupported filters (--committee, --issuer-id) bail with explicit supported-filter list
- 9 new tests (5 DB query + 4 output), all 271 workspace tests passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Add DbPoliticianRow, query_politicians, --db flag, and output functions** - `da940c4` (feat)
2. **Task 2: Add tests for DB politician query and output** - `e73630c` (test)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - Added DbPoliticianRow, DbPoliticianFilter structs and query_politicians() method; added 5 query tests
- `capitoltraders_lib/src/lib.rs` - Re-exported DbPoliticianRow and DbPoliticianFilter
- `capitoltraders_cli/src/commands/politicians.rs` - Added --db arg, capitalize_party(), run_db() function
- `capitoltraders_cli/src/main.rs` - Routed --db flag to run_db() in Politicians match arm
- `capitoltraders_cli/src/output.rs` - Added DbPoliticianOutputRow, build_db_politician_rows(), and 4 print_db_politicians_* functions
- `capitoltraders_cli/src/xml_output.rs` - Added db_politicians_to_xml() reusing items_to_xml generic
- `capitoltraders_cli/src/output_tests.rs` - Added 4 DB politician output tests

## Decisions Made
- Copied capitalize_party() to politicians.rs rather than extracting to a shared utility. The function is 6 lines and duplicating it avoids coupling the two command modules.
- Unsupported filters bail with an explicit list of supported filters (party, state, name, chamber) -- matches the pattern established in trades --db.
- query_politicians uses the same Vec<Box<dyn ToSql>> dynamic filter builder pattern as query_trades for consistency.

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None -- no external service configuration required.

## Next Phase Readiness
- Politicians --db path complete with committee data in all 5 output formats
- Phase 4 (Politician Enrichment) is now fully complete: committee scraping (04-01), sync integration (04-02), and CLI output (04-03) all done
- Ready for Phase 5 (Issuer Enrichment) or Phase 6 (Performance)

## Self-Check: PASSED

- All 7 modified files exist
- Both commits found (da940c4, e73630c)
- All 6 key functions/structs present (DbPoliticianRow, DbPoliticianFilter, query_politicians, run_db, print_db_politicians_table, db_politicians_to_xml)

---
*Phase: 04-politician-enrichment*
*Completed: 2026-02-08*
