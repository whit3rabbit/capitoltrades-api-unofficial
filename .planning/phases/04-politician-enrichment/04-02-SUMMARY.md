---
phase: 04-politician-enrichment
plan: 02
subsystem: cli, database, scraping
tags: [sync, enrichment, committee, politician, pipeline]

# Dependency graph
requires:
  - phase: 04-politician-enrichment
    plan: 01
    provides: "ScrapeClient::politicians_by_committee, Db::replace_all_politician_committees, Db::mark_politicians_enriched"
provides:
  - "enrich_politician_committees() async function in sync.rs"
  - "Unconditional committee enrichment wired into sync::run() after trade sync"
affects: [04-03-cli-output]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Unconditional post-ingest enrichment: committee scraping runs every sync without opt-in flag"
    - "Paginated committee iteration: inner loop handles multi-page committees (e.g., Appropriations)"

key-files:
  created: []
  modified:
    - "capitoltraders_cli/src/commands/sync.rs"

key-decisions:
  - "Committee enrichment runs unconditionally (POL-03) -- no --enrich flag needed since 48 requests is fast"
  - "Function returns inserted count (from replace_all_politician_committees) rather than total collected, giving accurate persistence count"
  - "Throttle delay applied between committees and between pages within multi-page committees"

patterns-established:
  - "Unconditional post-ingest enrichment: fast operations (< 30s) run every sync without opt-in"

# Metrics
duration: 3min
completed: 2026-02-08
---

# Phase 4 Plan 2: Sync Pipeline Integration Summary

**enrich_politician_committees() wired unconditionally into sync::run(), iterating all 48 committee codes with pagination and throttle support**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-08T21:17:43Z
- **Completed:** 2026-02-08T21:20:43Z
- **Tasks:** 1
- **Files modified:** 1 (sync.rs)

## Accomplishments
- Added enrich_politician_committees() async function that iterates all 48 COMMITTEE_MAP entries
- Handles pagination for committees with more than 12 members (page size is 12 per listing page)
- Respects configurable throttle delay (details_delay_ms) between all HTTP requests
- Wired unconditionally into sync::run() after trade enrichment and before Ok(())
- Calls replace_all_politician_committees() for atomic persistence and mark_politicians_enriched() for timestamp tracking
- Progress reported to stderr: per-committee member count plus overall summary
- All 262 workspace tests pass, no clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Add enrich_politician_committees and wire into sync::run** - `226351e` (feat)

## Files Created/Modified
- `capitoltraders_cli/src/commands/sync.rs` - Added enrich_politician_committees() function and unconditional call in sync::run()

## Decisions Made
- Committee enrichment is unconditional per POL-03 requirement. The ~48 requests at 500ms throttle take about 25 seconds, which is acceptable for every sync run.
- The function returns the count from replace_all_politician_committees (actually inserted rows after FK filtering) rather than memberships.len() (total collected). This gives the caller accurate information about how many memberships were persisted vs skipped due to unknown politician_ids.
- Throttle delay is applied both between pages within a multi-page committee and between committees. This avoids hammering the server during the committee scraping phase.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- sync::run() now performs full committee enrichment on every run
- Ready for 04-03 (CLI output) which will display committee memberships in politician queries
- Note: db.rs and lib.rs have uncommitted changes from a prior session adding query_politicians/DbPoliticianFilter for 04-03; these were not part of this plan

## Self-Check: PASSED

- sync.rs exists with enrich_politician_committees() function (line 240) and unconditional call (line 141)
- Commit 226351e found in git log
- Function iterates validation::COMMITTEE_MAP, handles pagination, respects throttle delay
- replace_all_politician_committees + mark_politicians_enriched called after loop

---
*Phase: 04-politician-enrichment*
*Completed: 2026-02-08*
