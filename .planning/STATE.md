# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-07)

**Core value:** Every synced record has complete data populated from detail pages, so downstream analysis works with real values instead of placeholders.
**Current focus:** Phase 4 in progress. Committee scraping, DB persistence, and sync integration done. Next: CLI output (04-03).

## Current Position

Phase: 4 of 6 (Politician Enrichment) -- IN PROGRESS
Plan: 2 of 3 in phase 4 (complete)
Status: Executing Phase 4
Last activity: 2026-02-08 -- Completed 04-02-PLAN.md (sync pipeline integration)

Progress: [########--] 75% (9 of ~12 total plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 9
- Average duration: 4.0 min
- Total execution time: 36 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation | 2/2 | 6 min | 3 min |
| 2. Trade Extraction | 2/2 | 12 min | 6 min |
| 3. Trade Sync | 3/3 | 10 min | 3.3 min |
| 4. Politician Enrichment | 2/3 | 8 min | 4 min |

**Recent Trend:**
- Last 5 plans: 03-01 (3 min), 03-02 (4 min), 03-03 (3 min), 04-01 (5 min), 04-02 (3 min)
- Trend: Consistent 3-5 min per plan

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
- 03-01: Enrichment runs post-ingest (after sync_trades) rather than inline, keeping existing --with-trade-details unchanged
- 03-01: Integration tests in db.rs rather than sync.rs since they exercise DB methods and reuse existing helpers
- 03-02: Used WHERE 1=1 idiom for clean dynamic clause appending without first-condition tracking
- 03-02: GROUP_CONCAT with DISTINCT and COALESCE for comma-separated join table values (empty string, not NULL)
- 03-02: issuer_ticker uses unwrap_or_default() since some issuers lack tickers
- 03-03: Unsupported DB filters bail with explicit supported-filter list rather than silently ignoring
- 03-03: capitalize_party() maps validation lowercase to DB capitalized format (Democrat not democrat)
- 03-03: Reused items_to_xml generic function for DbTradeRow XML serialization
- 04-01: Used real HTML fixture from live site instead of synthetic -- caught singular/plural label bug that synthetic fixtures would have missed
- 04-01: Fixed parse_politician_cards regex to handle singular labels (Trade/Issuer) globally, not just for committee-filtered pages
- 04-01: replace_all_politician_committees uses EXISTS subquery to silently skip unknown politician_ids (FK safety)
- 04-02: Committee enrichment runs unconditionally (POL-03) -- no --enrich flag needed since 48 requests is fast (~25s)
- 04-02: Throttle delay applied between committees and between pages within multi-page committees
- 04-02: enrich_politician_committees returns inserted count (after FK filtering) rather than total collected

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

Phase 3:
- Post-ingest enrichment: sync trades first, then loop over unenriched queue with configurable batch_size and throttle delay
- Hidden CLI alias: deprecated flags marked with hide=true and aliased to new flags in run()
- Dry-run pattern: check count_unenriched_trades() and report without HTTP calls
- Dynamic filter builder: push WHERE clauses and params into vecs, join at end
- DbTradeRow as canonical read-side trade type (vs Trade for API, ScrapedTrade for scraping)
- DB command path: --db flag routes to run_db() bypassing scraper entirely
- Filter validation reuse: same validation functions for both scrape and DB paths

Phase 4:
- Committee-filter iteration: scrape listing page per committee code to build reverse mapping
- Singular/plural label handling: Trades?/Issuers? in card regex for live site compatibility
- FK-safe bulk insert: EXISTS subquery in INSERT OR IGNORE skips unknown politician_ids
- Bulk replace pattern: DELETE all + INSERT with FK guard in single unchecked_transaction
- Unconditional post-ingest enrichment: fast operations (< 30s) run every sync without opt-in flag

### Pending Todos

None.

### Blockers/Concerns

- POL-01 risk RESOLVED: Politician detail pages confirmed to lack committee data. Committee-filter iteration approach implemented and tested in 04-01.
- TRADE-05 (committees) and TRADE-06 (labels): Present in synthetic fixtures but UNCONFIRMED on live RSC payloads. If absent from live data, committees should come from politician enrichment (Phase 4) and labels from issuer enrichment (Phase 5).
- Synthetic fixtures may not match actual live RSC payload structure. Field names or nesting may differ when scraper runs against the live site. Real fixtures now used for politician cards (04-01).

## Session Continuity

Last session: 2026-02-08
Stopped at: Completed 04-02-PLAN.md (sync pipeline integration). Next: 04-03 (CLI output).
Resume file: .planning/phases/04-politician-enrichment/04-03-PLAN.md
