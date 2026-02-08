---
phase: 04-politician-enrichment
plan: 01
subsystem: scraping, database
tags: [scrape, sqlite, politician, committee, regex, enrichment]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "SQLite schema with politician_committees table, enriched_at columns, enrichment indexes"
  - phase: 02-trade-extraction
    provides: "ScrapeClient, parse_politician_cards, extract_rsc_payload"
provides:
  - "ScrapeClient::politicians_by_committee(code, page) for committee-filtered listing pages"
  - "Db::replace_all_politician_committees() for atomic committee membership persistence"
  - "Db::mark_politicians_enriched() for enrichment timestamp tracking"
  - "Db::count_unenriched_politicians() for enrichment queue counting"
  - "Real HTML fixture for committee-filtered politician listing page"
affects: [04-02-sync-integration, 04-03-cli-output]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Committee-filter iteration: scrape listing page per committee code to build reverse mapping"
    - "Singular/plural label handling in RSC payload card parsing"
    - "FK-safe bulk insert with EXISTS subquery"

key-files:
  created:
    - "capitoltraders_lib/tests/fixtures/politicians_committee_filtered.html"
  modified:
    - "capitoltraders_lib/src/scrape.rs"
    - "capitoltraders_lib/src/db.rs"

key-decisions:
  - "Used real HTML fixture from live site instead of synthetic (better coverage, caught singular/plural bug)"
  - "Fixed parse_politician_cards regex to handle singular labels (Trade/Issuer) -- affects all politician page parsing, not just committee-filtered"
  - "replace_all_politician_committees uses unchecked_transaction for &self receiver consistency"
  - "EXISTS subquery silently skips unknown politician_ids (FK safety for committee members with no trades)"

patterns-established:
  - "Singular/plural label handling: Trades?/Issuers? in card regex"
  - "Bulk replace pattern: DELETE all + INSERT with FK guard in single transaction"

# Metrics
duration: 5min
completed: 2026-02-08
---

# Phase 4 Plan 1: Committee Membership Scraping and DB Persistence Summary

**ScrapeClient committee-filtered listing page method with real fixture verification, plus atomic DB persistence with FK-safe insert and enrichment tracking**

## Performance

- **Duration:** 5 min
- **Started:** 2026-02-08T21:09:18Z
- **Completed:** 2026-02-08T21:14:47Z
- **Tasks:** 2
- **Files modified:** 3 (scrape.rs, db.rs, new fixture)

## Accomplishments
- Verified parse_politician_cards works against real committee-filtered page from live capitoltrades.com
- Fixed singular/plural label bug in card regex ("Trade" vs "Trades", "Issuer" vs "Issuers")
- Added politicians_by_committee(code, page) method reusing existing card parser
- Added replace_all_politician_committees() with atomic clear+rebuild and FK safety
- Added mark_politicians_enriched() and count_unenriched_politicians() methods
- 6 new tests (1 scrape fixture test + 5 DB tests), all 262 workspace tests passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Verify parse_politician_cards, add scraper method and fixture** - `2ac45ad` (feat)
2. **Task 2: Add DB committee persistence and enrichment tracking** - `ccb5acd` (feat)

## Files Created/Modified
- `capitoltraders_lib/src/scrape.rs` - Fixed singular/plural regex, added politicians_by_committee() method, added fixture test
- `capitoltraders_lib/src/db.rs` - Added replace_all_politician_committees(), mark_politicians_enriched(), count_unenriched_politicians(), plus 5 tests
- `capitoltraders_lib/tests/fixtures/politicians_committee_filtered.html` - Real HTML fixture from /politicians?committee=ssfi (Senate Finance, 5 members)

## Decisions Made
- Used real HTML from live site as fixture rather than synthetic. This caught a real bug (singular/plural labels) that synthetic fixtures would have missed.
- Fixed parse_politician_cards regex globally (not just for committee pages) since the singular/plural issue affects all politician listing pages, not just filtered ones.
- The replace_all_politician_committees method returns the count of actually inserted rows, enabling callers to report how many memberships were persisted vs skipped.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed singular/plural label mismatch in parse_politician_cards**
- **Found during:** Task 1 (live verification of parse_politician_cards)
- **Issue:** The card regex hardcoded `children":"Trades"` and `children":"Issuers"` (plural only). The live site uses singular "Trade"/"Issuer" when a politician has exactly 1 trade/issuer. This caused 2 of 5 cards to fail parsing, triggering the "card count mismatch" error.
- **Fix:** Changed regex to `Trades?` and `Issuers?` (optional trailing 's') to accept both singular and plural.
- **Files modified:** capitoltraders_lib/src/scrape.rs
- **Verification:** All 5 cards from real fixture now parse correctly. All 262 workspace tests pass.
- **Committed in:** 2ac45ad (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Essential fix for correctness. The plan explicitly called for live verification first (Step 0) and anticipated this possibility ("If parse_politician_cards fails"). The fix was minimal (2 characters changed in regex).

## Issues Encountered
None beyond the planned verification step catching the singular/plural bug.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- politicians_by_committee() ready for sync pipeline integration (Plan 04-02)
- replace_all_politician_committees() ready for bulk persistence from enrichment loop
- mark_politicians_enriched() and count_unenriched_politicians() ready for enrichment status tracking
- All methods tested and verified against real data

## Self-Check: PASSED

- All 4 files exist (scrape.rs, db.rs, fixture, SUMMARY.md)
- Both commits found (2ac45ad, ccb5acd)
- All 4 methods present (politicians_by_committee, replace_all_politician_committees, mark_politicians_enriched, count_unenriched_politicians)

---
*Phase: 04-politician-enrichment*
*Completed: 2026-02-08*
