# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-07)

**Core value:** Every synced record has complete data populated from detail pages, so downstream analysis works with real values instead of placeholders.
**Current focus:** Phase 2 complete. Ready for Phase 3 (Trade Sync).

## Current Position

Phase: 2 of 6 (Trade Extraction) -- COMPLETE
Plan: 2 of 2 in phase 2 (complete)
Status: Phase complete
Last activity: 2026-02-08 -- Completed 02-02-PLAN.md (trade detail DB persistence)

Progress: [####------] 33% (4 of ~12 total plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 4
- Average duration: 5 min
- Total execution time: 18 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation | 2/2 | 6 min | 3 min |
| 2. Trade Extraction | 2/2 | 12 min | 6 min |

**Recent Trend:**
- Last 5 plans: 01-01 (3 min), 01-02 (3 min), 02-01 (8 min), 02-02 (4 min)
- Trend: Consistent 3-8 min per plan

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
- 02-02: Used unchecked_transaction() for &self receiver consistency with get_unenriched_*_ids methods
- 02-02: Asset type one-way upgrade: only updates from "unknown", never overwrites enriched values
- 02-02: Empty committees/labels treated as no-op, not clear-all, to protect previously extracted data

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
- unchecked_transaction for &self: use when method needs atomicity but not exclusive access
- asset_type one-way upgrade: WHERE asset_type = 'unknown' guard prevents overwrite of enriched values
- Join table refresh: delete+insert when new data available, skip when empty

### Pending Todos

None.

### Blockers/Concerns

- Research flagged that politician detail page RSC payload may not contain committee data (POL-01 risk). Needs verification during Phase 4 planning.
- TRADE-05 (committees) and TRADE-06 (labels): Present in synthetic fixtures but UNCONFIRMED on live RSC payloads. If absent from live data, committees should come from politician enrichment (Phase 4) and labels from issuer enrichment (Phase 5).
- Synthetic fixtures may not match actual live RSC payload structure. Field names or nesting may differ when scraper runs against the live site.

## Session Continuity

Last session: 2026-02-08
Stopped at: Completed 02-02-PLAN.md (trade detail DB persistence). Phase 2 complete. Next: Phase 3 planning.
Resume file: .planning/ROADMAP.md (next phase planning)
