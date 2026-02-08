# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-07)

**Core value:** Every synced record has complete data populated from detail pages, so downstream analysis works with real values instead of placeholders.
**Current focus:** All 6 phases complete. 15/15 plans executed.

## Current Position

Phase: 6 of 6 (Concurrency and Reliability)
Plan: 2 of 2 in phase 6 (06-02 complete)
Status: All phases complete
Last activity: 2026-02-08 -- Completed 06-02-PLAN.md (committee spinner + circuit breaker tests)

Progress: [##########] 100% (15 of 15 total plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 15
- Average duration: 3.7 min
- Total execution time: 55 min

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 1. Foundation | 2/2 | 6 min | 3 min |
| 2. Trade Extraction | 2/2 | 12 min | 6 min |
| 3. Trade Sync | 3/3 | 10 min | 3.3 min |
| 4. Politician Enrichment | 3/3 | 12 min | 4 min |
| 5. Issuer Enrichment | 3/3 | 10 min | 3.3 min |
| 6. Concurrency | 2/2 | 5 min | 2.5 min |

**Recent Trend:**
- Last 5 plans: 05-01 (4 min), 05-02 (2 min), 05-03 (4 min), 06-01 (3 min), 06-02 (2 min)
- Trend: Consistent 2-4 min per plan

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
- 04-03: Copied capitalize_party() to politicians.rs rather than factoring shared utility -- 6-line function not worth coupling
- 04-03: Unsupported filters (--committee, --issuer-id) bail with supported-filter list on DB path
- 04-03: query_politicians uses same dynamic filter pattern as query_trades (Vec of Box dyn ToSql, param_idx)
- 05-01: Performance JSON parsed inline from serde_json::Value rather than deserializing to DbPerformance struct
- 05-01: COALESCE on nullable issuer fields but direct overwrite on issuer_name (detail page authoritative)
- 05-01: Incomplete performance (missing required fields) treated same as null -- DELETE existing rows
- 05-02: Batch size shared independently: --batch-size N enriches up to N trades AND up to N issuers per run
- 05-02: Issuer enrichment placed after trade enrichment but before committee enrichment in sync pipeline
- 05-03: Vec filter params with IN clause for sector/state/country (more flexible than single-value for issuers)
- 05-03: Table output uses trailing30_change and trailing365_change (percentage returns) not raw trailing values
- 05-03: JSON/XML serializes full DbIssuerRow directly (21 fields) while table uses curated 9-column subset
- 05-03: format_large_number (T/B/M suffixes) and format_percent (+/-X.X%) helper functions for performance data display
- 06-01: Clone on ScrapeClient is cheap (reqwest::Client is Arc-backed), safe to clone for spawned tasks
- 06-01: Throttle delay is per-task, not global -- with concurrency=3 and delay=500ms, effective rate up to ~6 req/s
- 06-01: DB writes remain single-threaded via mpsc channel receiver loop, avoiding SQLite contention
- 06-01: CircuitBreaker is simple kill switch (consecutive failure counter), not full half-open/closed pattern
- 06-01: pb.println() for warnings instead of eprintln() to avoid garbled output with progress bar
- 06-02: Spinner shows cumulative membership total alongside per-committee count for progress context
- 06-02: Removed redundant eprintln status lines from run() since spinner finish_with_message provides the same info

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
- DB politician command path: --db flag routes to run_db(), DbPoliticianFilter, query_politicians()
- DbPoliticianRow as canonical read-side politician type (vs PoliticianDetail for API)
- DbPoliticianOutputRow with Committees column for all 5 output formats

Phase 5:
- Issuer enrichment COALESCE: nullable fields protected, issuer_name always overwritten, enriched_at set via datetime('now')
- Performance validation inline: check all 20 required fields present and non-null before persisting
- EOD price refresh: DELETE all for issuer_id then INSERT new entries in same transaction
- Shared enrichment flags: single --enrich/--dry-run/--batch-size flags control multiple enrichment passes, each getting independent limits
- DB issuer command path: --db flag routes to run_db(), DbIssuerFilter, query_issuers()
- DbIssuerRow as canonical read-side issuer type (vs IssuerDetail for API)
- IN clause multi-value filter: Vec<String> params with dynamic placeholder generation for multi-value IN clauses

Phase 6:
- Semaphore+JoinSet+mpsc pattern: spawn tasks with semaphore-guarded concurrency, collect results via mpsc channel, process in single receive loop
- CircuitBreaker struct: consecutive_failures counter with threshold, record_success resets to 0, record_failure increments, is_tripped checks >= threshold
- Progress bar via pb.println() for warnings (avoids garbled output with indicatif)
- join_set.abort_all() for fast shutdown when circuit breaker trips
- indicatif spinner for sequential iteration: new_spinner with set_message per item and finish_with_message for summary

### Pending Todos

None.

### Blockers/Concerns

- POL-01 risk RESOLVED: Politician detail pages confirmed to lack committee data. Committee-filter iteration approach implemented and tested in 04-01.
- TRADE-05 (committees) and TRADE-06 (labels): Present in synthetic fixtures but UNCONFIRMED on live RSC payloads. If absent from live data, committees should come from politician enrichment (Phase 4) and labels from issuer enrichment (Phase 5).
- Synthetic fixtures may not match actual live RSC payload structure. Field names or nesting may differ when scraper runs against the live site. Real fixtures now used for politician cards (04-01).

## Session Continuity

Last session: 2026-02-08
Stopped at: Completed 06-02-PLAN.md (committee spinner + circuit breaker tests). All plans complete.
Resume file: N/A -- project complete
