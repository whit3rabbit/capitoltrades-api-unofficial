# Phase 3: Trade Sync and Output - Research

**Researched:** 2026-02-08
**Domain:** Sync pipeline enrichment, batch checkpointing, smart-skip, CLI output extension, throttle tuning
**Confidence:** HIGH (all building blocks exist from Phases 1-2; no new external dependencies)

## Summary

Phase 3 wires the trade enrichment infrastructure built in Phases 1-2 into the sync pipeline and extends CLI output to display enriched fields. The core challenge is orchestration, not extraction: taking the `ScrapeClient::trade_detail()` method and `Db::update_trade_detail()` method and integrating them into the existing `sync` command with smart-skip logic, crash-safe batch checkpointing, a dry-run mode, configurable throttle delays, and output formatting.

The codebase is well-prepared. `get_unenriched_trade_ids()` already returns the queue of trades needing enrichment. `update_trade_detail()` already persists all fields with sentinel protection and sets `enriched_at`. `trade_detail()` already scrapes all enrichable fields from RSC payloads. What is missing is: (1) a sync enrichment loop that calls these in sequence with batch commits, (2) smart-skip logic that goes beyond the basic `enriched_at IS NULL` check, (3) a dry-run mode, (4) throttle configuration, (5) a database query path for reading enriched trades back out, and (6) extended output formatting for `asset_type`, `committees`, and `labels`.

The output requirement (OUT-01) is architecturally significant. Currently, the `trades` command scrapes live data and never queries the database. To show enriched fields, we need either a new command path that reads from the DB, or a modification of the live path to merge DB data. The cleanest approach is adding a `--db` flag to the `trades` command that reads from SQLite instead of scraping live data, since the sync pipeline already populates the DB with fully enriched records. This also avoids rate-limiting issues during normal browsing.

**Primary recommendation:** Split into 3 plans: (1) sync enrichment pipeline with smart-skip, batch checkpointing, dry-run, and throttle; (2) database query methods for reading enriched trades with joined committees/labels; (3) extend CLI output to display enriched fields in all formats.

## Standard Stack

### Core (no new dependencies)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.31 | Batch transaction commits, enrichment queries | Already used for all DB operations |
| chrono | 0.4 | enriched_at timestamps, throttle timing | Already used throughout |
| tokio | 1.x | async sleep for throttle delay | Already used for all async |
| clap | 4.x | New CLI flags (--enrich, --dry-run, --db) | Already used for CLI |
| serde_json | 1.x | Trade output serialization | Already used throughout |
| tabled | 0.17 | Extended TradeRow for table/markdown | Already used for CLI output |
| csv | 1.3 | Extended TradeRow for CSV | Already used for CLI output |
| quick-xml | 0.37 | Already handles committees/labels via JSON bridge | Already used for XML output |

### Supporting (no additions needed)

Phase 3 requires zero new crate dependencies. All work extends existing modules.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| --db flag on trades command | Separate `trades-db` subcommand | --db flag is more ergonomic and the trades command already has extensive filtering that should work on DB data too |
| Batch size as hardcoded constant | Environment variable CAPITOLTRADES_BATCH_SIZE | Env vars add flexibility but a CLI arg is more discoverable; recommend --batch-size flag |
| Smart-skip via enriched_at only | Full field completeness check | enriched_at IS NOT NULL is sufficient because update_trade_detail always sets it, and we trust our own enrichment pipeline |

## Architecture Patterns

### Current State: Sync Pipeline (sync.rs)

The existing sync command:
1. Opens DB, initializes schema
2. Scrapes trades pages sequentially
3. Optionally fetches trade details inline (`--with-trade-details`)
4. Upserts scraped trades to DB
5. Updates stats aggregates
6. Records last_trade_pub_date in ingest_meta

Current enrichment flow (inline in trade sync):
```
for trade in trades:
    if with_trade_details:
        detail = scraper.trade_detail(trade.tx_id)
        trade.filing_url = detail.filing_url  // Only merges filing fields!
        trade.filing_id = detail.filing_id
        sleep(details_delay_ms)
    db.upsert_scraped_trades(trades)  // Loses detail enrichment for other fields
```

**Critical problem with current approach:** The inline detail fetch in sync only captures filing_url and filing_id from the detail page, ignoring all the new fields that Phase 2 added to ScrapedTradeDetail (asset_type, size, price, committees, labels, etc.). Even when `--with-trade-details` is used, the enrichment is incomplete because `upsert_scraped_trades` does not accept those fields.

### Recommended Architecture: Post-Ingest Enrichment Loop

Separate trade listing ingest from detail enrichment:

```
Phase A: Ingest trade listings (existing flow, unchanged)
    for each page:
        scrape trades listing page
        upsert_scraped_trades(trades)  // Gets basic trade data

Phase B: Enrich individual trades (new flow)
    trade_ids = db.get_unenriched_trade_ids(batch_size)
    if dry_run:
        report count and exit
    for batch in trade_ids.chunks(batch_size):
        for tx_id in batch:
            detail = scraper.trade_detail(tx_id)
            db.update_trade_detail(tx_id, detail)
            sleep(detail_delay_ms)
        // Batch checkpoint: enriched_at is already committed per-trade
        // via update_trade_detail's transaction
```

This is cleaner than inline enrichment because:
1. Listing pages and detail pages have different rate-limiting needs (PERF-04)
2. Failed detail fetches do not block listing ingestion
3. Resumability is automatic: restart queries for unenriched IDs
4. Smart-skip is automatic: enriched_at IS NOT NULL skips the trade

### Recommended Project Structure

```
capitoltraders_lib/src/
  db.rs            # Add query_trades(), count_unenriched_trades()
  scrape.rs        # No changes needed
  lib.rs           # Re-export new types

capitoltraders_cli/src/
  commands/sync.rs # Add enrichment loop, --enrich/--dry-run/--batch-size flags
  output.rs        # Extend TradeRow with asset_type, committees, labels
```

### Pattern 1: Batch Checkpointing via Per-Trade Commits

**What:** Each call to `update_trade_detail()` commits its own transaction (via `unchecked_transaction` + `commit`). This means every successfully enriched trade is immediately persisted.

**When to use:** When individual items are expensive to fetch and you want crash-safety.

**Why this works for TRADE-10:** If the process crashes after enriching 500 of 1000 trades, restarting queries `get_unenriched_trade_ids()` which returns only the remaining 500. No re-fetching of already-enriched trades.

**Important:** This is already the behavior of `update_trade_detail()` from Phase 2. Each call opens, writes, and commits its own transaction. The "batch checkpointing" requirement is satisfied by the existing per-trade commit pattern. No additional checkpointing infrastructure is needed.

### Pattern 2: Smart-Skip via enriched_at

**What:** `get_unenriched_trade_ids()` already returns only trades with `enriched_at IS NULL`. This is the smart-skip.

**Refinement for TRADE-09:** The requirement says "skip when all enrichable fields are non-NULL/non-default." This is subtly different from just checking enriched_at. However, since `update_trade_detail()` ALWAYS sets `enriched_at` (even when the scraped detail has mostly empty fields), the `enriched_at IS NOT NULL` check is the correct proxy for "this trade has been processed."

**Edge case: what if detail page returned no data?** If a detail page yields an empty ScrapedTradeDetail (all fields None, no committees, no labels), `update_trade_detail()` still sets `enriched_at`. This is correct behavior: we tried to enrich, the data was not available, we should not retry. If the user wants to force re-enrichment, they can reset `enriched_at` to NULL.

**Optional enhancement:** Add a `--force-reenrich` flag that ignores enriched_at and re-processes all trades.

### Pattern 3: Throttle Delay Configuration (PERF-04)

**What:** Different delays for listing pages vs detail pages.

**Current state:** The sync command has `--details-delay-ms` (default 250ms). The trades command also has `--details-delay-ms`.

**Recommendation:** Keep `--details-delay-ms` for the enrichment loop. Consider increasing the default from 250ms to 500ms for detail pages, since they are heavier and more sensitive to rate limiting. The listing page scraping does not need a delay between pages (the current behavior).

### Pattern 4: Dry-Run Mode

**What:** Report how many trades would be enriched without making HTTP requests.

**Implementation:**
```rust
let unenriched_count = db.count_unenriched_trades()?;
if args.dry_run {
    eprintln!("{} trades would be enriched", unenriched_count);
    return Ok(());
}
```

This requires a new `Db::count_unenriched_trades()` method (a simple COUNT query).

### Pattern 5: Database Read Path for CLI Output (OUT-01)

**What:** Query trades from SQLite with enriched fields joined in.

**Challenge:** The current CLI trades command uses the vendored `Trade` type for output. The `Trade` struct has `committees` and `labels` as private fields, but they ARE serialized by serde (so JSON and XML output already include them). For table/CSV/markdown, we need to extend `TradeRow` to include asset_type, committees, and labels.

**Two approaches for getting enriched data into the output:**

**Approach A (recommended): New output struct for DB trades**

Create a `DbTradeOutput` struct in `db.rs` that represents a fully enriched trade from the database, with all fields needed for output. This avoids modifying the vendored `Trade` type.

```rust
pub struct DbTradeOutput {
    pub tx_id: i64,
    pub politician_name: String,
    pub party: String,
    pub issuer_name: String,
    pub ticker: String,
    pub tx_type: String,
    pub tx_date: String,
    pub value: i64,
    pub asset_type: String,
    pub committees: Vec<String>,
    pub labels: Vec<String>,
    // ... other fields as needed
}
```

Query with JOINs:
```sql
SELECT t.tx_id, t.tx_date, t.tx_type, t.value,
       p.first_name || ' ' || p.last_name AS politician_name,
       p.party,
       i.issuer_name, i.issuer_ticker,
       a.asset_type,
       GROUP_CONCAT(DISTINCT tc.committee) AS committees,
       GROUP_CONCAT(DISTINCT tl.label) AS labels
FROM trades t
JOIN politicians p ON t.politician_id = p.politician_id
JOIN issuers i ON t.issuer_id = i.issuer_id
JOIN assets a ON t.asset_id = a.asset_id
LEFT JOIN trade_committees tc ON t.tx_id = tc.tx_id
LEFT JOIN trade_labels tl ON t.tx_id = tl.tx_id
GROUP BY t.tx_id
ORDER BY t.pub_date DESC
```

**Approach B: Reconstruct Trade from DB rows**

Query the DB and construct `Trade` structs from the rows, then use existing output functions. This is more complex because the `Trade` struct is from the vendored crate and has many private fields that need to be set via JSON deserialization.

Approach A is simpler and more maintainable. The planner should use Approach A.

### Anti-Patterns to Avoid

- **Modifying the vendored Trade struct to add pub to committees/labels:** This is a modification to the vendored crate. Follow the existing pattern of using separate output types instead.

- **Wrapping entire enrichment loop in a single transaction:** This defeats crash-safety. Each trade should be committed independently so progress is preserved on crash.

- **Delaying enrichment until all listing pages are ingested:** Enrichment should be a separate pass, but it should run in the same sync invocation (after listing ingest). This gives users a single command for full sync + enrichment.

- **Making the --db flag required for seeing enriched data:** The trades command should work both ways: live scraping (default) and DB query (with --db flag). Users who have synced can use --db for instant enriched results.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Crash-safe checkpointing | Custom checkpoint file/table | Per-trade commit in update_trade_detail | Already commits each trade's enrichment immediately; restart just queries for remaining unenriched trades |
| Smart-skip logic | Complex field completeness check | enriched_at IS NULL check | update_trade_detail always sets enriched_at; if it was called, the trade was processed |
| Rate limiting | Custom token bucket / rate limiter | Simple tokio::time::sleep between requests | Sequential enrichment with configurable delay is sufficient; Phase 6 adds proper concurrency |
| Progress reporting | Custom progress bar | eprintln with current/total counts | Phase 6 adds proper progress bars; Phase 3 uses simple stderr logging |
| Retry logic for detail pages | New retry wrapper | Existing ScrapeClient::with_retry | Already handles 429, 5xx, timeouts with exponential backoff |

**Key insight:** Phase 3 is an orchestration phase. All the hard technical problems (extraction, persistence, retry, sentinel protection) were solved in Phases 1-2. Phase 3 connects them together with control flow.

## Common Pitfalls

### Pitfall 1: Inline Enrichment Losing Fields

**What goes wrong:** The developer tries to enrich trades inline during listing page ingestion (the current `--with-trade-details` approach) instead of as a post-ingest pass. The inline approach only merges filing_url and filing_id into the ScrapedTrade before upserting, discarding asset_type, size, price, committees, and labels.

**Why it happens:** The current code at sync.rs line 157-169 shows this exact pattern: it fetches trade_detail but only copies `filing_url` and `filing_id` onto the `ScrapedTrade`. The `ScrapedTrade` struct does not have fields for the other enrichment data.

**How to avoid:** Use a post-ingest enrichment loop that calls `db.update_trade_detail()` directly. This was specifically designed in Phase 2 for this purpose. Do NOT try to merge ScrapedTradeDetail fields into ScrapedTrade.

**Warning signs:** After sync, trades have enriched_at = NULL even though --with-trade-details was used.

### Pitfall 2: Entire Enrichment Run in One Transaction

**What goes wrong:** Wrapping the enrichment loop in a single BEGIN/COMMIT. If the process is killed mid-run, all enrichment is rolled back and must restart from scratch.

**Why it happens:** Developers often think "batch = one transaction" for performance. But here, the bottleneck is HTTP latency (500ms+ per trade), not SQLite writes.

**How to avoid:** `update_trade_detail()` already commits per-trade. Do not wrap the outer loop in a transaction. The per-trade commit IS the batch checkpoint.

**Warning signs:** Restarting after a crash re-enriches all trades from the beginning.

### Pitfall 3: Forgetting to Query Committees/Labels in DB Read Path

**What goes wrong:** The DB query for output returns trades but does not JOIN trade_committees and trade_labels. The output shows empty committees and labels even for enriched trades.

**Why it happens:** Committees and labels are in separate join tables, not columns on the trades table. A simple `SELECT * FROM trades` misses them.

**How to avoid:** Use LEFT JOIN with GROUP_CONCAT for the committee/label columns, or run sub-queries per trade. GROUP_CONCAT is simpler for display purposes.

**Warning signs:** JSON output shows `committees: []` and `labels: []` for trades that have been enriched.

### Pitfall 4: GROUP_CONCAT Returns NULL Instead of Empty String

**What goes wrong:** SQLite's GROUP_CONCAT returns NULL when there are no matching rows in the left join. If the code expects an empty string, it crashes or displays "null".

**Why it happens:** LEFT JOIN + GROUP_CONCAT on an empty result set produces NULL, not "".

**How to avoid:** Use `COALESCE(GROUP_CONCAT(...), '')` in the query, or handle NULL in the Rust code by mapping to an empty Vec.

**Warning signs:** Trades without committees/labels cause panics or display "null" text.

### Pitfall 5: Not Handling the --with-trade-details Flag Interaction

**What goes wrong:** The new `--enrich` flag and the old `--with-trade-details` flag both trigger detail page fetching but through different code paths. Users may be confused about which to use.

**Why it happens:** The old flag was a temporary measure before the enrichment pipeline was built. Now we have a proper pipeline.

**How to avoid:** Deprecate or remove `--with-trade-details`. Replace it with `--enrich` which uses the proper post-ingest pipeline. If keeping the old flag for backward compatibility, make it trigger the new pipeline instead.

**Warning signs:** Users use --with-trade-details and wonder why asset_type/committees/labels are not populated.

### Pitfall 6: Trade Output Struct Visibility

**What goes wrong:** The developer tries to access `trade.committees` or `trade.labels` directly from the `Trade` struct, but these fields are private in the vendored crate.

**Why it happens:** The Trade struct marks some fields as pub and others as private. committees and labels are private.

**How to avoid:** Do not try to read fields from the Trade struct for the DB output path. Use a custom output struct (DbTradeOutput) that is populated directly from SQL queries. For JSON/XML output of the Trade struct, serde serialization already includes the private fields (they are serde-annotated, not pub-access-annotated).

**Warning signs:** Compilation errors about field privacy when trying to build TradeRow from Trade.

## Code Examples

### Example 1: Enrichment Loop in Sync

```rust
// Source: Pattern derived from existing sync_trades + Phase 2 update_trade_detail
async fn enrich_trades(
    scraper: &ScrapeClient,
    db: &Db,
    batch_size: Option<i64>,
    detail_delay_ms: u64,
    dry_run: bool,
) -> Result<EnrichmentResult> {
    let trade_ids = db.get_unenriched_trade_ids(batch_size)?;
    let total = trade_ids.len();

    if dry_run {
        eprintln!("{} trades would be enriched", total);
        return Ok(EnrichmentResult { enriched: 0, total });
    }

    if total == 0 {
        eprintln!("No trades need enrichment");
        return Ok(EnrichmentResult { enriched: 0, total: 0 });
    }

    eprintln!("Enriching {} trades...", total);

    let mut enriched = 0;
    for (i, tx_id) in trade_ids.iter().enumerate() {
        match scraper.trade_detail(*tx_id).await {
            Ok(detail) => {
                db.update_trade_detail(*tx_id, &detail)?;
                enriched += 1;
            }
            Err(err) => {
                eprintln!("Failed to enrich trade {}: {}", tx_id, err);
            }
        }
        if detail_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(detail_delay_ms)).await;
        }
        if (i + 1) % 50 == 0 || i + 1 == total {
            eprintln!("  Progress: {}/{} enriched", i + 1, total);
        }
    }

    Ok(EnrichmentResult { enriched, total })
}
```

### Example 2: DB Trade Query with Committees/Labels

```rust
// Source: Pattern derived from existing Db methods + SQLite JOIN patterns
pub fn query_trades(&self, limit: Option<i64>) -> Result<Vec<DbTradeRow>, DbError> {
    let sql = format!(
        "SELECT t.tx_id, t.tx_date, t.tx_type, t.value, t.pub_date,
                t.price, t.size, t.filing_url, t.reporting_gap,
                t.enriched_at,
                p.first_name, p.last_name, p.party, p.state_id,
                i.issuer_name, i.issuer_ticker,
                a.asset_type,
                COALESCE(GROUP_CONCAT(DISTINCT tc.committee), '') AS committees,
                COALESCE(GROUP_CONCAT(DISTINCT tl.label), '') AS labels
         FROM trades t
         JOIN politicians p ON t.politician_id = p.politician_id
         JOIN issuers i ON t.issuer_id = i.issuer_id
         JOIN assets a ON t.asset_id = a.asset_id
         LEFT JOIN trade_committees tc ON t.tx_id = tc.tx_id
         LEFT JOIN trade_labels tl ON t.tx_id = tl.tx_id
         GROUP BY t.tx_id
         ORDER BY t.pub_date DESC
         {}",
        limit.map(|n| format!("LIMIT {}", n)).unwrap_or_default()
    );
    let mut stmt = self.conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let committees_str: String = row.get(17)?;
        let labels_str: String = row.get(18)?;
        Ok(DbTradeRow {
            tx_id: row.get(0)?,
            tx_date: row.get(1)?,
            tx_type: row.get(2)?,
            value: row.get(3)?,
            pub_date: row.get(4)?,
            price: row.get(5)?,
            size: row.get(6)?,
            filing_url: row.get(7)?,
            reporting_gap: row.get(8)?,
            enriched_at: row.get(9)?,
            politician_name: format!("{} {}", row.get::<_, String>(10)?, row.get::<_, String>(11)?),
            party: row.get(12)?,
            state: row.get(13)?,
            issuer_name: row.get(14)?,
            issuer_ticker: row.get::<_, Option<String>>(15)?.unwrap_or_default(),
            asset_type: row.get(16)?,
            committees: if committees_str.is_empty() {
                Vec::new()
            } else {
                committees_str.split(',').map(|s| s.to_string()).collect()
            },
            labels: if labels_str.is_empty() {
                Vec::new()
            } else {
                labels_str.split(',').map(|s| s.to_string()).collect()
            },
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
```

### Example 3: Extended TradeRow for Output

```rust
// Source: Pattern from existing TradeRow in output.rs
#[derive(Tabled, Serialize)]
struct TradeRow {
    #[tabled(rename = "Date")]
    #[serde(rename = "Date")]
    tx_date: String,
    #[tabled(rename = "Politician")]
    #[serde(rename = "Politician")]
    politician: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "Issuer")]
    #[serde(rename = "Issuer")]
    issuer: String,
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Type")]
    #[serde(rename = "Type")]
    tx_type: String,
    #[tabled(rename = "Asset")]
    #[serde(rename = "Asset")]
    asset_type: String,
    #[tabled(rename = "Value")]
    #[serde(rename = "Value")]
    value: String,
    #[tabled(rename = "Committees")]
    #[serde(rename = "Committees")]
    committees: String,
    #[tabled(rename = "Labels")]
    #[serde(rename = "Labels")]
    labels: String,
}
```

### Example 4: Dry-Run Mode

```rust
// Source: Pattern for dry-run flag
#[derive(Args)]
pub struct SyncArgs {
    // ... existing fields ...

    /// Enrich trade details (fetch individual trade pages)
    #[arg(long)]
    pub enrich: bool,

    /// Show what would be enriched without fetching
    #[arg(long, requires = "enrich")]
    pub dry_run: bool,

    /// Number of trades to enrich per run (default: all)
    #[arg(long)]
    pub batch_size: Option<i64>,

    /// Delay between detail page requests in milliseconds
    #[arg(long, default_value = "500")]
    pub detail_delay_ms: u64,
}
```

### Example 5: SyncArgs Flag Interaction

```rust
// In run():
if args.enrich {
    let result = enrich_trades(
        &scraper,
        &db,
        args.batch_size,
        args.detail_delay_ms,
        args.dry_run,
    ).await?;
    eprintln!("Enrichment: {}/{} trades processed", result.enriched, result.total);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| --with-trade-details inline enrichment | Post-ingest enrichment loop | Phase 3 (this work) | All enrichable fields are captured, not just filing URL |
| No enrichment resume | Per-trade commit = automatic resume | Phase 3 (this work) | Crash-safe; restart picks up where left off |
| All trades re-enriched on restart | enriched_at check skips already-processed | Phase 3 (this work) | Smart-skip; only new/unenriched trades are fetched |
| No dry-run | --dry-run reports count without fetching | Phase 3 (this work) | Users can preview enrichment workload |
| 250ms delay for detail pages | 500ms configurable delay | Phase 3 (this work) | More respectful to capitoltrades.com; PERF-04 |
| trades command scrapes live only | trades command can read from DB | Phase 3 (this work) | Enriched fields visible in output; OUT-01 |
| TradeRow has 7 columns | TradeRow has 10 columns (+ asset_type, committees, labels) | Phase 3 (this work) | Enriched data visible in table/csv/md; OUT-01 |

## Key Architectural Decisions for Planner

### Decision 1: Post-Ingest vs Inline Enrichment

**Recommendation:** Post-ingest enrichment loop. Enrichment runs AFTER all listing pages are ingested and upserted. This cleanly separates the two concerns and leverages the existing `get_unenriched_trade_ids()` + `update_trade_detail()` pipeline.

**Rationale:** The current inline approach (sync.rs lines 156-171) only captures filing_url and filing_id, ignoring all Phase 2 fields. Retrofitting it to capture all fields would require modifying ScrapedTrade or adding a parallel update path. The post-ingest loop is simpler, crash-safe, and already has test coverage.

### Decision 2: --enrich Flag vs Automatic Enrichment

**Recommendation:** Use an `--enrich` flag on the sync command. Enrichment is slow (1 HTTP request per trade at 500ms delay = ~30 minutes for 3000 trades). It should be opt-in, not automatic.

**Rationale:** Users doing a quick incremental sync should not be surprised by a long enrichment run. The flag makes it explicit. `--dry-run` pairs with `--enrich` to preview the workload.

### Decision 3: Trade Output from DB

**Recommendation:** Add a `--db` flag to the `trades` command. When set, reads from SQLite instead of scraping live. Uses a custom `DbTradeRow` struct (not the vendored Trade type) to include committees, labels, and asset_type in table/CSV/markdown output.

**Rationale:** The vendored `Trade` struct has `committees` and `labels` as private fields. For JSON/XML output, serde already serializes them. But for table/CSV/markdown, the `TradeRow` builder cannot access them. Using a DB query struct sidesteps this entirely.

### Decision 4: Smart-Skip Granularity

**Recommendation:** Use `enriched_at IS NOT NULL` as the only smart-skip criterion. Do NOT check individual field completeness.

**Rationale:** `update_trade_detail()` always sets `enriched_at`, even when the detail page yields empty data. This is correct: "we tried, there was nothing to get." Re-processing will not yield different results. If the user wants to force re-enrichment (e.g., after a site format change), add `--force-reenrich`.

### Decision 5: Deprecate --with-trade-details

**Recommendation:** Deprecate the `--with-trade-details` flag on the sync command. Replace with `--enrich`. The old flag only captured 2 of ~10 enrichable fields, and the new pipeline captures all of them.

**Rationale:** Keeping both flags creates confusion. The old flag's partial enrichment is strictly worse than the new pipeline.

## Open Questions

1. **Should the --db flag on trades support all existing filter flags?**
   - What we know: The existing trades command has 24 filter flags that work on live-scraped data. Making all of them work on DB data requires SQL WHERE clauses for each.
   - What is unclear: How many of these filters are actually useful on DB data. Some (like --market-cap) may not make sense without issuer enrichment (Phase 5).
   - Recommendation: Start with basic filters (--party, --state, --name, --issuer, --tx-type, --since/--until) on the DB path. Add more as needed. Do NOT block Phase 3 on full filter parity.

2. **What happens when a detail page returns a 404 or empty payload?**
   - What we know: ScrapeClient::with_retry handles 429/5xx with retries. 404 is not retried. extract_trade_detail returns a default (empty) ScrapedTradeDetail for unknown trade IDs.
   - What is unclear: Whether capitoltrades.com returns 404 for deleted/removed trades, or some other response.
   - Recommendation: Log the error, skip the trade, and continue. The trade will remain unenriched (enriched_at stays NULL). On a future run, it will be retried. Consider adding a "skip_count" tracker to report how many trades failed enrichment.

3. **Should the existing --with-trade-details be removed or aliased?**
   - What we know: Removing a CLI flag is a breaking change for any scripts using it.
   - Recommendation: Keep it as a hidden alias for --enrich in Phase 3. Remove it in a future major version.

## Implementation Plan Recommendations

Based on the research, Phase 3 should be organized into 3 plans:

**Plan 03-01: Sync enrichment pipeline**
- Add --enrich, --dry-run, --batch-size, --detail-delay-ms flags to SyncArgs
- Add count_unenriched_trades() method to Db
- Implement enrich_trades() loop with per-trade commit, error handling, progress logging
- Wire into sync::run() after trade listing ingest
- Deprecate --with-trade-details (hidden alias to --enrich)
- Increase default detail delay to 500ms (PERF-04)
- Tests: unit test for count_unenriched_trades; integration test with wiremock for enrichment loop

**Plan 03-02: Database trade query**
- Add DbTradeRow struct to db.rs
- Add query_trades() method with JOINs for committees/labels
- Add basic filter parameters (party, state, tx_type, date range)
- Re-export DbTradeRow from lib.rs
- Tests: unit tests for query_trades with various filter combinations

**Plan 03-03: CLI output extension**
- Extend TradeRow in output.rs with asset_type, committees, labels columns
- Add --db flag to trades command
- Add print functions for DbTradeRow across all formats
- Update build_trade_rows to extract asset_type from Trade.asset.asset_type (it is pub)
- For committees/labels in the live path: already serialized in JSON/XML; show empty in table/CSV/md
- Tests: unit tests for extended TradeRow, output format verification

## Sources

### Primary (HIGH confidence)
- Direct analysis of capitoltraders_lib/src/db.rs -- update_trade_detail, get_unenriched_trade_ids, upsert_scraped_trades
- Direct analysis of capitoltraders_lib/src/scrape.rs -- ScrapeClient::trade_detail, ScrapedTradeDetail, with_retry
- Direct analysis of capitoltraders_cli/src/commands/sync.rs -- current sync pipeline, inline detail fetch
- Direct analysis of capitoltraders_cli/src/output.rs -- TradeRow struct, output format functions
- Direct analysis of capitoltrades_api/src/types/trade.rs -- Trade struct field visibility (committees/labels private, asset pub)
- Direct analysis of capitoltraders_cli/src/xml_output.rs -- XML bridge handles committees/labels via serde
- Direct analysis of schema/sqlite.sql -- table structure, enrichment indexes
- Phase 2 research and plans -- established patterns (sentinel protection, fixture testing, unchecked_transaction)

### Secondary (MEDIUM confidence)
- Phase 1/2 state.md patterns -- batch checkpoint via per-trade commit, enriched_at as skip signal

### Tertiary (LOW confidence)
- Throttle delay recommendation (500ms) -- based on general web scraping etiquette, not specific CapitolTrades rate limits. May need adjustment based on actual 429 response frequency.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- zero new dependencies, all patterns established
- Architecture (enrichment pipeline): HIGH -- all building blocks exist and are tested from Phases 1-2
- Architecture (DB query path): HIGH -- standard SQLite JOIN patterns, well-understood
- Architecture (output extension): HIGH -- extending existing TradeRow struct, straightforward
- Pitfalls: HIGH -- derived from direct code analysis of existing limitations
- Throttle tuning: MEDIUM -- default value is a guess; may need adjustment

**Research date:** 2026-02-08
**Valid until:** 2026-03-08 (stable domain; no external dependency changes)
