---
phase: 03-trade-sync-and-output
plan: 03
subsystem: cli
tags: [clap, tabled, csv, xml, sqlite, output-formats]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "SQLite schema with trades, politicians, issuers, assets, trade_committees, trade_labels tables"
  - phase: 03-trade-sync-and-output plan 02
    provides: "DbTradeRow struct, DbTradeFilter struct, query_trades() method with 6-table JOINs"
provides:
  - "--db flag on trades command for reading enriched trades from SQLite"
  - "DbTradeOutputRow struct with 10 columns including Asset, Committees, Labels"
  - "print_db_trades_table/csv/markdown/xml functions for all output formats"
  - "db_trades_to_xml() using existing items_to_xml bridge"
  - "capitalize_party() helper for DB party format matching"
  - "Unsupported filter detection with helpful error messages"
affects: [future analysis dashboards, Phase 4 politician output, Phase 5 issuer output]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Unsupported filter bailout with explicit supported-filter list in error message"
    - "capitalize_party helper: validation returns lowercase, DB stores capitalized"
    - "Reuse items_to_xml bridge for DbTradeRow (same pattern as Trade/PoliticianDetail/IssuerDetail)"

key-files:
  modified:
    - "capitoltraders_cli/src/commands/trades.rs"
    - "capitoltraders_cli/src/main.rs"
    - "capitoltraders_cli/src/output.rs"
    - "capitoltraders_cli/src/xml_output.rs"
    - "capitoltraders_cli/src/output_tests.rs"

key-decisions:
  - "Implemented all output functions in Task 1 commit rather than stubs, since Task 2 tests depend on them"
  - "Unsupported DB filters bail with explicit list of supported alternatives rather than silently ignoring"
  - "capitalize_party maps validation output to DB format (Democrat not democrat)"
  - "Reused items_to_xml generic function for DbTradeRow XML serialization"

patterns-established:
  - "DB command path: --db flag routes to run_db() bypassing scraper entirely"
  - "Filter validation reuse: same validation functions for both scrape and DB paths"

# Metrics
duration: 3min
completed: 2026-02-08
---

# Phase 3 Plan 3: CLI DB Output Summary

**trades --db flag with DbTradeOutputRow rendering enriched columns (Asset, Committees, Labels) across all 5 output formats (table, json, csv, md, xml)**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-08T17:01:01Z
- **Completed:** 2026-02-08T17:04:33Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- --db flag on trades command routes to SQLite query path, bypassing live scraping entirely
- All 5 output formats (table, JSON, CSV, markdown, XML) display enriched columns: Asset, Committees, Labels
- Supported DB filters (party, state, tx_type, name, issuer, since, until, days) with proper validation
- Unsupported filters bail with clear error listing all supported alternatives
- 5 new tests verifying row mapping, empty committees/labels, JSON serialization, CSV headers, and XML structure

## Task Commits

Each task was committed atomically:

1. **Task 1: Add --db flag and DB query code path** - `1858740` (feat) -- trades.rs, main.rs, output.rs, xml_output.rs
2. **Task 2: Add DB trade output tests** - `7a975e6` (test) -- output_tests.rs

## Files Created/Modified
- `capitoltraders_cli/src/commands/trades.rs` - Added --db flag, run_db(), capitalize_party()
- `capitoltraders_cli/src/main.rs` - Routes --db to run_db() before scraper fallback
- `capitoltraders_cli/src/output.rs` - Added DbTradeOutputRow, build_db_trade_rows, print_db_trades_table/csv/markdown/xml
- `capitoltraders_cli/src/xml_output.rs` - Added db_trades_to_xml() using items_to_xml bridge
- `capitoltraders_cli/src/output_tests.rs` - 5 new tests for DB trade output

## Decisions Made
- Implemented output functions alongside the --db flag (Task 1) rather than as stubs, since they share a commit boundary and Task 2 tests depend on them being real implementations
- Unsupported DB filters produce an explicit bail with the full list of supported filter flags, rather than silently ignoring unsupported flags (prevents user confusion)
- capitalize_party() maps validation output ("democrat") to DB storage format ("Democrat") since the DB stores the capitalized form from the API
- Reused the generic items_to_xml function for DbTradeRow XML serialization -- no special-casing needed since DbTradeRow derives Serialize

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added unsupported filter detection for tx-days, tx-since, tx-until**
- **Found during:** Task 1
- **Issue:** Plan listed --committee through --politician as unsupported but did not mention --tx-days, --tx-since, --tx-until which are also unsupported on the DB path (DB query_trades has no tx_date filter fields)
- **Fix:** Added --tx-days, --tx-since, --tx-until to the unsupported filter check list
- **Files modified:** capitoltraders_cli/src/commands/trades.rs
- **Verification:** Passing an unsupported flag with --db produces a helpful bail message
- **Committed in:** 1858740 (Task 1 commit)

**2. [Rule 2 - Missing Critical] Added XML structure test**
- **Found during:** Task 2
- **Issue:** Plan specified 4 tests but the XML output path needed verification too. Added test_db_trade_xml_structure as a 5th test to verify XML singularization of committees/labels elements
- **Fix:** Added test verifying XML contains <committee> and <label> singular elements inside their plural containers
- **Files modified:** capitoltraders_cli/src/output_tests.rs
- **Verification:** Test passes, confirming correct XML structure
- **Committed in:** 7a975e6 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 missing critical)
**Impact on plan:** Both auto-fixes necessary for completeness. No scope creep.

## Issues Encountered
None -- all code compiled on first attempt, all tests passed immediately.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 3 is now complete: sync, enrichment, DB query, and CLI output all functional
- Users can run `capitoltraders sync --db trades.db --enrich` then `capitoltraders trades --db trades.db` to view enriched data
- Ready for Phase 4 (politician enrichment) and Phase 5 (issuer enrichment) which depend only on Phase 1

## Self-Check: PASSED

All 5 modified files exist on disk. Both task commits (1858740, 7a975e6) verified in git log. 256 tests passing, zero clippy warnings.

---
*Phase: 03-trade-sync-and-output*
*Completed: 2026-02-08*
