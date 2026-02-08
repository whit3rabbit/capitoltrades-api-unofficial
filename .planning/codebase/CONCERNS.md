# Codebase Concerns

**Analysis Date:** 2026-02-07

## Tech Debt

**HTML Scraping Fragility:**
- Issue: CLI relies entirely on unofficial HTML scraping from capitoltrades.com with no official API. All parsing uses hardcoded regex patterns and manual JSON payload extraction.
- Files: `capitoltraders_lib/src/scrape.rs`, `capitoltraders_cli/src/commands/trades.rs`, `capitoltraders_cli/src/commands/politicians.rs`, `capitoltraders_cli/src/commands/issuers.rs`
- Impact: Any website redesign, HTML structure change, or JavaScript payload format modification breaks all CLI functionality. No validation of upstream data structure stability.
- Fix approach: Monitor upstream HTML changes, add integration tests that run against live site periodically, consider contributing to upstream to expose official API endpoints, or document breaking changes and version compatibility.

**String Allocation in Hot Paths:**
- Issue: Heavy use of `to_string()` and `String::from()` in parsing loops (14+ instances in `scrape.rs` alone), especially in `parse_politician_cards()` regex loop.
- Files: `capitoltraders_lib/src/scrape.rs` lines 647, 652-654, 671, 716
- Impact: Memory fragmentation and unnecessary allocations during large dataset ingestion (sync can process 10,000+ trades).
- Fix approach: Use `Cow<str>` or string refs where possible, pre-allocate string capacity for known sizes, benchmark memory usage during full sync.

**Database Transaction Scope:**
- Issue: `upsert_scraped_trades()` and `upsert_trades()` in `db.rs` hold open transaction for entire input slice, with nested loop deleting/inserting join table records per trade.
- Files: `capitoltraders_lib/src/db.rs` lines 272-441
- Impact: Large syncs (1000+ trades) could lock database longer than necessary, or cause transaction rollback on single error partway through.
- Fix approach: Batch delete operations before loop, consider smaller transaction chunks for large inputs, add explicit transaction timeout handling.

**Regex Compilation in Parser:**
- Issue: Two regex patterns compiled on every call to `parse_politician_cards()` in `scrape.rs`.
- Files: `capitoltraders_lib/src/scrape.rs` lines 638-643
- Impact: Wasted CPU cycles during each politician page fetch. Regex compilation is not free.
- Fix approach: Compile regexes once at module level or lazy_static, add caching if function is called repeatedly.

## Known Bugs

**None explicitly documented.** All clippy warnings have been resolved (as of 2026-02-06).

## Security Considerations

**CSV Formula Injection Mitigation:**
- Risk: CSV output can be opened in Excel/Sheets and execute formulas if a trade value, issuer name, or politician name starts with `=`, `+`, `-`, or `@`.
- Files: `capitoltraders_cli/src/output.rs` (sanitize_csv_field function)
- Current mitigation: `sanitize_csv_field()` prefixes suspicious fields with tab character, breaks formula interpretation.
- Status: Already implemented and tested (`capitoltraders_cli/src/output_tests.rs`).

**Pagination Input Validation:**
- Risk: Page and page_size parameters accepted from CLI without validation before passing to sync loop.
- Files: `capitoltraders_cli/src/commands/sync.rs` line 52
- Current mitigation: `validate_page_size()` is called at start of sync command.
- Status: Already implemented (as of 2026-02-06 security hardening).

**Mutex Poison Handling:**
- Risk: Rate limiter in `CachedClient` uses `Mutex.lock().unwrap()` which panics on poison.
- Files: `capitoltraders_lib/src/client.rs` lines 75, 91
- Current mitigation: Changed to `unwrap_or_else(|e| e.into_inner())` to recover from poison.
- Status: Already fixed (as of 2026-02-06).

**Base URL Parameter Injection:**
- Risk: CLI accepts `--base-url` or env var `CAPITOLTRADES_BASE_URL` without validation.
- Files: `capitoltraders_cli/src/main.rs` line 66-72, `capitoltraders_cli/src/commands/sync.rs` line 85-91
- Current mitigation: Passed directly to reqwest which validates URL scheme and host.
- Recommendation: Consider additional validation (no localhost, enforced HTTPS).

## Performance Bottlenecks

**Trade Detail Fetching (--with-trade-details):**
- Problem: Sync with `--with-trade-details` makes one HTTP request per trade to fetch filing URLs. A 100-trade page = 100 sequential requests.
- Files: `capitoltraders_cli/src/commands/sync.rs` lines 156-170
- Cause: No concurrent fetching; sequential with configurable `--details-delay-ms` (default 250ms).
- Improvement path: Add parallel batch requests (tokio::spawn), respect rate limits with semaphore, cache filing_url lookups.

**Full Sync Time:**
- Problem: Full politician and issuer catalog ingestion is noted as "slow" in CLI help.
- Files: `capitoltraders_cli/src/commands/sync.rs` line 31-32
- Cause: Likely many pages to fetch sequentially, no parallelization of page fetches.
- Improvement path: Parallel page fetching with backoff, worker pool for politician/issuer detail enrichment.

**In-Memory Cache for Full Results:**
- Problem: `MemoryCache` stores entire JSON response as String, no compression, no size limits.
- Files: `capitoltraders_lib/src/cache.rs`, `capitoltraders_lib/src/client.rs` lines 141-142
- Cause: After 5000+ trades cached, memory footprint could grow significantly.
- Improvement path: Add max cache size policy, compress entries, consider disk-backed cache for large datasets.

## Fragile Areas

**HTML Payload Parsing:**
- Files: `capitoltraders_lib/src/scrape.rs` lines 429-725
- Why fragile: Deeply nested manual JSON extraction from escaped strings in HTML. Multiple `extract_*` functions use state machines and substring matching with hardcoded offsets.
- Safe modification: Add comprehensive integration tests against archived HTML snapshots, document payload format assumptions, add logging of payload structure on parse failures.
- Test coverage: No integration tests for scraping (only unit tests for URL encoding upstream). Live site tests would catch breakage.

**Politician Card Regex:**
- Files: `capitoltraders_lib/src/scrape.rs` lines 638-700
- Why fragile: Regex pattern is 400+ characters, depends on exact HTML structure (class names, field order). Pattern fails silently if structure changes.
- Safe modification: Extract field mappings to constants, add fallback parsers, log parsed vs expected field counts.
- Test coverage: No test fixtures for politician cards; would need snapshot of live HTML.

**Date Parsing:**
- Files: `capitoltraders_lib/src/db.rs` line 71, `capitoltraders_cli/src/commands/sync.rs` lines 139-140, 149, 175-179
- Why fragile: Hardcoded `split('T')` and `%Y-%m-%d` format assumption. If upstream returns different datetime format, sync fails silently or panics.
- Safe modification: Use chrono format-safe parsing with fallback formats, validate date ranges.
- Test coverage: Validation tests exist for user input dates, but no tests for scraped dates.

## Scaling Limits

**SQLite Database:**
- Current capacity: Tested and working with thousands of trades (see schema DDL).
- Limit: SQLite is single-writer. Concurrent writes from multiple processes will block.
- Scaling path: Migrate to PostgreSQL if concurrent ingestion needed, add WAL pragma (already done), consider read-only replicas.

**HTTP Concurrency:**
- Current capacity: Sync fetches one page at a time sequentially, respects 5-10 sec rate limit between requests.
- Limit: Full sync of thousands of politicians/issuers takes hours; trade detail fetch is O(n) sequential requests.
- Scaling path: Implement page-level parallelization with concurrent.Semaphore-like pattern, respect backoff-retry semantics per upstream.

**Memory Usage During Ingestion:**
- Current: Entire trade page buffered in memory, then upserted in single transaction.
- Limit: Large page sizes (100 items) × many pages × string allocations = potential OOM for very large syncs.
- Scaling path: Stream trades page-by-page without full buffering, batch inserts in chunks.

## Dependencies at Risk

**Vendored Upstream Crate:**
- Risk: `capitoltrades_api` is a vendored fork of `TommasoAmici/capitoltrades` (Telegram bot). Upstream may diverge or be abandoned.
- Impact: Bug fixes or features in upstream won't auto-apply; modifications in our fork may conflict with upstream changes.
- Migration plan: Monitor upstream repo, consider creating feature branches for divergences, document all local changes in CLAUDE.md (already done).

**reqwest HTTP Client:**
- Risk: Using `rustls-tls` feature (not native-tls). If rustls has a critical CVE, must update Cargo.toml.
- Status: Already using modern version (0.12), configured correctly.

## Missing Critical Features

**No Official API Fallback:**
- Problem: Entirely dependent on unofficial HTML scraping. If CapitolTrades changes site or blocks scrapers, CLI breaks completely.
- Blocks: Long-term reliability and maintenance.
- Recommendation: Contact upstream maintainers for API access, or add fallback data source.

**No Incremental Detail Fetching:**
- Problem: Trade details (filing_url, filing_id) can only be fetched via `--with-trade-details` which is slow. No way to enrich existing DB with details.
- Blocks: Background enrichment job or deferred detail loading.
- Recommendation: Add a separate `enrich-trades` subcommand that only fetches missing filing URLs for existing trades.

## Test Coverage Gaps

**Live Scraping Integration Tests:**
- What's not tested: Actual HTML parsing against live capitoltrades.com. All tests use unit/snapshot tests.
- Files: `capitoltraders_lib/src/scrape.rs`
- Risk: Parser breaks silently if HTML changes; no alerting mechanism.
- Priority: High. Add daily CI test that scrapes one politician page and validates schema, or use vcr-like fixture recording.

**Concurrent Cache Access:**
- What's not tested: Multi-threaded access to `MemoryCache` under high concurrency, eviction behavior under memory pressure.
- Files: `capitoltraders_lib/src/cache.rs`
- Risk: Race condition or cache corruption if many threads insert simultaneously.
- Priority: Medium. Add concurrent insert/get benchmarks and stress tests.

**Database Constraint Violations:**
- What's not tested: Foreign key cascade behavior, duplicate key handling in upsert loops, transaction rollback recovery.
- Files: `capitoltraders_lib/src/db.rs`
- Risk: Orphaned records, silent upsert failures on constraint violation.
- Priority: Medium. Add tests that deliberately trigger constraint violations and validate error handling.

**XML Output Edge Cases:**
- What's not tested: Very large arrays (1000+ items), deeply nested performance structures, special characters in all possible fields.
- Files: `capitoltraders_cli/src/xml_output.rs`
- Risk: Memory spike, malformed XML, or character encoding issues with large datasets.
- Priority: Low. Covered by schema validation tests, but no large-scale stress tests.

---

*Concerns audit: 2026-02-07*
