# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-11)

**Core value:** Users can see what politicians are trading, what those positions are currently worth, and whether politicians are making or losing money on their trades.
**Current focus:** v1.2 FEC Donation Integration -- Phase 7 next

## Current Position

Phase: 10 of 12 (Donation Sync Pipeline)
Plan: 1 of 2 (completed)
Status: In Progress
Last activity: 2026-02-13 -- Completed plan 10-01 (Donation Sync DB Operations)

Progress: [######....] 75% (5/6 v1.2 phases)

## Performance Metrics

**Velocity (v1.1 + v1.2):**
- Total plans completed: 14
- Average duration: 8.6 min (v1.1: 4.4 min, v1.2: 12.1 min)
- Total execution time: 1.87 hours

**Phase 7 Plan 1:**
- Duration: 20 min
- Completed: 2026-02-12
- Tasks: 2
- Files: 8 (7 modified, 1 created)

**Phase 7 Plan 2:**
- Duration: 37 min
- Completed: 2026-02-12
- Tasks: 2
- Files: 7 (5 modified, 2 created)

**Phase 8 Plan 1:**
- Duration: 2 min
- Completed: 2026-02-12
- Tasks: 2
- Files: 5 (4 created, 1 modified)

**Phase 8 Plan 2:**
- Duration: 3 min
- Completed: 2026-02-12
- Tasks: 2
- Files: 6 (5 created, 1 modified)

**Phase 9 Plan 1:**
- Duration: 5 min
- Completed: 2026-02-13
- Tasks: 2
- Files: 2 (2 modified)

**Phase 9 Plan 2:**
- Duration: 7 min
- Completed: 2026-02-13
- Tasks: 2
- Files: 4 (2 created, 2 modified)

**Phase 10 Plan 1:**
- Duration: 5 min
- Completed: 2026-02-13
- Tasks: 2
- Files: 2 (modified)

## Accumulated Context

### Decisions

**Phase 7 Plan 1:**
- dotenvy loads .env silently at startup (no panic if missing) to allow non-donation commands without API key
- require_openfec_api_key() defers API key validation until donation commands need it (Phase 8+)
- fec_mappings uses composite PK (politician_id, fec_candidate_id) for multiple FEC IDs per politician
- Schema v3 migration follows IF NOT EXISTS pattern for idempotency

**Phase 7 Plan 2:**
- Use (last_name, state) composite key for matching instead of first_name matching to minimize false positives
- Skip matches when multiple politicians have same (last_name, state) to avoid incorrect FEC ID assignment
- Store bioguide_id in fec_mappings even though not used for lookup (audit trail)
- Download both current + historical legislators to maximize match coverage
- Use tracing::warn! for collision detection instead of failing entire sync

**Phase 8 Plan 1:**
- No DashMap cache in OpenFecClient (caching belongs at DB level in Phase 9)
- Schedule A query has NO page field - keyset pagination only with last_index + last_contribution_receipt_date
- API key passed as query parameter, never as header
- HTTP 429 and 403 status codes mapped to typed errors for circuit breaker logic

**Phase 8 Plan 2:**
- Mount more specific wiremock mocks first to prevent false matches in pagination test
- Include /v1 in base_url for with_base_url() to match production client behavior
- Combined deserialization and integration tests in single file (both validate same fixtures)

**Phase 9 Plan 1:**
- No FOREIGN KEY from donations to fec_committees (donations may arrive before committee metadata)
- No FOREIGN KEY from donation_sync_meta to politicians (reduces cascade overhead for metadata-only table)
- Refactored upsert_committee to accept Committee struct to avoid clippy 8-parameter warning
- JSON column merges across multiple FEC candidate IDs for same politician (deduplicates committee list)

**Phase 9 Plan 2:**
- Designation checked FIRST in classification (leadership PACs have designation D regardless of H/S/P type)
- Arc<Mutex<Db>> for shared DB access (rusqlite::Connection is not Send+Sync)
- Drop db lock before any async operations to prevent clippy warnings and potential deadlocks
- Made Db::open_in_memory() and Db::conn() public for integration test usage
- Empty committee results cached to prevent repeated API calls for not-found politicians

**Phase 10 Plan 1:**
- save_sync_cursor_with_donations uses unchecked_transaction for atomicity (prevents cursor state desync)
- insert_donation returns false for NULL sub_id (no panic, no insert)
- load_sync_cursor filters WHERE last_index IS NOT NULL (completion check)
- mark_sync_completed sets last_index to NULL (signals no more pages)

All decisions logged in PROJECT.md Key Decisions table.

### Pending Todos

None.

### Blockers/Concerns

- Phase 9 research flag: CapitolTrades politician_id format needs investigation to determine crosswalk strategy (Bioguide ID vs proprietary). Validate with 5-10 real politician records.
- Phase 12 research flag: Employer fuzzy matching thresholds need empirical tuning with real FEC data.
- OpenFEC rate limit ambiguity: 100 vs 1,000 calls/hour needs empirical verification via X-RateLimit-Limit headers during Phase 8 development.

## Session Continuity

Last session: 2026-02-13
Stopped at: Completed Phase 10 Plan 1 (Donation Sync DB Operations)
Next step: Ready for Phase 10 Plan 2 (sync-donations CLI command)
