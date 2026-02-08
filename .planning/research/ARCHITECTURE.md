# Architecture Patterns: Detail-Page Enrichment Pipeline

**Domain:** Web scraper enrichment for congressional trading data
**Researched:** 2026-02-07
**Confidence:** HIGH (based on direct codebase analysis, not external sources)

## Current Architecture

The existing system follows a three-layer workspace pattern:

```
capitoltrades_api/     (vendored types/enums -- read-only, no modifications for enrichment)
  |
capitoltraders_lib/    (ScrapeClient, Db, validation)
  |
capitoltraders_cli/    (sync command orchestration, output formatting)
```

The sync command today runs a single pipeline:

```
trades_page(1) --> trades_page(2) --> ... --> trades_page(N)
    |                |                           |
    v                v                           v
  [optional: trade_detail(tx_id) per trade for filing URLs]
    |                |                           |
    v                v                           v
  upsert_scraped_trades(batch)
    |
    v
  compute & upsert politician_stats / issuer_stats from trade aggregation
```

### Data Gaps by Entity Type

| Entity | Listing Page Fields | Detail Page Adds | Join Tables Affected |
|--------|-------------------|------------------|---------------------|
| **Trade** | tx_id, politician, issuer, dates, value, tx_type | filing_url, filing_id | trade_committees (empty), trade_labels (empty) |
| **Politician** | name, party, state, stats (trades/issuers/volume) | committees, dob, gender, chamber, nickname, social links, website, district | politician_committees (empty) |
| **Issuer** | issuer_id, name, ticker, sector, stats | state_id, c2iq, country, performance (mcap, trailing*, eod_prices) | issuer_performance (empty), issuer_eod_prices (empty) |

The word "empty" above is the core problem. The SQLite schema has tables for committees, labels, performance, and eod_prices, but the sync pipeline never populates them because it only walks listing pages, and those pages do not contain this data.

## Recommended Architecture

### Design Principle: Enrichment as a Separate Pass

Do not interleave detail fetching into the listing-page loop. Instead, structure enrichment as distinct post-listing passes. This matters for three reasons:

1. **Resumability.** If enrichment fails partway through (rate limiting, network errors), the listing data is already persisted. Re-running only needs to enrich un-enriched rows.
2. **Selectivity.** Not every row needs enrichment on every run. Incremental runs should only enrich new/changed entities, not re-fetch every detail page.
3. **Rate limiting.** Detail pages are 1:1 with entities. For trades, that is potentially thousands of requests. Separating the pass makes it easy to add delays, progress tracking, and resume logic without complicating the listing loop.

### Component Boundaries

```
+-----------------------+     +-----------------------+     +------------------+
|   ScrapeClient        |     |   Db                  |     |   sync command   |
|   (capitoltraders_lib |     |   (capitoltraders_lib |     |   (CLI crate)    |
|    /src/scrape.rs)    |     |    /src/db.rs)        |     |   orchestrator   |
+-----------------------+     +-----------------------+     +------------------+
| trades_page()         |     | upsert_scraped_trades |     | sync_trades()    |
| trade_detail()        |     | upsert_politician_*   |     | enrich_trades()  |
| politicians_page()    |     | upsert_issuer_*       |     | enrich_pols()    |
| politician_detail()   |     | needs_enrichment_*()  |     | enrich_issuers() |
| issuers_page()        |     | mark_enriched_*()     |     |                  |
| issuer_detail()       |     | get_unenriched_*()    |     |                  |
+-----------------------+     +-----------------------+     +------------------+
         |                              |                           |
         | HTTP (1 req per entity)      | SQLite read/write         | orchestration
         v                              v                           v
   capitoltrades.com              capitoltraders.db           CLI user / CI
```

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| ScrapeClient | HTTP fetching + RSC payload parsing. Already has all detail methods. No changes needed for enrichment. | capitoltrades.com |
| Db (enrichment queries) | New: query which rows need enrichment, update rows with detail data, track enrichment state. | SQLite file |
| sync command (enrichment passes) | New: orchestrate detail fetching for unenriched rows with progress, delays, and error handling. | ScrapeClient, Db |

### Data Flow: Full Enriched Sync

```
Phase 1: Listing Ingest (existing)
=========================================
  for page in 1..total_pages:
    trades = scraper.trades_page(page)
    db.upsert_scraped_trades(trades)
    accumulate stats

  db.upsert_politician_stats(computed)
  db.upsert_issuer_stats(computed)


Phase 2: Trade Enrichment (modified from --with-trade-details)
=========================================
  unenriched_trade_ids = db.get_unenriched_trade_ids()
  -- "unenriched" = filing_url is empty string AND filing_id is 0

  for tx_id in unenriched_trade_ids:
    detail = scraper.trade_detail(tx_id)
    db.update_trade_detail(tx_id, detail)
    sleep(delay)


Phase 3: Politician Enrichment (new)
=========================================
  unenriched_pol_ids = db.get_unenriched_politician_ids()
  -- "unenriched" = no rows in politician_committees for this ID
  --   (or a dedicated enriched_at column is NULL)

  for pol_id in unenriched_pol_ids:
    detail = scraper.politician_detail(pol_id)
    db.update_politician_detail(pol_id, detail)
    sleep(delay)


Phase 4: Issuer Enrichment (new)
=========================================
  unenriched_issuer_ids = db.get_unenriched_issuer_ids()
  -- "unenriched" = no row in issuer_performance for this ID

  for issuer_id in unenriched_issuer_ids:
    detail = scraper.issuer_detail(issuer_id)
    db.update_issuer_detail(issuer_id, detail)
    sleep(delay)
```

### How to Detect "Needs Enrichment"

There are two viable approaches. Use the simpler one.

**Option A: Sentinel values (simple, no schema migration)**

Check for known defaults that indicate listing-only data:

| Entity | Unenriched Condition |
|--------|---------------------|
| Trade | `filing_url = '' AND filing_id = 0` |
| Politician | `NOT EXISTS (SELECT 1 FROM politician_committees WHERE politician_id = ?)` combined with checking if enriched data is expected |
| Issuer | `NOT EXISTS (SELECT 1 FROM issuer_performance WHERE issuer_id = ?)` |

Problem: politicians with zero committees are indistinguishable from unenriched politicians. Same for issuers without performance data (e.g., non-public companies).

**Option B: Enrichment tracking column (recommended)**

Add an `enriched_at TEXT` column to each entity table:

```sql
ALTER TABLE trades ADD COLUMN enriched_at TEXT;
ALTER TABLE politicians ADD COLUMN enriched_at TEXT;
ALTER TABLE issuers ADD COLUMN enriched_at TEXT;
```

- NULL means not yet enriched.
- ISO 8601 timestamp means enriched on that date.
- To force re-enrichment (e.g., stale performance data), set `enriched_at = NULL` for target rows.

This is unambiguous, supports re-enrichment scheduling, and costs one column per table.

**Recommendation: Option B.** The sentinel approach breaks for politicians who genuinely have zero committees and issuers that legitimately lack performance data. The `enriched_at` column is cheap and eliminates ambiguity. It also enables a future `--refresh-stale` flag that re-enriches rows older than N days.

### Db Module Additions (capitoltraders_lib/src/db.rs)

New methods needed:

```
// Query methods
fn get_unenriched_trade_ids(&self, limit: Option<i64>) -> Result<Vec<i64>>
fn get_unenriched_politician_ids(&self, limit: Option<i64>) -> Result<Vec<String>>
fn get_unenriched_issuer_ids(&self, limit: Option<i64>) -> Result<Vec<i64>>

// Update methods
fn update_trade_detail(&self, tx_id: i64, detail: &ScrapedTradeDetail) -> Result<()>
fn update_politician_detail(&self, politician_id: &str, detail: &ScrapedPolitician, committees: &[String]) -> Result<()>
fn update_issuer_detail(&self, issuer_id: i64, detail: &ScrapedIssuerDetail) -> Result<()>

// Re-enrichment support
fn get_stale_issuer_ids(&self, older_than_days: i64) -> Result<Vec<i64>>
fn clear_enrichment(&self, entity: &str) -> Result<()>  // for --force-refresh
```

These methods should be on the existing `Db` struct. They use the same connection and transaction patterns already established.

### Sync Command Changes (capitoltraders_cli/src/commands/sync.rs)

New CLI flags:

```
--enrich-trades       Fetch detail pages for trades missing filing data
--enrich-politicians  Fetch detail pages for politicians missing committee data
--enrich-issuers      Fetch detail pages for issuers missing performance data
--enrich-all          Shorthand for all three enrichment flags
--enrich-delay-ms     Delay between enrichment requests (default: 500)
--enrich-limit        Max entities to enrich per run (for time-bounded CI)
--force-enrich        Re-enrich all entities regardless of enriched_at
```

The `--with-trade-details` flag should be deprecated in favor of `--enrich-trades` to maintain a consistent naming pattern.

New functions in sync.rs:

```
async fn enrich_trades(scraper, db, delay_ms, limit) -> Result<EnrichResult>
async fn enrich_politicians(scraper, db, delay_ms, limit) -> Result<EnrichResult>
async fn enrich_issuers(scraper, db, delay_ms, limit) -> Result<EnrichResult>
```

Each follows the same pattern:
1. Query unenriched IDs from SQLite
2. For each ID, fetch detail page
3. Update the entity row + join tables
4. Set enriched_at
5. Sleep between requests
6. Log progress

### Schema Migration Strategy

The project uses `CREATE TABLE IF NOT EXISTS` and `include_str!` for schema initialization.

For adding `enriched_at` columns, use `ALTER TABLE ... ADD COLUMN` with `IF NOT EXISTS` (SQLite 3.35+, 2021). However, `ALTER TABLE ADD COLUMN IF NOT EXISTS` is not supported by all SQLite versions in the wild. Safer approach:

```sql
-- In a migration block or guarded by checking PRAGMA table_info
ALTER TABLE trades ADD COLUMN enriched_at TEXT;
ALTER TABLE politicians ADD COLUMN enriched_at TEXT;
ALTER TABLE issuers ADD COLUMN enriched_at TEXT;
```

Wrap each ALTER in a try/catch in Rust (check for "duplicate column name" error and ignore it). This is idempotent and backward-compatible.

Alternatively, add the columns directly to `schema/sqlite.sql` since the system uses `CREATE TABLE IF NOT EXISTS`. Existing databases will need the ALTER statements. Best practice: have the `db.init()` method run both the CREATE TABLE statements (for new databases) and the ALTER TABLE statements (for existing databases).

## Patterns to Follow

### Pattern 1: Batch-Query-Then-Iterate

**What:** Query all unenriched IDs upfront, then iterate with delays.
**When:** Always, for enrichment passes.
**Why:** Avoids interleaving SQLite reads with HTTP requests. Simpler error handling. The ID list is small enough to hold in memory (at most tens of thousands of IDs, each a few bytes).

```rust
let ids = db.get_unenriched_trade_ids(limit)?;
let total = ids.len();
for (i, tx_id) in ids.iter().enumerate() {
    match scraper.trade_detail(*tx_id).await {
        Ok(detail) => {
            db.update_trade_detail(*tx_id, &detail)?;
            enriched += 1;
        }
        Err(e) => {
            eprintln!("Failed to enrich trade {}: {}", tx_id, e);
            failed += 1;
        }
    }
    if i + 1 < total {
        sleep(Duration::from_millis(delay_ms)).await;
    }
    if (i + 1) % 50 == 0 {
        eprintln!("Enriched {}/{} trades", i + 1, total);
    }
}
```

### Pattern 2: Granular Updates (Not Full Upserts)

**What:** For enrichment, use targeted UPDATE statements rather than full upsert.
**When:** Updating detail fields on rows that already exist from listing ingest.
**Why:** Full upsert (INSERT ... ON CONFLICT UPDATE) is correct for listing ingest because the row may or may not exist. But for enrichment, the row MUST already exist (we queried its ID). A targeted UPDATE is clearer, avoids accidentally nullifying listing-provided fields, and is slightly more efficient.

```rust
fn update_trade_detail(&self, tx_id: i64, detail: &ScrapedTradeDetail) -> Result<()> {
    self.conn.execute(
        "UPDATE trades SET filing_url = ?1, filing_id = ?2, enriched_at = ?3
         WHERE tx_id = ?4",
        params![
            detail.filing_url.as_deref().unwrap_or(""),
            detail.filing_id.unwrap_or(0),
            Utc::now().to_rfc3339(),
            tx_id
        ],
    )?;
    Ok(())
}
```

### Pattern 3: Transaction-Per-Entity for Join Tables

**What:** Wrap committee/label inserts in a transaction with their parent update.
**When:** Politician enrichment (committees), potentially trade enrichment if labels become available.
**Why:** The delete-then-insert pattern for join tables requires atomicity. If the process crashes between DELETE and INSERT, the join data is lost.

```rust
fn update_politician_detail(&mut self, pol_id: &str, detail: &ScrapedPolitician, committees: &[String]) -> Result<()> {
    let tx = self.conn.transaction()?;
    tx.execute(
        "UPDATE politicians SET dob = ?1, gender = ?2, ... , enriched_at = ?N WHERE politician_id = ?",
        params![...],
    )?;
    tx.execute("DELETE FROM politician_committees WHERE politician_id = ?1", params![pol_id])?;
    for committee in committees {
        tx.execute(
            "INSERT INTO politician_committees (politician_id, committee) VALUES (?1, ?2)",
            params![pol_id, committee],
        )?;
    }
    tx.commit()?;
    Ok(())
}
```

### Pattern 4: Progress Reporting via stderr

**What:** All progress output goes to stderr, matching the existing convention.
**When:** During enrichment passes.
**Why:** stdout is reserved for data output. The codebase already follows this pattern with `eprintln!`.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Enriching Inside the Listing Loop

**What:** Calling `trade_detail()` for every trade while iterating listing pages (the current `--with-trade-details` approach).
**Why bad:** If the listing loop fails on page 50 of 200, you lose the detail data for trades on pages 1-49 because the whole batch was in-memory. Enrichment is also not resumable -- you have to re-fetch all listing pages to re-trigger detail fetches.
**Instead:** Persist listings first, then enrich from the database. This is exactly what the recommended architecture does.

### Anti-Pattern 2: Using the CachedClient for Enrichment

**What:** Routing enrichment through the `CachedClient` wrapper.
**Why bad:** The CachedClient has a 5-10 second random delay between requests (for the legacy API). Enrichment needs its own delay cadence (configurable, lower for detail pages). The in-memory cache is also useless for enrichment since each detail page is fetched exactly once.
**Instead:** Use `ScrapeClient` directly, which is what the sync command already does.

### Anti-Pattern 3: Re-enriching on Every Sync Run

**What:** Fetching detail pages for all entities, not just unenriched ones.
**Why bad:** Wastes HTTP requests and time. A full sync with trade detail enrichment could be 5000+ trade detail requests, 500+ politician detail requests, and 2000+ issuer detail requests.
**Instead:** Only enrich rows where `enriched_at IS NULL`. Provide `--force-enrich` for manual full re-enrichment.

### Anti-Pattern 4: Shared Enrichment State Between Entity Types

**What:** Having a single "enrichment status" table or flag that covers all three entity types.
**Why bad:** You might want to enrich issuers (for performance data that changes daily) more frequently than politicians (whose committees change rarely). Per-entity-type control is essential.
**Instead:** Separate `enriched_at` columns per table, separate CLI flags per entity type.

## Build Order (Dependencies)

The recommended build order follows from data dependencies:

```
Phase 1: Schema + Db queries (no HTTP, pure SQLite)
  - Add enriched_at columns to schema
  - Implement migration logic in db.init()
  - Implement get_unenriched_*() query methods
  - Implement update_*_detail() methods
  - Unit tests against in-memory SQLite

Phase 2: Trade enrichment pass (simplest, modifies existing pattern)
  - Extract existing --with-trade-details logic into standalone enrich_trades()
  - Wire up --enrich-trades flag
  - Deprecate --with-trade-details
  - This is the simplest because trade_detail() already exists and is used

Phase 3: Politician enrichment pass
  - Implement enrich_politicians() in sync.rs
  - Politician detail page returns ScrapedPolitician but NOT committees
    (committees are not in the politician detail RSC payload -- needs verification)
  - Wire up --enrich-politicians flag

Phase 4: Issuer enrichment pass
  - Implement enrich_issuers() in sync.rs
  - Issuer detail page returns performance + eod_prices (richest detail data)
  - Wire up --enrich-issuers flag
  - This is the most complex because issuer_performance and issuer_eod_prices
    involve multiple related tables

Phase 5: CI integration
  - Update sqlite-sync.yml to use --enrich-all
  - Consider --enrich-limit for time-bounded CI runs
  - Consider splitting into daily (trades) vs weekly (issuers performance) enrichment
```

**Why this order:**

- Phase 1 must come first because all enrichment passes depend on the Db query/update methods.
- Phase 2 should come second because it is a refactor of existing working code (`--with-trade-details`), which de-risks the pattern.
- Phases 3 and 4 are independent of each other but both depend on Phase 1. They could be done in parallel, but politician enrichment is simpler (fewer related tables), so it makes a better third step.
- Phase 5 comes last because it depends on all enrichment passes working correctly.

## Scalability Considerations

| Concern | Current (hundreds) | At 10K trades | At 50K+ trades |
|---------|-------------------|---------------|----------------|
| Enrichment time | Minutes | Hours (with 500ms delays) | Use --enrich-limit to cap per run |
| SQLite write contention | None (single-threaded) | None (single-threaded) | None unless concurrent access added |
| Memory for ID lists | Negligible | ~80KB for 10K i64 IDs | ~400KB, still fine |
| HTTP rate limiting | Manual delays | May need adaptive backoff | Existing retry+exponential backoff handles this |

The system is inherently single-threaded for HTTP requests (to avoid overwhelming the target). Parallelism is not a concern or an optimization target here. The bottleneck is network I/O with polite delays, which is intentional.

## Open Question: Politician Committees

The `politician_detail()` method returns `Option<ScrapedPolitician>`, which contains basic bio fields (name, party, state, dob, gender, chamber) but does NOT include committees. The `ScrapedPolitician` struct has no committees field. This means politician enrichment via the detail page may not actually fill the `politician_committees` table.

Committees might only be available through:
1. The CapitolTrades BFF API (not scraped, may still work)
2. A different page on the site that includes committee data
3. An external data source (e.g., congress.gov)

This needs investigation before Phase 3 implementation. The architecture accommodates this -- if committees are not available from the politician detail page, that enrichment pass simply fills in the bio fields (dob, gender, social links, etc.) and leaves committees for a future data source integration.

## Open Question: Trade Committees and Labels

Similar to politician committees, the trade listing pages produce empty committees and labels arrays. The trade detail page (`trade_detail()`) only extracts `filing_url` and `filing_id`. Committees and labels for trades may also require the BFF API or a different scraping approach.

The `ScrapedTradeDetail` struct only has `filing_url` and `filing_id`. If trade-level committees and labels are desired, the `trade_detail()` method or its payload parser would need to be extended to extract them, assuming the data exists in the RSC payload.

## Sources

- Direct analysis of `capitoltraders_lib/src/scrape.rs` (ScrapeClient methods, struct definitions)
- Direct analysis of `capitoltraders_lib/src/db.rs` (upsert methods, schema interaction)
- Direct analysis of `capitoltraders_cli/src/commands/sync.rs` (pipeline orchestration)
- Direct analysis of `capitoltraders_cli/src/commands/trades.rs` (existing detail enrichment in listing flow)
- Direct analysis of `capitoltraders_cli/src/commands/politicians.rs` (politician_detail usage)
- Direct analysis of `capitoltraders_cli/src/commands/issuers.rs` (issuer_detail usage)
- Direct analysis of `schema/sqlite.sql` (table structure, join tables)
