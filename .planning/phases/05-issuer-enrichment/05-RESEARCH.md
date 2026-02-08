# Phase 5: Issuer Enrichment - Research

**Researched:** 2026-02-08
**Domain:** Issuer detail scraping, performance/EOD data extraction, sync pipeline extension, CLI output
**Confidence:** HIGH (all infrastructure exists; this phase assembles existing pieces)

## Summary

Phase 5 extends the enrichment pipeline to issuer records. The goal is populating `issuer_performance` and `issuer_eod_prices` tables with data extracted from issuer detail pages, then surfacing that data in all CLI output formats via a `--db` flag on the issuers command.

The good news: almost everything needed already exists. The `issuer_detail()` scrape method fetches `/issuers/{id}` and extracts the `"issuerData":{}` JSON object from the RSC payload, returning a `ScrapedIssuerDetail` with `performance: Option<serde_json::Value>`. The `upsert_issuers()` DB method already writes to `issuer_performance` and `issuer_eod_prices` tables (with proper upsert semantics). The `get_unenriched_issuer_ids()` method already queries the enrichment queue. The `issuers` table already has an `enriched_at` column with an index. The schema for `issuer_performance` and `issuer_eod_prices` is already defined in `sqlite.sql`.

What is missing: (1) A function to update a single issuer's detail data from a scrape result and mark it enriched -- either a new `update_issuer_detail()` method or conversion of `ScrapedIssuerDetail` to `IssuerDetail` and use of existing `upsert_issuers()`; (2) wiring issuer enrichment into the sync pipeline; (3) `query_issuers()` DB method with `DbIssuerRow` and `DbIssuerFilter`; (4) `--db` flag on the issuers command with performance-aware output rows; (5) an HTML fixture for testing the extraction.

**Critical caveat confirmed:** Issuer detail pages return loading states via curl (same as trade and politician pages). The RSC payload is streamed client-side. This means we cannot fetch a real HTML fixture for automated testing. We must use synthetic fixtures that model the expected RSC payload structure based on the BFF API types (same approach as Phase 2 for trade detail fixtures). However, Phase 4 showed that real fixtures catch bugs synthetic ones miss (the singular/plural label issue), so if a live fixture can be obtained through browser devtools, it should be used.

**Primary recommendation:** Split into 3 plans: (1) Issuer detail extraction testing with fixture and `update_issuer_detail()` DB persistence; (2) Sync pipeline integration for issuer enrichment (automatic or opt-in, see Open Questions); (3) CLI issuers `--db` output with performance and EOD data in all formats.

## Standard Stack

### Core (no new dependencies)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.31 | issuer_performance/eod_prices CRUD, query_issuers | Already used for all DB operations |
| chrono | 0.4 | enriched_at timestamps, EOD price dates | Already used throughout |
| tokio | 1.x | async sleep for throttle delay | Already used for all async |
| serde_json | 1.x | Performance JSON parsing, issuer output serialization | Already used throughout |
| tabled | 0.17 | Extended IssuerRow for table/markdown | Already used for CLI output |
| csv | 1.3 | Extended IssuerRow for CSV | Already used for CLI output |
| quick-xml | 0.37 | Issuer XML via JSON bridge | Already used for XML output |

### Supporting (no additions needed)

Phase 5 requires zero new crate dependencies. All work extends existing modules.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Synthetic HTML fixtures | Real HTML from browser devtools | Real fixtures catch more bugs but require manual capture; synthetic is faster and consistent |
| Per-issuer detail page scraping | Batch BFF API call | BFF returns 503; detail page is the only source |
| Automatic enrichment (no flag) | Reuse existing --enrich flag | Issuer enrichment requires one HTTP request per issuer (could be thousands); too slow for automatic. Should use --enrich like trades. |
| `--db` flag on issuers command | Always merge DB data into live output | --db flag is cleaner, follows Phase 3/4 pattern |
| New `update_issuer_detail()` method | Convert ScrapedIssuerDetail to IssuerDetail + use existing `upsert_issuers()` | update_issuer_detail is more targeted; upsert_issuers already handles the complex performance+EOD logic but expects IssuerDetail not ScrapedIssuerDetail |

## Architecture Patterns

### Existing Infrastructure (already implemented)

The following components are already in the codebase and do not need modification:

1. **Scrape: `issuer_detail(issuer_id: i64)`** in `scrape.rs` -- fetches `/issuers/{id}`, extracts `"issuerData":{}` via `extract_json_object_after`, returns `ScrapedIssuerDetail` with `performance: Option<serde_json::Value>`.

2. **DB: `upsert_issuers(&mut self, issuers: &[IssuerDetail])`** in `db.rs` -- handles issuer base row, issuer_stats, issuer_performance (all 20 columns), and issuer_eod_prices. Uses `eod_pair()` helper to extract (date, price) tuples from the nested `Vec<Vec<DbEodValue>>` structure. When performance is None, deletes existing performance and EOD data.

3. **DB: `get_unenriched_issuer_ids(limit: Option<i64>)`** in `db.rs` -- queries `SELECT issuer_id FROM issuers WHERE enriched_at IS NULL ORDER BY issuer_id [LIMIT n]`.

4. **Schema:** `issuer_performance` and `issuer_eod_prices` tables already defined in `sqlite.sql` with proper indexes and foreign keys. The `issuers.enriched_at` column and `idx_issuers_enriched` index already exist.

5. **Types:** `IssuerDetail`, `Performance`, `EodPrice`, `Stats` in `capitoltrades_api/src/types/issuer.rs`. `DbIssuerDetail`, `DbPerformance`, `DbEodValue`, `DbIssuerStats` in `db.rs` (private deserialization types for upsert).

6. **Issuers command:** `commands/issuers.rs` already handles single-issuer lookup by `--id`, which calls `issuer_detail()` and converts to `IssuerDetail` via `scraped_issuer_detail_to_detail()` with `normalize_performance()`.

### What Needs to Be Built

```
capitoltraders_lib/src/
  scrape.rs        # No changes needed (issuer_detail already works)
  db.rs            # Add: update_issuer_detail(), mark_issuers_enriched(),
                   #       count_unenriched_issuers(), query_issuers(),
                   #       DbIssuerRow, DbIssuerFilter

capitoltraders_lib/tests/fixtures/
  issuer_detail_with_performance.html    # Synthetic HTML fixture
  issuer_detail_no_performance.html      # Synthetic HTML fixture (no perf data)

capitoltraders_cli/src/
  commands/sync.rs       # Add enrich_issuers() step, wire into run()
  commands/issuers.rs    # Add --db flag, run_db()
  output.rs              # Add DbIssuerOutputRow, print_db_issuers_* functions
  xml_output.rs          # Add db_issuers_to_xml()

capitoltraders_lib/src/lib.rs  # Re-export DbIssuerRow, DbIssuerFilter
```

### Pattern 1: Issuer Detail to IssuerDetail Conversion

**What:** Convert a `ScrapedIssuerDetail` (with `performance: Option<serde_json::Value>`) into an `IssuerDetail` (vendored type with strongly-typed `Performance`) for use with `upsert_issuers()`.

**When to use:** The conversion path already exists in `commands/issuers.rs` as `scraped_issuer_detail_to_detail()` with `normalize_performance()`. This same conversion can be reused for the enrichment path.

**Key insight:** Rather than creating a new `update_issuer_detail()` method that duplicates the complex performance+EOD SQL, the enrichment pipeline should convert `ScrapedIssuerDetail` to `IssuerDetail` and call `upsert_issuers()`. This reuses all existing performance/EOD persistence logic.

**Caution:** `upsert_issuers()` takes `&mut self` (needs mutable Db) and does NOT set `enriched_at`. The enrichment flow needs to set `enriched_at` after upserting. A separate `mark_issuer_enriched(issuer_id)` call or a combined `update_issuer_detail()` method that does both is needed.

### Pattern 2: Issuer DB Query with Performance JOIN

**What:** Query issuers from SQLite with performance data joined in, following the Phase 3 DbTradeRow and Phase 4 DbPoliticianRow patterns.

**Example:**
```sql
SELECT i.issuer_id, i.issuer_name, i.issuer_ticker, i.sector,
       i.state_id, i.country, i.enriched_at,
       COALESCE(s.count_trades, 0) AS trades,
       COALESCE(s.count_politicians, 0) AS politicians,
       COALESCE(s.volume, 0) AS volume,
       s.date_last_traded,
       p.mcap, p.trailing1, p.trailing1_change,
       p.trailing7, p.trailing7_change,
       p.trailing30, p.trailing30_change,
       p.trailing90, p.trailing90_change,
       p.trailing365, p.trailing365_change
FROM issuers i
LEFT JOIN issuer_stats s ON i.issuer_id = s.issuer_id
LEFT JOIN issuer_performance p ON i.issuer_id = p.issuer_id
WHERE 1=1
GROUP BY i.issuer_id
ORDER BY COALESCE(s.volume, 0) DESC
```

**Note:** EOD prices are NOT included in the list query (too many rows per issuer). They should be available in single-issuer detail view or as a separate command.

### Pattern 3: Enrichment Pipeline (Matching Trade Enrichment)

**What:** Post-ingest loop over unenriched issuers, fetch detail page, persist, mark enriched.

**Why opt-in (--enrich), not automatic:** Unlike politician committee enrichment (48 requests total), issuer enrichment requires one HTTP request per issuer. A database could have hundreds or thousands of issuers. At 500ms throttle, 1000 issuers = ~8 minutes. This is too slow for automatic execution. Use the same `--enrich` flag pattern as trade enrichment.

**However:** The `--enrich` flag currently only enriches trades. It needs to be extended to also enrich issuers in the same run, or a separate flag (`--enrich-issuers`) is needed. The simplest approach is to have `--enrich` run both trade and issuer enrichment sequentially.

### Pattern 4: Fixture Creation (Synthetic)

**What:** Create synthetic HTML fixtures modeling the issuer detail RSC payload structure.

**Why synthetic:** Live issuer detail pages return loading states via curl (confirmed 2026-02-08). The RSC data is streamed client-side via JavaScript hydration.

**Structure to model:** The `"issuerData":{}` key contains a full `IssuerDetail`-shaped object. The fixture must include:
- `"issuerData":{"_issuerId":123,"_stateId":"ca","c2iq":"AAPL","country":"us","issuerName":"Apple Inc.","issuerTicker":"AAPL","performance":{...},"sector":"information-technology","stats":{...}}`
- The performance object with all 20 trailing/period fields plus `eodPrices` nested array
- A second fixture with `"performance":null` for issuers without market data

**Key concern:** Phase 4 showed synthetic fixtures can miss real-world bugs. If a real issuer detail HTML can be captured via browser devtools (Network tab, copy response), it should be used. The synthetic fixture is a fallback.

### Anti-Patterns to Avoid

- **Duplicating the performance/EOD SQL in a new update method:** The `upsert_issuers()` already handles the full complexity of performance and EOD persistence. Creating `update_issuer_detail()` with duplicate SQL is error-prone. Instead, convert to `IssuerDetail` and reuse `upsert_issuers()`, plus a separate `mark_issuer_enriched()` call.

- **Making issuer enrichment automatic (no flag):** Unlike politician committees (48 requests), issuer enrichment is O(n) in the number of issuers. It can be slow for large databases. Keep it opt-in via `--enrich`.

- **Including EOD prices in list-view output:** EOD price history could be hundreds of rows per issuer. Include it only for single-issuer `--id` lookups or as a separate subcommand. The list view should show summary performance metrics (mcap, trailing returns) but not the full price time series.

- **Modifying the vendored IssuerDetail struct:** Use a separate `DbIssuerRow` output type for the DB read path, matching the DbTradeRow/DbPoliticianRow pattern.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Performance/EOD persistence | New SQL for issuer_performance and issuer_eod_prices | Existing `upsert_issuers()` method | Already handles all 20 performance columns + EOD pairs with proper upsert semantics |
| Issuer detail scraping | New page parser | Existing `issuer_detail()` in scrape.rs | Already extracts `"issuerData":{}` from RSC payload |
| Performance normalization | New validation logic | Existing `normalize_performance()` in commands/issuers.rs | Already validates all required fields are non-null |
| Enrichment queue | Custom state tracking | Existing `enriched_at` column + `get_unenriched_issuer_ids()` | Same pattern as trades and politicians |
| Retry logic | New retry wrapper | Existing `ScrapeClient::with_retry` | Already handles 429, 5xx, timeouts |
| EOD date/price parsing | Custom parser | Existing `eod_pair()` helper in db.rs | Already handles the `Vec<Vec<DbEodValue>>` untagged enum structure |

**Key insight:** Phase 5 is largely an assembly exercise. The scraper, DB persistence, enrichment queue, and schema all exist. The new work is: (1) testing the extraction against fixtures, (2) wiring the enrichment into the sync command, (3) building the query/output layer for the `--db` path.

## Common Pitfalls

### Pitfall 1: Performance JSON Normalization Mismatch

**What goes wrong:** The `ScrapedIssuerDetail.performance` is `Option<serde_json::Value>` (raw JSON). Converting to `IssuerDetail.performance: Option<Performance>` (strongly typed) fails silently if field names don't match between RSC payload and the vendored types.

**Why it happens:** The RSC payload uses `camelCase` (e.g., `eodPrices`, `trailing1Change`). The vendored `Performance` struct expects `camelCase` via `#[serde(rename_all = "camelCase")]`. If the RSC payload uses different naming (e.g., `trailing_1` instead of `trailing1`), deserialization fails.

**How to avoid:** Use `normalize_performance()` as a guard -- it checks all 20 required fields are present and non-null before passing to serde. If normalization returns null, the issuer has no performance data (which is valid for non-public companies). Test the fixture with the full round-trip: RSC payload -> ScrapedIssuerDetail -> IssuerDetail -> db insert -> db query.

**Warning signs:** After enrichment, `issuer_performance` table is empty even though issuers were processed. Check `normalize_performance()` output.

### Pitfall 2: EOD Price Structure Assumptions

**What goes wrong:** EOD prices are stored as `Vec<Vec<EodPrice>>` where each inner vec is an untagged enum of either a float (price) or a NaiveDate (date). The `eod_pair()` helper extracts (date, price) tuples. If the actual RSC data has a different nesting (e.g., flat array, or objects with named fields), the extraction fails.

**Why it happens:** The vendored type models the BFF API response format. The RSC payload may use a different representation.

**How to avoid:** Build the fixture with the known BFF API structure first. If live testing reveals a different format, add an adapter. The `eod_pair()` function is straightforward and easy to modify.

**Warning signs:** After enrichment, `issuer_eod_prices` table is empty even though `issuer_performance` has data.

### Pitfall 3: upsert_issuers Requires &mut self

**What goes wrong:** `upsert_issuers(&mut self, ...)` takes a mutable reference because it uses `self.conn.transaction()`. The enrichment loop in sync.rs has a `&Db` (immutable borrow) because `get_unenriched_issuer_ids()` also borrows the DB.

**Why it happens:** The trade enrichment pattern uses `update_trade_detail(&self, ...)` with `unchecked_transaction()` to avoid the mutability issue. But `upsert_issuers` uses `transaction()` which requires `&mut self`.

**How to avoid:** Either: (a) create a dedicated `update_issuer_detail(&self, ...)` that uses `unchecked_transaction()` (like `update_trade_detail`), or (b) restructure the enrichment loop to collect results first, then do a single `upsert_issuers(&mut db, ...)` call with all enriched issuers. Option (a) is more consistent with the established pattern.

**Warning signs:** Compiler error about borrowing `db` as mutable while also borrowed as immutable.

### Pitfall 4: Missing enriched_at Set

**What goes wrong:** After calling `upsert_issuers()`, the issuer's `enriched_at` is still NULL because `upsert_issuers()` preserves existing `enriched_at` values (the ON CONFLICT clause has `enriched_at = issuers.enriched_at`).

**Why it happens:** `upsert_issuers()` is designed for bulk import where enrichment state should be preserved. It does not set `enriched_at` on the rows it inserts/updates.

**How to avoid:** After upserting enriched issuer data, explicitly set `enriched_at` with a separate UPDATE: `UPDATE issuers SET enriched_at = datetime('now') WHERE issuer_id = ?1`. Or create a combined `update_issuer_detail()` that does the upsert and sets `enriched_at` in one transaction.

**Warning signs:** Issuers show enriched performance data but `enriched_at` is still NULL. Re-running enrichment re-fetches already-enriched issuers.

### Pitfall 5: Large Number of Issuers

**What goes wrong:** The database has thousands of issuers (every company mentioned in any politician's trade). Enriching all of them takes a very long time.

**Why it happens:** Unlike politicians (~500 in Congress), issuers can number in the thousands. Each requires a separate HTTP request.

**How to avoid:** Use `--batch-size` to limit per-run enrichment. Use the same progress reporting pattern as trade enrichment (every 50 items). Consider prioritizing issuers with more trades (order by trade count instead of ID).

**Warning signs:** Enrichment runs for hours. Users interrupt and lose progress (but checkpointing via `enriched_at` means restart picks up where it left off).

### Pitfall 6: Issuers Without Performance Data

**What goes wrong:** Some issuers are private companies, trusts, or funds without public market data. Their performance is null. The output formatting code crashes or shows ugly "null" values.

**Why it happens:** Not all issuers are publicly traded. The `performance` field on `IssuerDetail` is `Option<Performance>`.

**How to avoid:** Output formatting must handle None performance gracefully. Show "-" or empty values for private issuers. The `normalize_performance()` function already returns null for incomplete data, so the typed `Option<Performance>` will be None for these.

**Warning signs:** Table output shows "null" or crashes on issuers without performance data.

## Code Examples

### Example 1: update_issuer_detail (Recommended Approach)

```rust
// Source: Pattern from update_trade_detail in db.rs
// Uses unchecked_transaction for &self (established pattern)
pub fn update_issuer_detail(
    &self,
    issuer_id: i64,
    detail: &ScrapedIssuerDetail,
) -> Result<(), DbError> {
    let tx = self.conn.unchecked_transaction()?;

    // 1. Update issuers base row + enriched_at
    tx.execute(
        "UPDATE issuers SET
           state_id = COALESCE(?1, state_id),
           c2iq = COALESCE(?2, c2iq),
           country = COALESCE(?3, country),
           issuer_name = ?4,
           issuer_ticker = COALESCE(?5, issuer_ticker),
           sector = COALESCE(?6, sector),
           enriched_at = ?7
         WHERE issuer_id = ?8",
        params![
            detail.state_id,
            detail.c2iq,
            detail.country,
            detail.issuer_name,
            detail.issuer_ticker,
            detail.sector,
            chrono::Utc::now().to_rfc3339(),
            issuer_id,
        ],
    )?;

    // 2. Update issuer_stats
    tx.execute(
        "INSERT INTO issuer_stats (issuer_id, count_trades, count_politicians, volume, date_last_traded)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(issuer_id) DO UPDATE SET
           count_trades = excluded.count_trades,
           count_politicians = excluded.count_politicians,
           volume = excluded.volume,
           date_last_traded = excluded.date_last_traded",
        params![
            issuer_id,
            detail.stats.count_trades,
            detail.stats.count_politicians,
            detail.stats.volume,
            detail.stats.date_last_traded,
        ],
    )?;

    // 3. Handle performance data (if present)
    // ... parse performance JSON, upsert issuer_performance, upsert issuer_eod_prices ...

    tx.commit()?;
    Ok(())
}
```

### Example 2: Enrichment Pipeline (in sync.rs)

```rust
// Source: Pattern from enrich_trades in sync.rs
async fn enrich_issuers(
    scraper: &ScrapeClient,
    db: &Db,
    batch_size: Option<i64>,
    detail_delay_ms: u64,
    dry_run: bool,
) -> Result<EnrichmentResult> {
    if dry_run {
        let total = db.count_unenriched_issuers()?;
        let selected = match batch_size {
            Some(n) => n.min(total),
            None => total,
        };
        eprintln!("{} issuers would be enriched ({} selected)", total, selected);
        return Ok(EnrichmentResult { enriched: 0, skipped: 0, failed: 0, total: total as usize });
    }

    let queue = db.get_unenriched_issuer_ids(batch_size)?;
    if queue.is_empty() {
        eprintln!("No issuers need enrichment");
        return Ok(EnrichmentResult { enriched: 0, skipped: 0, failed: 0, total: 0 });
    }

    let total = queue.len();
    eprintln!("Enriching {} issuers...", total);
    let mut enriched = 0usize;
    let mut failed = 0usize;

    for (i, issuer_id) in queue.iter().enumerate() {
        match scraper.issuer_detail(*issuer_id).await {
            Ok(detail) => {
                db.update_issuer_detail(*issuer_id, &detail)?;
                enriched += 1;
            }
            Err(err) => {
                eprintln!("  Warning: issuer {} failed: {}", issuer_id, err);
                failed += 1;
            }
        }
        if detail_delay_ms > 0 && i + 1 < total {
            sleep(Duration::from_millis(detail_delay_ms)).await;
        }
        if (i + 1) % 50 == 0 || i + 1 == total {
            eprintln!("  Progress: {}/{} ({} enriched, {} failed)", i + 1, total, enriched, failed);
        }
    }

    Ok(EnrichmentResult { enriched, skipped: 0, failed, total })
}
```

### Example 3: DbIssuerRow for Output

```rust
// Source: Pattern from DbTradeRow and DbPoliticianRow in db.rs
#[derive(Debug, Clone, Serialize)]
pub struct DbIssuerRow {
    pub issuer_id: i64,
    pub issuer_name: String,
    pub issuer_ticker: Option<String>,
    pub sector: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub trades: i64,
    pub politicians: i64,
    pub volume: i64,
    pub last_traded: Option<String>,
    pub mcap: Option<i64>,
    pub trailing1: Option<f64>,
    pub trailing1_change: Option<f64>,
    pub trailing7: Option<f64>,
    pub trailing7_change: Option<f64>,
    pub trailing30: Option<f64>,
    pub trailing30_change: Option<f64>,
    pub trailing365: Option<f64>,
    pub trailing365_change: Option<f64>,
    pub enriched_at: Option<String>,
}
```

### Example 4: DbIssuerOutputRow for CLI

```rust
// Source: Pattern from DbPoliticianOutputRow in output.rs
#[derive(Tabled, Serialize)]
struct DbIssuerOutputRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Sector")]
    sector: String,
    #[tabled(rename = "Mcap")]
    mcap: String,
    #[tabled(rename = "Trailing 1D")]
    trailing1: String,
    #[tabled(rename = "Trailing 30D")]
    trailing30: String,
    #[tabled(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Volume")]
    volume: String,
    #[tabled(rename = "Last Traded")]
    last_traded: String,
}
```

### Example 5: Synthetic HTML Fixture

```html
<!-- Source: Model from BFF API IssuerDetail type + existing trade detail fixtures -->
<html>
<body>
<script>self.__next_f.push([1,"... RSC header data ..."])</script>
<script>self.__next_f.push([1,"... \"issuerData\":{\"_issuerId\":12345,\"_stateId\":\"ca\",\"c2iq\":\"AAPL\",\"country\":\"us\",\"issuerName\":\"Apple Inc.\",\"issuerTicker\":\"AAPL\",\"performance\":{\"eodPrices\":[[\"2026-01-15\",225.5],[\"2026-01-16\",227.3]],\"mcap\":3500000000000,\"trailing1\":227.3,\"trailing1Change\":1.8,\"trailing7\":225.0,\"trailing7Change\":-0.5,\"trailing30\":220.0,\"trailing30Change\":-2.1,\"trailing90\":215.0,\"trailing90Change\":-4.5,\"trailing365\":185.0,\"trailing365Change\":-18.5,\"wtd\":226.0,\"wtdChange\":1.0,\"mtd\":222.0,\"mtdChange\":-1.5,\"qtd\":218.0,\"qtdChange\":-3.0,\"ytd\":210.0,\"ytdChange\":-5.0},\"sector\":\"information-technology\",\"stats\":{\"countTrades\":450,\"countPoliticians\":85,\"volume\":25000000,\"dateLastTraded\":\"2026-01-10\"}} ..."])</script>
</body>
</html>
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| BFF API issuer detail | RSC payload scraping via issuer_detail() | Phase 5 (BFF 503) | Must scrape individual issuer pages |
| No DB read path for issuers | --db flag on issuers command (this work) | Phase 5 | Enriched performance data visible in output |
| IssuerRow without performance | DbIssuerOutputRow with mcap + trailing returns | Phase 5 | OUT-03 fulfilled |
| Issuers only shown with trade stats | Issuers shown with performance metrics and EOD history | Phase 5 | Users can evaluate issuer market performance alongside trading activity |

**Deprecated/outdated:**
- BFF API (`bff.capitoltrades.com`): Returns 503. All issuer data must come from RSC payloads on the detail pages.

## Key Architectural Decisions for Planner

### Decision 1: Enrichment Approach

**Decision:** Per-issuer detail page scraping, matching the trade enrichment pattern. Use `--enrich` flag (opt-in), not automatic.

**Rationale:** Each issuer requires a separate HTTP request to `/issuers/{id}`. The number of issuers can be large (hundreds to thousands). This is the same scale concern as trade enrichment. The existing `--enrich` flag and `--batch-size`/`--dry-run` controls should extend to cover issuers.

**Impact:** `--enrich` flag on sync command now triggers both trade AND issuer enrichment. Order: trades first, then issuers.

### Decision 2: update_issuer_detail vs. upsert_issuers Reuse

**Decision:** Create a dedicated `update_issuer_detail(&self, issuer_id, detail)` method that uses `unchecked_transaction()` and sets `enriched_at`.

**Rationale:** `upsert_issuers(&mut self, ...)` requires mutable access (uses `transaction()`) and does NOT set `enriched_at`. The enrichment loop needs `&self` (immutable) for interleaving with `get_unenriched_issuer_ids()`. A new method following the `update_trade_detail` pattern (using `unchecked_transaction()`) is more consistent. It also allows targeted updates without the full upsert overhead.

**Tradeoff:** Some SQL duplication between `upsert_issuers` and `update_issuer_detail`, particularly for the performance and EOD persistence. The performance SQL is complex (20 columns), so the duplication is non-trivial. An alternative is to factor out the performance/EOD SQL into helper methods called by both.

### Decision 3: EOD Prices in Output

**Decision:** Include summary performance metrics (mcap, key trailing returns) in the list-view output. Exclude full EOD price history from list view.

**Rationale:** EOD price history could be hundreds of rows per issuer. Including it in list output (which may show dozens of issuers) would be overwhelming. The list view shows a summary; the single-issuer `--id` lookup can show full EOD history if needed (this already works in the current scrape path since `IssuerDetail` includes `Performance.eod_prices`).

### Decision 4: DbIssuerOutputRow Column Selection

**Decision:** Show: Name, Ticker, Sector, Mcap, key trailing returns (1D, 30D or similar subset), Trades, Volume, Last Traded. Not all 20 performance fields.

**Rationale:** A table with 20+ columns is unusable in terminal. The JSON output can include all fields. The table/CSV/markdown/XML should include a curated subset. The exact columns can be decided during implementation.

### Decision 5: Fixture Strategy

**Decision:** Use synthetic fixtures first. If a real fixture can be captured via browser devtools, use it instead or in addition.

**Rationale:** Live site returns loading states via curl (confirmed). Synthetic fixtures are the only option without browser automation. Phase 2 used synthetic fixtures successfully for trade detail. Phase 4 showed real fixtures catch more bugs. A hybrid approach (synthetic for CI, real for validation) is ideal.

## Open Questions

1. **Should `--enrich` trigger issuer enrichment in addition to trade enrichment, or should there be a separate `--enrich-issuers` flag?**
   - What we know: Currently `--enrich` triggers only trade enrichment. Issuer enrichment is a similar but separate operation.
   - What is unclear: Whether users want to control trade vs issuer enrichment independently or always run them together.
   - Recommendation: Extend `--enrich` to run both trade and issuer enrichment (trades first, then issuers). If separate control is needed later, add `--enrich-trades-only` and `--enrich-issuers-only` flags. Keep it simple for now.

2. **Should `update_issuer_detail()` handle the full performance/EOD SQL, or should it convert to `IssuerDetail` and delegate to `upsert_issuers()`?**
   - What we know: `upsert_issuers()` already has the complex SQL for performance (20 columns) and EOD prices. But it requires `&mut self`.
   - What is unclear: Whether the SQL duplication is worth the architectural consistency.
   - Recommendation: Create `update_issuer_detail(&self, ...)` with `unchecked_transaction()` that includes the performance/EOD SQL. The duplication is acceptable because (a) it follows the established `update_trade_detail` pattern, and (b) it avoids the `&self` vs `&mut self` borrowing issues in the enrichment loop.

3. **Should the enrichment order be by issuer_id or by trade volume (highest-traded issuers first)?**
   - What we know: Current `get_unenriched_issuer_ids()` orders by `issuer_id`. Trade enrichment orders by `tx_id`.
   - What is unclear: Whether users benefit from seeing high-volume issuers enriched first.
   - Recommendation: Keep `ORDER BY issuer_id` for consistency. If users want volume-priority, add it as a future enhancement.

4. **What columns should the issuer `--db` table output show?**
   - What we know: The current (non-DB) issuer table shows Name, Ticker, Trades, Politicians, Volume, Last Traded (6 columns).
   - What is unclear: Which performance fields are most useful in a terminal table.
   - Recommendation: Add Sector, Mcap, and one or two trailing return columns (e.g., Trailing 30D, Trailing 365D). Keep the total under 10 columns. JSON/CSV/XML can include all fields.

## Implementation Plan Recommendations

Based on the research, Phase 5 should be organized into 3 plans:

**Plan 05-01: Issuer detail extraction testing and DB persistence**
- Create synthetic HTML fixture(s) for issuer detail page with performance data
- Create fixture without performance data (private company)
- Add fixture-based tests for `issuer_detail()` extraction via `extract_rsc_payload` + `extract_json_object_after`
- Add `update_issuer_detail(&self, issuer_id, detail)` method to Db
- Add `count_unenriched_issuers()` and `mark_issuer_enriched()` methods to Db
- Tests: fixture extraction, DB persistence of performance + EOD, enriched_at marking, round-trip verification

**Plan 05-02: Sync pipeline integration for issuer enrichment**
- Add `enrich_issuers()` async function to sync.rs
- Wire into `sync::run()` after trade enrichment when `--enrich` flag is set
- Uses existing `--batch-size`, `--dry-run`, `--details-delay-ms` controls
- Progress reporting matching trade enrichment pattern
- Tests: integration tests for enrichment flow

**Plan 05-03: CLI issuers --db output with performance data (OUT-03)**
- Add `DbIssuerRow` and `DbIssuerFilter` to db.rs
- Add `query_issuers()` method with LEFT JOIN issuer_stats and issuer_performance
- Add `--db` flag to issuers command
- Add `DbIssuerOutputRow` to output.rs with performance columns
- Add `print_db_issuers_*` functions for all 5 formats
- Add `db_issuers_to_xml()` to xml_output.rs
- Re-export `DbIssuerRow` and `DbIssuerFilter` from lib.rs
- Tests: DB query tests, output format tests for performance data

## Sources

### Primary (HIGH confidence)
- Direct analysis of `capitoltraders_lib/src/scrape.rs` -- `issuer_detail()`, `ScrapedIssuerDetail`, `extract_json_object_after`
- Direct analysis of `capitoltraders_lib/src/db.rs` -- `upsert_issuers()`, `get_unenriched_issuer_ids()`, `DbIssuerDetail`, `DbPerformance`, `DbEodValue`, `eod_pair()`
- Direct analysis of `capitoltrades_api/src/types/issuer.rs` -- `IssuerDetail`, `Performance`, `EodPrice`, `Stats`
- Direct analysis of `capitoltraders_cli/src/commands/issuers.rs` -- `scraped_issuer_detail_to_detail()`, `normalize_performance()`
- Direct analysis of `capitoltraders_cli/src/commands/sync.rs` -- `enrich_trades()` pattern, `enrich_politician_committees()` pattern
- Direct analysis of `capitoltraders_cli/src/output.rs` -- `IssuerRow`, `build_issuer_rows()`, all print_issuers_* functions
- Direct analysis of `schema/sqlite.sql` -- `issuer_performance`, `issuer_eod_prices` table definitions
- Phase 4 research and implementation -- established patterns for enrichment pipeline, DB output, --db flag
- Live site verification: `https://www.capitoltrades.com/issuers/1` -- confirmed RSC payload returns loading state via curl (2026-02-08)

### Secondary (MEDIUM confidence)
- Upstream `capitoltrades_api` types match what the RSC payload should contain (based on Phase 2/3 experience where RSC payloads matched BFF API types)
- `normalize_performance()` validation logic correctly identifies incomplete performance data (based on code analysis, not live testing)

### Tertiary (LOW confidence)
- Assumption that issuer detail RSC payload structure matches the vendored `IssuerDetail` type -- confirmed for trades in Phase 2, but NOT specifically tested for issuers. The RSC format could differ (e.g., different field names, different nesting for eodPrices).
- EOD price format assumption: `[[date, price], [date, price], ...]` based on the vendored type's `Vec<Vec<EodPrice>>`. The actual RSC payload may use a different structure.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- zero new dependencies, all patterns established in Phases 1-4
- Architecture (enrichment pipeline): HIGH -- identical to trade enrichment pattern, all components exist
- Architecture (DB persistence): HIGH -- upsert_issuers() already handles full complexity; update_issuer_detail() is a targeted variant
- Architecture (DB output): HIGH -- follows DbTradeRow/DbPoliticianRow pattern from Phases 3/4
- Fixture accuracy: LOW -- synthetic fixtures only; live site confirmed to return loading states; actual RSC payload structure for issuers is unconfirmed
- EOD price format: LOW -- based on vendored type inference, not live data verification

**Research date:** 2026-02-08
**Valid until:** 2026-03-08 (stable domain; issuer page structure may evolve)
