# Domain Pitfalls

**Domain:** Web scraper detail-page enrichment (Next.js RSC site, SQLite storage)
**Project:** Capitol Traders -- extending listing-page scraper to detail-page enrichment
**Researched:** 2026-02-07

## Critical Pitfalls

Mistakes that cause rewrites, data corruption, or service-level breakage.

### Pitfall 1: COALESCE Direction in Upserts Silently Erases Enriched Data

**What goes wrong:** The existing `upsert_scraped_trades` in `db.rs` unconditionally overwrites `filing_id`, `filing_url`, `size`, `size_range_high`, `size_range_low`, and `price` on conflict. When a subsequent incremental sync re-ingests the same trade from a listing page (which has `filing_id=0`, `filing_url=""`, `size=NULL`), it overwrites the enriched detail-page values with placeholder defaults. The enrichment work is silently undone.

**Why it happens:** The current upsert on lines 355-375 of `db.rs` uses `excluded.filing_id`, `excluded.filing_url`, etc. without COALESCE guards. Listing-page data always has these fields set to defaults (0, empty string, NULL), so every re-sync of existing trades clobbers previously enriched data. The code assumes data only flows in one direction (insert then never touch again), but incremental sync re-processes overlapping date ranges.

**Consequences:** After a full detail enrichment pass over 35K trades, the very next incremental sync erases filing URLs, filing IDs, and sizing data for any trades whose publication date falls in the overlap window. Users see enriched data disappear between runs with no error or warning.

**Warning signs:**
- After running `sync` followed by `sync --full`, enriched fields revert to defaults
- `SELECT COUNT(*) FROM trades WHERE filing_url = ''` increases after incremental runs
- Detail data for recently-published trades keeps disappearing

**Prevention:**
- Change upsert for enrichment-target columns to use COALESCE in the correct direction: `filing_id = CASE WHEN excluded.filing_id > 0 THEN excluded.filing_id ELSE trades.filing_id END`
- For nullable columns: `size = COALESCE(excluded.size, trades.size)` -- this preserves the existing non-NULL value when the incoming value is NULL
- For non-nullable columns with sentinel defaults (filing_id=0, filing_url=""): use CASE expressions rather than COALESCE, since the sentinel is not NULL
- Add a test that upserts a trade with detail data, then upserts the same trade with listing-page defaults, and asserts the detail data survives

**Detection:**
- Add a `detail_enriched_at` timestamp column to trades; if it gets overwritten by a listing-page upsert, the timestamp should survive
- Log a warning when an upsert would downgrade a non-default value to a default

**Phase:** Must be addressed in Phase 1 before any detail fetching begins. The upsert SQL is the foundation; getting it wrong means every enrichment run is wasted work.

**Confidence:** HIGH -- verified by reading `db.rs` lines 327-376 directly. The current code unconditionally sets all fields from `excluded.*`.

---

### Pitfall 2: Sequential Detail Fetching Makes Full Enrichment Take 24+ Hours

**What goes wrong:** The current `--with-trade-details` implementation in `sync.rs` (lines 156-170) fetches one trade detail page at a time with a 250ms delay between requests. For 35,266 trades, this is: `35266 * (request_time + 250ms)`. Even with a generous 500ms average request time, that is `35266 * 0.75s = ~7.3 hours`. With retry delays and the occasional 429, a full enrichment run easily exceeds 24 hours.

**Why it happens:** The original design handled a few detail fetches per sync page (12 trades max). Scaling from 12 to 35K without changing the concurrency model creates a linear time explosion. The 250ms delay was reasonable for 12 items but is the dominant cost multiplier at scale.

**Consequences:**
- Full enrichment runs time out in CI (GitHub Actions has a 6-hour limit)
- Long-running processes are fragile: network interruptions, OOM, or laptop sleep kill the process, losing all progress
- Users abandon the feature because it takes too long

**Warning signs:**
- `sync --with-trade-details --full` logs show it is still on page 50/2900 after an hour
- CI runs start timing out
- Memory usage grows linearly because all trades are held in memory during the run

**Prevention:**
- Implement bounded concurrency: use a `tokio::sync::Semaphore` with 3-5 permits to fetch detail pages in parallel while respecting rate limits
- Separate enrichment from sync: add an `enrich` subcommand (or `sync --enrich`) that only targets rows with missing detail data, not the full trade pipeline
- Process in batches: fetch 50 detail pages, upsert to SQLite, commit, then fetch the next 50. This caps memory usage and provides natural checkpoints
- Adaptive delay: start at 250ms, back off to 2s on 429, recover to 250ms after 10 consecutive successes

**Detection:**
- Measure and log elapsed time per batch of 100 detail fetches
- Emit a progress estimate: "Estimated time remaining: X hours"

**Phase:** Phase 2 (after upsert correctness is established). Concurrency is an optimization; correctness of the merge logic comes first.

**Confidence:** HIGH -- the math is straightforward from the code. The existing `details_delay_ms` default of 250 and the sequential loop are visible in `sync.rs`.

---

### Pitfall 3: RSC Payload Format Changes Break All Scrapers Silently

**What goes wrong:** The `extract_rsc_payload` function in `scrape.rs` depends on the exact string `self.__next_f.push([1,"` appearing in the HTML. Next.js has changed this format across major versions (introduced in 13.4 with the App Router, evolved through 14, 15, and 16). A Next.js upgrade on capitoltrades.com changes the payload delivery mechanism, and every scraper method returns `ScrapeError::MissingPayload` or parses garbage data.

**Why it happens:** The RSC "Flight" protocol is an internal React/Next.js implementation detail with no stability guarantees. The payload format is not a public API. The `self.__next_f.push` mechanism could be replaced by streaming, a different script injection pattern, or a binary format in any Next.js release. Additionally, the December 2025 RSC RCE vulnerability (CVE-2025-55182) forced Next.js to patch payload handling, which could change encoding or validation.

**Consequences:**
- All scraper methods (`trades_page`, `politicians_page`, `issuers_page`, and all detail methods) fail simultaneously
- No data can be ingested until the parser is updated
- Detail-page enrichment is especially vulnerable because detail pages may use a different rendering path than listing pages (e.g., different React component boundaries, different payload chunking)

**Warning signs:**
- `ScrapeError::MissingPayload` errors appear in logs for URLs that previously worked
- Parse errors increase gradually (not all-or-nothing) if the site deploys progressively
- The `self.__next_f.push` needle returns no matches but the page loads fine in a browser
- HTTP 200 responses but with empty or differently-structured payload data

**Prevention:**
- Add a canary test that runs daily in CI against a known-good URL and validates the RSC payload structure. If the canary fails, block the enrichment pipeline
- Make the payload extraction needle configurable (env var or const) so it can be updated without a code change
- Implement a secondary extraction method: check for `__NEXT_DATA__` script tags as a fallback (older Next.js pattern)
- Store raw HTML of failed pages temporarily for debugging, rather than discarding them
- Log the first 200 characters of the payload on parse failures to aid diagnosis

**Detection:**
- Monitor the ratio of successful parses to attempts. A sudden drop from 100% to 0% indicates a format change
- Check the `X-Next-Build-Id` or `X-Powered-By` response headers for Next.js version changes

**Phase:** Should be addressed before starting enrichment. A canary test and fallback extraction belong in Phase 1.

**Confidence:** HIGH -- the `self.__next_f.push` dependency is explicit in `scrape.rs` line 430. Next.js version instability is well-documented; the December 2025 CVE forced patches across all supported versions.

---

### Pitfall 4: No Checkpoint/Resume Means Failures Waste Hours of Work

**What goes wrong:** The sync loop in `sync.rs` processes all pages sequentially and only updates `last_trade_pub_date` in `ingest_meta` after the entire run completes (line 103). If the process crashes at trade 30,000 of 35,266, no progress is saved. The next run starts from scratch (or from the last successful `last_trade_pub_date`, which may be weeks old).

**Why it happens:** The listing-page sync is fast enough (hundreds of pages, not tens of thousands of individual requests) that losing progress is annoying but not catastrophic. Detail-page enrichment at 35K items makes the cost of lost progress orders of magnitude higher.

**Consequences:**
- A crash at 95% completion wastes 20+ hours of work
- Network interruptions during overnight runs are common and unrecoverable
- Users learn to distrust the tool and manually babysit long runs

**Warning signs:**
- After a crash, `SELECT value FROM ingest_meta WHERE key = 'last_trade_pub_date'` still shows the pre-crash value
- Log output shows "Starting full sync" after a crash that happened mid-run
- Database has some enriched trades and some un-enriched trades with no way to tell which were processed

**Prevention:**
- Track enrichment progress separately from sync progress. Add `ingest_meta` keys like `last_enriched_trade_id` or `enrichment_batch_cursor`
- Commit enrichment results in batches of 50-100 trades. After each batch commit, update the cursor. On restart, resume from the cursor
- Use the SQLite database itself as the checkpoint: query `SELECT tx_id FROM trades WHERE filing_url = '' ORDER BY tx_id` to find un-enriched trades, then process them. No separate cursor needed
- Alternative: add a `detail_fetched` boolean column to trades; enrichment processes only rows where `detail_fetched = 0`

**Detection:**
- Log batch progress: "Enriched 500/35266 trades (1.4%)"
- On startup, log "Resuming enrichment from trade ID X (Y remaining)"

**Phase:** Phase 2, designed alongside the concurrency model. The checkpoint strategy determines the batch size and commit frequency.

**Confidence:** HIGH -- the single-commit-at-end pattern is visible in `sync.rs` line 103 and the `upsert_scraped_trades` transaction scope in `db.rs` lines 272-441.

---

### Pitfall 5: trades_detail Already Fetches for Every Trade in CLI Mode (Redundant Work)

**What goes wrong:** The `trades` CLI command in `trades.rs` lines 397-404 calls `scraper.trade_detail(trade.tx_id)` for every single trade returned from the listing page, unconditionally. This is not behind `--with-trade-details` -- it happens on every `capitoltraders trades` invocation. This means the CLI already makes 12 sequential detail requests per page view. When detail enrichment is added to sync, these two code paths will do redundant work and may have inconsistent extraction logic.

**Why it happens:** The trades command needs filing URLs for its output format, and listing pages do not include them. So every trade display requires a detail fetch. This was acceptable for 12 trades but becomes a design problem when enrichment also fetches details.

**Consequences:**
- If the detail extraction logic is updated in one place but not the other, the CLI and sync produce different data
- The CLI is slow: viewing one page of trades requires 13 HTTP requests (1 listing + 12 details) with 250ms delays = ~6 seconds minimum
- If enrichment populates the SQLite DB with filing URLs, the CLI should be able to read from the DB instead of re-scraping, but currently has no DB-read path

**Warning signs:**
- Changing `extract_trade_detail` in `scrape.rs` fixes sync output but not CLI output (or vice versa)
- Users report `capitoltraders trades` is slow even though sync has already enriched all data

**Prevention:**
- Share extraction logic: detail parsing should live in exactly one place (`scrape.rs` already has this, but the conversion functions in `trades.rs` and `sync.rs` duplicate the field mapping)
- After enrichment, add a `--from-db` flag or auto-detect: if a SQLite DB exists with enriched data, read from it instead of scraping
- At minimum, document that `trades` command always hits detail pages and that enrichment in sync makes this redundant

**Detection:**
- Count HTTP requests per CLI invocation; if it exceeds `page_size + 1`, detail fetching is happening

**Phase:** Phase 3 (CLI integration). The first priority is getting enrichment working in sync; CLI optimization comes after.

**Confidence:** HIGH -- directly observed in `trades.rs` lines 397-404. The unconditional `trade_detail` call is not behind any flag.

## Moderate Pitfalls

### Pitfall 6: Detail Pages Return Different Data Structures Than Listing Pages

**What goes wrong:** The listing-page RSC payload contains trade data in a JSON array with key `_txId`. The detail-page RSC payload wraps the same trade data in a different structure (e.g., under `"tradeId"` key, as seen in `extract_trade_detail` on line 470 of `scrape.rs`). The field names, nesting, and available data differ between listing and detail pages. Naively assuming they share a format leads to parse failures or silent data loss.

**Prevention:**
- Document the expected payload structure for each detail page type (trade, politician, issuer) separately from listing pages
- Write fixture tests with captured HTML from each page type
- The existing `ScrapedTradeDetail` struct is minimal (only `filing_url` and `filing_id`). Extending it to include committees, labels, asset type, and sizing requires understanding a completely different payload structure than the listing page

**Phase:** Phase 1 -- the very first task should be capturing and documenting the actual detail-page payloads for each entity type before writing any extraction code.

**Confidence:** MEDIUM -- the `extract_trade_detail` function uses `"tradeId"` while listing pages use `"_txId"`, confirming structural differences. The full extent of differences for extended fields (committees, labels, etc.) is not yet verified.

---

### Pitfall 7: SQLite Transaction Size During Batch Enrichment

**What goes wrong:** The current `upsert_scraped_trades` wraps all trades in a single transaction. If enrichment processes 500 trades and commits them all at once, a single parse error or constraint violation on trade #499 rolls back all 498 successful upserts. The all-or-nothing transaction scope is appropriate for page-at-a-time ingestion but destructive for batch enrichment.

**Prevention:**
- Use smaller transaction batches (25-50 trades per transaction) for enrichment operations
- Separate the enrichment upsert from the initial ingestion upsert -- create a dedicated `update_trade_details` function that only touches detail columns, not the full trade row
- Log and skip individual failures rather than rolling back the batch: catch the error, record the failed `tx_id`, continue with the next trade

**Phase:** Phase 2 -- when implementing the enrichment pipeline and batch processing.

**Confidence:** HIGH -- the transaction scope is visible in `db.rs` and the rollback behavior is standard SQLite.

---

### Pitfall 8: Rate Limiting Calibration for 40x Request Volume Increase

**What goes wrong:** Current scraping makes ~2,900 listing page requests for a full sync (35K trades / 12 per page). Detail enrichment adds 35K + 500 + 5K = ~40,500 additional requests. The total request volume increases by 14x. The existing retry/backoff config (2s base, 30s max, 3 retries) was tuned for hundreds of requests, not tens of thousands. The site starts returning 429s, the backoff maxes out at 30s, and the scraper enters a retry storm that makes things worse.

**Prevention:**
- Add a global rate limiter (token bucket or leaky bucket) in addition to per-request backoff. Target 1-2 requests per second sustained
- Implement circuit breaker pattern: after N consecutive 429s, pause all requests for 5 minutes rather than retrying individually
- Make the sustained request rate configurable via env var (e.g., `CAPITOLTRADES_REQUESTS_PER_SECOND`)
- Consider time-of-day scheduling: run enrichment during off-peak hours (overnight US time)

**Phase:** Phase 2 -- alongside the concurrency implementation.

**Confidence:** MEDIUM -- the rate limit behavior of capitoltrades.com is not documented. The 14x multiplier is derived from the data; the actual blocking threshold is unknown.

---

### Pitfall 9: Politician Detail Page Returns Partial Data Compared to Listing Page

**What goes wrong:** The `politician_detail` method in `scrape.rs` (line 289) returns `Option<ScrapedPolitician>`, which has fields for party, state, gender, etc. but no fields for committees, trade counts, or volume statistics. The `ScrapedPoliticianCard` from listing pages has `trades`, `issuers`, `volume`, and `last_traded`. The detail page may have committee data (which is the enrichment target) but lacks the statistical data from the listing page. If the enrichment upsert naively overwrites the politician row from detail-page data, it loses the stats.

**Prevention:**
- The enrichment upsert for politicians should only update committee-related columns, not the full politician row
- Create separate `upsert_politician_committees` and `update_politician_detail_fields` functions rather than reusing the full `upsert_politicians` function
- Verify what data the detail page actually provides by capturing a real payload before writing the upsert

**Phase:** Phase 1 -- data model decisions about which columns each page type populates.

**Confidence:** MEDIUM -- the `ScrapedPolitician` struct is confirmed to lack stats fields. The actual committee data availability on detail pages needs verification against the live site.

---

### Pitfall 10: The `bail!` on Missing Filing URL Blocks Non-Detail Output

**What goes wrong:** In `trades.rs` line 449, `scraped_trade_to_trade` calls `bail!("missing filing URL for trade {}", trade.tx_id)` if the filing URL is empty. This means that if any single trade's detail page fails to return a filing URL, the entire `trades` command fails. This is overly strict: many trades legitimately have no filing URL (Senate filings often use UUID-style URLs that the parser cannot extract a numeric ID from, and `filing_id` may be `0` while `filing_url` is populated -- or both may be absent).

**Prevention:**
- Change the bail to a warning or use a default value. Filing URL should be optional in the output, not a hard requirement
- The `ScrapedTradeDetail` already handles this correctly by using `Option<String>` for `filing_url`, but the conversion function in `trades.rs` treats empty as fatal
- During enrichment, track which trades have incomplete detail data and report them rather than failing the entire operation

**Phase:** Phase 1 -- this is a pre-existing bug that will surface more frequently when enrichment is running at scale.

**Confidence:** HIGH -- directly visible in `trades.rs` line 449. The `bail!` is unconditional.

## Minor Pitfalls

### Pitfall 11: User-Agent Rotation Without Session Consistency

**What goes wrong:** `ScrapeClient::new()` calls `get_user_agent()` once at construction time and uses the same user-agent for all requests. This is fine for short-lived CLI commands but looks suspicious for a 24-hour enrichment run: the same user-agent making 40K requests. Conversely, rotating the user-agent per-request (if the implementation changes) can trigger bot detection because real users do not switch browsers mid-session.

**Prevention:**
- Keep the current per-session user-agent for consistency within a run
- Consider rotating the user-agent between enrichment batches (every ~500 requests) rather than per-request
- Add common browser headers (Accept-Language, Accept-Encoding) that match the chosen user-agent string

**Phase:** Phase 2 -- when tuning the request profile.

**Confidence:** LOW -- the actual bot detection behavior of capitoltrades.com is unknown. This is a general best practice.

---

### Pitfall 12: Memory Growth During Full Enrichment

**What goes wrong:** The sync loop in `sync.rs` accumulates `issuer_stats` and `politician_stats` HashMaps for the entire run (lines 130-131). For 35K trades, these maps grow to hold ~5K issuer entries and ~500 politician entries. This is manageable. However, if the enrichment process also holds all pending detail results in memory before committing, the memory footprint could spike.

**Prevention:**
- Commit enrichment results in batches rather than accumulating them
- The existing per-page commit pattern in sync is fine; the risk is in the detail enrichment loop if it buffers results
- Monitor memory usage during test runs with 1000+ detail fetches

**Phase:** Phase 2 -- design the enrichment loop to commit incrementally.

**Confidence:** MEDIUM -- the HashMaps are bounded by entity count (manageable), but buffering detail results is a design choice not yet made.

---

### Pitfall 13: Schema Migration for New Columns

**What goes wrong:** Adding tracking columns like `detail_fetched` or `detail_enriched_at` to the trades table requires a schema migration. The current `init()` method in `db.rs` uses `CREATE TABLE IF NOT EXISTS`, which will not add new columns to existing tables. An existing database from a previous version will silently lack the new columns, causing INSERT failures.

**Prevention:**
- Add `ALTER TABLE trades ADD COLUMN detail_fetched INTEGER DEFAULT 0` after the `CREATE TABLE IF NOT EXISTS` block, wrapped in a try/ignore for "duplicate column name" errors
- Or use a version number in `ingest_meta` and apply migrations sequentially
- Test with both fresh databases and databases created by the current version

**Phase:** Phase 1 -- schema changes must be planned before any code that depends on new columns.

**Confidence:** HIGH -- standard SQLite migration issue. The current `init()` does not handle column additions.

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Phase 1: Payload Discovery | Detail pages have different RSC structure than listing pages (#6) | Capture and archive real HTML fixtures before writing extraction code |
| Phase 1: Schema Changes | No migration path for existing databases (#13) | Add ALTER TABLE with duplicate-column guards |
| Phase 1: Upsert Correctness | COALESCE direction erases enriched data (#1) | Write the upsert SQL first, test the overwrite scenario before any enrichment code |
| Phase 1: Error Handling | bail! on missing filing URL kills entire output (#10) | Change to warning + default before scaling up detail fetches |
| Phase 2: Concurrency | Sequential fetching is too slow (#2) | Semaphore-bounded parallel fetches with adaptive delay |
| Phase 2: Checkpointing | No resume after crash (#4) | Use DB state as checkpoint; query for un-enriched rows on startup |
| Phase 2: Rate Limiting | 14x volume increase triggers blocking (#8) | Global rate limiter + circuit breaker pattern |
| Phase 2: Transaction Scope | Large batch rollback on single failure (#7) | Smaller transaction batches, skip individual failures |
| Phase 3: CLI Integration | Redundant detail fetching in trades command (#5) | Read enriched data from DB when available |
| Phase 3: Politician Enrichment | Detail page lacks stats, upsert clobbers them (#9) | Column-specific upsert for committee data only |

## Sources

- SQLite UPSERT documentation: https://sqlite.org/lang_upsert.html
- Next.js RSC payload format discussion: https://github.com/vercel/next.js/discussions/42170
- RSC Flight protocol payload decoding: https://edspencer.net/2024/7/1/decoding-react-server-component-payloads
- Scraping Next.js in 2025: https://www.trickster.dev/post/scraping-nextjs-web-sites-in-2025/
- Next.js RSC RCE CVE-2025-55182 (format changes): https://snyk.io/blog/security-advisory-critical-rce-vulnerabilities-react-server-components/
- Large-scale web scraping guide: https://crawlbase.com/blog/large-scale-web-scraping/
- Web scraping best practices 2025: https://www.scraperapi.com/web-scraping/best-practices/
- Codebase analysis: `capitoltraders_lib/src/scrape.rs`, `capitoltraders_lib/src/db.rs`, `capitoltraders_cli/src/commands/sync.rs`, `capitoltraders_cli/src/commands/trades.rs`

---

*Pitfalls audit: 2026-02-07*
