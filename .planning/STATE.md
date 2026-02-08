# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-07)

**Core value:** Every synced record has complete data populated from detail pages, so downstream analysis works with real values instead of placeholders.
**Current focus:** Phase 2 - Trade Extraction (Plan 1 complete, Plan 2 next)

## Current Position

Phase: 2 of 6 (Trade Extraction)
Plan: 1 of 2 in phase 2 (complete)
Status: In progress
Last activity: 2026-02-08 -- Completed 02-01-PLAN.md (trade detail fixture capture and scraper extension)

Progress: [###-------] 25% (3 of ~12 total plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 3
- Average duration: 5 min
- Total execution time: 14 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation | 2/2 | 6 min | 3 min |
| 2. Trade Extraction | 1/2 | 8 min | 8 min |

**Recent Trend:**
- Last 5 plans: 01-01 (3 min), 01-02 (3 min), 02-01 (8 min)
- Trend: Slightly longer (fixture creation + more complex code)

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- Roadmap: PERF-04 (throttle delay) grouped with Phase 3 (trade sync) rather than Phase 6 (concurrency) because throttle tuning is needed for sequential enrichment, not just parallel
- Roadmap: OUT-01/02/03 distributed to their entity phases (3/4/5) rather than a separate output phase, so each phase delivers end-to-end value
- Roadmap: Phases 4 and 5 depend only on Phase 1, not on Phase 3, allowing politician/issuer enrichment to proceed in parallel with trade sync work
- 01-01: Run migration before schema batch in init() so enrichment indexes can reference enriched_at on pre-migration databases
- 01-01: Schema versioning pattern established: PRAGMA user_version tracks migration state, numbered private methods (migrate_v1, migrate_v2, etc.)
- 02-01: Used synthetic fixtures because live capitoltrades.com returns loading states via curl (RSC data streamed client-side)
- 02-01: Rewrote extract_trade_detail to use full object extraction (backward walk + extract_json_object) instead of 500-char window
- 02-01: Support both filingUrl and filingURL key names for RSC/BFF compatibility

### Patterns Established

Phase 1:
- Schema versioning: PRAGMA user_version tracks migration state
- Sentinel CASE pattern: WHEN excluded.field != sentinel THEN excluded.field ELSE table.field END
- enriched_at pinning: every upsert ON CONFLICT clause includes enriched_at = table.enriched_at
- Enrichment queue pattern: SELECT id FROM table WHERE enriched_at IS NULL ORDER BY id [LIMIT n]

Phase 2:
- Full JSON object extraction: walk backwards from needle to opening brace, use extract_json_object
- Synthetic HTML fixtures: model RSC payload structure from BFF API types when live site unavailable
- Fixture-based scrape testing: include_str! fixtures through extract_rsc_payload, then test extraction

### Pending Todos

None.

### Blockers/Concerns

- Research flagged that politician detail page RSC payload may not contain committee data (POL-01 risk). Needs verification during Phase 4 planning.
- TRADE-05 (committees) and TRADE-06 (labels): Present in synthetic fixtures but UNCONFIRMED on live RSC payloads. If absent from live data, committees should come from politician enrichment (Phase 4) and labels from issuer enrichment (Phase 5).
- Synthetic fixtures may not match actual live RSC payload structure. Field names or nesting may differ when scraper runs against the live site.

## Session Continuity

Last session: 2026-02-08
Stopped at: Completed 02-01-PLAN.md (trade detail fixtures and scraper extension). Next: 02-02-PLAN.md (Db update_trade_detail method).
Resume file: .planning/phases/02-trade-extraction/02-02-PLAN.md
