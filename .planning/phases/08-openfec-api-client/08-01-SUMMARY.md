---
phase: 08-openfec-api-client
plan: 01
subsystem: api
tags: [openfec, fec, reqwest, serde, http-client]

# Dependency graph
requires:
  - phase: 07-fec-mapping
    provides: Environment setup and .env loading for API key management
provides:
  - Typed Rust client for OpenFEC API (candidate search, committee lookup, Schedule A contributions)
  - Error handling with HTTP status code mapping (429 -> RateLimited, 403 -> InvalidApiKey)
  - Query builders with fluent interface for candidate search and Schedule A queries
  - Keyset pagination support for Schedule A endpoint (no page numbers)
affects: [09-politician-committee-mapping, 10-donation-sync-pipeline]

# Tech tracking
tech-stack:
  added: [openfec module]
  patterns: [Query builder pattern, keyset pagination, HTTP status code error mapping]

key-files:
  created:
    - capitoltraders_lib/src/openfec/error.rs
    - capitoltraders_lib/src/openfec/types.rs
    - capitoltraders_lib/src/openfec/client.rs
    - capitoltraders_lib/src/openfec/mod.rs
  modified:
    - capitoltraders_lib/src/lib.rs

key-decisions:
  - "No DashMap cache in OpenFecClient (caching belongs at DB level in Phase 9)"
  - "Schedule A query has NO page field - keyset pagination only with last_index + last_contribution_receipt_date"
  - "API key passed as query parameter, never as header"
  - "HTTP 429 and 403 status codes mapped to typed errors for circuit breaker logic"

patterns-established:
  - "Query builder to_query_pairs() produces Vec<(String, String)> for reqwest"
  - "with_base_url() constructor for wiremock testing (mirrors YahooClient pattern)"
  - "Private get<T>() helper handles all HTTP logic with status code mapping"

# Metrics
duration: 2 min
completed: 2026-02-12
---

# Phase 8 Plan 1: OpenFEC API Client Summary

**Typed Rust client for OpenFEC API with candidate search, committee lookup, and Schedule A contributions using keyset pagination**

## Performance

- **Duration:** 2 minutes
- **Started:** 2026-02-12T12:33:22Z
- **Completed:** 2026-02-12T12:36:08Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Created OpenFecClient with three endpoint methods (search_candidates, get_candidate_committees, get_schedule_a)
- Implemented query builders with fluent interface for all OpenFEC query parameters
- HTTP status code error mapping (429 -> RateLimited, 403 -> InvalidApiKey) for circuit breaker integration
- Keyset pagination support for Schedule A with last_index + last_contribution_receipt_date cursors
- 6 unit tests for query builder behavior including critical test that Schedule A never emits page parameter
- All 385 existing tests still pass (zero regressions)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create OpenFEC error and type definitions** - `c7128c8` (feat)
   - OpenFecError enum with 5 variants
   - Candidate, Committee, Contribution response types with serde derives
   - CandidateSearchQuery and ScheduleAQuery builders with to_query_pairs()
   - Schedule A query has NO page field (keyset pagination only)
   - 6 unit tests for query builder behavior

2. **Task 2: Create OpenFEC client with three endpoint methods** - `81689b7` (feat)
   - OpenFecClient with new() and with_base_url() constructors
   - search_candidates() for candidate search endpoint
   - get_candidate_committees() for candidate-specific committees
   - get_schedule_a() for Schedule A contributions
   - HTTP 429 -> RateLimited, 403 -> InvalidApiKey error mapping
   - API key passed as query parameter
   - lib.rs exports OpenFecClient and OpenFecError

## Files Created/Modified

- `capitoltraders_lib/src/openfec/error.rs` - OpenFecError enum with thiserror derives
- `capitoltraders_lib/src/openfec/types.rs` - Request/response types, query builders, 6 unit tests
- `capitoltraders_lib/src/openfec/client.rs` - HTTP client implementation with three endpoint methods
- `capitoltraders_lib/src/openfec/mod.rs` - Module exports
- `capitoltraders_lib/src/lib.rs` - Added openfec module and re-exports

## Decisions Made

**No cache in client layer:**
Decision to NOT add DashMap cache to OpenFecClient, unlike YahooClient which has caching.
Rationale: Phase 9 will implement three-tier cache (memory -> SQLite -> API) at a higher level. Adding cache here would create redundant layer and complicate invalidation. YahooClient caches because it wraps third-party connector without caching; OpenFecClient uses reqwest directly and downstream phases control caching strategy.

**Schedule A keyset pagination:**
Schedule A uses keyset cursors (last_index + last_contribution_receipt_date), never page numbers. Query builder explicitly has NO page field and includes test verifying "page" parameter is never emitted.
Rationale: OpenFEC API uses keyset pagination for Schedule A to handle 67+ million contribution records efficiently. Page-based pagination would be inconsistent/unreliable at that scale.

**API key as query parameter:**
API key passed via query parameter `?api_key=...`, not Authorization header.
Rationale: OpenFEC API specification uses query parameter authentication, not header-based auth. Following API convention ensures compatibility.

**Typed error mapping:**
HTTP status codes 429 and 403 mapped to OpenFecError::RateLimited and OpenFecError::InvalidApiKey.
Rationale: Enables circuit breaker logic in Phase 10 to distinguish rate limit errors from authentication errors. Circuit breaker needs to count consecutive 429s to trip, but should fail fast on 403.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required. API key management was handled in Phase 7.

## Next Phase Readiness

- OpenFEC client foundation complete, ready for Phase 8 Plan 2 (wiremock integration tests)
- All endpoint methods accept correct query types and return typed responses
- Error handling supports circuit breaker pattern for Phase 10
- Query builders follow project conventions (mirror capitoltrades_api patterns)

## Self-Check: PASSED

**Files created:**
- [FOUND] capitoltraders_lib/src/openfec/error.rs
- [FOUND] capitoltraders_lib/src/openfec/types.rs
- [FOUND] capitoltraders_lib/src/openfec/client.rs
- [FOUND] capitoltraders_lib/src/openfec/mod.rs

**Files modified:**
- [FOUND] capitoltraders_lib/src/lib.rs

**Commits:**
- [FOUND] c7128c8 (Task 1)
- [FOUND] 81689b7 (Task 2)

**Tests:**
- [PASSED] 6 new query builder tests
- [PASSED] 385 existing tests (zero regressions)
- [PASSED] Zero clippy warnings

All verification checks passed.

---
*Phase: 08-openfec-api-client*
*Completed: 2026-02-12*
