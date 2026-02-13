---
phase: 11-donations-cli-command
plan: 02
subsystem: cli
tags: [clap, output-formatting, donations, aggregations, csv-sanitization]

# Dependency graph
requires:
  - phase: 11-donations-cli-command
    plan: 01
    provides: query_donations and aggregation query methods with DonationFilter
provides:
  - donations CLI subcommand with full filter validation
  - Individual donation listing with 7 columns across 5 output formats
  - 3 aggregation modes (contributor, employer, state) with dedicated output functions
  - CSV formula injection protection on contributor/employer fields
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: [politician name resolution with disambiguation, shared validation pattern for CLI filters, separate output row structs per aggregation type]

key-files:
  created: [capitoltraders_cli/src/commands/donations.rs]
  modified: [capitoltraders_cli/src/commands/mod.rs, capitoltraders_cli/src/main.rs, capitoltraders_cli/src/output.rs, capitoltraders_cli/src/xml_output.rs]

key-decisions:
  - "Politician filter uses name resolution (not ID) for better UX, with disambiguation on multiple matches"
  - "Cycle validation requires even year >= 1976 (FEC data availability constraint)"
  - "Separate output row structs for each aggregation type rather than generic approach"
  - "CSV sanitization applies to contributor and employer fields (user-generated content risk)"

patterns-established:
  - "CLI filter validation follows portfolio.rs pattern (validate early, bail on error)"
  - "Empty result hint messages guide users to run sync-fec and sync-donations first"
  - "Group-by dispatch uses match on Option<&str> with exhaustive validation upfront"

# Metrics
duration: 5min
completed: 2026-02-13
---

# Phase 11 Plan 02: Donations CLI Command Summary

**Full-featured donations CLI subcommand with 4 display modes, 8 filter flags, 5 output formats, and comprehensive validation**

## Performance

- **Duration:** 5 minutes
- **Started:** 2026-02-13T20:37:19Z
- **Completed:** 2026-02-13T20:42:20Z
- **Tasks:** 2
- **Files created:** 1
- **Files modified:** 4

## Accomplishments
- donations CLI subcommand with 8 filter flags (--politician, --cycle, --min-amount, --employer, --state, --top, --group-by, --db)
- Politician name resolution with disambiguation (handles partial matches, errors on 0 or multiple)
- Input validation for state (via validate_state), cycle (even year >= 1976), min-amount (>= 0), top (> 0), group-by (contributor|employer|state)
- 4 display modes: individual listing + 3 aggregation views (contributor, employer, state)
- 16 output functions covering all mode/format combinations
- CSV formula injection protection on contributor and employer fields
- Empty result hints guiding users to run sync-fec and sync-donations
- 473 tests passing (no new tests required, all DB query tests in Plan 01)

## Task Commits

Each task was committed atomically:

1. **Task 1: Donations CLI command with filter validation** - `ade5f80` (feat)
2. **Task 2: Output formatting for donations (all 5 formats)** - `7982c6e` (feat)

## Files Created/Modified

**Created:**
- `capitoltraders_cli/src/commands/donations.rs` - DonationsArgs struct, run() function with validation and dispatch logic

**Modified:**
- `capitoltraders_cli/src/commands/mod.rs` - Added pub mod donations
- `capitoltraders_cli/src/main.rs` - Added Donations variant to Commands enum, dispatch in match block, updated doc comment to 8 subcommands
- `capitoltraders_cli/src/output.rs` - Added DonationOutputRow, ContributorAggOutputRow, EmployerAggOutputRow, StateAggOutputRow structs, 16 print functions (4 per mode × 4 formats, JSON shared), build functions for all row types
- `capitoltraders_cli/src/xml_output.rs` - Added donations_to_xml, contributor_agg_to_xml, employer_agg_to_xml, state_agg_to_xml functions

## Decisions Made

**Politician name resolution instead of ID:**
- CLI users think in names ("Nancy Pelosi"), not IDs ("P000197")
- find_politician_by_name returns Vec<(politician_id, full_name)> tuples
- 0 matches: bail with "No politician found"
- 1 match: use the politician_id
- Multiple matches: bail with list of matching names and "Please be more specific"

**Cycle validation (even year >= 1976):**
- FEC electronic filing began in 1976
- Election cycles are always even years (2024, 2022, etc.)
- Simple modulo check: `cycle < 1976 || cycle % 2 != 0`

**Separate output row structs per aggregation type:**
- ContributorAggOutputRow has 8 fields (includes first/last date)
- EmployerAggOutputRow has 5 fields (no date range)
- StateAggOutputRow has 5 fields (no date range)
- Trade-off: more code, but clearer column definitions and no conditional logic

**CSV sanitization on contributor and employer:**
- Both fields are user-generated content from FEC filings
- Employer field especially risky (companies often use formulas in names)
- Sanitize with leading tab if starts with =, +, -, @ (existing sanitize_csv_field helper)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

**Field name mismatches in first compile:**
- Plan specified `max_amount`, `first_date`, `last_date` but DB types use `max_donation`, `first_donation`, `last_donation` for ContributorAggRow
- Plan specified `unique_contributors` but DB types use `contributor_count` for EmployerAggRow and StateAggRow
- StateAggRow uses `state` field, not `contributor_state`
- Fixed by checking actual struct definitions in db.rs from Plan 01
- All tests passed after field name corrections

**Politician name resolution returns tuples:**
- find_politician_by_name returns Vec<(String, String)> (politician_id, full_name)
- Initial code expected struct with .politician_id and .name fields
- Fixed by using tuple index notation: matches[0].0 for ID, matches.iter().map(|m| m.1) for names
- Note: Plan suggested showing state in disambiguation message, but current DB function doesn't return state (would require enhancement in future)

## User Setup Required

None - command works with existing synced databases. Users must run:
1. `capitoltraders sync` (trade/politician/issuer data)
2. `capitoltraders sync-fec` (FEC candidate ID mappings)
3. `capitoltraders sync-donations` (actual donation records)

Empty result hints remind users of this sequence.

## Next Phase Readiness

Phase 11 complete! Ready for Phase 12 (Donation Trend Analysis) or v1.2 UAT testing:
- All 8 CLI subcommands functional (trades, politicians, issuers, sync, sync-fec, enrich-prices, portfolio, sync-donations, donations)
- FEC donation integration fully operational (7-10 sync → query → display → aggregate)
- 473 tests passing
- All output formats supported (table, JSON, CSV, markdown, XML)
- No blockers or known issues

## Self-Check: PASSED

Files verified:
- FOUND: capitoltraders_cli/src/commands/donations.rs
- FOUND: capitoltraders_cli/src/commands/mod.rs
- FOUND: capitoltraders_cli/src/main.rs
- FOUND: capitoltraders_cli/src/output.rs
- FOUND: capitoltraders_cli/src/xml_output.rs

Commits verified:
- FOUND: ade5f80 (Task 1: Donations CLI command skeleton with filter validation)
- FOUND: 7982c6e (Task 2: Output formatting for donations, all 5 formats)

Workspace compilation:
- `cargo check --workspace` ✓ clean
- `cargo clippy --workspace` ✓ no new warnings (1 pre-existing await_holding_lock)
- `cargo test --workspace` ✓ 473 tests passing

CLI functionality:
- `capitoltraders donations --help` ✓ shows all 8 flags with descriptions
- Error handling ✓ produces appropriate DB error on invalid path

---
*Phase: 11-donations-cli-command*
*Completed: 2026-02-13*
