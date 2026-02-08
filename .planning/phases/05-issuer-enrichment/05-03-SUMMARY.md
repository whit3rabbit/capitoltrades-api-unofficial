---
phase: 05-issuer-enrichment
plan: 03
subsystem: cli, database
tags: [sqlite, rusqlite, tabled, csv, xml, cli-output, issuer-performance]

# Dependency graph
requires:
  - phase: 05-01
    provides: "issuer_performance table, issuer_stats table, update_issuer_detail()"
provides:
  - "DbIssuerRow and DbIssuerFilter structs for querying enriched issuers"
  - "query_issuers() DB method with LEFT JOINed performance data"
  - "--db flag on issuers CLI command with full output format support"
  - "DbIssuerOutputRow with curated performance columns for table/markdown"
  - "9 new tests (5 DB query + 4 output)"
affects: [06-concurrency-and-polish]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "DbIssuerOutputRow with format_large_number (T/B/M) and format_percent for performance data"
    - "IN clause with Vec filter params for multi-value sector/state/country"

key-files:
  modified:
    - "capitoltraders_lib/src/db.rs"
    - "capitoltraders_lib/src/lib.rs"
    - "capitoltraders_cli/src/commands/issuers.rs"
    - "capitoltraders_cli/src/main.rs"
    - "capitoltraders_cli/src/output.rs"
    - "capitoltraders_cli/src/xml_output.rs"
    - "capitoltraders_cli/src/output_tests.rs"

key-decisions:
  - "Used Vec filter params with IN clause for sector/state/country (more flexible than single-value pattern in query_trades)"
  - "Table output uses trailing30_change and trailing365_change (percentage change) not raw trailing values"
  - "JSON/XML output serializes full DbIssuerRow directly (all performance fields) while table uses curated subset"
  - "format_large_number helper for T/B/M suffixes, format_percent for +X.X%/-X.X% display"

patterns-established:
  - "DB issuer command path: --db flag routes to run_db(), DbIssuerFilter, query_issuers()"
  - "DbIssuerRow as canonical read-side issuer type (vs IssuerDetail for API)"
  - "IN clause multi-value filter: Vec<String> params with dynamic placeholder generation"

# Metrics
duration: 4min
completed: 2026-02-08
---

# Phase 5 Plan 3: CLI Issuer DB Output Summary

**issuers --db command with performance metrics (mcap, trailing returns) across all 5 output formats (table/json/csv/md/xml)**

## Performance

- **Duration:** 4 min
- **Started:** 2026-02-08T22:06:58Z
- **Completed:** 2026-02-08T22:10:53Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- query_issuers() returns issuer rows with LEFT JOINed stats and performance data from 3 tables
- issuers --db flag routes to DB-backed output with filters --search, --sector, --state, --country, --limit
- Table output shows curated columns: Name, Ticker, Sector, Mcap ($3.5T), 30D Return (+2.5%), YTD, Trades, Volume, Last Traded
- JSON/CSV/XML include all performance fields (mcap, all trailing/period returns)
- 9 new tests (5 DB query, 4 output) all pass, 289 total workspace tests with zero regressions

## Task Commits

Each task was committed atomically:

1. **Task 1: Add DbIssuerRow, DbIssuerFilter, query_issuers, --db flag, and output functions** - `7c50932` (feat)
2. **Task 2: Add tests for DB issuer query and output** - `52c653a` (test)

## Files Created/Modified
- `capitoltraders_lib/src/db.rs` - DbIssuerRow, DbIssuerFilter, query_issuers() with dynamic filter builder
- `capitoltraders_lib/src/lib.rs` - Re-exports for DbIssuerRow and DbIssuerFilter
- `capitoltraders_cli/src/commands/issuers.rs` - --db/--limit flags, run_db() with filter validation
- `capitoltraders_cli/src/main.rs` - Route issuers --db to run_db()
- `capitoltraders_cli/src/output.rs` - DbIssuerOutputRow, format_large_number, format_percent, print_db_issuers_*
- `capitoltraders_cli/src/xml_output.rs` - db_issuers_to_xml() via items_to_xml generic
- `capitoltraders_cli/src/output_tests.rs` - 4 output tests (row mapping, no performance, JSON, CSV headers)

## Decisions Made
- Used Vec<String> filter params with IN clause for sector/state/country rather than single-value pattern -- issuers naturally benefit from multi-value filtering
- Table output displays trailing30_change and trailing365_change (percentage returns) not raw price values -- percentages are what users care about for performance
- JSON/XML output serializes full DbIssuerRow directly (21 fields) while table shows curated 9-column subset
- format_large_number uses T/B/M suffixes with one decimal (e.g., $3.5T) and format_percent uses +/-X.X% format

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 5 (Issuer Enrichment) is fully complete: fixtures, DB persistence, sync pipeline, and CLI output
- All three entity types (trades, politicians, issuers) now have --db output support
- Ready for Phase 6 (concurrency and polish)

---
*Phase: 05-issuer-enrichment*
*Completed: 2026-02-08*
