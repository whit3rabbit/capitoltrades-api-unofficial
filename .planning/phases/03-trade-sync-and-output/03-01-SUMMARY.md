---
phase: 03-trade-sync-and-output
plan: 01
subsystem: database, cli
tags: [sqlite, enrichment, sync, clap, tokio]

# Dependency graph
requires:
  - phase: 02-trade-extraction
    provides: "trade_detail() scraper and update_trade_detail() DB persistence"
provides:
  - "sync --enrich command for post-ingest trade enrichment"
  - "count_unenriched_trades() DB method"
  - "--dry-run and --batch-size enrichment controls"
  - "ScrapedTradeDetail re-export from lib crate"
affects: [03-02 (output formatting), 03-03 (trade analysis), phase-06 (concurrency/parallel enrichment)]

# Tech tracking
tech-stack:
  added: []
  patterns: ["post-ingest enrichment loop with per-trade commit", "hidden CLI alias for backward compatibility"]

key-files:
  created: []
  modified:
    - "capitoltraders_cli/src/commands/sync.rs"
    - "capitoltraders_lib/src/db.rs"
    - "capitoltraders_lib/src/lib.rs"

key-decisions:
  - "Enrichment runs post-ingest (after sync_trades) rather than inline, keeping existing --with-trade-details inline fetch unchanged for backward compat"
  - "EnrichmentResult.skipped field included but unused (reserved for future smart-skip reporting)"
  - "Integration tests placed in db.rs test module since they exercise DB methods directly and reuse existing test helpers"

patterns-established:
  - "Post-ingest enrichment: sync trades first, then loop over unenriched queue with configurable batch_size and throttle delay"
  - "Hidden CLI alias: deprecated flags marked with hide=true and aliased to new flags in run()"
  - "Dry-run pattern: check count_unenriched_trades() and report without HTTP calls"

# Metrics
duration: 3min
completed: 2026-02-08
---

# Phase 3 Plan 1: Trade Enrichment Pipeline Summary

**Post-ingest enrichment loop wired into sync command with --enrich, --dry-run, --batch-size flags and 500ms default throttle**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-08T16:53:22Z
- **Completed:** 2026-02-08T16:56:45Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- `sync --enrich` fetches trade detail pages for all unenriched trades and persists results via update_trade_detail()
- `--dry-run` reports unenriched count without making HTTP requests
- `--batch-size N` limits enrichment to N trades per run for crash-safe incremental processing
- `--with-trade-details` is now a hidden backward-compatible alias for `--enrich`
- Default detail page delay changed from 250ms to 500ms (PERF-04)
- 6 new tests: 3 count_unenriched_trades unit tests + 3 enrichment pipeline integration tests

## Task Commits

Each task was committed atomically:

1. **Task 1: Add count_unenriched_trades and enrich_trades pipeline** - `47ec909` (feat)
2. **Task 2: Integration test for enrichment pipeline** - `44d4e63` (test)

## Files Created/Modified
- `capitoltraders_cli/src/commands/sync.rs` - Added --enrich/--dry-run/--batch-size flags, EnrichmentResult struct, enrich_trades() async function, updated run() to call enrichment after sync
- `capitoltraders_lib/src/db.rs` - Added count_unenriched_trades() method, 3 unit tests for it, 3 enrichment pipeline integration tests
- `capitoltraders_lib/src/lib.rs` - Added ScrapedTradeDetail to public re-exports

## Decisions Made
- Enrichment runs post-ingest (after sync_trades completes) rather than being woven into the inline trade loop. This keeps the existing --with-trade-details inline fetch path unchanged to avoid breaking anything. The post-ingest enrichment captures all fields via update_trade_detail, making the inline fetch harmless but redundant.
- Integration tests were placed in db.rs rather than sync.rs because they only exercise DB methods and benefit from the existing make_test_scraped_trade helper.
- The EnrichmentResult.skipped field is included but currently unused -- it exists for future reporting when smart-skip statistics are surfaced to the user.

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness
- Enrichment pipeline is wired and tested at the DB level
- Ready for 03-02 (output formatting) and 03-03 (trade analysis) plans
- Live site testing of enrichment requires network access to capitoltrades.com
- Total workspace tests: 241 (all passing)

---
*Phase: 03-trade-sync-and-output*
*Completed: 2026-02-08*
