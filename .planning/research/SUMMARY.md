# Project Research Summary

**Project:** Capitol Traders - Detail-Page Enrichment Pipeline
**Domain:** Web scraper enrichment for congressional stock trading data
**Researched:** 2026-02-07
**Confidence:** HIGH

## Executive Summary

This project extends an existing congressional trading scraper to populate missing data fields by fetching detail pages for 35K+ trades, 500+ politicians, and 2,500+ issuers. The scraper uses Next.js RSC (React Server Components) payload extraction from HTML script tags, not traditional DOM parsing. Research focused on the four critical dimensions: technology stack additions (concurrency control, progress reporting), required features (smart-sync, idempotent upserts, batch processing), architectural patterns (separate enrichment passes, checkpoint/resume), and domain-specific pitfalls (data corruption via COALESCE direction, payload format instability, rate limiting at scale).

The recommended approach is to structure enrichment as distinct post-listing passes (not interleaved with listing scraping), use SQLite state as the checkpoint mechanism (query for un-enriched rows), and implement bounded concurrency (3-5 parallel detail fetches via tokio::sync::Semaphore) with batch commits (50-100 entities per transaction). The existing retry/backoff logic handles server pushback; the missing pieces are concurrency caps, progress reporting, and correct upsert merge logic that never overwrites enriched data with listing-page defaults.

Key risks: (1) COALESCE direction in upserts silently erases enriched data during incremental syncs - this is a data corruption bug that must be fixed before any enrichment runs; (2) RSC payload format changes between Next.js versions can break all scrapers simultaneously - mitigated by canary tests and fallback extraction patterns; (3) Sequential detail fetching takes 24+ hours for 35K trades - requires parallel fetching with semaphore-bounded concurrency. All three risks have proven mitigation strategies derived from direct codebase analysis.

## Key Findings

### Recommended Stack

The project already has the core HTTP (reqwest 0.12), async runtime (tokio), and parsing (regex, serde_json) infrastructure. Enrichment requires only three additions: concurrency control (tokio::sync::Semaphore, already in dependency tree), concurrent stream processing (futures 0.3 for buffer_unordered), and progress reporting (indicatif 0.17 for multi-hour runs). Do NOT add governor (overlaps with existing retry logic), do NOT add scraper crate (RSC payloads are JSON-in-script-tags, not DOM-queryable), do NOT add headless browsers (no JS execution needed). The existing regex-based RSC extraction is correct and efficient.

**Core technologies:**
- tokio::sync::Semaphore (already available): Bounded concurrency for 3-5 parallel detail fetches - simpler than adding governor for rate shaping, works with existing retry/backoff
- futures 0.3: StreamExt::buffer_unordered provides clean bounded-concurrency pipeline with natural backpressure
- indicatif 0.17: Progress bars essential for multi-hour enrichment runs (35K trades at 250ms delay = 2.4+ hours minimum)

### Expected Features

The enrichment pipeline has clear table-stakes features (without these, it is broken or useless) and valuable differentiators (significantly improve usability without being expected).

**Must have (table stakes):**
- Extract all missing trade fields from detail pages (asset_type, committees, labels, size) - currently 35K trades show "unknown" asset type, blocking all --asset-type filtering
- Smart-sync: skip already-enriched rows - enriching 35K trades takes 2+ hours minimum; users must not re-fetch detail pages for rows with complete data
- Idempotent upserts for enriched fields - re-running enrichment must not corrupt existing good data (use COALESCE so listing-page NULLs do not overwrite detail-page values)
- Progress reporting during enrichment - users need to see "Enriching trade 1,234 / 35,000 (3.5%)" with ETA for multi-hour runs
- Issuer detail enrichment for performance data (market cap, trailing returns, EOD prices) - listing pages return no performance data
- Surface enriched data in all output formats - enrichment is pointless if CLI output still shows "unknown" for asset type

**Should have (competitive):**
- Checkpoint/resume for interrupted enrichment - if a 2-hour run dies at trade 15,000, should resume from 15,001 not restart from scratch (store last_enriched_tx_id in ingest_meta)
- Parallel enrichment with concurrency limit - fetch 3-5 detail pages concurrently with shared rate limiter to saturate bandwidth without hammering server
- Selective enrichment by entity type (--enrich trades vs --enrich issuers vs --enrich all) - let users enrich only needed data
- Enrichment statistics summary - after completion, print "Enriched 12,345 trades. Asset types resolved: 11,890. Still missing: 455 trades"
- Dry-run mode for enrichment - query DB for what would be enriched and report counts without making HTTP requests

**Defer (v2+):**
- Real-time / streaming enrichment - data source updates daily (45-day STOCK Act window); daily batch is correct cadence
- Scraping full BFF API directly - unofficial, undocumented, changes without notice; stick with HTML/RSC payload scraping (more stable)
- Multi-threaded SQLite writes - SQLite does not support concurrent writers; parallelize HTTP fetching but serialize DB writes
- Enrichment via external APIs (SEC EDGAR, market data APIs) - expanding scope, focus on capitoltrades.com as single source

### Architecture Approach

The architecture separates enrichment into distinct post-listing passes (Phase 1: listing ingest, Phase 2: trade enrichment, Phase 3: politician enrichment, Phase 4: issuer enrichment). This matters for resumability (listing data already persisted if enrichment fails), selectivity (only enrich un-enriched rows), and rate limiting (easy to add delays, progress tracking, resume logic). Detection of "needs enrichment" uses an enriched_at timestamp column per entity table (NULL means not yet enriched, ISO 8601 means enriched on that date) rather than sentinel values, eliminating ambiguity for politicians with zero committees and issuers lacking performance data.

**Major components:**
1. ScrapeClient (capitoltraders_lib/src/scrape.rs) - HTTP fetching + RSC payload parsing; all detail methods already exist; no changes needed for enrichment
2. Db enrichment queries (capitoltraders_lib/src/db.rs) - NEW: get_unenriched_*_ids(), update_*_detail(), get_stale_*_ids() methods for query/update/tracking
3. sync command enrichment passes (capitoltraders_cli/src/commands/sync.rs) - NEW: enrich_trades(), enrich_politicians(), enrich_issuers() orchestrate detail fetching with progress, delays, error handling

### Critical Pitfalls

The most dangerous failure modes are data corruption, silent breakage, and scalability collapse. All three require design-time prevention.

1. **COALESCE Direction in Upserts Silently Erases Enriched Data** - Current upsert unconditionally overwrites filing_id, filing_url, size from excluded.* without COALESCE guards. Listing-page data has these fields set to defaults (0, empty string, NULL), so every re-sync clobbers previously enriched data. After a full detail enrichment pass, the next incremental sync erases filing URLs for any trades in the overlap window. Prevention: Change upsert to use CASE expressions (for non-nullable with sentinels) and COALESCE (for nullable) to preserve existing non-default values. Add test that upserts trade with detail data, then upserts same trade with listing-page defaults, and asserts detail data survives. MUST be addressed in Phase 1 before any detail fetching.

2. **RSC Payload Format Changes Break All Scrapers Silently** - extract_rsc_payload depends on exact string `self.__next_f.push([1,"` appearing in HTML. Next.js has changed this format across major versions. The December 2025 RSC RCE vulnerability (CVE-2025-55182) forced payload handling patches. A Next.js upgrade changes payload delivery mechanism, every scraper method returns ScrapeError::MissingPayload. Prevention: Add canary test that runs daily in CI against known-good URL, validate RSC payload structure. Make payload extraction needle configurable (env var or const). Implement fallback extraction (__NEXT_DATA__ script tags). Log first 200 chars of payload on parse failures. Should be addressed before starting enrichment.

3. **Sequential Detail Fetching Makes Full Enrichment Take 24+ Hours** - Current --with-trade-details fetches one trade detail at a time with 250ms delay. For 35,266 trades: 35266 * (request_time + 250ms). Even with 500ms average request time, that is 35266 * 0.75s = ~7.3 hours. With retries and occasional 429s, easily exceeds 24 hours. GitHub Actions has 6-hour limit; long-running processes are fragile. Prevention: Implement bounded concurrency via tokio::sync::Semaphore with 3-5 permits, use futures::stream::buffer_unordered for concurrent detail fetching while respecting rate limits. Process in batches (fetch 50 details, upsert, commit, repeat) to cap memory and provide checkpoints. Phase 2 after upsert correctness established.

4. **No Checkpoint/Resume Means Failures Waste Hours of Work** - sync loop processes all pages sequentially, only updates last_trade_pub_date in ingest_meta after entire run completes. If process crashes at trade 30,000 of 35,266, no progress saved. Next run starts from scratch or from last successful last_trade_pub_date (may be weeks old). Prevention: Track enrichment progress separately (add ingest_meta keys like last_enriched_trade_id). Commit enrichment results in batches of 50-100 trades; after each batch commit, update cursor. On restart, query SELECT tx_id FROM trades WHERE enriched_at IS NULL to find un-enriched trades. Phase 2, designed alongside concurrency model.

5. **Rate Limiting Calibration for 40x Request Volume Increase** - Current scraping makes ~2,900 listing page requests for full sync. Detail enrichment adds 35K + 500 + 5K = ~40,500 requests (14x total volume increase). Existing retry/backoff (2s base, 30s max, 3 retries) tuned for hundreds of requests, not tens of thousands. Site starts returning 429s, backoff maxes out, retry storm. Prevention: Add global rate limiter (token bucket) targeting 1-2 requests per second sustained. Implement circuit breaker: after N consecutive 429s, pause all requests for 5 minutes. Make sustained request rate configurable via env var. Phase 2 alongside concurrency implementation.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Foundation - Schema, Upserts, Extraction

**Rationale:** Data corruption prevention and correct merge logic must come before any HTTP work. The upsert bug (Pitfall #1) silently erases enriched data; fixing this is the foundation. Adding enriched_at tracking columns enables checkpoint/resume and selective enrichment. Documenting actual detail-page payload structures before writing extraction code prevents wasted effort on wrong assumptions.

**Delivers:**
- enriched_at column added to trades, politicians, issuers tables (ALTER TABLE with duplicate-column guards)
- upsert_scraped_trades fixed to use CASE/COALESCE for enrichable fields (never overwrite good data with defaults)
- Test that verifies upsert correctness (upsert detail, then upsert listing, assert detail survives)
- Db module additions: get_unenriched_trade_ids(), get_unenriched_politician_ids(), get_unenriched_issuer_ids()
- Db module additions: update_trade_detail(), update_politician_detail(), update_issuer_detail()
- Captured HTML fixtures from trade/politician/issuer detail pages documenting RSC payload structure
- Canary test in CI validating RSC payload extraction (monitors for Next.js format changes)

**Addresses:**
- Must-have: Idempotent upserts for enriched fields (FEATURES.md table stakes #3)
- Must-have: Smart-sync skip enriched rows foundation (FEATURES.md table stakes #2)

**Avoids:**
- Critical: COALESCE direction erases enriched data (PITFALLS.md #1)
- Critical: RSC payload format changes break scrapers (PITFALLS.md #3)
- Moderate: Detail pages return different data structures (PITFALLS.md #6)
- Minor: Schema migration for new columns (PITFALLS.md #13)

### Phase 2: Trade Enrichment - Core Pipeline

**Rationale:** Trade enrichment is the simplest entity type (trade_detail() already exists and is used by CLI) and has the highest user impact (35K trades with "unknown" asset_type block filtering). This phase establishes the enrichment pattern that politicians/issuers will follow. Sequential implementation proves correctness before adding concurrency complexity. Batch commits provide natural checkpoints.

**Delivers:**
- enrich_trades() function in sync.rs: query unenriched trade IDs, fetch detail pages, update trades + committees + labels tables, set enriched_at
- Sequential implementation with configurable delay (--enrich-delay-ms, default 500ms)
- Batch commit pattern (commit every 50-100 trades with progress logging to stderr)
- Progress reporting: "Enriched 1,234 / 35,000 trades (3.5%) - ETA: 2h 15m"
- Enrichment statistics summary: "Enriched 12,345 trades. Asset types resolved: 11,890. Committees populated: 8,234. Still missing: 455."
- CLI flags: --enrich-trades, --enrich-delay-ms, --enrich-limit
- Extend trade_detail scraper to extract asset_type, committees, labels, size, size_range_high, size_range_low, has_capital_gains from RSC payload
- Update TradeRow in output.rs to surface asset_type and size in table/CSV/Markdown formats

**Uses:**
- Existing reqwest 0.12 for HTTP
- Existing ScrapeClient.trade_detail() method (extended to parse more fields)
- New Db.get_unenriched_trade_ids() and Db.update_trade_detail() from Phase 1

**Implements:**
- Batch-query-then-iterate pattern (query all IDs upfront, iterate with delays)
- Granular UPDATE statements (not full upsert) for enrichment-only columns
- Transaction-per-batch (50-100 trades per transaction, skip individual failures)
- Progress reporting via stderr (matches existing convention)

**Addresses:**
- Must-have: Extract all missing trade fields from detail pages (FEATURES.md table stakes #1)
- Must-have: Smart-sync skip enriched rows (FEATURES.md table stakes #2 - uses enriched_at)
- Must-have: Progress reporting during enrichment (FEATURES.md table stakes #4)
- Must-have: Surface enriched data in output formats (FEATURES.md table stakes #6)
- Should-have: Enrichment statistics summary (FEATURES.md differentiator #4)

**Avoids:**
- Moderate: SQLite transaction size during batch enrichment (PITFALLS.md #7 - small batches)
- Minor: Memory growth during full enrichment (PITFALLS.md #12 - incremental commits)

### Phase 3: Concurrency and Reliability

**Rationale:** Sequential enrichment works but takes too long (7+ hours for 35K trades). Bounded concurrency cuts this to 2-3 hours without overwhelming the server. Checkpoint/resume makes failures recoverable. Progress bars improve UX for multi-hour runs. This phase optimizes the proven Phase 2 pattern for production use.

**Delivers:**
- Parallel enrichment with tokio::sync::Semaphore (3-5 permits) + futures::stream::buffer_unordered
- Progress bars via indicatif 0.17 (replace stderr logging with MultiProgress showing per-entity-type progress)
- Checkpoint/resume: track last_enriched_trade_id in ingest_meta, resume from checkpoint on restart
- Global rate limiter (1-2 requests per second sustained) to prevent 429 storms
- Circuit breaker: after N consecutive 429s, pause all requests for 5 minutes, then resume
- Adaptive delay: increase on 429/timeouts, decrease after consecutive successes
- --enrich-limit flag for time-bounded CI runs (enrich max N entities per run)
- --force-enrich flag to re-enrich all entities regardless of enriched_at

**Uses:**
- tokio::sync::Semaphore from STACK.md recommendation #1
- futures 0.3 StreamExt from STACK.md recommendation #2
- indicatif 0.17 from STACK.md recommendation #3

**Addresses:**
- Should-have: Parallel enrichment with concurrency limit (FEATURES.md differentiator #2)
- Should-have: Checkpoint/resume for interrupted enrichment (FEATURES.md differentiator #1)

**Avoids:**
- Critical: Sequential fetching takes 24+ hours (PITFALLS.md #2)
- Critical: No checkpoint/resume wastes hours of work (PITFALLS.md #4)
- Moderate: Rate limiting calibration for 40x volume increase (PITFALLS.md #8)

### Phase 4: Politician and Issuer Enrichment

**Rationale:** With the pattern proven and optimized for trades, extend to other entity types. Politician enrichment may only populate bio fields (committees data availability on detail pages needs verification). Issuer enrichment is the richest (performance + eod_prices tables). Both follow the same query-fetch-update-commit pattern established for trades.

**Delivers:**
- enrich_politicians() in sync.rs: populate dob, gender, chamber, nickname, social links, website, district (committees if available in detail page RSC payload)
- enrich_issuers() in sync.rs: populate state_id, c2iq, country, performance (mcap, trailing returns), eod_prices
- CLI flags: --enrich-politicians, --enrich-issuers, --enrich-all (shorthand)
- Selective enrichment: users can run --enrich issuers without touching trades/politicians
- Dry-run mode: --dry-run queries DB for what would be enriched, reports counts without HTTP requests

**Implements:**
- Same batch-query-iterate-commit pattern from Phase 2
- Same concurrency/checkpoint/progress patterns from Phase 3
- Column-specific updates for politicians (only touch committee-related columns, not stats from listing pages)

**Addresses:**
- Must-have: Issuer detail enrichment for performance data (FEATURES.md table stakes #5)
- Should-have: Selective enrichment by entity type (FEATURES.md differentiator #3)
- Should-have: Dry-run mode for enrichment (FEATURES.md differentiator #5)

**Avoids:**
- Moderate: Politician detail page returns partial data vs listing (PITFALLS.md #9 - column-specific update)

### Phase 5: CI Integration and Monitoring

**Rationale:** With all entity types enrichable, integrate into the daily sqlite-sync.yml workflow. Split enrichment by cadence (daily for trades, weekly for issuer performance). Add monitoring to detect payload format changes before users report breakage.

**Delivers:**
- Update .github/workflows/sqlite-sync.yml to run --enrich-trades daily
- Add weekly workflow for --enrich-issuers (performance data changes more frequently than politician committees)
- Alert on canary test failure (payload format change detected)
- Log enrichment statistics to workflow summary (enriched counts, skipped counts, failed counts)
- Consider time-of-day scheduling (overnight US time for off-peak server load)

**Implements:**
- Workflow orchestration for production enrichment cadence
- Monitoring for RSC payload format stability

**Avoids:**
- All pitfalls via continuous validation

### Phase Ordering Rationale

- Phase 1 must come first because all enrichment passes depend on correct upsert logic and enriched_at tracking. The data corruption bug is silent and destructive; fixing it before any enrichment runs is mandatory.
- Phase 2 should come second because it refactors existing working code (--with-trade-details pattern) and has the highest user impact (35K trades with missing data). Proving the sequential pattern works reduces risk before adding concurrency.
- Phase 3 optimizes the proven Phase 2 pattern for production scale. Concurrency, checkpoint, and progress are all enhancements to a working baseline.
- Phase 4 extends the proven pattern to other entity types. With trades working and optimized, politicians and issuers are straightforward applications of the same pipeline.
- Phase 5 integrates production workflows after all entity types are proven.

This ordering follows data dependency (upserts before fetching), risk reduction (sequential before concurrent), and incremental value delivery (highest-impact entity type first).

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 4 (Politician enrichment):** Need to verify committee data availability in politician detail page RSC payload. The ScrapedPolitician struct has no committees field currently. May require different scraping approach or external data source.
- **Phase 4 (Issuer enrichment):** issuer_performance and issuer_eod_prices involve multiple related tables; need to verify detail page payload structure for complete extraction.

Phases with standard patterns (skip research-phase):
- **Phase 1:** Schema migrations and upsert logic are standard SQLite patterns, well-documented.
- **Phase 2:** Sequential HTTP fetching with batch commits is established pattern in codebase.
- **Phase 3:** tokio::sync::Semaphore, futures::stream, indicatif are all mature libraries with clear documentation.
- **Phase 5:** GitHub Actions workflow patterns are well-known.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All recommendations based on direct codebase analysis. tokio::sync::Semaphore already in dependency tree. futures and indicatif are mature, widely-used libraries. Verified against crates.io (2026-02-07). RSC extraction approach validated by reading existing scrape.rs implementation. |
| Features | HIGH | Table stakes features derived from analyzing data gaps (35K trades with asset_type='unknown', empty committees/labels). Differentiators based on standard scraper reliability patterns (checkpoint/resume, dry-run). Anti-features identified from scope analysis (no real-time, no external APIs, no multi-threaded SQLite writes). |
| Architecture | HIGH | Based on direct analysis of capitoltraders_lib/src/scrape.rs, db.rs, and sync.rs. Component boundaries, data flow, and patterns extracted from existing code. Enrichment-as-separate-pass pattern follows from resumability and selectivity requirements. enriched_at tracking column pattern eliminates sentinel-value ambiguity. |
| Pitfalls | HIGH | Critical pitfalls verified by reading db.rs lines 327-376 (COALESCE bug), scrape.rs line 430 (RSC payload dependency), sync.rs lines 156-170 (sequential fetch), sync.rs line 103 (checkpoint timing). Moderate/minor pitfalls based on SQLite transaction semantics, HTTP rate limiting best practices, and Next.js version history. All sources cited with line numbers. |

**Overall confidence:** HIGH

### Gaps to Address

Research identified two open questions requiring validation before implementation:

- **Politician committees data source**: The politician_detail() method returns ScrapedPolitician with bio fields but no committees field. Need to inspect actual politician detail page RSC payload to determine if committees are present. If not available from detail pages, politician enrichment fills bio fields only and committees require separate data source (BFF API, external source like congress.gov, or different page on site).

- **Trade committees and labels extraction**: Trade detail page currently only extracts filing_url and filing_id. The ScrapedTradeDetail struct lacks committees and labels fields. Need to verify these fields exist in trade detail page RSC payload (as opposed to only being available via BFF API). If present, extend ScrapedTradeDetail struct and extraction logic; if not, trade committees/labels remain empty or require alternative source.

Both gaps affect Phase 4 (entity expansion) scope but do not block Phase 1-3 (foundation, trade enrichment, concurrency). Resolution strategy: capture and inspect real detail page HTML fixtures during Phase 1 payload documentation. If data is not present, document limitation and defer to future enhancement.

## Sources

### Primary (HIGH confidence)
- Direct codebase analysis: capitoltraders_lib/src/scrape.rs (ScrapeClient methods, struct definitions, RSC extraction)
- Direct codebase analysis: capitoltraders_lib/src/db.rs (upsert methods, schema interaction, transaction patterns)
- Direct codebase analysis: capitoltraders_cli/src/commands/sync.rs (pipeline orchestration, checkpoint timing)
- Direct codebase analysis: capitoltraders_cli/src/commands/trades.rs (existing detail enrichment, CLI redundancy)
- Direct codebase analysis: schema/sqlite.sql (table structure, join tables, missing performance/committee data)
- tokio::sync::Semaphore official docs: https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html
- futures StreamExt official docs: https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html
- SQLite UPSERT documentation: https://sqlite.org/lang_upsert.html
- indicatif crate: https://crates.io/crates/indicatif (0.17.11 verified 2026-02-07)
- Next.js RSC RCE CVE-2025-55182: https://snyk.io/blog/security-advisory-critical-rce-vulnerabilities-react-server-components/

### Secondary (MEDIUM confidence)
- RSC Flight protocol payload decoding: https://edspencer.net/2024/7/1/decoding-react-server-component-payloads
- Scraping Next.js in 2025: https://www.trickster.dev/post/scraping-nextjs-web-sites-in-2025/
- Next.js RSC payload format discussion: https://github.com/vercel/next.js/discussions/42170
- Bounded concurrency patterns in Rust: https://medium.com/@jaderd/you-should-never-do-bounded-concurrency-like-this-in-rust-851971728cfb
- Incremental Web Scraping: https://stabler.tech/blog/how-to-perform-incremental-web-scraping
- Scrapy DeltaFetch for Incremental Crawls: https://www.zyte.com/blog/scrapy-tips-from-the-pros-july-2016/
- Idempotent Data Pipelines: https://airbyte.com/data-engineering-resources/idempotency-in-data-pipelines
- Building Idempotent Data Pipelines: https://medium.com/towards-data-engineering/building-idempotent-data-pipelines-a-practical-guide-to-reliability-at-scale-2afc1dcb7251
- Rate Limiting in Web Scraping: https://www.scrapehero.com/rate-limiting-in-web-scraping/
- Scrapy AutoThrottle: https://dev.to/ikram_khan/scrapy-autothrottle-rate-limiting-stop-getting-blocked-4kje
- Large-scale web scraping guide: https://crawlbase.com/blog/large-scale-web-scraping/
- Web scraping best practices 2025: https://www.scraperapi.com/web-scraping/best-practices/

### Tertiary (LOW confidence)
- governor crate: https://crates.io/crates/governor (0.10.2, NOT recommended - overlaps with existing retry logic)
- scraper crate: https://crates.io/crates/scraper (0.25.0, NOT recommended - RSC payloads are not DOM-queryable)
- futures-rs on crates.io: https://crates.io/crates/futures (0.3.31 verified 2026-02-07)

---
*Research completed: 2026-02-07*
*Ready for roadmap: yes*
