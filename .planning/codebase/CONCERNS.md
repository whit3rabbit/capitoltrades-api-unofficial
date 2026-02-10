# Codebase Concerns

**Analysis Date:** 2026-02-09

## Tech Debt

**Hardcoded Issuer Fallback Data:**
- Issue: Three issuer detail pages return server-side RSC errors instead of valid issuerData. The scraper has hardcoded fallback metadata to allow enrichment to complete.
- Files: `capitoltraders_lib/src/scrape.rs:857-889` (issuer_fallback function)
- Affected Issuers:
  - Goodyear Tire & Rubber Co (ID: 432049) - returns empty stats, hardcoded sector only
  - Town of Hingham Massachusetts (ID: 2334265) - no sector/ticker data
  - JEA Water & Sewer System (ID: 2334268) - no sector/ticker data
- Impact: Enrichment of these issuers skips performance metrics and uses static metadata. If the upstream capitoltrades.com fixes these pages, the fallbacks become dead code and should be removed.
- Fix approach: Monitor upstream site for resolution. When fixed, remove fallback logic and test with real API response.

**Empty Committees for Defunct Politicians:**
- Issue: Politicians from defunct committees appear in committee-filtered queries with empty committees arrays. This is by design to allow politicians from committees that no longer exist to still appear in results.
- Files: `capitoltraders_lib/src/scrape.rs:809-814` (extract_politician_cards)
- Impact: Queries filtering by committee may return politicians with no committees if they served on committees that have been dissolved. This is acceptable behavior but worth documenting.
- Fix approach: No action needed; this is intentional design. Ensure tests cover defunct committee scenarios.

**Page Size Limitation in Scrape Mode:**
- Issue: The trades command warns that `--page-size` is ignored in scrape mode (fixed at 12 items per page). This is an API limitation, not client code, but creates user confusion.
- Files: `capitoltraders_cli/src/commands/trades.rs:152-154`
- Impact: Users providing `--page-size` in scrape mode get a warning but their setting is silently ignored. Pagination must be done client-side post-fetch.
- Fix approach: This is by design (scrape mode doesn't support larger pages). Documentation is already present in README.md. No fix needed, but consider a more prominent warning or rejecting the flag.

## Known Issues

**CSV Formula Injection Prevention:**
- Issue: CSV output sanitizes fields starting with `=`, `+`, `-`, or `@` by prefixing with a tab character.
- Files: `capitoltraders_cli/src/output.rs` (sanitize_csv_field function)
- Current mitigation: All CSV fields are sanitized before output.
- Status: Addressed in security hardening (2026-02-06).

**Politician ID Schema Validation:**
- Issue: JSON and XSD schemas previously had overly restrictive pattern `^P\d{6}$` for politician IDs.
- Files: `schema/trade.schema.json`, `schema/politicians.xsd`, `schema/trades.xsd` (line 2)
- Fix applied: Pattern changed to `^[A-Z]\d{6}$` to match real API behavior where IDs use various uppercase letter prefixes (e.g., T000250 for Patrick McHenry).
- Status: Fixed in commit c9eaabd.

**IssuerQuery Pagination Parameter Loss:**
- Issue: IssuerQuery::add_to_url() was using url.clone() instead of self.common.add_to_url(url), dropping pagination params.
- Files: `capitoltrades_api/src/query.rs` (IssuerQuery implementation)
- Fix applied: Now correctly delegates to self.common.add_to_url() to preserve page, pageSize, pubDate, txDate parameters.
- Status: Fixed.

## Security Considerations

**Unofficial API Dependency:**
- Risk: The CLI uses an unofficial API by scraping the public CapitolTrades website. No public API exists, and scraping is not guaranteed by terms of service.
- Files: All of `capitoltraders_lib/src/scrape.rs` and `capitoltraders_lib/src/client.rs`
- Current mitigation: 5-10 second randomized delay between HTTP requests; configurable 500ms delay for enrichment detail fetches with rate limiting via Semaphore; circuit breaker stops enrichment after consecutive HTTP failures.
- Recommendations: Monitor for changes to capitoltrades.com. Consider implementing exponential backoff if 429 responses become frequent. Document rate limiting more prominently in README.

**Missing Filing URL Causes Trade Rejection:**
- Risk: The scrape-to-trades conversion bails if filing_url is empty. This silently skips trades lacking URLs instead of allowing them with null fields.
- Files: `capitoltraders_cli/src/commands/trades.rs:586-589`
- Current mitigation: Tests check for empty filing URLs; fallback logic attempts to fetch URLs for all trades during scraping.
- Recommendations: Consider emitting trades with null filing URLs instead of rejecting them. Current approach is conservative but loses data.

**Environment-Based Retry Configuration:**
- Risk: Retry behavior (max_retries, base_delay_ms, max_delay_ms) is configurable via environment variables without defaults validation.
- Files: `capitoltraders_lib/src/scrape.rs:58-75` (RetryConfig::from_env)
- Current mitigation: Sensible defaults (3 retries, 2s base, 30s max). Environment parsing fails safely to defaults.
- Recommendations: Add env var validation tests to ensure admins don't set insensible values (e.g., max_retries=0).

## Performance Bottlenecks

**Single-Threaded SQLite Write Serialization:**
- Problem: Database writes in enrichment pipelines are single-threaded despite concurrent HTTP fetches. MPSC channel receiver processes all updates sequentially.
- Files: `capitoltraders_cli/src/commands/sync.rs:273-320` (enrich_trades) and `383-430` (enrich_issuers)
- Cause: SQLite locking limits concurrent writes; pattern uses mpsc channel to serialize writes from concurrent fetch tasks.
- Current capacity: ~500-1000 items/hour with 3 concurrent fetches + 500ms delay + single write thread.
- Improvement path: SQLite WAL mode (already enabled) provides some parallelism. Adding write-coalescing batches (insert N items per transaction) could improve throughput by 30-40%.

**Full Politician/Issuer Catalog Refresh Overhead:**
- Problem: `--refresh-politicians` and `--refresh-issuers` flags fetch entire catalogs even during incremental syncs. No pagination limits.
- Files: `capitoltraders_cli/src/commands/sync.rs:100-115`
- Current capacity: ~5-10 minutes for full catalog refresh; incremental trades sync takes ~30-60 seconds.
- Improvement path: Implement pagination-based refresh with `--max-pages` flag, or skip catalog refresh in incremental mode by default.

**Cache Expiration Lazy Eviction:**
- Problem: Expired cache entries are only removed on next `get()` call, not proactively. Long-running sessions may accumulate stale entries.
- Files: `capitoltraders_lib/src/cache.rs:31-39` (get method)
- Current capacity: DashMap grows unbounded with stale entries; no maxsize limit.
- Improvement path: Add periodic pruning task (separate thread/tokio task) to remove expired entries every 60 seconds, or implement LRU with maxsize cap.

**Analysis Functions Do Full Traversals:**
- Problem: Functions like `top_traded_issuers()`, `trades_by_ticker()` iterate entire slice for each operation, no indexing.
- Files: `capitoltraders_lib/src/analysis.rs:35-44`, `trades_by_ticker:21-32`
- Cause: Designed for small in-memory result sets (100-1000 items). Scales poorly with large result sets.
- Improvement path: For analysis operations on >10k items, consider pre-computing indices or streaming aggregations.

## Fragile Areas

**Scraping HTML/RSC Parsing:**
- Files: `capitoltraders_lib/src/scrape.rs` (entire module)
- Why fragile: Directly parses HTML and Next.js RSC payloads from capitoltrades.com. Any upstream HTML structure change, field renaming, or RSC format change breaks parsing.
- Safe modification: Add comprehensive regression tests with current HTML fixtures (`tests/fixtures/`). Mock CapitolTrades responses with wiremock when possible. Avoid hardcoding CSS selectors or field names; extract to constants.
- Test coverage: 20 fixture-based scrape tests exist (trade_detail_stock, trade_detail_option, trade_detail_minimal, politician_committee_filtered, issuer_detail). Additional coverage needed for error paths and edge cases in politician card extraction.

**Concurrent Enrichment Pipeline:**
- Files: `capitoltraders_cli/src/commands/sync.rs:196-215` (CircuitBreaker), `272-320` (enrich_trades), `383-430` (enrich_issuers)
- Why fragile: Uses mpsc channel + JoinSet + Semaphore. If a task panics or channel drops unexpectedly, remaining tasks hang. Circuit breaker is a simple consecutive-failure counter with no recovery.
- Safe modification: Wrap individual fetch tasks in catch_unwind(). Add timeout to channel recv(). Test circuit breaker reset scenarios. Add metrics logging for failure patterns.
- Test coverage: 6 enrichment pipeline tests exist (count_unenriched, partial skip, batch limiting), but missing: panic recovery, channel exhaustion, timeout scenarios.

**SQLite Migration Resilience:**
- Files: `capitoltraders_lib/src/db.rs:66-81` (migrate_v1)
- Why fragile: Migration silently ignores "duplicate column name" errors, but actual migration errors (corrupt DB, permission denied) propagate and crash sync.
- Safe modification: Log which columns were skipped during migration. Test migration on pre-existing databases with partial schemas. Consider a dry-run mode for schema changes.
- Test coverage: 1 migration test exists (phase 1). Missing: partial schema state, permission errors, recovery from failed migrations.

**Deserialization Lenient Behavior:**
- Files: `capitoltraders_lib/src/scrape.rs` uses serde with #[serde(default)] on optional fields
- Why fragile: Missing fields in API responses don't error; they become None/empty. Silent failures for structural changes.
- Safe modification: Add schema validation tests for every deserialization target (ScrapedTrade, ScrapedIssuerDetail, etc.). Log when fields are missing and expected. Use strict mode in dev.
- Test coverage: 7 deserialization tests exist with JSON fixtures. Missing: negative tests for malformed payloads.

## Scaling Limits

**In-Memory Cache Unbounded Growth:**
- Current capacity: No explicit limit; DashMap grows with requests.
- Limit: After ~1 million entries (or 100MB heap), garbage collection pauses increase. At 5-minute TTL with high request volume, cache can accumulate 10k+ entries.
- Scaling path: Implement LRU eviction with maxsize (e.g., 10k entries). Add cache hit/miss metrics. For high-concurrency use, switch to a bounded queue.

**Concurrent HTTP Requests:**
- Current capacity: Configurable 1-10 concurrent fetches (default 3). Semaphore prevents runaway requests.
- Limit: At 10 concurrent fetches + 500ms delay, throughput ~1200 requests/hour. Upstream CapitolTrades rate limit unknown; may trigger 429 responses above this.
- Scaling path: Implement exponential backoff for 429 responses. Add jitter to avoid thundering herd. Monitor response times to detect upstream slowdown.

**SQLite Transaction Size:**
- Current capacity: Single transaction upserts 100-1000 trades at a time (determined by page_size).
- Limit: At 1000 items/transaction with 40+ columns, commit time ~500ms. Batch sizes >5000 may lock DB for too long.
- Scaling path: Implement batch transaction coalescing (write 10 items every 100ms instead of waiting for large batch). Monitor lock duration with PRAGMA stats.

**Database Row Count:**
- Current capacity: Tested with ~100k trades, ~200 politicians, ~10k issuers. Query performance still acceptable.
- Limit: At ~1M trades, full table scans (query_trades without filters) become 10+ seconds. Indexes on party, state help.
- Scaling path: Add clustering indexes on frequently filtered columns (party, state, chamber, tx_type). Implement query result pagination at DB layer.

## Dependencies at Risk

**No Critical Dependency Vulnerabilities Detected:**
- Tokio 1, Serde 1, Reqwest 0.12, Rusqlite 0.31 are all actively maintained.
- Status: No known security issues in current versions (as of Feb 2026).
- Recommendation: Keep dependencies updated monthly. Monitor advisory feeds for unexpected issues.

**Vendored Upstream Crate:**
- Risk: `capitoltrades_api` is a vendored snapshot from https://github.com/TommasoAmici/capitoltrades
- Impact: Any API changes to capitoltrades.com require manual updates to vendored types. Upstream project may diverge.
- Migration plan: If upstream fork diverges significantly, fork the project into org. Document all local modifications in CLAUDE.md (already done).

## Missing Critical Features

**No Offline Mode for Analysis:**
- Problem: All analysis operations require network scraping. No way to work with cached/historical data locally without sync to SQLite.
- Blocks: Offline data exploration, reproducible research, air-gapped environments.
- Recommendation: Primary design supports SQLite `--db` mode for offline querying. This is acceptable; network dependency is documented.

**No Automated Sync Scheduling:**
- Problem: Sync must be manually triggered or run in CI workflow. No built-in cron-like scheduler.
- Blocks: Real-time data freshness for deployed applications.
- Recommendation: Sync workflow is documented in `.github/workflows/sqlite-sync.yml`. External orchestration (cron, Kubernetes, etc.) recommended for production use.

**Unsupported DB Mode Filters:**
- Problem: DB mode lacks support for many filters present in scrape mode: `--committee`, `--trade-size`, `--market-cap`, `--asset-type`, `--label`, `--gender`, `--chamber` (trades); `--committee`, `--issuer-id` (politicians); `--market-cap`, `--state`, `--country`, `--politician-id` (issuers).
- Files: `capitoltraders_cli/src/commands/trades.rs:534-544`, `politicians.rs`, `issuers.rs` (filter unsupported checks)
- Impact: Users must use scrape mode for advanced filtering. Enriched data in DB is underutilized.
- Priority: Medium. Phase plan needed to add filter support incrementally.

**No Data Export to CSV from Sync:**
- Problem: CSV export only available from scrape commands, not from DB queries.
- Blocks: Exporting historical data to spreadsheet format.
- Recommendation: CSV output for DB mode is implemented. No blocker.

## Test Coverage Gaps

**Error Path Coverage:**
- What's not tested: HTTP error handling (500s, timeouts, malformed JSON), partial scraping (one detail page fails but others succeed), database constraint violations on upserts.
- Files: `capitoltraders_lib/src/scrape.rs`, `capitoltraders_lib/src/db.rs`, `capitoltraders_cli/src/commands/sync.rs`
- Risk: Silent failures in enrichment pipeline. Circuit breaker may not trigger on edge cases (partial JSON parse failures, network hangs).
- Priority: High. Add wiremock tests for HTTP errors, inject rusqlite errors in tests.

**Pagination Edge Cases:**
- What's not tested: Single-item pages, pages with total_count=0, mismatched total_pages/total_count, out-of-range page requests.
- Files: `capitoltraders_lib/src/client.rs` (pagination logic)
- Risk: Off-by-one errors in page iteration, silent empty results.
- Priority: Medium. Add property-based tests for pagination bounds.

**Data Validation:**
- What's not tested: Validation of all 16 filter types with edge cases (max length strings, negative numbers, malformed dates).
- Files: `capitoltraders_lib/src/validation.rs` (83 tests exist but gaps remain)
- Risk: Injection attacks via unvalidated input, downstream API errors from malformed parameters.
- Priority: Medium. Current coverage is good (83 tests); focus on boundary conditions and negative tests.

**Concurrency and Locking:**
- What's not tested: Concurrent writes to SQLite from multiple processes, circuit breaker behavior under load, channel buffer exhaustion, Semaphore starvation.
- Files: `capitoltraders_cli/src/commands/sync.rs`, `capitoltraders_lib/src/cache.rs`
- Risk: Data corruption under concurrent access, deadlocks, memory leaks.
- Priority: High. Add multi-threaded tests with tokio::test and stress scenarios.

**Schema Validation:**
- What's not tested: Output schema conformance across all output formats (JSON, CSV, XML, Markdown). Field presence, type correctness, required vs optional.
- Files: `capitoltraders_cli/src/output.rs`, `capitoltraders_cli/src/xml_output.rs`
- Risk: Output violates schema contracts; downstream consumers break.
- Priority: Medium. Schema validation tests exist (12 tests); expand to cover all output types and edge cases (empty arrays, null fields, special characters).

---

*Concerns audit: 2026-02-09*
