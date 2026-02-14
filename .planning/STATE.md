# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-11)

**Core value:** Users can see what politicians are trading, what those positions are currently worth, and whether politicians are making or losing money on their trades.
**Current focus:** v1.2 FEC Donation Integration -- Phase 7 next

## Current Position

Phase: 12 of 12 (Employer Correlation Analysis)
Plan: 4 of 5 (completed)
Status: In Progress
Last activity: 2026-02-14 -- Completed plan 12-04 (Donor Context UI)

Progress: [#########.] 97% (Phase 12 in progress: 4/5 plans complete)

## Performance Metrics

**Velocity (v1.1 + v1.2):**
- Total plans completed: 17
- Average duration: 7.6 min (v1.1: 4.4 min, v1.2: 9.6 min)
- Total execution time: 2.14 hours

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

**Phase 10 Plan 2:**
- Duration: 5 min
- Completed: 2026-02-13
- Tasks: 2
- Files: 3 (1 created, 2 modified)

**Phase 11 Plan 1:**
- Duration: 4 min
- Completed: 2026-02-13
- Tasks: 2
- Files: 2 (modified)

**Phase 11 Plan 2:**
- Duration: 5 min
- Completed: 2026-02-13
- Tasks: 2
- Files: 5 (1 created, 4 modified)

**Phase 12 Plan 1:**
- Duration: 4 min
- Completed: 2026-02-14
- Tasks: 2
- Files: 5 (2 created, 3 modified)

**Phase 12 Plan 2:**
- Duration: 9 min
- Completed: 2026-02-14
- Tasks: 2
- Files: 3 (3 modified)

**Phase 12 Plan 4:**
- Duration: 3 min
- Completed: 2026-02-14
- Tasks: 2
- Files: 4 (4 modified)

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

**Phase 10 Plan 2:**
- Circuit breaker threshold 5 (lower than enrich_prices' 10 due to OpenFEC rate limits)
- Concurrency 3 workers (lower than enrich_prices' 5 for same reason)
- 403 InvalidApiKey causes immediate failure with helpful message (not circuit breaker)
- Separate DB handles: setup_db for queries, receiver_db for writes (avoids Arc<Mutex> contention)
- Duration formatting uses as_secs_f64() instead of humantime crate (avoid new dependency)

**Phase 11 Plan 1:**
- All donation queries join through donation_sync_meta (not direct politician_id on donations table)
- NULL contributor names display as 'Unknown' via COALESCE in all queries
- build_donation_where_clause shared helper avoids duplicating filter logic across 4 query methods
- Aggregations use COUNT(DISTINCT contributor_name) for accurate contributor counts despite NULL values

**Phase 11 Plan 2:**
- Politician filter uses name resolution (not ID) for better UX, with disambiguation on multiple matches
- Cycle validation requires even year >= 1976 (FEC data availability constraint)
- Separate output row structs for each aggregation type rather than generic approach
- CSV sanitization applies to contributor and employer fields (user-generated content risk)

**Phase 12 Plan 1:**
- Scoped MutexGuard pattern fixes await_holding_lock clippy warning (auto-fixed pre-existing bug from Phase 9)
- Start with 52 seed mappings instead of aspirational 200 (quality over quantity, incremental growth via Plan 03)
- Corporate suffix list sorted by length descending (prevents incorrect partial matches like 'corp' before 'corporation')
- Short employer names (< 5 chars) require exact match only (prevents false positives from fuzzy matching abbreviations)

**Phase 12 Plan 2:**
- issuer_ticker used as FK instead of issuer_id for cross-database portability (issuer_id values are auto-increment specific)
- employer_lookup table enables SQL JOINs without calling Rust normalization functions for raw donation employer strings
- Donation summary includes ALL donations in total (even without employer matches) + top 5 sectors from matched employers only
- Schema version test expectations updated from 4 to 5 (7 migration tests affected)

**Phase 12 Plan 4:**
- DbTradeRow extended with politician_id and issuer_sector for donor context lookup (no additional JOINs needed)
- Donor context groups by (politician, sector) to avoid duplicate output for same sector
- --show-donor-context requires --db mode, shows helpful note in scrape mode
- --show-donations requires --politician filter for targeted donation summary
- All donor/donation output on stderr to preserve stdout for piped data formats
- Non-fatal error handling: donation summary errors print warnings, don't fail portfolio command

All decisions logged in PROJECT.md Key Decisions table.

### Pending Todos

None.

### Blockers/Concerns

- Phase 9 research flag: CapitolTrades politician_id format needs investigation to determine crosswalk strategy (Bioguide ID vs proprietary). Validate with 5-10 real politician records.
- Phase 12 research flag: Employer fuzzy matching thresholds need empirical tuning with real FEC data.
- OpenFEC rate limit ambiguity: 100 vs 1,000 calls/hour needs empirical verification via X-RateLimit-Limit headers during Phase 8 development.

## Session Continuity

Last session: 2026-02-14
Stopped at: Completed Phase 12 Plan 4 (Donor Context UI)
Next step: Continue Phase 12 with Plan 5 (UAT - Employer Correlation Analysis).
