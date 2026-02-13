---
phase: 10-donation-sync-pipeline
plan: 02
subsystem: cli
tags: [donation-sync, openfec, concurrent-pipeline, circuit-breaker, cli]

# Dependency graph
requires:
  - phase: 10-donation-sync-pipeline
    provides: "Donation sync DB operations (Plan 01)"
  - phase: 09-politician-to-committee-mapping-schema-v3
    provides: "CommitteeResolver with three-tier caching"
  - phase: 08-openfec-api-client
    provides: "OpenFecClient with Schedule A support"
provides:
  - "sync-donations CLI command with concurrent pipeline"
  - "Circuit breaker for OpenFEC rate limiting (threshold 5)"
  - "Keyset pagination with cursor persistence"
affects: [10-donation-sync-pipeline]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Semaphore + JoinSet + mpsc concurrent pipeline (from enrich_prices)"
    - "Separate DB handles for setup vs receiver (avoids mutex contention)"
    - "Jittered rate limiting (200-500ms between API calls)"

key-files:
  created:
    - "capitoltraders_cli/src/commands/sync_donations.rs"
  modified:
    - "capitoltraders_cli/src/commands/mod.rs"
    - "capitoltraders_cli/src/main.rs"

key-decisions:
  - "Circuit breaker threshold 5 (lower than enrich_prices' 10 due to stricter OpenFEC rate limits)"
  - "Concurrency 3 workers (lower than enrich_prices' 5 for same reason)"
  - "403 InvalidApiKey causes immediate failure with helpful message (not circuit breaker increment)"
  - "Cursors loaded before task spawn to avoid async DB access in spawned tasks"
  - "Separate DB handles: setup_db for queries, receiver_db for writes (avoids Arc<Mutex> overhead)"
  - "Duration formatting uses as_secs_f64() instead of humantime crate (avoid new dependency)"

patterns-established:
  - "Donation sync pipeline: politician resolution -> committee resolution -> concurrent keyset pagination -> atomic DB writes"

# Metrics
duration: 5min
completed: 2026-02-13
---

# Phase 10 Plan 02: sync-donations CLI Command Summary

**Concurrent FEC donation sync pipeline with circuit breaker, keyset pagination, and CommitteeResolver integration**

## Performance

- **Duration:** 5 min
- **Started:** 2026-02-13T02:28:32Z
- **Completed:** 2026-02-13T02:33:37Z
- **Tasks:** 2
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments

- Implemented sync-donations CLI command using Semaphore + JoinSet + mpsc pattern
- Integrated CommitteeResolver for politician-to-committee mapping with three-tier caching
- Circuit breaker halts sync after 5 consecutive 429 rate limit errors
- 403 InvalidApiKey produces immediate failure with helpful API key setup message
- Keyset pagination with cursor persistence via save_sync_cursor_with_donations
- Progress bar shows donations synced count and elapsed time
- All 464 tests pass, zero clippy warnings

## Task Commits

1. **Task 1: Create sync_donations.rs with concurrent pipeline** - `b665c17` (feat)
2. **Task 2: Wire sync-donations into CLI** - `aee3791` (feat)

**Plan metadata:** (pending)

## Files Created/Modified

- `capitoltraders_cli/src/commands/sync_donations.rs` - Donation sync pipeline with concurrent fetching
- `capitoltraders_cli/src/commands/mod.rs` - Registered sync_donations module
- `capitoltraders_cli/src/main.rs` - Added SyncDonations variant and dispatch

## Decisions Made

**Circuit breaker threshold 5:** Lower than enrich_prices' threshold of 10 due to OpenFEC's stricter rate limits. Hitting 429 five times in a row indicates we need to back off completely.

**Concurrency 3 workers:** Lower than enrich_prices' 5 workers for the same rate limiting reason. OpenFEC's 1000 calls/hour limit is tighter than Yahoo Finance.

**403 immediate failure:** InvalidApiKey error causes immediate abort with helpful message, not a circuit breaker increment. This is a configuration error, not a transient failure.

**Separate DB handles:** Opened two Db instances (setup_db for queries, receiver_db for writes) to avoid Arc<Mutex<Db>> overhead in the hot path. SQLite's WAL mode handles concurrent readers fine.

**Duration formatting:** Used `as_secs_f64()` instead of adding humantime dependency. Keeps dependency tree lean.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Removed unused import and fixed type complexity**
- **Found during:** Task 1 (clippy check after initial implementation)
- **Issue:** `use anyhow::anyhow` was unused, and Vec<(String, String, String, Option<(i64, String)>)> triggered clippy type_complexity warning
- **Fix:** Removed unused import, introduced `type CommitteeTask` alias
- **Files modified:** capitoltraders_cli/src/commands/sync_donations.rs
- **Verification:** cargo clippy --workspace returned zero warnings
- **Committed in:** aee3791 (part of Task 2 commit)

**2. [Rule 3 - Blocking] Fixed missing rusqlite dependency**
- **Found during:** Task 2 (compilation after wiring into CLI)
- **Issue:** Used `rusqlite::params!` macro but rusqlite is not a direct dependency of capitoltraders_cli
- **Fix:** Replaced `params![a, b]` with array syntax `[a, b]` which works without the macro
- **Files modified:** capitoltraders_cli/src/commands/sync_donations.rs
- **Verification:** cargo check --workspace succeeded
- **Committed in:** aee3791 (part of Task 2 commit)

**3. [Rule 3 - Blocking] Fixed missing humantime dependency**
- **Found during:** Task 2 (compilation after wiring into CLI)
- **Issue:** Used `humantime::format_duration()` which requires humantime crate (not in dependencies)
- **Fix:** Replaced with `elapsed.as_secs_f64()` for simple seconds formatting
- **Files modified:** capitoltraders_cli/src/commands/sync_donations.rs
- **Verification:** cargo check --workspace succeeded
- **Committed in:** aee3791 (part of Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 blocking issues)
**Impact on plan:** All fixes were necessary to make the code compile and pass clippy. No scope creep - kept implementation aligned with enrich_prices pattern.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required. Users need OPENFEC_API_KEY in .env, but that's already documented from Phase 7.

## Next Phase Readiness

Phase 10 complete. Ready for Phase 11 (Donation Query Commands) or Phase 12 (Employer Fuzzy Matching).

## Self-Check: PASSED

- ✓ capitoltraders_cli/src/commands/sync_donations.rs exists (439 lines)
- ✓ capitoltraders_cli/src/commands/mod.rs modified (sync_donations registered)
- ✓ capitoltraders_cli/src/main.rs modified (SyncDonations variant added)
- ✓ Commit b665c17 exists (Task 1: concurrent pipeline)
- ✓ Commit aee3791 exists (Task 2: CLI wiring)
- ✓ Total tests: 464 (all passing)
- ✓ Clippy warnings: 0 (excluding MutexGuard in capitoltraders_lib)
- ✓ `cargo run -p capitoltraders_cli -- sync-donations --help` shows all flags
- ✓ Missing API key produces helpful error message
- ✓ Circuit breaker threshold is 5
- ✓ Concurrency is 3
- ✓ save_sync_cursor_with_donations called in receiver loop
- ✓ 403 causes immediate bail with API key help
- ✓ Cursors loaded before task spawn
- ✓ drop(tx) called before receiver loop

---
*Phase: 10-donation-sync-pipeline*
*Completed: 2026-02-13*
