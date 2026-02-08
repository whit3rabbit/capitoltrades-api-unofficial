---
phase: 02-trade-extraction
plan: 01
subsystem: scraping
tags: [rsc-payload, html-fixtures, trade-detail, serde-json]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "enriched_at columns, sentinel protection patterns, enrichment queue queries"
provides:
  - "Extended ScrapedTradeDetail struct with all enrichable fields"
  - "Full JSON object extraction in extract_trade_detail()"
  - "3 synthetic HTML fixtures for trade detail pages"
  - "16 fixture-based unit tests covering TRADE-01 through TRADE-06"
  - "TRADE-05/TRADE-06 findings: committees and labels present in BFF model, UNCONFIRMED on live RSC"
affects: [02-02, trade-sync, politician-enrichment]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Full JSON object extraction: walk backwards from needle to opening brace, use extract_json_object"
    - "Synthetic HTML fixtures: model RSC payload structure from BFF API types when live site unavailable"

key-files:
  created:
    - "capitoltraders_lib/tests/fixtures/trade_detail_stock.html"
    - "capitoltraders_lib/tests/fixtures/trade_detail_option.html"
    - "capitoltraders_lib/tests/fixtures/trade_detail_minimal.html"
  modified:
    - "capitoltraders_lib/src/scrape.rs"

key-decisions:
  - "Used synthetic fixtures because live capitoltrades.com returns loading states via curl (RSC data streamed client-side)"
  - "Rewrote extract_trade_detail to use full object extraction (backward walk + extract_json_object) instead of 500-char window"
  - "Retained window-based fallback for backward compatibility if object extraction fails"
  - "Support both filingUrl (RSC style) and filingURL (BFF API style) key names"

patterns-established:
  - "Fixture-based scrape testing: include_str! fixtures through extract_rsc_payload, then test extraction"
  - "Object extraction pattern: find needle, rfind opening brace, extract_json_object, serde_json::Value"
  - "TRADE-05/TRADE-06 investigative pattern: test documents findings with comments explaining confidence level"

# Metrics
duration: 8min
completed: 2026-02-08
---

# Phase 2 Plan 1: Trade Detail Fixture Capture and Scraper Extension Summary

**Full JSON object extraction from trade detail RSC payloads with synthetic fixtures and 16 tests covering asset_type, size, price, filing, capital gains, committees, and labels**

## Performance

- **Duration:** 8 min
- **Started:** 2026-02-08T15:06:55Z
- **Completed:** 2026-02-08T15:14:32Z
- **Tasks:** 3
- **Files modified:** 4 (1 source, 3 fixtures)

## Accomplishments
- Created 3 synthetic HTML fixtures modeling the trade detail RSC payload structure (stock, stock-option, minimal/sparse)
- Extended ScrapedTradeDetail with 8 new fields: asset_type, size, size_range_high, size_range_low, price, has_capital_gains, committees, labels
- Rewrote extract_trade_detail() from 500-char window approach to full JSON object extraction with backward brace walking
- Added 16 fixture-based unit tests (total workspace tests: 209 -> 225)
- Documented TRADE-05 (committees) and TRADE-06 (labels) findings: present in BFF API model but UNCONFIRMED on live RSC payload

## Task Commits

Each task was committed atomically:

1. **Task 1: Capture trade detail HTML fixtures** - `8effb8d` (test)
2. **Task 2: Extend ScrapedTradeDetail and extract_trade_detail()** - `1fbfbb3` (feat)
3. **Task 3: Add fixture-based unit tests** - `53b381d` (test)

## Files Created/Modified
- `capitoltraders_lib/tests/fixtures/trade_detail_stock.html` - Synthetic fixture: stock trade (ID 172000) with all fields populated including committees and labels
- `capitoltraders_lib/tests/fixtures/trade_detail_option.html` - Synthetic fixture: stock-option trade (ID 171500) with has_capital_gains=true, path-based filing URL
- `capitoltraders_lib/tests/fixtures/trade_detail_minimal.html` - Synthetic fixture: minimal/older trade (ID 3000) with null price/size, empty filing URL
- `capitoltraders_lib/src/scrape.rs` - Extended ScrapedTradeDetail struct, rewrote extract_trade_detail, added extract_fields_from_trade_object helper, added 16 unit tests

## Decisions Made
- **Synthetic fixtures over live HTML:** The live capitoltrades.com trade detail pages return client-side loading states when fetched via curl. The RSC data is streamed via a separate flight response that cannot be captured with a simple HTTP GET. Created synthetic fixtures modeled from (1) existing extract_trade_detail code patterns, (2) BFF API Trade struct field names, and (3) the existing trades.json test fixture. All fixtures are marked with "SYNTHETIC FIXTURE" comments.
- **Full object extraction over expanded window:** Instead of increasing the window from 500 to 2000 characters, implemented the preferred Pattern A from the research: walk backwards from the "tradeId" match to find the enclosing `{`, then use the existing extract_json_object() to get the complete object. This is more robust and captures all fields regardless of object size.
- **Both filingUrl and filingURL:** The RSC payload may use either key name. Supporting both ensures the extraction works regardless of which format the live site uses.
- **Committees/labels as Vec<String>:** Modeled from the BFF API Trade struct. The extraction handles both populated arrays and empty arrays gracefully.

## Deviations from Plan

None -- plan executed exactly as written. The plan anticipated the fallback to synthetic fixtures when the live site returns loading states.

## Issues Encountered
- **Live site returns loading states:** All trade detail page fetches via curl returned HTML with "Loading ..." client-side rendering states and RSC error boundaries (digest codes). The actual trade data is loaded via a separate RSC flight stream that requires JavaScript execution. This was expected per the research document and the plan's fallback provision.
- **BFF API also unreachable:** Attempting to fetch directly from `bff.capitoltrades.com` returned a 503 CloudFront error ("Lambda function invalid or doesn't have required permissions"). This confirms the site's data is served through a protected server-side rendering pipeline.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness
- ScrapedTradeDetail is ready for Plan 02-02 to consume in Db::update_trade_detail()
- All sentinel protection patterns from Phase 1 can be applied to the new fields
- The extract_fields_from_trade_object helper is well-tested and handles null/missing fields gracefully
- **Risk:** Synthetic fixtures may not match the actual live RSC payload structure. When the scraper runs against the live site, field names or nesting may differ. The backward brace walking + object extraction approach is resilient, but specific field names (e.g., "filingUrl" vs "filingURL", nested "asset.assetType" vs flat "assetType") may need adjustment.
- **TRADE-05 (committees) risk:** Committees may NOT be present in the live RSC payload. The BFF API includes them, but the trade detail page may not render them. If absent, committees should be sourced from politician enrichment (Phase 4).
- **TRADE-06 (labels) risk:** Same uncertainty as committees. Labels are a property of the issuer and may not be embedded in the trade detail RSC payload.

## Self-Check: PASSED

- [x] trade_detail_stock.html exists
- [x] trade_detail_option.html exists
- [x] trade_detail_minimal.html exists
- [x] scrape.rs exists with all new fields
- [x] Commit 8effb8d exists (Task 1)
- [x] Commit 1fbfbb3 exists (Task 2)
- [x] Commit 53b381d exists (Task 3)
- [x] Total workspace tests: 225 (was 209, +16 new)
- [x] All 16 extract_trade_detail tests pass

---
*Phase: 02-trade-extraction*
*Completed: 2026-02-08*
