# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-11)

**Core value:** Users can see what politicians are trading, what those positions are currently worth, and whether politicians are making or losing money on their trades.
**Current focus:** v1.2 FEC Donation Integration -- Phase 7 next

## Current Position

Phase: 7 of 12 (Foundation & Environment Setup)
Plan: 2 of 2 (completed)
Status: Phase 7 complete
Last activity: 2026-02-12 -- Completed plan 07-02 (FEC mapping module and sync-fec CLI)

Progress: [##........] 17% (1/6 v1.2 phases)

## Performance Metrics

**Velocity (v1.1 + v1.2):**
- Total plans completed: 9
- Average duration: 10.3 min (v1.1: 4.4 min, v1.2: 28.5 min)
- Total execution time: 1.47 hours

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

All decisions logged in PROJECT.md Key Decisions table.

### Pending Todos

None.

### Blockers/Concerns

- Phase 9 research flag: CapitolTrades politician_id format needs investigation to determine crosswalk strategy (Bioguide ID vs proprietary). Validate with 5-10 real politician records.
- Phase 12 research flag: Employer fuzzy matching thresholds need empirical tuning with real FEC data.
- OpenFEC rate limit ambiguity: 100 vs 1,000 calls/hour needs empirical verification via X-RateLimit-Limit headers during Phase 8 development.

## Session Continuity

Last session: 2026-02-12
Stopped at: Completed Phase 7 Plan 2 (FEC Mapping Module and sync-fec CLI)
Next step: Plan and execute Phase 8 (OpenFEC API Client)
