# Phase 4: Politician Enrichment - Research

**Researched:** 2026-02-08
**Domain:** Politician detail scraping, committee membership extraction, sync pipeline extension, CLI output
**Confidence:** MEDIUM (committee data requires alternative approach; core enrichment pipeline is well-understood)

## Summary

Phase 4 extends the enrichment pipeline from Phase 3 to politician records. The primary goal is populating `politician_committees` with committee membership data and displaying it in all CLI output formats. However, the critical POL-01 risk has been confirmed: **politician detail pages do NOT contain committee membership data in their RSC payload**. The `"politician":{}` object embedded in the detail page RSC payload contains only the basic summary fields (firstName, lastName, party, chamber, stateId, dob, gender, nickname) -- the same fields already captured during trade listing ingest. Committee data, social media links, website, and district information are rendered as separate UI elements on the page but are not part of a parseable JSON politician object.

The BFF API (`bff.capitoltrades.com/politicians/{id}`) previously returned a `PoliticianDetail` struct with a `committees: Vec<String>` field, but the BFF API now returns 503 and is confirmed unusable.

**Alternative approach for committee data:** The politician listing page (`/politicians?committee={code}`) supports server-side filtering by committee code. By iterating through all 48 known committee codes and scraping the resulting politician lists, we can build a reverse mapping of politician-to-committee memberships. This requires 48 requests (one per committee, most fit on a single page) rather than one request per politician. This is more efficient than per-politician enrichment for committees and produces a complete mapping.

**Primary recommendation:** Split into 3 plans: (1) Committee membership scraping via committee-filter iteration on the listing page, with `update_politician_committees()` persistence and `count_unenriched_politicians()` support; (2) Politician enrichment wired into the sync command as automatic post-ingest step (POL-03: no opt-in flag); (3) Extended politician CLI output with committee data from DB (OUT-02: `--db` flag on politicians command, or augmented live-scrape path).

## Standard Stack

### Core (no new dependencies)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.31 | politician_committees CRUD, query_politicians | Already used for all DB operations |
| chrono | 0.4 | enriched_at timestamps | Already used throughout |
| tokio | 1.x | async sleep for throttle delay | Already used for all async |
| clap | 4.x | No new CLI flags on sync (POL-03 says automatic) | Already used for CLI |
| serde_json | 1.x | Politician output serialization | Already used throughout |
| tabled | 0.17 | Extended PoliticianRow for table/markdown | Already used for CLI output |
| csv | 1.3 | Extended PoliticianRow for CSV | Already used for CLI output |
| quick-xml | 0.37 | Already handles committees via JSON bridge | Already used for XML output |
| regex | 1.x | Politician card parsing (already used in scrape.rs) | Already used |

### Supporting (no additions needed)

Phase 4 requires zero new crate dependencies. All work extends existing modules.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Committee-filter iteration on listing page | Per-politician detail page scraping for committees | Detail pages do NOT contain committee data; listing filter is the only source |
| Automatic enrichment (no flag) | --enrich-politicians opt-in flag | POL-03 requires automatic; committee scrape is fast (48 requests), not slow like per-trade enrichment |
| --db flag on politicians command | Always merge DB data into live-scrape output | --db flag is cleaner, follows Phase 3 pattern; live path stays unmodified |
| Committee scraping during sync | Separate `capitoltraders committees` command | Sync integration is required by POL-03; committee data is a dependency of politician output |

## Architecture Patterns

### Critical Finding: Committee Data Source

The politician detail page RSC payload contains a `"politician":{}` object with only these fields:
```json
{
  "_stateId": "ca",
  "chamber": "house",
  "dob": "1940-03-26",
  "firstName": "Nancy",
  "gender": "female",
  "lastName": "Pelosi",
  "nickname": null,
  "party": "democrat"
}
```

This is the same `Politician` summary struct already captured during trade listing ingestion. There is **no** `committees`, `socialFacebook`, `socialTwitter`, `website`, `district`, `middleName`, `fullName`, or `partyOther` field in this object.

The BFF API (which DID include `committees: Vec<String>` in `PoliticianDetail`) returns HTTP 503 and is not available.

**Viable approach:** The politician listing page supports committee filtering:
- URL: `https://www.capitoltrades.com/politicians?committee=ssfi`
- Returns all politicians matching that committee
- Each politician card includes: politician_id, name, party, state, trades count, issuers count, volume, last_traded date
- 48 committee codes are known (in `validation.rs` COMMITTEE_MAP)
- Most committees have 5-30 members, fitting on 1-2 pages

By scraping `/politicians?committee={code}` for each of the 48 codes, we can build a complete `politician_id -> Vec<committee_code>` mapping.

### Recommended Architecture

```
Phase A: Trade/Politician/Issuer listing ingest (existing sync flow, unchanged)

Phase B: Committee membership sync (NEW, automatic, no flag needed)
    for each committee_code in COMMITTEE_MAP:
        page = scraper.politicians_page_with_committee(committee_code)
        for politician_card in page.data:
            db.insert_politician_committee(politician_card.politician_id, committee_code)
        sleep(throttle_delay)
    db.mark_politicians_committee_enriched()

Phase C: (future, optional) Per-politician detail enrichment for social/website/district
    This is NOT needed for Phase 4 requirements.
    POL-01 asks for committee memberships only.
```

### Recommended Project Structure

```
capitoltraders_lib/src/
  scrape.rs        # Add politicians_page_with_committee() method
  db.rs            # Add update_politician_committees(), count/get_unenriched methods, query_politicians()

capitoltraders_cli/src/
  commands/sync.rs # Add automatic committee enrichment step after trade sync
  commands/politicians.rs # Add --db flag with committee-aware output
  output.rs        # Extend PoliticianRow with committees column
```

### Pattern 1: Committee-Filter Iteration

**What:** Scrape the politician listing page once per committee code to build the reverse mapping.

**When to use:** When the data model supports server-side filtering but does not expose the filter values on individual records.

**Example:**
```rust
// Source: Pattern derived from existing politicians_page + COMMITTEE_MAP
async fn scrape_committee_memberships(
    scraper: &ScrapeClient,
    db: &Db,
    throttle_ms: u64,
) -> Result<usize> {
    let mut total = 0;
    for &(code, name) in validation::COMMITTEE_MAP {
        let resp = scraper.politicians_page_with_committee(code, 1).await?;
        let politician_ids: Vec<String> = resp.data.iter()
            .map(|card| card.politician_id.clone())
            .collect();
        db.set_politician_committees(code, &politician_ids)?;
        total += politician_ids.len();
        eprintln!("  {} ({}): {} members", name, code, politician_ids.len());
        if throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
        }
        // Handle pagination if needed (most committees fit on 1 page)
        // ... additional pages ...
    }
    Ok(total)
}
```

### Pattern 2: Politician DB Query with Committee JOIN

**What:** Query politicians from SQLite with committee data joined in, following the Phase 3 DbTradeRow pattern.

**Example:**
```sql
SELECT p.politician_id, p.first_name, p.last_name, p.party,
       p.state_id, p.chamber, p.gender, p.dob, p.enriched_at,
       ps.count_trades, ps.count_issuers, ps.volume, ps.date_last_traded,
       COALESCE(GROUP_CONCAT(DISTINCT pc.committee), '') AS committees
FROM politicians p
LEFT JOIN politician_stats ps ON p.politician_id = ps.politician_id
LEFT JOIN politician_committees pc ON p.politician_id = pc.politician_id
GROUP BY p.politician_id
ORDER BY ps.volume DESC
```

### Pattern 3: Automatic Enrichment (POL-03)

**What:** Committee enrichment runs automatically during sync without requiring a flag.

**Why this is different from trade enrichment:** Trade enrichment requires one HTTP request per trade (potentially thousands of requests at 500ms each). Committee enrichment requires only ~48 requests (one per committee code), taking ~25 seconds at 500ms throttle. This is fast enough to run automatically.

**Implementation:** In `sync::run()`, after `sync_trades()` and any trade enrichment, always call `enrich_politician_committees()`. No --enrich flag needed. Use the same `--details-delay-ms` throttle.

### Pattern 4: ScrapeClient Extension

**What:** Add a method to ScrapeClient for fetching politician listing pages with committee filter.

**Current state:** `politicians_page()` takes only a page number. The URL format is `/politicians?page={page}`. Committee filtering adds `?committee={code}&page={page}`.

**Implementation:** Add `politicians_page_filtered(page: i64, committee: Option<&str>)` or a separate `politicians_by_committee(code: &str, page: i64)` method. The card parsing logic (`parse_politician_cards`) is reusable.

### Anti-Patterns to Avoid

- **Scraping individual politician detail pages for committee data:** Detail pages do NOT contain committee data. This was the original plan but is confirmed infeasible.

- **Making committee enrichment opt-in:** POL-03 says automatic. The request volume (48 requests) is small enough to always run.

- **Clearing all politician_committees before re-scraping:** Use upsert/replace semantics. If a committee scrape returns an empty result (network error, changed page structure), do not wipe existing data. Only update when we have positive results.

- **Modifying the vendored PoliticianDetail struct:** Use a separate DbPoliticianRow output type for the DB read path, as Phase 3 did with DbTradeRow. The PoliticianDetail type is from the vendored crate.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Committee-to-politician mapping | Custom API integration / Congress API | Listing page committee filter iteration | CapitolTrades already knows the mapping; just scrape by filter |
| Politician card parsing | New regex or DOM parser | Existing `parse_politician_cards()` in scrape.rs | Already handles the politician listing page RSC format |
| Enrichment queue tracking | Custom state table | `enriched_at` column on politicians table | Same pattern as Phase 1-3; already has indexes |
| Retry logic | New retry wrapper | Existing `ScrapeClient::with_retry` | Already handles 429, 5xx, timeouts |
| Committee name/code mapping | Hardcoded lookup in enrichment | Existing `COMMITTEE_MAP` in validation.rs | Already maintained with 48 entries |

**Key insight:** The committee data problem has a different shape than trade enrichment. Instead of "fetch detail page for each record," it is "fetch listing page for each filter value." The codebase already has the politician card parser and the committee code mapping. The new work is orchestrating these existing pieces.

## Common Pitfalls

### Pitfall 1: Assuming Politician Detail Pages Have Committee Data

**What goes wrong:** The developer implements `politician_detail()` enrichment expecting a `committees` field in the RSC payload, then gets empty results for every politician.

**Why it happens:** The BFF API type `PoliticianDetail` has a `committees: Vec<String>` field, and the vendored crate defines it. The developer assumes the RSC payload matches this struct.

**How to avoid:** The RSC payload's `"politician":{}` object only has basic summary fields. Committee data must come from the listing page committee filter approach.

**Warning signs:** `politician_detail()` returns a `ScrapedPolitician` with no committee field. After enrichment, `politician_committees` table is empty for all politicians.

### Pitfall 2: Pagination in Committee-Filtered Listing Pages

**What goes wrong:** A committee with 30+ members spans multiple pages, but only page 1 is scraped. Some members are missed.

**Why it happens:** Most committees have <20 members and fit on one page (12 per page). But larger committees like House Appropriations could have 30+.

**How to avoid:** Check `total_pages` from the response. If >1, paginate through all pages for that committee. The existing `politicians_page()` pattern handles this.

**Warning signs:** Popular committees (Appropriations, Finance, Armed Services) show fewer members than expected.

### Pitfall 3: Committee Code vs Committee Name in politician_committees Table

**What goes wrong:** The `politician_committees` table stores the full committee name ("Senate - Finance") when the code ("ssfi") is what the scraper receives, or vice versa. Joins and filters break.

**Why it happens:** The scraper iterates by committee code, but the listing page may display the full name. There is ambiguity about which to store.

**How to avoid:** Store the committee CODE (e.g., "ssfi") in `politician_committees.committee`. This matches the filter parameter and validation module format. The full name can be looked up from `COMMITTEE_MAP` at display time.

**Warning signs:** CLI committee filter (`--committee ssfi`) does not match stored values.

### Pitfall 4: Stale Committee Data After Politician Turnover

**What goes wrong:** A politician leaves office or changes committees, but their old committee memberships persist in the DB.

**Why it happens:** The committee scrape only adds memberships, never removes them.

**How to avoid:** During committee enrichment, clear all `politician_committees` rows and rebuild from scratch. This is a "replace all" operation, not incremental. Since the total volume is small (48 committees x ~15 members = ~720 rows), replacing is cheap and correct.

**Warning signs:** Former committee members still show up in committee-filtered output.

### Pitfall 5: Confusing enriched_at Semantics for Politicians

**What goes wrong:** The developer marks politicians as enriched after committee sync, but there is nothing else to enrich on the detail page. Future phases might need a different enrichment signal.

**Why it happens:** For trades, `enriched_at` means "we fetched the detail page and got additional data." For politicians, committee data comes from the listing page, not the detail page.

**How to avoid:** Set `enriched_at` on politicians after committee data is populated. This is semantically "enrichment from any source is complete." If a future phase adds per-politician detail enrichment (social links, district), it would reset `enriched_at` to NULL for those politicians needing the new enrichment.

**Warning signs:** `get_unenriched_politician_ids()` returns 0 even though some politicians have no committee data.

### Pitfall 6: Race Between Trade Sync and Committee Sync

**What goes wrong:** Committee enrichment runs before trade sync has populated the politicians table. Foreign key violations occur when inserting into `politician_committees` for politicians not yet in the `politicians` table.

**Why it happens:** Committee filtering returns politician IDs that may not have been ingested yet (e.g., politicians with 0 trades who appear on committee lists but not in trade data).

**How to avoid:** Run committee enrichment AFTER trade sync. Use `INSERT OR IGNORE` for `politician_committees` entries where the politician_id does not exist in the `politicians` table, or pre-populate the politician with basic data from the committee scrape card.

**Warning signs:** Foreign key constraint errors during committee enrichment. Politicians who sit on committees but have no trades cause failures.

## Code Examples

### Example 1: ScrapeClient Committee-Filtered Politicians Page

```rust
// Source: Pattern derived from existing politicians_page()
pub async fn politicians_by_committee(
    &self,
    committee_code: &str,
    page: i64,
) -> Result<ScrapePage<ScrapedPoliticianCard>, ScrapeError> {
    let url = format!(
        "{}/politicians?committee={}&page={}",
        self.base_url, committee_code, page
    );
    let html = self.fetch_html(&url).await?;
    let payload = extract_rsc_payload(&html)?;
    let total_count = extract_number(&payload, "\"totalCount\":");

    let cards = parse_politician_cards(&payload)?;
    let total_pages = total_count.and_then(|count| {
        let page_size = cards.len() as i64;
        if page_size > 0 {
            Some((count + page_size - 1) / page_size)
        } else {
            None
        }
    });

    Ok(ScrapePage {
        data: cards,
        total_pages,
        total_count,
    })
}
```

### Example 2: DB Committee Persistence

```rust
// Source: Pattern derived from existing update_trade_detail committee handling
pub fn replace_all_politician_committees(
    &self,
    memberships: &[(String, String)],  // (politician_id, committee_code)
) -> Result<(), DbError> {
    let tx = self.conn.unchecked_transaction()?;

    // Clear all existing committee memberships
    tx.execute("DELETE FROM politician_committees", [])?;

    // Insert new memberships (skip unknown politicians)
    let mut stmt = tx.prepare(
        "INSERT OR IGNORE INTO politician_committees (politician_id, committee)
         SELECT ?1, ?2 WHERE EXISTS (
             SELECT 1 FROM politicians WHERE politician_id = ?1
         )"
    )?;

    for (pol_id, committee) in memberships {
        stmt.execute(params![pol_id, committee])?;
    }

    tx.commit()?;
    Ok(())
}
```

### Example 3: Committee Enrichment in Sync Pipeline

```rust
// Source: Pattern derived from existing enrich_trades in sync.rs
async fn enrich_politician_committees(
    scraper: &ScrapeClient,
    db: &Db,
    throttle_ms: u64,
) -> Result<usize> {
    eprintln!("Syncing politician committee memberships...");
    let mut memberships: Vec<(String, String)> = Vec::new();

    for &(code, name) in validation::COMMITTEE_MAP {
        let mut page = 1;
        loop {
            let resp = scraper.politicians_by_committee(code, page).await?;
            for card in &resp.data {
                memberships.push((card.politician_id.clone(), code.to_string()));
            }
            let total_pages = resp.total_pages.unwrap_or(1);
            if page >= total_pages {
                break;
            }
            page += 1;
            if throttle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
            }
        }
        if throttle_ms > 0 {
            tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
        }
    }

    let count = memberships.len();
    db.replace_all_politician_committees(&memberships)?;
    // Mark all politicians with committee data as enriched
    db.mark_politicians_enriched()?;
    eprintln!("  {} committee memberships across 48 committees", count);
    Ok(count)
}
```

### Example 4: DbPoliticianRow for Output

```rust
// Source: Pattern from Phase 3 DbTradeRow
#[derive(Debug, Serialize)]
pub struct DbPoliticianRow {
    pub politician_id: String,
    pub name: String,
    pub party: String,
    pub state: String,
    pub chamber: String,
    pub committees: Vec<String>,
    pub trades: i64,
    pub volume: i64,
    pub last_traded: Option<String>,
}
```

### Example 5: Extended PoliticianRow for Output

```rust
// Source: Pattern from existing PoliticianRow in output.rs
#[derive(Tabled, Serialize)]
struct DbPoliticianOutputRow {
    #[tabled(rename = "Name")]
    #[serde(rename = "Name")]
    name: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Chamber")]
    #[serde(rename = "Chamber")]
    chamber: String,
    #[tabled(rename = "Committees")]
    #[serde(rename = "Committees")]
    committees: String,
    #[tabled(rename = "Trades")]
    #[serde(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Volume")]
    #[serde(rename = "Volume")]
    volume: String,
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| BFF API PoliticianDetail.committees | RSC listing page committee filter iteration | Phase 4 (BFF API 503) | Must scrape 48 committee pages instead of per-politician API calls |
| politician_detail() for committee data | politicians_by_committee() listing page scrape | Phase 4 (detail pages lack committees) | Architectural shift from per-record to per-committee enrichment |
| No DB read path for politicians | --db flag on politicians command | Phase 4 (this work) | Enriched committee data visible in output |
| PoliticianRow without committees | PoliticianRow/DbPoliticianOutputRow with committees column | Phase 4 (this work) | OUT-02 fulfilled |
| Trade enrichment opt-in (--enrich) | Politician enrichment automatic (no flag) | Phase 4 (this work) | POL-03: committee sync is fast enough to always run |

**Deprecated/outdated:**
- BFF API (`bff.capitoltrades.com`): Returns 503. All politician data must come from RSC payloads.
- `politician_detail()` for committee extraction: Returns only summary politician data. Not useful for committee enrichment.

## Key Architectural Decisions for Planner

### Decision 1: Committee Data Source

**Decision:** Use committee-filter iteration on the listing page, NOT per-politician detail page scraping.

**Rationale:** Confirmed via live site testing that politician detail page RSC payloads do not contain committee data. The listing page's server-side committee filter is the only available source.

**Impact:** ~48 requests (one per committee code) instead of ~500+ requests (one per politician). Faster, more reliable, and produces a complete mapping.

### Decision 2: Automatic vs Opt-In Enrichment

**Decision:** Automatic (no flag). POL-03 requires this.

**Rationale:** Committee enrichment takes ~48 requests at 500ms = ~25 seconds. This is fast enough to always run during sync. Unlike trade enrichment (thousands of requests), the cost is negligible.

### Decision 3: Committee Storage Format

**Decision:** Store committee CODES (e.g., "ssfi") in `politician_committees.committee`, not full names.

**Rationale:** Codes match the filter parameter, validation module, and existing trade_committees pattern. Full names can be resolved from `COMMITTEE_MAP` at display time. Codes are shorter and less prone to formatting inconsistencies.

### Decision 4: Replace-All vs Incremental Committee Updates

**Decision:** Replace-all: clear `politician_committees` and rebuild from scratch each sync.

**Rationale:** Committee memberships change over time (politicians join/leave committees). Incremental updates would leave stale memberships. The total data volume is small (~720 rows for 48 committees x ~15 avg members). Full replacement is cheap and ensures correctness.

### Decision 5: Handling Politicians Not in DB

**Decision:** Use `INSERT OR IGNORE` with EXISTS subquery for politician_committees inserts. Politicians who appear on committee lists but have no trades (and thus no row in `politicians` table) are silently skipped.

**Rationale:** The politicians table is populated from trade data. Some committee members may not have disclosed trades. Foreign key constraints would fail for unknown politicians. Skipping them is correct -- they have no trade data to enrich.

### Decision 6: DB Read Path for Politicians

**Decision:** Add `--db` flag to politicians command, following the Phase 3 pattern. Create `DbPoliticianRow` struct and `query_politicians()` DB method.

**Rationale:** The live-scrape path uses the vendored `PoliticianDetail` type which includes a `committees: Vec<String>` field (always empty in current scrape path). The DB path uses a custom `DbPoliticianRow` that populates committees from the join table. This is the same pattern as Phase 3's `DbTradeRow`.

## Open Questions

1. **Should committee enrichment handle pagination for large committees?**
   - What we know: The listing page shows 12 politicians per page. Most committees have fewer than 24 members (2 pages).
   - What is unclear: Whether any committee has 24+ members with trades on CapitolTrades.
   - Recommendation: Implement pagination support (check total_pages, loop if >1). The cost is negligible and ensures completeness. Likely only 1-2 extra requests total across all committees.

2. **Should the live-scrape politicians path show committee data too?**
   - What we know: The current `politicians` command scrapes the listing page and fetches detail pages per-politician. Neither source has committee data.
   - What is unclear: Whether users expect committee data in live-scrape mode.
   - Recommendation: Do NOT attempt to show committees in live-scrape mode. Committees require a separate scrape pass (48 requests). The `--db` path shows committees. Document this in help text: "Use --db for enriched committee data."

3. **What about additional politician detail fields (social links, district, website)?**
   - What we know: These appear in the detail page sidebar as rendered elements. They may be extractable from the RSC payload with additional parsing, but are NOT in the `"politician":{}` JSON object.
   - What is unclear: Whether they are in a separate data structure in the RSC payload (e.g., a page props object) or only in rendered HTML.
   - Recommendation: Out of scope for Phase 4. POL-01 asks only for committee memberships. Social/district enrichment can be a future enhancement (v2) if needed.

4. **parse_politician_cards may fail on some committee-filtered pages**
   - What we know: The existing `parse_politician_cards` regex is complex and tightly coupled to the current page structure. Committee-filtered pages may have slightly different formatting.
   - What is unclear: Whether the card format is identical across filtered and unfiltered listing pages.
   - Recommendation: Test with a few committee-filtered pages early. The card structure should be identical (same UI component), but verify. If different, a simplified parser that only extracts politician_id from the cards is sufficient for committee mapping.

## Implementation Plan Recommendations

Based on the research, Phase 4 should be organized into 3 plans:

**Plan 04-01: Committee membership scraping and persistence**
- Add `politicians_by_committee(code, page)` method to ScrapeClient
- Add `replace_all_politician_committees()` method to Db
- Add `count_unenriched_politicians()` method to Db (parallel to trades)
- Add `mark_politicians_enriched()` method to Db
- Create synthetic fixture for committee-filtered listing page
- Tests: fixture-based test for committee-filtered card parsing, DB persistence tests for replace_all_politician_committees

**Plan 04-02: Sync pipeline integration**
- Add `enrich_politician_committees()` async function to sync.rs
- Wire into `sync::run()` AFTER trade sync (and any trade enrichment)
- Runs automatically (POL-03: no opt-in flag)
- Uses existing `--details-delay-ms` for throttle
- Progress reporting: eprintln with committee name and member count
- Handle pagination for large committees
- Tests: integration tests with wiremock (or simple unit tests for the orchestration logic)

**Plan 04-03: CLI politician output with committee data (OUT-02)**
- Add `DbPoliticianRow` struct to db.rs
- Add `query_politicians()` method with LEFT JOIN politician_committees
- Add `--db` flag to politicians command
- Add `DbPoliticianOutputRow` to output.rs with committees column
- Add `print_db_politicians_*` functions for all 5 formats
- Re-export `DbPoliticianRow` from lib.rs
- Tests: DB query tests, output format tests for committee data

## Sources

### Primary (HIGH confidence)
- Live site verification: `https://www.capitoltrades.com/politicians/P000197` -- confirmed RSC payload politician object has NO committee data (fetched 2026-02-08)
- Live site verification: `https://www.capitoltrades.com/politicians/T000250` -- confirmed RSC payload politician object has NO committee data (fetched 2026-02-08)
- Live site verification: `https://www.capitoltrades.com/politicians?committee=ssfi` -- confirmed committee filter returns politician cards with IDs (5 results for Senate Finance, fetched 2026-02-08)
- Direct analysis of `capitoltraders_lib/src/scrape.rs` -- `politician_detail()`, `parse_politician_cards()`, `extract_politician_detail()`
- Direct analysis of `capitoltraders_lib/src/db.rs` -- `upsert_politicians()`, `get_unenriched_politician_ids()`, `update_trade_detail()` pattern
- Direct analysis of `capitoltraders_cli/src/commands/sync.rs` -- enrichment pipeline pattern
- Direct analysis of `capitoltraders_cli/src/commands/politicians.rs` -- current scrape flow, output routing
- Direct analysis of `capitoltraders_cli/src/output.rs` -- PoliticianRow struct, output pattern
- Direct analysis of `schema/sqlite.sql` -- politician_committees table definition
- Direct analysis of `capitoltraders_lib/src/validation.rs` -- COMMITTEE_MAP (48 entries)
- Upstream source: `https://raw.githubusercontent.com/TommasoAmici/capitoltrades/main/crates/capitoltrades_api/src/types/politician.rs` -- PoliticianDetail has `committees: Vec<String>` (BFF API format, no longer available)
- Phase 3 research and plans -- established patterns for enrichment pipeline, DB output, --db flag

### Secondary (MEDIUM confidence)
- BFF API `bff.capitoltrades.com/politicians/P000197` returns HTTP 503 (verified 2026-02-08) -- confirms BFF is unavailable as data source
- Committee-filtered listing page structure appears identical to unfiltered listing page (based on successful parse_politician_cards regex match against committee=ssfi results)

### Tertiary (LOW confidence)
- Assumption that all 48 committee codes in COMMITTEE_MAP produce valid results on capitoltrades.com -- some codes may be obsolete or return empty results. Should be handled gracefully.
- Assumption that committee-filtered pages have the same RSC payload structure as unfiltered pages -- needs verification during implementation with a real request.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- zero new dependencies, all patterns established in Phases 1-3
- Architecture (committee source): HIGH -- verified via live site that detail pages lack committee data; listing filter approach confirmed working
- Architecture (sync integration): HIGH -- follows established enrichment pipeline pattern from Phase 3
- Architecture (DB output): HIGH -- follows DbTradeRow pattern from Phase 3
- Committee filter iteration: MEDIUM -- verified for ssfi (Senate Finance), not tested for all 48 codes
- Pitfalls: HIGH -- derived from direct code analysis and live site verification

**Research date:** 2026-02-08
**Valid until:** 2026-03-08 (stable domain; committee codes may change slowly)
