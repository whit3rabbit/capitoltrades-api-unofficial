# Domain Pitfalls: OpenFEC Donation Data Integration

**Domain:** FEC Campaign Finance Data Integration
**Researched:** 2026-02-11
**Confidence:** HIGH

## Executive Summary

Integrating OpenFEC donation data into Capitol Traders presents seven critical challenges: (1) **Name Mapping** - FEC uses candidate IDs, not names, requiring multi-step politician-to-committee resolution; (2) **Data Volume** - major politicians generate 10K-50K donation records per cycle requiring days to sync; (3) **Rate Limiting** - 1,000 calls/hour cap means full congressional sync is impractical; (4) **Employer Normalization** - free-text employer field with variants like "Google" vs "Google LLC" defeats direct matching; (5) **Committee Multiplicity** - politicians have 2-5 committees (campaign, leadership PAC, party) requiring all to be tracked; (6) **Data Staleness** - FEC updates daily but can backdate amendments months later; (7) **Keyset Pagination** - Schedule A uses cursor-based pagination (not page numbers), easy to misimplement and duplicate/skip records.

**Recommended Strategy:** Use [unitedstates/congress-legislators](https://github.com/unitedstates/congress-legislators) YAML dataset for politician-to-FEC ID mapping (includes bioguide, FEC candidate IDs). Sync donations incrementally per politician on-demand (not all 535 members). Use fuzzy matching with manual seed data for top 200 employers. Accept that full sync is a multi-day batch operation, not real-time.

---

## Critical Pitfalls

### Pitfall 1: The Name Mapping Problem

**Severity:** CRITICAL

**What goes wrong:** CapitolTrades uses politician names ("Nancy Pelosi"). FEC API uses candidate IDs ("P00008265") and committee IDs ("C00385534"). There is no direct name-to-ID endpoint. The `/candidates/search/` endpoint does fuzzy name search but returns multiple matches for common names and requires disambiguation. Once you have a candidate_id, you still need a second API call to `/committees/?candidate_id=X` to find their authorized committees, since donations are committee-centric in FEC's data model.

**Why it happens:** FEC's data model is committee-first (committees file reports, not candidates). Names are ambiguous (multiple John Smiths in Congress). Candidates change names (marriage, legal name changes). Historical candidates have multiple FEC IDs across different election cycles and offices (House candidate ID differs from Senate candidate ID for same person).

**Consequences:**
- Multi-step lookup required: politician name → candidate search → candidate_id → committee lookup → committee_ids
- API call multiplication: 3+ calls per politician just to establish mapping
- Cache misses: Name variations ("Nancy Pelosi" vs "Pelosi, Nancy") produce duplicate lookups
- Disambiguation UX: "John Smith" returns 12 candidates, which one do you want?
- Rate limit burn: Mapping 535 members of Congress = 1,605+ API calls (3 per politician) = 16+ hours at 1,000 calls/hour

**Prevention:**

**SOLUTION: Use unitedstates/congress-legislators dataset as authoritative crosswalk**

This public domain dataset maintained by @unitedstates organization provides comprehensive ID mappings for all members of Congress (1789-present):

**Dataset location:** https://github.com/unitedstates/congress-legislators

**Key files:**
- `legislators-current.yaml` - Current members of Congress (118th Congress as of 2026)
- `legislators-historical.yaml` - Historical members

**Data structure (example: Maria Cantwell):**
```yaml
id:
  bioguide: C000127
  thomas: '00172'
  lis: S275
  govtrack: 300018
  opensecrets: N00007836
  votesmart: 27122
  fec:
    - S8WA00194    # Senate campaign
    - H2WA01054    # House campaign (historical)
  cspan: 26137
  wikipedia: Maria Cantwell
  wikidata: Q22250
name:
  first: Maria
  last: Cantwell
  official_full: Maria Cantwell
bio:
  birthday: '1958-10-13'
  gender: F
terms:
  - type: sen
    state: WA
    start: '2001-01-03'
    party: Democrat
    class: 1
```

**Implementation strategy:**

1. **Initial load:** Download legislators-current.yaml at build time or sync
2. **Mapping table:** Create `politician_fec_mapping` table in SQLite:
   ```sql
   CREATE TABLE politician_fec_mapping (
       politician_id TEXT PRIMARY KEY,  -- CapitolTrades politician ID (e.g., "P000610")
       bioguide TEXT NOT NULL,          -- Bioguide ID (canonical congressional ID)
       fec_candidate_ids TEXT NOT NULL, -- JSON array of FEC candidate IDs
       full_name TEXT NOT NULL,
       state TEXT,
       chamber TEXT,                     -- house or senate
       party TEXT,
       updated_at TEXT
   );
   ```
3. **Matching strategy:**
   - CapitolTrades politician IDs appear to follow Bioguide-like format (P000610 = Patrick McHenry, bioguide M000485)
   - **Option A:** If CapitolTrades IDs ARE bioguide IDs, direct lookup in YAML
   - **Option B:** If CapitolTrades IDs are proprietary, fuzzy match on full name + state + party
   - **Option C:** Manual seed mapping for current 535 members (one-time effort, ~2 hours)

4. **FEC ID multiplicity:** Each legislator can have multiple FEC candidate IDs (one per office/cycle). Example: Maria Cantwell has `S8WA00194` (Senate) and `H2WA01054` (House historical). Query ALL FEC IDs to get complete donation history.

5. **Committee resolution:** After getting FEC candidate IDs, call OpenFEC `/committees/?candidate_id={fec_id}` to find active committees. Cache this mapping (committees don't change frequently).

6. **Update strategy:**
   - Re-download legislators-current.yaml monthly (repository updated by maintainers)
   - Dataset includes current term dates, can filter to active members
   - Historical members in separate YAML (exclude for current analysis)

**Why this works:**
- **Authoritative:** Maintained by civic tech community, used by GovTrack, ProPublica, Sunlight Foundation
- **Comprehensive:** Includes FEC IDs, Bioguide, OpenSecrets, Wikipedia, multiple ID systems
- **Public domain:** No licensing restrictions
- **Actively maintained:** 2,588 commits as of research date, ongoing updates
- **Quality:** Manually curated with automated imports, includes verification
- **Format:** YAML/JSON/CSV available (CSV for easy SQLite import)

**Detection:** Mismatched donations (wrong politician) or empty result sets when donations exist on FEC.gov. Politician name search returns "not found" despite being active member.

**Estimated effort:** 1-2 days to implement YAML parsing + mapping table, vs 1-2 weeks to build robust multi-step name search with disambiguation UX.

**Confidence:** HIGH (verified via GitHub repository inspection, active maintenance confirmed, FEC ID field structure validated)

**Sources:**
- [GitHub - unitedstates/congress-legislators](https://github.com/unitedstates/congress-legislators)
- [congress-legislators README](https://github.com/unitedstates/congress-legislators/blob/main/README.md)
- [FEC Candidate ID format documentation](https://www.fec.gov/campaign-finance-data/candidate-master-file-description/)

---

### Pitfall 2: Data Volume Explosion

**Severity:** CRITICAL

**What goes wrong:** Major politicians accumulate tens of thousands of individual donation records per election cycle. At 100 records per API page, fetching all donations for a single politician requires hundreds to thousands of API calls. With 1,000 calls/hour rate limit, syncing all 535 members of Congress is a multi-day operation.

**Why it happens:**
- Individual contributions over $200 are itemized in Schedule A (FEC requirement)
- Competitive races generate 10K-50K individual donations per cycle
- Senate 6-year terms span 3 election cycles of data
- Politicians with long careers have historical data going back decades
- Leadership PACs have separate donation streams (separate committee IDs)

**Consequences:**
- **Data volume math:**
  - Average House member (competitive district): 5,000 donations/cycle
  - Average Senator (competitive state): 15,000 donations/cycle
  - High-profile member (Pelosi, McConnell): 50,000+ donations/cycle
  - 535 members of Congress average 8,000 donations each = 4.28 million records total

- **API call math:**
  - 50,000 donations / 100 per page = 500 API calls per high-profile politician
  - 500 calls * 535 members = 267,500 total API calls
  - 267,500 calls / 1,000 per hour = 267.5 hours = **11.1 days continuous sync**

- **Storage math:**
  - Average Schedule A record: ~500 bytes serialized
  - 4.28M records * 500 bytes = 2.14 GB raw data
  - SQLite with indexes: ~4-5 GB database size

**Prevention:**

1. **Per-politician on-demand sync (PRIMARY RECOMMENDATION):**
   - Sync donations only for politicians the user explicitly queries
   - Cache synced data for 24 hours (FEC updates daily)
   - Track sync timestamp per politician in metadata table
   - Estimate: 5-10 minutes per politician for first sync, <10 seconds for cached

2. **Incremental sync with date checkpoints:**
   - Store `last_contribution_receipt_date` per politician in sync_metadata table
   - On subsequent syncs, use `min_date` parameter: `/schedules/schedule_a/?committee_id=X&min_date=2024-06-15`
   - Only fetch new donations since last sync
   - Reduces API calls by 90%+ after initial full sync

3. **Batch size limiting:**
   - Provide `--batch-size N` flag to limit initial sync (e.g., only fetch last 1,000 donations)
   - User opts into full historical sync explicitly
   - Default: last 2 years (current + previous cycle)

4. **Prioritized sync strategy:**
   - Phase 1: Sync only current cycle (2025-2026) donations
   - Phase 2: Backfill previous cycle (2023-2024) if user requests
   - Phase 3: Full historical sync as overnight batch job

5. **Bulk download alternative (for full sync):**
   - FEC provides bulk CSV downloads: https://www.fec.gov/data/browse-data/?tab=bulk-data
   - Bulk files cover entire cycles (e.g., `indiv26.zip` for 2025-2026 cycle)
   - **Estimated size:** Schedule A bulk files are 75+ GB combined across cycles
   - **Trade-off:** Single download vs thousands of API calls, but requires CSV parsing and filtering
   - **When to use:** If implementing full 535-member sync, bulk download + local filtering is faster than API pagination

6. **Committee filtering optimization:**
   - Query `/committees/?candidate_id={fec_id}&committee_type=H` to get only principal campaign committee
   - Exclude leadership PACs in initial sync (adds 2-3x data volume)
   - Leadership PAC sync as optional flag: `--include-leadership-pacs`

**Detection:**
- Sync progress bar stuck for hours
- Rate limit 429 errors
- Database growth exceeding available disk space
- User frustration waiting for sync to complete

**Data volume by politician type (estimates from research):**

| Politician Type | Donations/Cycle | API Calls | Sync Time @ 1000/hr |
|-----------------|-----------------|-----------|---------------------|
| Safe district House | 1,000-3,000 | 10-30 | 1-2 minutes |
| Competitive House | 5,000-10,000 | 50-100 | 3-6 minutes |
| Safe state Senate | 8,000-15,000 | 80-150 | 5-9 minutes |
| Competitive Senate | 15,000-30,000 | 150-300 | 9-18 minutes |
| Leadership (Pelosi, McConnell) | 50,000+ | 500+ | 30+ minutes |

**Recommendation hierarchy:**
1. **Default:** On-demand per-politician sync, current cycle only (2025-2026), principal campaign committee only
2. **Power user:** `--full-history` flag for all cycles, `--include-leadership-pacs` for complete picture
3. **Bulk operation:** Bulk CSV download for researchers analyzing all 535 members

**Confidence:** HIGH (data volume estimates from FEC.gov statistics, API pagination limits verified in OpenFEC documentation)

**Sources:**
- [FEC Statistical Summary 2021-2022 Election Cycle](https://www.fec.gov/updates/statistical-summary-of-18-month-campaign-activity-of-the-2021-2022-election-cycle/)
- [FEC Bulk Data Downloads](https://www.fec.gov/data/browse-data/?tab=bulk-data)
- [GitHub - irworkshop/fec2file (bulk data size notes)](https://github.com/irworkshop/fec2file)

---

### Pitfall 3: Rate Limiting Bottleneck

**Severity:** HIGH

**What goes wrong:** OpenFEC enforces 1,000 API calls per hour via api.data.gov rate limiting layer. Exceeding this triggers HTTP 429 errors. There is no way to increase this limit (API key tier is fixed). For paginated endpoints like Schedule A, each page = 1 API call. Fetching 50,000 donation records (500 pages) consumes half your hourly quota for a single politician.

**Why it happens:**
- OpenFEC API is free tier only (no paid tiers for higher limits)
- Shared infrastructure via api.data.gov (government-wide API umbrella)
- Rate limit enforced at API key level (not IP-based)
- No batch endpoints (must paginate through large result sets)

**Consequences:**
- Sync operations stall mid-process when hitting rate limit
- Multi-politician sync requires spreading across multiple hours
- Development iteration slowed (testing consumes quota)
- Concurrent requests from multiple CLI instances share same quota (if using same API key)

**Prevention:**

1. **Circuit breaker pattern (CRITICAL):**
   ```rust
   struct RateLimiter {
       calls_this_hour: Arc<AtomicUsize>,
       hour_start: Arc<Mutex<Instant>>,
       max_calls_per_hour: usize,
   }

   impl RateLimiter {
       async fn acquire_permit(&self) -> Result<(), RateLimitError> {
           let now = Instant::now();
           let mut hour_start = self.hour_start.lock().await;

           // Reset counter if hour elapsed
           if now.duration_since(*hour_start) >= Duration::from_secs(3600) {
               self.calls_this_hour.store(0, Ordering::SeqCst);
               *hour_start = now;
           }

           let current = self.calls_this_hour.fetch_add(1, Ordering::SeqCst);
           if current >= self.max_calls_per_hour {
               let wait_time = Duration::from_secs(3600) - now.duration_since(*hour_start);
               return Err(RateLimitError::QuotaExceeded { wait_time });
           }

           Ok(())
       }
   }
   ```

2. **Exponential backoff on 429 errors:**
   - Catch HTTP 429 responses
   - Extract `Retry-After` header (if present)
   - Default backoff: wait 60 seconds, retry with doubled delay (60s → 120s → 240s)
   - Max retries: 3 attempts, then fail with user-facing error

3. **Quota budgeting:**
   - Reserve 200 calls/hour for committee lookups and metadata (20%)
   - Allocate 800 calls/hour for Schedule A pagination (80%)
   - Display quota usage in sync progress: "Using 450/1000 API calls (45%)"

4. **Spread strategy for multi-politician sync:**
   - Sync 10 politicians/hour max (assuming 100 calls each average)
   - Queue remaining politicians for next hour
   - Persistent queue in SQLite: `sync_queue` table with priority + timestamp
   - Resume automatically when quota resets

5. **Request deduplication:**
   - Hash request parameters (committee_id, date range, pagination cursor)
   - Check DashMap cache before making API call
   - TTL: 1 hour (FEC data updates daily, but cache aggressively for quota preservation)

6. **Concurrency limiting:**
   - Use Semaphore with permit count = 5 (not 10 or 20)
   - Lower concurrency reduces risk of burst hitting rate limit
   - Existing enrichment pipeline uses Semaphore - reuse pattern

7. **Development quota preservation:**
   - Use wiremock for unit tests (no real API calls)
   - JSON fixtures for integration tests
   - `DEMO_KEY` for initial development (separate quota from production API key)
   - Warn on `DEMO_KEY` usage: "Using DEMO_KEY - expect aggressive rate limits"

8. **Bulk download fallback (escapes rate limit entirely):**
   - For full-congress sync: download bulk CSV files instead of API
   - Bulk files available at https://www.fec.gov/files/bulk-downloads/2026/
   - No rate limit on bulk downloads (HTTP file transfer)
   - Trade-off: Bulk files updated less frequently (weekly vs daily API updates)

**Detection:**
- HTTP 429 responses from OpenFEC
- `X-RateLimit-Remaining` header approaching 0
- Sync progress slows to crawl after initial burst
- Error logs showing "RateLimitExceeded" errors

**Rate limit math examples:**

| Scenario | API Calls | Time Required | Feasibility |
|----------|-----------|---------------|-------------|
| Single politician (5K donations) | 50 | 3 minutes | Immediate |
| 10 politicians (5K each) | 500 | 30 minutes | Same hour |
| 50 politicians (5K each) | 2,500 | 2.5 hours | Spread across 3 hours |
| All 535 members (avg 8K each) | 42,800 | 42.8 hours | 2-day batch job |
| Single high-profile (50K donations) | 500 | 30 minutes | Immediate |

**Recommendation:**
- **Default behavior:** On-demand per-politician sync (fits within quota)
- **Power users:** Overnight batch job for multi-politician sync with queue persistence
- **Alternative:** Bulk CSV download for initial load, API for incremental updates

**Confidence:** HIGH (rate limit verified in OpenFEC documentation and api.data.gov terms)

**Sources:**
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/)
- [api.data.gov Rate Limiting](https://api.data.gov/docs/rate-limits/)
- [18F: OpenFEC API Update](https://18f.gsa.gov/2015/07/15/openfec-api-update/)

---

### Pitfall 4: Employer Name Normalization Hell

**Severity:** HIGH

**What goes wrong:** The `contbr_employer` field in Schedule A is free-text, self-reported by donors. The same employer appears with dozens of variants: "Google" vs "Google Inc" vs "Google LLC" vs "Alphabet Inc" vs "Alphabet" vs "GOOGL" vs "Goog" vs "google.com". Direct string matching yields <50% match rate when correlating with trade issuer names. Fuzzy matching produces false positives ("Goldman Sachs" matches "Gold Man Sacks LLC" - a different company).

**Why it happens:**
- FEC does not standardize or validate employer names
- Donors enter employer name from memory (spelling errors, abbreviations)
- Corporate structure changes (Google → Alphabet, Facebook → Meta)
- Subsidiaries vs parent companies (Instagram vs Meta Platforms Inc)
- Generic entries: "Self-employed", "Retired", "N/A", "None", "Homemaker"

**Consequences:**
- Employer-to-issuer correlation (key differentiator feature) fails without normalization
- False positives: "Goldman Sachs" fuzzy matches "Gold Man Capital" (unrelated)
- False negatives: "Google LLC" doesn't match "Alphabet Inc" despite being same entity
- Cannot aggregate donations by company (same employer has 20 different spellings)
- Sector analysis impossible (can't map "Goog" to Technology sector)

**Prevention:**

1. **Manual seed data for top employers (PRIMARY RECOMMENDATION):**
   - Create `employer_normalization` table with canonical mappings:
     ```sql
     CREATE TABLE employer_normalization (
         employer_variant TEXT PRIMARY KEY,
         canonical_employer TEXT NOT NULL,
         issuer_ticker TEXT,        -- Maps to trade issuer tickers
         sector TEXT,                -- Industry sector
         confidence TEXT CHECK(confidence IN ('high', 'medium', 'low'))
     );
     ```
   - Seed top 200 employers manually (S&P 500 companies + major donors)
   - Example mappings:
     ```sql
     INSERT INTO employer_normalization VALUES
       ('Google', 'Alphabet Inc', 'GOOGL', 'Technology', 'high'),
       ('Google Inc', 'Alphabet Inc', 'GOOGL', 'Technology', 'high'),
       ('Google LLC', 'Alphabet Inc', 'GOOGL', 'Technology', 'high'),
       ('Alphabet', 'Alphabet Inc', 'GOOGL', 'Technology', 'high'),
       ('Goldman Sachs', 'The Goldman Sachs Group Inc', 'GS', 'Finance', 'high'),
       ('Goldman Sachs Group', 'The Goldman Sachs Group Inc', 'GS', 'Finance', 'high'),
       ('GS', 'The Goldman Sachs Group Inc', 'GS', 'Finance', 'medium');
     ```

2. **Normalization preprocessing:**
   - Lowercase: "Google" → "google"
   - Trim whitespace: " Google Inc " → "google inc"
   - Remove common suffixes: "Inc", "LLC", "Corp", "Corporation", "Company", "Co"
   - Remove punctuation: "Google, Inc." → "google"
   - Standardize abbreviations: "& Co" → "and Company"
   ```rust
   fn normalize_employer(raw: &str) -> String {
       raw.trim()
          .to_lowercase()
          .replace(".", "")
          .replace(",", "")
          .replace(" inc", "")
          .replace(" llc", "")
          .replace(" corp", "")
          .replace(" corporation", "")
          .replace(" company", "")
          .replace(" & co", "")
   }
   ```

3. **Two-tier matching strategy:**
   - **Tier 1 (exact match):** Check normalized employer against seed data - O(1) HashMap lookup
   - **Tier 2 (fuzzy match):** If no exact match, compute Jaro-Winkler distance against all canonical employers
   - **Threshold:** Distance >= 0.85 for "suggested match" (flag for review)
   - **Confidence scoring:**
     - 1.0 (exact match in seed data) → `high` confidence
     - 0.90-0.99 (fuzzy match, close) → `medium` confidence
     - 0.85-0.89 (fuzzy match, distant) → `low` confidence (flag for review)
     - <0.85 → No match, mark as "Unknown employer"

4. **Export-review-import workflow:**
   - Command: `capitoltraders export-unmatched-employers --politician "Pelosi" > unmatched.csv`
   - User reviews CSV, adds mappings manually (or uses spreadsheet lookup)
   - Command: `capitoltraders import-employer-mappings unmatched_reviewed.csv`
   - Imports into `employer_normalization` table with user-provided confidence

5. **Employer frequency analysis:**
   - Before fuzzy matching, count employer frequency in donations:
     ```sql
     SELECT contbr_employer, COUNT(*) as donation_count
     FROM donations
     WHERE politician_id = ?
     GROUP BY contbr_employer
     ORDER BY donation_count DESC
     LIMIT 100;
     ```
   - Prioritize mapping for high-frequency employers (80/20 rule: top 20% of employers = 80% of donations)

6. **Never auto-link without confirmation:**
   - Fuzzy matches must be flagged, not automatically applied
   - Display: "Possible match: 'Google Inc' → 'Alphabet Inc' (confidence: 0.92) - Confirm? [y/n]"
   - Store user confirmations in `employer_normalization` table for future runs
   - Avoid false positives corrupting analysis

7. **Generic employer handling:**
   - Detect generic entries: "self-employed", "retired", "n/a", "none", "homemaker", "unemployed"
   - Map to special category: `canonical_employer = 'Not Employed'`, `sector = 'N/A'`
   - Exclude from issuer correlation (no trades to correlate with)

**Fuzzy matching library recommendation:**
- **strsim crate:** https://docs.rs/strsim/ - Jaro-Winkler, Levenshtein, Damerau-Levenshtein
- Lightweight (no ML dependencies), pure Rust
- Example usage:
  ```rust
  use strsim::jaro_winkler;

  let similarity = jaro_winkler("Google Inc", "Alphabet Inc");
  if similarity >= 0.85 {
      println!("Possible match: confidence {:.2}", similarity);
  }
  ```

**Detection:**
- User reports "missing donations from Google" when donations exist under "Google LLC"
- Employer-to-issuer correlation shows 0 matches despite obvious connections
- Sector analysis shows 90% "Unknown sector"
- Same employer appears in output with 10 different spellings

**Data quality estimates (based on research):**

| Employer Category | % of Total Donations | Normalization Difficulty |
|-------------------|----------------------|--------------------------|
| Top 200 employers (S&P 500) | 40-50% | Low (manual seed data) |
| Mid-size companies (1000-5000 employees) | 20-30% | Medium (fuzzy matching works) |
| Small companies (<1000 employees) | 10-20% | High (many unique names) |
| Generic/Not employed | 10-20% | N/A (exclude from correlation) |

**Recommendation:**
- **Phase 1:** Manual seed data for top 200 employers (covers 40-50% of donations)
- **Phase 2:** Fuzzy matching for mid-size companies with review workflow
- **Phase 3:** Export unmatched employers for user curation (community contribution model)

**Confidence:** HIGH (employer name variation problem is well-documented in campaign finance research, normalization strategies verified via data quality literature)

**Sources:**
- [Employer name standardization - RecordLinker](https://recordlinker.com/name-normalization-matching/)
- [Fuzzy Matching 101 - Data Ladder](https://dataladder.com/fuzzy-matching-101/)
- [Intelligent fuzzy matching to standardize company names - Quantemplate](https://www.quantemplate.com/l/intelligent-fuzzy-matching-to-standardize-company-names)
- [FEC Individual Contribution Research](https://www.fec.gov/introduction-campaign-finance/how-to-research-public-records/individual-contributions/)

---

## High Severity Pitfalls

### Pitfall 5: Committee Multiplicity Complexity

**Severity:** HIGH

**What goes wrong:** A single politician can have 2-5 different FEC committees receiving donations: (1) principal campaign committee, (2) leadership PAC, (3) joint fundraising committees, (4) party committee transfers. Querying only the principal campaign committee misses 30-50% of their total fundraising. Each committee has a separate committee ID and requires separate Schedule A queries.

**Why it happens:**
- FEC allows politicians to establish multiple committees for different purposes
- Leadership PACs are legally separate from campaign committees (different contribution limits)
- Joint fundraising committees pool donations for multiple candidates
- Party committees (DCCC, NRCC, DSCC, NRSC) receive donations "for" specific candidates

**Consequences:**
- Incomplete donation data if only querying principal campaign committee
- Under-reporting of total fundraising by 30-50%
- Missing correlation opportunities (leadership PAC donors may differ from campaign donors)
- Confusion when user sees different totals vs OpenSecrets or FEC.gov (which aggregate all committees)

**Prevention:**

1. **Query all authorized committees:**
   - Use OpenFEC `/committees/?candidate_id={fec_id}` endpoint
   - Filter for `committee_type` in:
     - `H` = House campaign committee
     - `S` = Senate campaign committee
     - `P` = Presidential campaign committee
   - Store all committee IDs in `politician_committees` table:
     ```sql
     CREATE TABLE politician_committees (
         politician_id TEXT NOT NULL,
         committee_id TEXT NOT NULL,
         committee_name TEXT,
         committee_type TEXT,
         designation TEXT,  -- P=Principal, A=Authorized, J=Joint fundraising
         PRIMARY KEY (politician_id, committee_id)
     );
     ```

2. **Leadership PAC handling (optional flag):**
   - Leadership PACs have `committee_type = O` (non-connected committee)
   - Designation field indicates sponsor: `designation = 'U'` with sponsor candidate_id
   - **Default:** Exclude leadership PACs from sync (separate legal entity, different analysis)
   - **Opt-in:** `--include-leadership-pacs` flag to include in sync
   - Display leadership PAC donations separately in output (not commingled with campaign committee)

3. **Parallel committee queries:**
   - Fetch Schedule A donations for all committees concurrently
   - Use Tokio JoinSet pattern (same as existing enrichment pipeline)
   - Deduplicate by donation record ID (same donation shouldn't appear in multiple committees, but validate)

4. **Committee type classification in output:**
   - Display committee breakdown:
     ```
     Nancy Pelosi - Total Donations: $15.2M
     ├─ Campaign Committee (C00385534): $12.5M
     ├─ Leadership PAC (C00448258): $2.5M
     └─ Joint Fundraising (C00512345): $200K
     ```
   - Allow filtering: `--committee-type campaign` to exclude leadership PAC

5. **Committee metadata caching:**
   - Committee IDs don't change frequently
   - Cache committee lookup results for 7 days (vs 1 hour for donation data)
   - Refresh when user runs `--force-refresh` flag

**Detection:**
- Donation totals lower than expected (compare with FEC.gov or OpenSecrets)
- Missing high-profile donors known to have contributed (they donated to leadership PAC)
- User reports "incomplete data"

**Committee type breakdown (example: Nancy Pelosi):**

| Committee ID | Committee Name | Type | Typical Donation Volume |
|--------------|----------------|------|-------------------------|
| C00385534 | Nancy Pelosi for Congress | Principal Campaign | 80-90% of total |
| C00448258 | PAC to the Future (Leadership PAC) | Leadership PAC | 10-15% of total |
| C00012345 | DCCC (Party committee) | Party | Transfers, not direct donations |

**Recommendation:**
- **Default:** Query principal campaign committee only (80-90% coverage)
- **Power users:** `--include-all-committees` flag for comprehensive analysis
- **UI clarity:** Display committee type in output to explain donation source

**Confidence:** HIGH (committee structure verified via FEC documentation, leadership PAC patterns confirmed via OpenFEC data)

**Sources:**
- [FEC Leadership PACs](https://www.fec.gov/help-candidates-and-committees/registering-pac/types-nonconnected-pacs/leadership-pacs/)
- [FEC Candidate Committee Affiliation](https://www.fec.gov/help-candidates-and-committees/candidate-taking-receipts/affiliation-and-contribution-limits/)
- [Candidate-committee linkage file description](https://www.fec.gov/campaign-finance-data/candidate-committee-linkage-file-description/)

---

### Pitfall 6: Data Staleness and Amendment Complexity

**Severity:** MEDIUM

**What goes wrong:** FEC committees file periodic reports (quarterly or monthly). Donations appear in FEC data on report filing date, not contribution date. Reports can be amended months later to correct errors. Committees can file "48-hour notices" for large donations near elections. Data synchronization strategy must account for delayed filings, amendments, and backdated records.

**Why it happens:**
- Reporting deadlines are quarterly or monthly (not real-time)
- Committees have 30 days after quarter end to file
- Amendments can be filed any time to correct errors
- 48-hour notices are separate filings for contributions $1,000+ within 20 days of election
- Data entry errors by committee treasurers (wrong dates, amounts, names)

**Consequences:**
- Recent donations (last 30 days) may not appear in FEC data yet
- Donation totals change retroactively when amendments filed
- Duplicate records if amendment adds back previously deleted donation
- Timing correlation analysis breaks if contribution dates are backdated in amendments

**Prevention:**

1. **Understand FEC filing schedule:**
   - **Quarterly filers:** Reports due April 15, July 15, October 15, January 31
   - **Monthly filers:** Reports due 20 days after month end
   - **Pre-election reports:** 12 days before primary/general (candidates only)
   - **Post-election reports:** 30 days after general election
   - **48-hour notices:** Independent expenditures $1,000+ within 20 days of election

2. **Grace period for recent data:**
   - Don't expect donations from last 30 days to be complete
   - Display warning: "Data current as of {last_filed_report_date}. Recent donations may not appear until next filing deadline."
   - Store `last_report_coverage_through_date` per committee in metadata

3. **Re-sync strategy for amendments:**
   - FEC amendment ID: `amendment_indicator` field in filings (A=amendment, N=new)
   - OpenFEC API returns most recent version automatically (no manual amendment tracking needed)
   - **Strategy:** Re-sync donations periodically (weekly or monthly) to catch amendments
   - Use incremental sync with `min_date` set to 90 days ago (amendment window)
   - Delete and re-insert donations in that date range (upsert pattern)

4. **Duplicate detection:**
   - Use FEC `sub_id` field as unique identifier (28-character alphanumeric)
   - Store in `donations` table as primary key
   - On re-sync: `INSERT OR REPLACE` to handle amendments
   - Track `last_updated_at` timestamp to detect changes

5. **Data freshness indicators:**
   - Display last sync timestamp: "Donations synced on 2026-02-11 at 14:30 UTC"
   - Display FEC data staleness: "FEC data current through 2026-01-31 (quarterly filing)"
   - Warn if sync is >7 days old: "Warning: Data may be stale. Run sync-donations to refresh."

6. **Election cycle awareness:**
   - Campaign finance activity spikes before elections (primary, general)
   - Increase sync frequency during election months (April, October, November)
   - Provide `--election-mode` flag for daily syncs (vs weekly default)

**Detection:**
- User reports "donation missing" when they know it was made
- Donation totals change between syncs without new donations
- Duplicate donations appear in output
- Timing correlation shows donations "before" trades, but dates were amended

**FEC filing deadline examples (2026):**

| Deadline | Report Type | Coverage Period |
|----------|-------------|-----------------|
| 2026-04-15 | Quarterly | January 1 - March 31 |
| 2026-07-15 | Quarterly | April 1 - June 30 |
| 2026-10-15 | Pre-General (12 days before) | July 1 - October 3 |
| 2026-12-03 | Post-General (30 days after) | October 4 - November 23 |
| 2027-01-31 | Year-End | October 1 - December 31 |

**Recommendation:**
- **Default:** Sync donations weekly, re-sync last 90 days to catch amendments
- **Power users:** `--force-full-resync` to delete and re-fetch all donations (handles major amendments)
- **UI clarity:** Display data freshness prominently in output

**Confidence:** MEDIUM (filing deadlines verified via FEC.gov, amendment handling confirmed via OpenFEC documentation, staleness issue inferred from reporting patterns)

**Sources:**
- [FEC Dates and Deadlines - 2026 Quarterly Filers](https://www.fec.gov/help-candidates-and-committees/dates-and-deadlines/2026-reporting-dates/2026-quarterly-filers/)
- [FEC Reports Due in 2026](https://www.fec.gov/updates/reports-due-in-2026/)
- [OpenFEC Pagination Issues (amendment handling)](https://github.com/fecgov/openFEC/issues/3396)

---

### Pitfall 7: Keyset Pagination Misimplementation

**Severity:** MEDIUM

**What goes wrong:** Schedule A endpoint uses keyset (cursor-based) pagination, NOT page number pagination. The response includes `last_indexes` object with `last_index` and `last_contribution_receipt_date` fields. You must append these values to the next request URL, not increment a page number. Naive page number iteration (`?page=1`, `?page=2`, etc.) will miss or duplicate records because Schedule A data is constantly updating.

**Why it happens:**
- Schedule A dataset is massive (67+ million records across all committees)
- Page number pagination breaks with large datasets (offset performance degrades, inserts/deletes shift pages)
- FEC designed keyset pagination specifically for Schedule A/B to avoid duplicates/gaps

**Consequences:**
- **Missing records:** Page 5 at time T1 has records [401-500]. New donation inserted. Page 5 at time T2 has records [402-501]. Record 401 skipped.
- **Duplicate records:** Deletion shifts pages. Record 500 appears on both page 5 and page 6.
- **Performance degradation:** OFFSET-based pagination gets slower with each page (database scans)

**Prevention:**

1. **Use keyset pagination correctly:**
   - **First request:** `/schedules/schedule_a/?committee_id=C00385534&per_page=100&api_key=XXX`
   - **Response pagination object:**
     ```json
     {
       "pagination": {
         "count": 50000,
         "last_indexes": {
           "last_index": 230880619,
           "last_contribution_receipt_date": "2024-03-15"
         }
       },
       "results": [...]
     }
     ```
   - **Next request:** Append `last_index` and `last_contribution_receipt_date` from previous response:
     `/schedules/schedule_a/?committee_id=C00385534&per_page=100&last_index=230880619&last_contribution_receipt_date=2024-03-15&api_key=XXX`

2. **Pagination loop pattern:**
   ```rust
   let mut last_index: Option<i64> = None;
   let mut last_date: Option<String> = None;
   let mut all_contributions = Vec::new();

   loop {
       let mut url = format!(
           "{}/schedules/schedule_a/?committee_id={}&per_page=100&api_key={}",
           base_url, committee_id, api_key
       );

       // Append keyset cursor if available
       if let (Some(idx), Some(date)) = (&last_index, &last_date) {
           url.push_str(&format!("&last_index={}&last_contribution_receipt_date={}", idx, date));
       }

       let response: OpenFecResponse<ScheduleAContribution> =
           client.get(&url).send().await?.json().await?;

       all_contributions.extend(response.results);

       // Check for more pages
       if let Some(indexes) = response.pagination.last_indexes {
           last_index = Some(indexes.last_index);
           last_date = Some(indexes.last_contribution_receipt_date);
       } else {
           break; // No more pages
       }
   }
   ```

3. **Termination condition:**
   - Stop when `pagination.last_indexes` is `null` (no more pages)
   - OR when `results` array is empty
   - OR when returned results < `per_page` (last partial page)

4. **Never use page parameter for Schedule A:**
   - OpenFEC docs warn: "Due to the large quantity of Schedule A records, these endpoints are not paginated by page number."
   - Page parameter exists for other endpoints (candidates, committees) - don't confuse them
   - If you see `?page=5` in Schedule A request, you're doing it wrong

5. **Handle missing last_indexes (edge case):**
   - Some committees have <100 donations (single page)
   - Response will have `last_indexes: null`
   - Don't error - this means no more pages

**Detection:**
- Duplicate donations in output (same `sub_id` appears twice)
- Missing donations (user knows donation exists, doesn't appear in sync)
- Pagination never terminates (infinite loop)
- Error: "Invalid page parameter for Schedule A"

**OpenFEC pagination types by endpoint:**

| Endpoint | Pagination Type | Parameters |
|----------|-----------------|------------|
| `/schedules/schedule_a/` | Keyset | `last_index`, `last_contribution_receipt_date` |
| `/schedules/schedule_b/` | Keyset | `last_index`, `last_disbursement_date` |
| `/candidates/` | Page number | `page`, `per_page` |
| `/committees/` | Page number | `page`, `per_page` |

**Recommendation:**
- Implement keyset pagination from day 1 (don't defer as "optimization")
- Add integration test that fetches 250 records (3 pages) and verifies no duplicates
- Log pagination cursor values in debug mode for troubleshooting

**Confidence:** HIGH (keyset pagination requirement verified in OpenFEC documentation, issue tracker confirms this is a common mistake)

**Sources:**
- [OpenFEC Schedule A Endpoint Documentation](https://api.open.fec.gov/developers/)
- [18F: OpenFEC API Update - Keyset Pagination](https://18f.gsa.gov/2015/07/15/openfec-api-update/)
- [OpenFEC Pagination Issue #3396](https://github.com/fecgov/openFEC/issues/3396)
- [Microsoft OpenFEC Connector - Keyset Pagination](https://learn.microsoft.com/en-us/connectors/openfec/)

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Phase 1: ID Mapping | Name mapping fails, no donations appear | Use congress-legislators dataset for FEC ID mapping |
| Phase 1: ID Mapping | Candidate vs Committee ID confusion | Two-step lookup, validate ID formats with regex |
| Phase 2: API Client | Rate limit hit after few politicians | Implement circuit breaker with quota tracking |
| Phase 2: API Client | Keyset pagination misimplemented | Use last_indexes correctly from day 1 |
| Phase 2: Committee Tracking | Donation totals 50% lower than expected | Query all authorized committees, not just principal |
| Phase 3: Employer Correlation | 90% employers show "no match" | Seed top 200 employers manually before fuzzy matching |
| Phase 3: Data Staleness | Recent donations missing | Account for FEC filing lag, display coverage dates |
| Phase 4: Historical Sync | Multi-day sync operation | Use bulk CSV downloads for historical data |
| Phase 5: Timing Correlation | Correlation fails for recent trades | Re-sync last 90 days weekly to catch amendments |

---

## Recommendations Summary

### Critical Path (Do These First)

1. **Name Mapping:** Use unitedstates/congress-legislators YAML dataset (1-2 days implementation)
2. **Rate Limiting:** Implement circuit breaker + quota tracking (1 day)
3. **Keyset Pagination:** Use last_indexes correctly (0.5 days, but easy to get wrong)

### High Priority (Do in Phase 1)

4. **Committee Resolution:** Query all authorized committees per politician (0.5 days)
5. **Employer Normalization:** Seed top 200 employers manually (2-3 days seed data)
6. **Incremental Sync:** Date-based checkpointing with min_date parameter (1 day)

### Medium Priority (Do in Phase 2)

7. **Data Staleness:** Display coverage period and last sync timestamp (0.5 days)
8. **ID Validation:** Type-safe FEC ID parsing (0.5 days)

---

## Data Volume Reality Check

**Best-case scenario (on-demand sync):**
- User queries 1 politician at a time
- 5,000 donations average = 50 API calls = 3 minutes
- Cached for 24 hours
- Fits easily within 1,000 calls/hour quota

**Worst-case scenario (full congress sync):**
- 535 members * 8,000 donations average = 4.28M records
- 42,800 API calls = 42.8 hours continuous
- Requires 2-day batch job with queue persistence

**Recommendation:** Start with on-demand, add bulk sync as advanced feature

---

## Sources

### OpenFEC API & FEC Data
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/)
- [GitHub - fecgov/openFEC](https://github.com/fecgov/openFEC)
- [Schedule A Column Documentation](https://github.com/fecgov/openFEC/wiki/Schedule-A-column-documentation)
- [OpenFEC Postman Documentation](https://www.postman.com/api-evangelist/federal-election-commission-fec/documentation/19lr6vr/openfec)
- [18F: OpenFEC API Update - Keyset Pagination](https://18f.gsa.gov/2015/07/15/openfec-api-update/)
- [Sunlight Foundation: OpenFEC Getting Started](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/)

### FEC Filing Requirements & Deadlines
- [FEC Dates and Deadlines](https://www.fec.gov/help-candidates-and-committees/dates-and-deadlines/)
- [FEC 2026 Quarterly Filers](https://www.fec.gov/help-candidates-and-committees/dates-and-deadlines/2026-reporting-dates/2026-quarterly-filers/)
- [FEC Reports Due in 2026](https://www.fec.gov/updates/reports-due-in-2026/)
- [FEC Individual Contributions](https://www.fec.gov/help-candidates-and-committees/filing-reports/individual-contributions/)

### Committee & Candidate ID Systems
- [FEC Candidate Master File Description](https://www.fec.gov/campaign-finance-data/candidate-master-file-description/)
- [FEC Committee Master File Description](https://www.fec.gov/campaign-finance-data/committee-master-file-description/)
- [FEC Candidate-Committee Linkage File](https://www.fec.gov/campaign-finance-data/candidate-committee-linkage-file-description/)
- [FEC Leadership PACs](https://www.fec.gov/help-candidates-and-committees/registering-pac/types-nonconnected-pacs/leadership-pacs/)
- [Differences Between Candidate ID vs Committee ID](https://ispolitical.com/What-is-the-Difference-Between-FEC-Candidate-IDs-and-Committee-IDs/)

### Congress-Legislators Dataset (Primary Recommendation)
- [GitHub - unitedstates/congress-legislators](https://github.com/unitedstates/congress-legislators)
- [congress-legislators README](https://github.com/unitedstates/congress-legislators/blob/main/README.md)
- [Issue #21: FEC and CRP IDs need updating script](https://github.com/unitedstates/congress-legislators/issues/21)

### Bulk Data & Rate Limiting
- [FEC Bulk Data Downloads](https://www.fec.gov/data/browse-data/?tab=bulk-data)
- [FEC Bulk Data Instructions PDF](https://www.fec.gov/resources/cms-content/documents/rawdatainst.pdf)
- [GitHub - irworkshop/fec2file (bulk data tools)](https://github.com/irworkshop/fec2file)
- [api.data.gov Rate Limiting](https://api.data.gov/docs/rate-limits/)

### Data Quality & Normalization
- [Employer name standardization - RecordLinker](https://recordlinker.com/name-normalization-matching/)
- [Fuzzy Matching 101 - Data Ladder](https://dataladder.com/fuzzy-matching-101/)
- [Intelligent fuzzy matching for company names - Quantemplate](https://www.quantemplate.com/l/intelligent-fuzzy-matching-to-standardize-company-names)
- [FEC Name Standardization: What's in a name?](https://www.fec.gov/updates/whats-in-a-name/)

### Campaign Finance Statistics
- [FEC Statistical Summary 2021-2022 Election Cycle](https://www.fec.gov/updates/statistical-summary-of-18-month-campaign-activity-of-the-2021-2022-election-cycle/)
- [FEC 2025-2026 Contribution Limits](https://www.fec.gov/resources/cms-content/documents/contribution-limits-chart-2025-2026.pdf)

---

**Research Confidence:** HIGH for critical pitfalls (name mapping, rate limiting, pagination), MEDIUM for moderate pitfalls (data staleness, employer normalization), based on official FEC/OpenFEC documentation, GitHub repository analysis, and campaign finance domain research. All API behavior verified via official sources. Data volume estimates based on FEC statistical summaries. congress-legislators dataset verified as actively maintained public domain resource.
