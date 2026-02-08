# Phase 2: Trade Extraction - Research

**Researched:** 2026-02-08
**Domain:** RSC payload parsing, trade detail field extraction, HTML fixture testing
**Confidence:** MEDIUM-HIGH (payload structure must be verified against live site)

## Summary

Phase 2 extends the existing `trade_detail()` scraper to extract all missing fields from trade detail page RSC payloads. Currently, `extract_trade_detail()` in `scrape.rs` only extracts `filing_url` and `filing_id` from the trade detail page. The BFF API's `Trade` type (in `capitoltrades_api/src/types/trade.rs`) shows that each trade has `asset_type`, `size`, `size_range_high`, `size_range_low`, `price`, `committees`, `labels`, `has_capital_gains`, `filing_id`, and `filing_url` -- all of which are either hardcoded to defaults or left NULL when trades come from listing pages.

The listing page scraper (`trades_page()`) populates `ScrapedTrade` with a subset of fields: tx_id, politician, issuer, dates, value, tx_type, owner, chamber, price (sometimes NULL), comment, reporting_gap. It does NOT provide: asset_type (hardcoded to "unknown" in `upsert_scraped_trades`), size/size_range_high/size_range_low (set to NULL), committees (empty), labels (empty), has_capital_gains (set to 0). These fields exist in the BFF API response (as shown by the `trades.json` test fixture and the `Trade` struct), and are expected to be present in the trade detail page's RSC payload as well, since the website renders them when viewing individual trades.

The central risk for this phase is RSC payload structure uncertainty. The WebFetch tool could not extract the actual trade data from detail pages (receiving loading states), so the exact structure of the trade detail RSC payload for the extended fields must be discovered empirically during implementation. The existing `extract_trade_detail()` function already demonstrates that trade detail data IS accessible via the RSC payload (it successfully extracts `tradeId`, `filingUrl`, and `filingId`), which is strong evidence that the full trade object is present.

**Primary recommendation:** Extend `ScrapedTradeDetail` to include all enrichable fields, capture real HTML fixtures from trade detail pages for test coverage, and implement field extraction following the existing `extract_json_string` / `extract_json_object_after` patterns. Structure the work as: (1) capture fixtures and document payload structure, (2) extend the scraper and struct, (3) add Db update method and tests. TRADE-05 and TRADE-06 (committees/labels) are investigative -- document findings even if the data is not available.

## Standard Stack

### Core (already in workspace, no new dependencies)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde_json | 1 | Deserialize JSON fragments from RSC payload | Already used in `extract_trade_detail` |
| regex | 1 | Pattern matching within RSC payload text | Already used in `scrape.rs` for politician cards |
| rusqlite | 0.31 | Persist extracted fields to SQLite | Already used for all DB operations |
| chrono | 0.4 | Timestamp for `enriched_at` marking | Already used throughout |
| reqwest | 0.12 | HTTP fetching of detail pages | Already used via `ScrapeClient` |
| wiremock | 0.6 | Mock HTTP for integration tests | Already a dev-dependency |

### Supporting (no additions needed)

Phase 2 requires zero new crate dependencies. All work is extending existing RSC parsing functions and database update methods.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| String-search + serde_json for field extraction | Full JSON parse of entire RSC payload | Full parse would be cleaner but the RSC payload is not a single valid JSON document -- it is multiple fragments concatenated. The existing approach of finding JSON substrings by key needle and parsing just the relevant fragment is correct for this format. |
| Individual `extract_json_string` calls per field | Deserialize a single JSON object containing all fields | If the trade detail page embeds the full trade as a single JSON object (like the BFF API does), deserializing the whole object is cleaner. But this needs to be verified against the live payload. Recommend trying the object approach first and falling back to individual field extraction if the data is scattered. |

## Architecture Patterns

### Current State of `extract_trade_detail`

The existing function (scrape.rs line 468-485):

```rust
fn extract_trade_detail(payload: &str, trade_id: i64) -> ScrapedTradeDetail {
    let mut detail = ScrapedTradeDetail::default();
    let trade_needle = format!("\"tradeId\":{}", trade_id);
    // Searches for "tradeId":XXXX, then looks in a +/-500 char window
    // for "filingUrl":"..." and extracts filing_id from the URL
}
```

Key observations:
1. The function searches for `"tradeId":XXXX` (note: different key than listing pages which use `"_txId"`)
2. It uses a 500-char window around the match point
3. It currently only extracts `filingUrl` (and derives `filing_id` from the URL)
4. The function returns a `ScrapedTradeDetail` with only 2 optional fields

### Recommended Approach: Full Object Extraction

**Pattern A: Full JSON object extraction (preferred, try first)**

If the trade detail page embeds the complete trade as a JSON object (similar to how `issuer_detail` extracts `"issuerData":{ ... }`), we can find and deserialize the entire object:

```rust
fn extract_trade_detail(payload: &str, trade_id: i64) -> ScrapedTradeDetail {
    // Try to find the full trade object by locating "tradeId":XXXXX
    // and then finding the enclosing JSON object
    let trade_needle = format!("\"tradeId\":{}", trade_id);
    if let Some(pos) = payload.find(&trade_needle) {
        // Walk backwards from the match to find the opening '{'
        // Then use extract_json_object to get the complete object
        // Deserialize into ScrapedTradeDetail
    }
    // Fallback to individual field extraction
}
```

This approach is used successfully by `issuer_detail()` which calls `extract_json_object_after(payload, "\"issuerData\":")`.

**Pattern B: Individual field extraction (fallback)**

If the data is scattered across multiple RSC chunks rather than in a single object, use the existing `extract_json_string` pattern per field:

```rust
fn extract_trade_detail(payload: &str, trade_id: i64) -> ScrapedTradeDetail {
    let trade_needle = format!("\"tradeId\":{}", trade_id);
    // For each occurrence of tradeId, search nearby window for each field
    // This is the current approach, just extended to more fields
}
```

### Recommended ScrapedTradeDetail Extension

```rust
#[derive(Debug, Default)]
pub struct ScrapedTradeDetail {
    // Existing fields
    pub filing_url: Option<String>,
    pub filing_id: Option<i64>,
    // New fields for TRADE-01
    pub asset_type: Option<String>,
    // New fields for TRADE-02
    pub size: Option<i64>,
    pub size_range_high: Option<i64>,
    pub size_range_low: Option<i64>,
    // New fields for TRADE-03
    pub price: Option<f64>,
    // New fields for TRADE-05 (may be empty if not in payload)
    pub committees: Vec<String>,
    // New fields for TRADE-06 (may be empty if not in payload)
    pub labels: Vec<String>,
    // Additional enrichment fields
    pub has_capital_gains: Option<bool>,
}
```

### Recommended Db Method for Trade Detail Updates

Following the Phase 1 architecture pattern (Pattern 2: Granular Updates), add a targeted UPDATE method instead of going through the full upsert:

```rust
pub fn update_trade_detail(&self, tx_id: i64, detail: &ScrapedTradeDetail) -> Result<(), DbError> {
    // Update trade fields
    self.conn.execute(
        "UPDATE trades SET
           price = COALESCE(?1, price),
           size = COALESCE(?2, size),
           size_range_high = COALESCE(?3, size_range_high),
           size_range_low = COALESCE(?4, size_range_low),
           filing_id = CASE WHEN ?5 > 0 THEN ?5 ELSE filing_id END,
           filing_url = CASE WHEN ?6 != '' THEN ?6 ELSE filing_url END,
           has_capital_gains = CASE WHEN ?7 IS NOT NULL THEN ?7 ELSE has_capital_gains END,
           enriched_at = ?8
         WHERE tx_id = ?9",
        params![...],
    )?;

    // Update asset_type on the assets table
    if let Some(ref asset_type) = detail.asset_type {
        self.conn.execute(
            "UPDATE assets SET asset_type = CASE
               WHEN ?1 != 'unknown' THEN ?1
               ELSE asset_type
             END WHERE asset_id = ?2",
            params![asset_type, tx_id],
        )?;
    }

    // Update trade_committees (delete + insert pattern)
    // Update trade_labels (delete + insert pattern)
    Ok(())
}
```

**Important:** The `enriched_at` timestamp should be set by this method (not by the caller) to ensure consistency. Use `chrono::Utc::now().to_rfc3339()`.

### Recommended Project Structure

No new files needed. All changes go into existing modules:

```
capitoltraders_lib/src/
  scrape.rs        # Extend ScrapedTradeDetail, extend extract_trade_detail()
  db.rs            # Add update_trade_detail() method
tests/
  fixtures/        # NEW: directory for captured HTML fixtures
    trade_detail_stock.html      # Trade with stock asset type
    trade_detail_etf.html        # Trade with ETF asset type
    trade_detail_no_filing.html  # Trade without filing URL
```

### Test Strategy: HTML Fixture Capture

**Critical:** Before writing any extraction code, capture real HTML from trade detail pages. These fixtures serve as:
1. The source of truth for RSC payload structure
2. Regression tests against format changes
3. Documentation of what fields are actually available

**Fixture capture approach:**

```bash
# Capture a few representative trade detail pages
curl -s -o fixtures/trade_171585.html \
  -H 'accept: text/html' \
  -H 'user-agent: Mozilla/5.0' \
  'https://www.capitoltrades.com/trades/171585'
```

Then write unit tests that parse the fixture HTML using the existing `extract_rsc_payload` + new extraction logic:

```rust
#[test]
fn test_extract_trade_detail_from_fixture() {
    let html = include_str!("../../tests/fixtures/trade_detail_stock.html");
    let payload = extract_rsc_payload(html).unwrap();
    let detail = extract_trade_detail(&payload, 171585);
    assert_eq!(detail.asset_type, Some("stock".to_string()));
    assert!(detail.size.is_some());
    // ... verify all fields
}
```

### Anti-Patterns to Avoid

- **Modifying `ScrapedTrade` to add all enrichment fields:** The `ScrapedTrade` struct represents listing-page data. Enrichment data should go into `ScrapedTradeDetail`. These are different page types with different data availability.

- **Attempting to parse the full RSC payload as a single JSON document:** The RSC payload is multiple JSON fragments concatenated with RSC protocol markers. It is not valid JSON. Continue using the needle-search approach.

- **Widening the search window beyond necessity:** The current 500-char window in `extract_trade_detail` may be too small if the full trade object is large. If using the object extraction approach, walk to the enclosing braces rather than using a fixed window.

- **Assuming the trade detail page has the same data structure as the BFF API:** The BFF API returns `Trade` objects with full field sets. The RSC payload on the detail page may embed the same data differently. Verify with fixtures before coding.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON object boundary detection | Custom brace-matching parser | Existing `extract_json_object()` in scrape.rs | Already handles nested objects, string escaping, and depth tracking correctly |
| JSON string extraction | Manual character iteration | Existing `extract_json_string()` in scrape.rs | Already handles escape sequences |
| Filing ID extraction from URL | Custom URL parser | Existing `filing_id_from_url()` in scrape.rs | Handles trailing .pdf, query params, numeric detection |
| RSC payload extraction | New parser | Existing `extract_rsc_payload()` in scrape.rs | Already handles multi-chunk payloads, escape decoding |
| Sentinel-safe upsert | New upsert logic | Existing CASE/COALESCE patterns from Phase 1 | Already proven and tested for all sentinel types |

**Key insight:** The parsing infrastructure for RSC payloads is already mature and well-tested in scrape.rs. Phase 2 is about using these existing tools to extract more fields, not building new parsing infrastructure.

## Common Pitfalls

### Pitfall 1: Trade Detail Page RSC Payload May Use Different Keys Than Expected

**What goes wrong:** The developer assumes the trade detail page embeds a JSON object with the same field names as the BFF API (`size`, `sizeRangeHigh`, `committees`, etc.) but the RSC payload uses different keys, a different structure, or the data is split across multiple RSC chunks.

**Why it happens:** The existing code found `"tradeId"` on detail pages vs `"_txId"` on listing pages, proving that key names can differ between contexts. The RSC Flight format embeds React component props, which may be structured differently from raw API responses.

**How to avoid:** Capture real HTML fixtures FIRST. Examine the actual RSC payload text manually (search for known values like the trade's filing URL or known issuer names). Map the actual field layout before writing extraction code.

**Warning signs:** Tests against fixtures fail with "field not found" errors. The `extract_json_string` calls return None for fields that should be present.

### Pitfall 2: The 500-char Window Is Too Small for Full Trade Data

**What goes wrong:** The current `extract_trade_detail` uses a 500-character window around the `"tradeId"` match. A full trade object with nested politician, issuer, and asset data is easily 1000-2000 characters. Fields at the edges of the object (like `committees` at the end) fall outside the window and are missed.

**Why it happens:** The window was sized for extracting just `filingUrl`, which is typically near `tradeId`. More distant fields require a larger window or a different extraction approach.

**How to avoid:** Switch from fixed-window extraction to JSON object boundary extraction. Find the `"tradeId"` match, walk backwards to find the enclosing `{`, then use `extract_json_object()` to get the complete object. Alternatively, increase the window to 2000+ characters.

**Warning signs:** Filing URL is extracted correctly but size/committees/labels are always None/empty.

### Pitfall 3: Committees and Labels May Not Be Present on Trade Detail Pages

**What goes wrong:** The developer writes extraction code for committees and labels, but the trade detail page RSC payload does not include these fields. The BFF API returns them as part of the `Trade` response, but the RSC rendering may source them differently (e.g., from a separate API call, or they may be omitted from the server component props).

**Why it happens:** The CapitolTrades website may render committee badges from a different data source than the trade object itself. Committees are inherently a property of the politician, not the trade -- the BFF API denormalizes them into the trade response for convenience, but the detail page rendering may not.

**How to avoid:** TRADE-05 and TRADE-06 are explicitly "investigate and extract" requirements, not "extract" requirements. The success criteria says "attempted, with documented findings." This means: try to find them, document what you find, and report whether extraction is possible.

**Warning signs:** Searching the raw RSC payload for committee names yields no matches. The committees array is always empty even though the politician has known committee memberships.

### Pitfall 4: Confusing asset_id with tx_id

**What goes wrong:** The existing `upsert_scraped_trades` function sets `asset_id = trade.tx_id` (line 449 in db.rs). This means the assets table uses the same ID as the trade. When updating the asset_type via the detail enrichment, the code must use `tx_id` as the `asset_id`, not look for a separate asset identifier.

**Why it happens:** The scraped data does not have a separate `_assetId` field (unlike the BFF API `Trade` struct which has `_assetId`). The scraper synthesizes it from the tx_id.

**How to avoid:** In `update_trade_detail`, update assets with `WHERE asset_id = tx_id`. Document this mapping clearly.

**Warning signs:** Asset type updates silently affect no rows because the WHERE clause uses the wrong ID.

### Pitfall 5: Test Fixtures Becoming Stale

**What goes wrong:** HTML fixtures captured today reflect the current Next.js RSC format. If CapitolTrades upgrades Next.js, the fixture format may no longer match what the live site produces. Tests pass against old fixtures but the scraper fails against the live site.

**Why it happens:** Fixtures are snapshots in time. RSC payload format is an implementation detail of Next.js, not a public API.

**How to avoid:** Keep fixtures in version control. When the live scraper starts failing, capture fresh fixtures and update the tests. Consider adding a canary test that runs against the live site (gated behind an env var for CI) to detect format changes early.

**Warning signs:** All unit tests pass but `cargo run -- sync --with-trade-details` fails with MissingPayload or Parse errors.

## Code Examples

### Example 1: Extended ScrapedTradeDetail with serde deserialization

```rust
// Source: Pattern derived from existing ScrapedIssuerDetail + Trade struct
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScrapedTradeDetailFull {
    #[serde(rename = "tradeId")]
    pub trade_id: Option<i64>,
    pub filing_url: Option<String>,
    pub filing_id: Option<i64>,
    // Trade detail page may use "asset" nested object
    pub asset: Option<ScrapedTradeAsset>,
    pub size: Option<i64>,
    pub size_range_high: Option<i64>,
    pub size_range_low: Option<i64>,
    pub price: Option<f64>,
    pub has_capital_gains: Option<bool>,
    pub committees: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScrapedTradeAsset {
    pub asset_type: Option<String>,
    pub asset_ticker: Option<String>,
    pub instrument: Option<String>,
}
```

### Example 2: Object-based extraction (preferred approach)

```rust
// Source: Pattern from existing extract_json_object_after (scrape.rs line 579)
fn extract_trade_detail_v2(payload: &str, trade_id: i64) -> ScrapedTradeDetail {
    let mut detail = ScrapedTradeDetail::default();

    // Strategy 1: Look for a trade data object containing tradeId
    let trade_needle = format!("\"tradeId\":{}", trade_id);
    if let Some(pos) = payload.find(&trade_needle) {
        // Walk backward to find the opening brace of the enclosing object
        if let Some(obj_start) = payload[..pos].rfind('{') {
            if let Some(obj_str) = extract_json_object(payload, obj_start) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&obj_str) {
                    // Extract fields from the parsed object
                    detail.filing_url = parsed.get("filingUrl")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    detail.filing_id = detail.filing_url.as_ref()
                        .and_then(|url| filing_id_from_url(url));
                    detail.asset_type = parsed.get("asset")
                        .and_then(|a| a.get("assetType"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    detail.size = parsed.get("size")
                        .and_then(|v| v.as_i64());
                    detail.size_range_high = parsed.get("sizeRangeHigh")
                        .and_then(|v| v.as_i64());
                    detail.size_range_low = parsed.get("sizeRangeLow")
                        .and_then(|v| v.as_i64());
                    detail.price = parsed.get("price")
                        .and_then(|v| v.as_f64());
                    // committees/labels may or may not be present
                    detail.committees = parsed.get("committees")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    detail.labels = parsed.get("labels")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    return detail;
                }
            }
        }
    }

    // Fallback: try the existing window-based approach for filing_url
    // (current implementation, as last resort)
    detail
}
```

### Example 3: Db update_trade_detail method

```rust
// Source: Pattern from Phase 1 architecture research
pub fn update_trade_detail(
    &self,
    tx_id: i64,
    detail: &ScrapedTradeDetail,
) -> Result<(), DbError> {
    let now = chrono::Utc::now().to_rfc3339();
    let filing_id = detail.filing_id.unwrap_or(0);
    let filing_url = detail.filing_url.as_deref().unwrap_or("");
    let has_cg = detail.has_capital_gains.map(|b| if b { 1 } else { 0 });

    self.conn.execute(
        "UPDATE trades SET
           price = COALESCE(?1, price),
           size = COALESCE(?2, size),
           size_range_high = COALESCE(?3, size_range_high),
           size_range_low = COALESCE(?4, size_range_low),
           filing_id = CASE WHEN ?5 > 0 THEN ?5 ELSE filing_id END,
           filing_url = CASE WHEN ?6 != '' THEN ?6 ELSE filing_url END,
           has_capital_gains = COALESCE(?7, has_capital_gains),
           enriched_at = ?8
         WHERE tx_id = ?9",
        params![
            detail.price,
            detail.size,
            detail.size_range_high,
            detail.size_range_low,
            filing_id,
            filing_url,
            has_cg,
            now,
            tx_id
        ],
    )?;

    // Update asset_type on the assets table (asset_id = tx_id for scraped trades)
    if let Some(ref asset_type) = detail.asset_type {
        if asset_type != "unknown" {
            self.conn.execute(
                "UPDATE assets SET asset_type = ?1 WHERE asset_id = ?2 AND asset_type = 'unknown'",
                params![asset_type, tx_id],
            )?;
        }
    }

    // Update trade_committees if any were found
    if !detail.committees.is_empty() {
        self.conn.execute(
            "DELETE FROM trade_committees WHERE tx_id = ?1",
            params![tx_id],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO trade_committees (tx_id, committee) VALUES (?1, ?2)"
        )?;
        for committee in &detail.committees {
            stmt.execute(params![tx_id, committee])?;
        }
    }

    // Update trade_labels if any were found
    if !detail.labels.is_empty() {
        self.conn.execute(
            "DELETE FROM trade_labels WHERE tx_id = ?1",
            params![tx_id],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO trade_labels (tx_id, label) VALUES (?1, ?2)"
        )?;
        for label in &detail.labels {
            stmt.execute(params![tx_id, label])?;
        }
    }

    Ok(())
}
```

### Example 4: Fixture-based unit test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trade_detail_asset_type() {
        // Fixture contains a trade with known asset_type = "stock"
        let html = include_str!("../../tests/fixtures/trade_detail_stock.html");
        let payload = extract_rsc_payload(html).expect("payload should parse");
        let detail = extract_trade_detail(&payload, 171585); // known trade ID from fixture
        assert_eq!(
            detail.asset_type.as_deref(),
            Some("stock"),
            "asset_type should be extracted from fixture"
        );
    }

    #[test]
    fn test_extract_trade_detail_size_fields() {
        let html = include_str!("../../tests/fixtures/trade_detail_stock.html");
        let payload = extract_rsc_payload(html).expect("payload should parse");
        let detail = extract_trade_detail(&payload, 171585);
        assert!(detail.size.is_some(), "size should be present");
        // size_range fields depend on what the fixture contains
    }

    #[test]
    fn test_extract_trade_detail_committees_documented() {
        // This test documents whether committees are available
        let html = include_str!("../../tests/fixtures/trade_detail_stock.html");
        let payload = extract_rsc_payload(html).expect("payload should parse");
        let detail = extract_trade_detail(&payload, 171585);
        // Document the finding -- this assertion documents availability
        // If committees are NOT in the payload, this test documents that fact
        eprintln!(
            "Committees in trade detail payload: {:?} (count: {})",
            detail.committees, detail.committees.len()
        );
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Only extract filing_url/filing_id from trade detail | Extract all enrichable fields | Phase 2 (this work) | Populates asset_type, size, price, potentially committees/labels |
| Hardcode asset_type = "unknown" for scraped trades | Get real asset_type from detail page | Phase 2 (this work) | Enables --asset-type filtering |
| NULL size/size_range fields for scraped trades | Get real size data from detail page | Phase 2 (this work) | Users can see trade size brackets |
| 500-char search window for detail extraction | Full JSON object extraction | Phase 2 (this work) | Captures all fields in the trade object |

## RSC Payload Field Investigation

### What the BFF API Trade Object Contains (from upstream types)

The complete `Trade` struct in `capitoltrades_api/src/types/trade.rs` shows these fields:

| Field | Type | Present in ScrapedTrade (listing) | Present in ScrapedTradeDetail (current) | Needed in Phase 2 |
|-------|------|----------------------------------|----------------------------------------|-------------------|
| `_txId` | i64 | Yes | No (passed as parameter) | No |
| `pubDate` | DateTime | Yes | No | No |
| `txDate` | NaiveDate | Yes | No | No |
| `txType` | TxType | Yes | No | No |
| `owner` | Owner | Yes | No | No |
| `chamber` | Chamber | Yes | No | No |
| `value` | i64 | Yes | No | No |
| `reportingGap` | i64 | Yes | No | No |
| `comment` | Option<String> | Yes | No | No |
| `price` | Option<f64> | Yes (often NULL) | No | **Yes (TRADE-03)** |
| `filingId` | i64 | Optional | Yes (existing) | No (already done) |
| `filingURL` | String | Optional | Yes (existing) | No (already done) |
| `size` | Option<i64> | No (NULL) | No | **Yes (TRADE-02)** |
| `sizeRangeHigh` | Option<i64> | No (NULL) | No | **Yes (TRADE-02)** |
| `sizeRangeLow` | Option<i64> | No (NULL) | No | **Yes (TRADE-02)** |
| `hasCapitalGains` | bool | No (hardcoded 0) | No | **Yes** |
| `asset.assetType` | String | No ("unknown") | No | **Yes (TRADE-01)** |
| `asset.assetTicker` | Option<String> | Via issuer | No | No (already populated) |
| `committees` | Vec<String> | No (empty) | No | **Investigate (TRADE-05)** |
| `labels` | Vec<String> | No (empty) | No | **Investigate (TRADE-06)** |

### Likelihood Assessment for TRADE-05 and TRADE-06

**Committees (TRADE-05) -- LOW-MEDIUM likelihood of being in detail page payload:**
- The BFF API returns committees as part of the Trade object (denormalized from the politician)
- The trade detail page on capitoltrades.com does not visually display committee badges per trade
- Committees are a property of the politician, not the trade
- The RSC payload may not include them since the UI does not render them on the trade detail view
- **Recommendation:** Search the payload for committee-related strings. If not found, document this and note that committees should be obtained through politician enrichment (Phase 4) instead.

**Labels (TRADE-06) -- MEDIUM likelihood of being in detail page payload:**
- The BFF API returns labels as a list of strings (faang, crypto, memestock, spac)
- The capitoltrades.com trade detail page may display label badges
- Labels are a property of the issuer, applied to trades -- they may be in the RSC payload
- **Recommendation:** Search for known label values in the payload. Extract if found, document absence if not.

## Open Questions

1. **What is the exact RSC payload structure on trade detail pages?**
   - What we know: The payload contains `"tradeId":XXXXX` and `"filingUrl":"..."` (proven by existing code). The BFF API Trade type has all target fields. The site renders asset_type and size on the trade detail view.
   - What is unclear: Whether all BFF API fields are present in the RSC payload, or only the subset rendered by the UI component. The exact JSON nesting structure.
   - Recommendation: First task in the plan must be HTML fixture capture. Download 3-5 trade detail pages with curl and examine the RSC payload manually to map the structure. This unblocks all subsequent extraction work.

2. **Is the trade data in a single JSON object or scattered across RSC chunks?**
   - What we know: `issuer_detail()` successfully uses `extract_json_object_after` to find a single `"issuerData":{...}` object. Trade listing pages embed trades in a `"data":[...]` array.
   - What is unclear: Whether the trade detail page has a similar single-object structure (e.g., `"tradeData":{...}`) or presents data differently.
   - Recommendation: Examine fixtures. If single object, use the object extraction approach. If scattered, extend the window-based approach.

3. **Are size values numeric or string-encoded in the RSC payload?**
   - What we know: The BFF API Trade fixture shows `"size": 50000`, `"sizeRangeHigh": 100000`, `"sizeRangeLow": 15001` as numeric values.
   - What is unclear: Whether the RSC payload uses the same encoding or string representations.
   - Recommendation: Check the fixture. Implement parsing that handles both numeric and string-encoded values.

4. **How does the asset_type map from the detail page to the assets table?**
   - What we know: Scraped trades use `asset_id = tx_id` (db.rs line 449). The assets table has `asset_type` which defaults to "unknown". The Phase 1 sentinel protection ensures "unknown" does not overwrite a real value.
   - What is unclear: Whether the detail page asset_type uses the same kebab-case strings as the API (`"stock"`, `"stock-option"`, `"etf"`, etc.) or a different format.
   - Recommendation: Capture fixture, verify the format. The `AssetType` enum in trade.rs has 22 variants -- the detail page should match these.

## Implementation Plan Recommendations

Based on the research, the phase should be organized into 2 plans:

**Plan 02-01: Fixture capture and payload discovery**
- Capture 3-5 trade detail HTML fixtures using curl
- Examine RSC payload to document which fields are present
- Document the JSON structure (single object vs scattered)
- Write baseline fixture-parsing tests that verify RSC extraction works
- Document TRADE-05/TRADE-06 findings (committees/labels availability)

**Plan 02-02: Extend scraper and database**
- Extend `ScrapedTradeDetail` struct with new fields
- Implement the extraction logic (object-based or window-based depending on 02-01 findings)
- Add `Db::update_trade_detail()` method with sentinel protection
- Write unit tests against fixtures for all extractable fields
- Write Db tests for update_trade_detail (using in-memory SQLite)

## Sources

### Primary (HIGH confidence)
- Direct analysis of `capitoltraders_lib/src/scrape.rs` -- extract_trade_detail function, ScrapedTradeDetail struct, RSC extraction utilities
- Direct analysis of `capitoltrades_api/src/types/trade.rs` -- complete Trade struct with all field definitions
- Direct analysis of `capitoltrades_api/tests/fixtures/trades.json` -- example BFF API response with all fields populated
- Direct analysis of `capitoltraders_lib/src/db.rs` -- upsert patterns, sentinel protection (Phase 1 verified)
- Direct analysis of `schema/sqlite.sql` -- current table definitions with enriched_at columns
- Phase 1 verification report confirming all foundation work is complete

### Secondary (MEDIUM confidence)
- RSC Flight payload format analysis (edspencer.net) -- general RSC payload structure documentation
- Direct analysis of `capitoltraders_cli/src/commands/trades.rs` -- scraped_trade_to_trade conversion showing exactly which fields are hardcoded defaults
- Direct analysis of `capitoltraders_cli/src/commands/sync.rs` -- current trade_detail integration in sync pipeline

### Tertiary (LOW confidence)
- WebFetch attempts on capitoltrades.com trade detail pages -- returned loading states, could not extract actual RSC payload data. This means the exact detail page payload structure is UNVERIFIED against the live site and must be confirmed during implementation.
- WebSearch for capitoltrades detail page structure -- no public documentation found on the RSC payload format

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies needed, all existing tools are sufficient
- Architecture (extraction approach): MEDIUM-HIGH -- approach is sound and follows existing patterns, but exact payload structure is unverified
- Architecture (Db update method): HIGH -- follows proven Phase 1 patterns, no ambiguity
- Pitfalls: HIGH -- all pitfalls derived from direct code analysis and verified Phase 1 learnings
- TRADE-05/06 data availability: LOW -- cannot verify without examining actual payload

**Research date:** 2026-02-08
**Valid until:** 2026-03-08 (stable domain for extraction patterns; payload structure may change if Next.js is upgraded)
