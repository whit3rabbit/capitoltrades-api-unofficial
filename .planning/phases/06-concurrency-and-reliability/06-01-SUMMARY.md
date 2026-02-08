---
phase: 06-concurrency-and-reliability
plan: 01
subsystem: sync
tags: [tokio, semaphore, concurrency, indicatif, progress-bar, circuit-breaker]

# Dependency graph
requires:
  - phase: 05-issuer-enrichment
    provides: "Sequential enrich_trades and enrich_issuers functions in sync.rs"
provides:
  - "Bounded concurrent trade enrichment via Semaphore+JoinSet+mpsc"
  - "Bounded concurrent issuer enrichment using same pattern"
  - "indicatif progress bars with elapsed/ETA/position for enrichment"
  - "CircuitBreaker kill switch for consecutive failure halting"
  - "--concurrency and --max-failures CLI flags on sync subcommand"
  - "Clone derive on ScrapeClient for spawned task use"
affects: [06-02]

# Tech tracking
tech-stack:
  added: [indicatif 0.17]
  patterns: [semaphore-bounded-concurrency, mpsc-channel-result-collection, circuit-breaker-kill-switch]

key-files:
  created: []
  modified:
    - capitoltraders_lib/src/scrape.rs
    - capitoltraders_cli/Cargo.toml
    - capitoltraders_cli/src/commands/sync.rs

key-decisions:
  - "Clone on ScrapeClient is cheap: reqwest::Client is Arc-backed internally"
  - "Throttle delay is per-task (each spawned task sleeps before its request), not global"
  - "DB writes remain single-threaded via mpsc channel receiver loop, avoiding SQLite contention"
  - "CircuitBreaker is a simple kill switch, not a full half-open/closed circuit breaker"
  - "indicatif 0.17 chosen over 0.18 for compatibility with existing dependency tree"

patterns-established:
  - "Semaphore+JoinSet+mpsc pattern: spawn tasks with semaphore-guarded concurrency, collect results via mpsc channel, process in single receive loop"
  - "CircuitBreaker struct: consecutive_failures counter with threshold, record_success resets, record_failure increments, is_tripped checks"
  - "Progress bar via pb.println() for warnings instead of eprintln (avoids garbled output)"
  - "join_set.abort_all() for fast shutdown when circuit breaker trips"

# Metrics
duration: 3min
completed: 2026-02-08
---

# Phase 6 Plan 1: Concurrent Enrichment Summary

**Bounded concurrent trade/issuer enrichment via tokio Semaphore with indicatif progress bars and circuit breaker halt-on-failure**

## Performance

- **Duration:** 3 min
- **Started:** 2026-02-08T23:18:40Z
- **Completed:** 2026-02-08T23:21:37Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Replaced sequential for-loop enrichment with bounded concurrency (default 3 parallel HTTP fetches)
- Added indicatif progress bars showing position/total/ETA/ok-err counts during enrichment
- Added CircuitBreaker that halts enrichment after N consecutive post-retry failures
- Added --concurrency (1-10) and --max-failures (>= 1) CLI flags to sync subcommand
- Made ScrapeClient cloneable for use in spawned tasks

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Clone to ScrapeClient, indicatif dependency, CLI args, and CircuitBreaker struct** - `d59c137` (feat)
2. **Task 2: Rewrite enrich_trades and enrich_issuers to concurrent pattern** - `e005a5d` (feat)

## Files Created/Modified
- `capitoltraders_lib/src/scrape.rs` - Added #[derive(Clone)] to ScrapeClient
- `capitoltraders_cli/Cargo.toml` - Added indicatif 0.17 dependency
- `capitoltraders_cli/src/commands/sync.rs` - Concurrent enrichment with Semaphore+JoinSet+mpsc, ProgressBar, CircuitBreaker, --concurrency/--max-failures args

## Decisions Made
- Clone on ScrapeClient is cheap since reqwest::Client is Arc-backed internally, so cloning for each spawned task adds negligible overhead
- Throttle delay is per-task rather than global: with concurrency=3 and delay=500ms, effective rate is up to ~6 req/s in bursts. Existing retry/backoff in ScrapeClient handles 429 responses if the server complains
- DB writes remain single-threaded: results flow through mpsc channel to the receive loop which calls db.update_trade_detail/db.update_issuer_detail sequentially, avoiding any SQLite write contention
- CircuitBreaker is intentionally simple (just a consecutive failure counter with threshold), not a full circuit breaker with half-open/closed states
- indicatif 0.17 was resolved by cargo (0.18 available but 0.17 auto-selected for compatibility)
- Used pb.println() for warnings instead of eprintln() to avoid garbled output with the progress bar

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None -- no external service configuration required.

## Next Phase Readiness
- Concurrent enrichment pipeline ready for production use
- Plan 06-02 can build on this foundation for any additional reliability work
- All 288 existing tests pass with zero clippy warnings

## Self-Check: PASSED

All files found. All commits verified (d59c137, e005a5d).

---
*Phase: 06-concurrency-and-reliability*
*Completed: 2026-02-08*
