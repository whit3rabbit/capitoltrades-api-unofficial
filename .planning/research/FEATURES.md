# Feature Landscape: OpenFEC Donation Integration

**Domain:** FEC Campaign Donation Integration for Congressional Trade Analysis
**Researched:** 2026-02-11
**Confidence:** MEDIUM

OpenFEC API provides programmatic access to Federal Election Commission campaign finance data. This research documents available data fields, mapping strategies, and feature recommendations for integrating donation data with Capitol Traders' stock trade analysis.

## Executive Summary

The OpenFEC API exposes Schedule A (individual contributions) data via `/schedules/schedule_a/` endpoint with keyset pagination and 100 calls/hour rate limit. Core challenge: FEC is committee-centric (not politician-centric), requiring multi-step name-to-candidate-to-committee mapping. Employer field is free text (self-reported), making employer-to-issuer correlation complex but valuable.

**Recommended approach:** Phase 1 sync Schedule A data to SQLite with politician-to-committee mapping. Phase 2 add employer-to-issuer fuzzy matching. Phase 3 add timing correlation between donations and trades.

## Schedule A Data Fields

Based on OpenFEC API documentation and Schedule A column documentation, individual contribution records include:

### Core Identification Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `committee_id` | String | FEC committee ID (C + 8 digits) | "C00385534" |
| `committee_name` | String | Committee name | "Pelosi for Congress" |
| `contributor_id` | String | Unique contributor ID (when available) | "C12345678" |
| `transaction_id` | String | Unique transaction ID | "SA11AI.12345" |
| `sub_id` | Integer | OpenFEC internal unique ID | 123456789 |

### Contributor Information Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `contributor_name` | String | Full name of contributor | "John Smith" |
| `contributor_first_name` | String | First name | "John" |
| `contributor_middle_name` | String | Middle name | "A" |
| `contributor_last_name` | String | Last name | "Smith" |
| `contributor_prefix` | String | Name prefix | "Dr." |
| `contributor_suffix` | String | Name suffix | "Jr." |
| `contributor_street_1` | String | Street address line 1 | "123 Main St" |
| `contributor_street_2` | String | Street address line 2 | "Apt 4B" |
| `contributor_city` | String | City | "San Francisco" |
| `contributor_state` | String | Two-letter state code | "CA" |
| `contributor_zip` | String | ZIP code (5 or 9 digits) | "94102" |
| `contributor_employer` | String | Self-reported employer name | "Google LLC" |
| `contributor_occupation` | String | Self-reported occupation | "Software Engineer" |

**Data Quality Notes:**
- Employer and occupation required for contributions >$200, but self-reported (inconsistent spelling/format)
- Name fields can be incomplete (some filings use only `contributor_name`, not split fields)
- Address standardization varies by committee

### Contribution Amount Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `contribution_receipt_amount` | Decimal | Individual contribution amount | 2500.00 |
| `contributor_aggregate_ytd` | Decimal | Year-to-date total from this contributor | 5000.00 |
| `receipt_type` | String | Type of contribution (code) | "15" |
| `receipt_type_full` | String | Full receipt type description | "Contribution" |

**Legal Context:**
- Individual contribution limit (2025-2026 cycle): $3,300 per election
- Primary and general elections have separate limits
- `contributor_aggregate_ytd` useful for identifying max-out donors

### Date and Timing Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `contribution_receipt_date` | Date | Date contribution received | "2024-03-15" |
| `report_year` | Integer | Year of report | 2024 |
| `report_type` | String | Filing report type | "Q1" |
| `election_type` | String | Election type (P=primary, G=general) | "G" |
| `two_year_transaction_period` | Integer | Election cycle (even year) | 2024 |

**Cycle Notes:**
- FEC uses 2-year cycles (2023-2024, 2025-2026)
- Senate candidates report across multiple cycles for 6-year terms
- Use `two_year_transaction_period` to filter by cycle

### Metadata Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `memo_code` | String | "X" if memo entry (excluded from totals) | "X" or null |
| `memo_text` | String | Additional notes/explanation | "Reattribution from joint account" |
| `file_number` | Integer | FEC filing number | 123456 |
| `amendment_indicator` | String | "A" if amended filing | "A" or "N" |
| `image_number` | String | Link to source PDF | "201234567890" |
| `pdf_url` | String | Direct link to PDF | "https://docquery.fec.gov/pdf/..." |

**Important:** `memo_code="X"` means amount excluded from totals (reimbursements, redesignations, reattributions). Filter these out when aggregating totals.

## Candidate-to-Committee Mapping

FEC uses committee_id for donations, not candidate_id. Mapping politicians to committees requires multi-step lookup.

### Step 1: Politician Name to Candidate ID

**Endpoint:** `/candidates/search/?q={name}`

**Example:**
```
GET /v1/candidates/search/?q=Nancy%20Pelosi&api_key={key}
```

**Response Fields (relevant):**
- `candidate_id` - FEC candidate ID (format: H2CA12123 = House, CA district 12, ID 123)
- `name` - Official candidate name
- `office` - H (House), S (Senate), or P (Presidential)
- `state` - Two-letter state code
- `district` - District number (House only, 00 for at-large)
- `party` - Party affiliation
- `election_years` - Array of election years
- `active_through` - Most recent election year

**Candidate ID Format:**
- First letter: H (House), S (Senate), P (Presidential)
- Next 2 letters: State code
- Next 2 digits: District (House) or 00 (Senate/Presidential)
- Last 3 digits: Sequential ID

**Examples:**
- H2CA12123 = House, California district 12
- S2GA00172 = Senate, Georgia
- P00003392 = Presidential

### Step 2: Candidate ID to Committee IDs

**Endpoint:** `/committees/?candidate_id={candidate_id}`

**Example:**
```
GET /v1/committees/?candidate_id=H2CA12123&api_key={key}
```

**Response Fields (relevant):**
- `committee_id` - Committee ID (C + 8 digits)
- `name` - Committee name
- `committee_type` - Committee type code (see below)
- `designation` - P (principal), A (authorized), U (unauthorized)
- `candidate_ids` - Array of associated candidate IDs

**Committee Types:**
- H = House candidate committee
- S = Senate candidate committee
- P = Presidential candidate committee
- X = Non-qualified committee
- Y = Leadership PAC
- Z = Party committee

**Multiple Committees:**
Most politicians have 2-5 associated committees:
- Principal campaign committee (designation=P)
- Joint fundraising committees (designation=A)
- Leadership PACs (committee_type=Y, separate from campaign)

**For complete donation picture:** Query Schedule A for all authorized committees associated with the candidate.

### Step 3: Cache Strategy

Multi-step lookups are expensive (2 API calls per politician). Cache aggressively:

**Cache Key:** `politician_name_normalized`
**Cache Value:**
```json
{
  "candidate_id": "H2CA12123",
  "committee_ids": ["C00385534", "C00475863"],
  "cached_at": "2024-03-15T10:00:00Z"
}
```

**TTL:** 30 days (committee associations rarely change mid-cycle)

**Normalization:** Lowercase, strip punctuation, collapse whitespace
- "Nancy Pelosi" -> "nancy pelosi"
- "Alexandria Ocasio-Cortez" -> "alexandria ocasiocortez"

## OpenFEC API Constraints

### Rate Limiting

- **Limit:** 100 calls per hour via api.data.gov
- **Cannot increase:** api.data.gov enforces globally, no higher tiers
- **Enforcement:** 429 Too Many Requests after limit
- **Reset:** Rolling 1-hour window

**Impact on sync:**
- 50K donation records at 100 records/page = 500 API calls = 5 hours minimum
- Full sync for 535 members of Congress = days to weeks
- **Mitigation:** Incremental sync (date-based), on-demand per politician, aggressive caching

### Pagination

Schedule A uses **keyset pagination** (not page numbers):

**Request:**
```
GET /v1/schedules/schedule_a/?committee_id=C00385534&per_page=100&api_key={key}
```

**Response pagination object:**
```json
{
  "pagination": {
    "count": 12543,
    "per_page": 100,
    "last_indexes": {
      "last_index": "123456789",
      "last_contribution_receipt_date": "2024-03-15"
    }
  }
}
```

**Next page request:**
```
GET /v1/schedules/schedule_a/?committee_id=C00385534&per_page=100&last_index=123456789&last_contribution_receipt_date=2024-03-15&api_key={key}
```

**Key differences from standard pagination:**
- No page numbers
- Must pass `last_index` and `last_contribution_receipt_date` from previous response
- Cannot jump to arbitrary page
- Performance: Constant time (no offset penalty)

### Query Filters

Available Schedule A filters:

| Parameter | Type | Description | Example |
|-----------|------|-------------|---------|
| `committee_id` | String | Filter by committee | `committee_id=C00385534` |
| `contributor_name` | String | Partial name match | `contributor_name=Smith` |
| `contributor_state` | String | Two-letter state | `contributor_state=CA` |
| `contributor_city` | String | City name | `contributor_city=San%20Francisco` |
| `contributor_employer` | String | Employer name | `contributor_employer=Google` |
| `contributor_occupation` | String | Occupation | `contributor_occupation=Engineer` |
| `min_date` | Date | Minimum contribution date | `min_date=2024-01-01` |
| `max_date` | Date | Maximum contribution date | `max_date=2024-12-31` |
| `min_amount` | Decimal | Minimum contribution amount | `min_amount=1000` |
| `max_amount` | Decimal | Maximum contribution amount | `max_amount=5000` |
| `two_year_transaction_period` | Integer | Election cycle | `two_year_transaction_period=2024` |
| `is_individual` | Boolean | Filter to individual contributors | `is_individual=true` |

**Filter caveats:**
- `is_individual=true` should exclude PACs, but known issues with ActBlue conduit contributions showing as committees
- `contributor_name` is substring match (not fuzzy)
- No employer-to-industry mapping in API (must do client-side)

### Sort Options

| Parameter | Values | Description |
|-----------|--------|-------------|
| `sort` | `contribution_receipt_date`, `contributor_aggregate_ytd` | Sort field |
| `sort_order` | `asc`, `desc` | Sort direction |

**Default:** `sort=-contribution_receipt_date` (newest first)

**Pagination requirement:** When sorting by `contribution_receipt_date`, pagination uses `last_contribution_receipt_date` in keyset.

### Data Coverage

**Historical Range:**
- Candidate/committee data: 1980+ (full FEC history)
- Schedule A itemized contributions: **Last 4 years only via API**
- Older Schedule A data: Available via bulk downloads, not API

**Current cycles available (as of 2026-02-11):**
- 2022 (2021-2022 cycle) - complete
- 2024 (2023-2024 cycle) - complete
- 2026 (2025-2026 cycle) - in progress

**Update frequency:** Daily (FEC processes filings daily)

**Amendment handling:** API returns most recent version (amended filings replace original)

## Employer-to-Issuer Correlation Strategy

Core differentiator: Link donation employers to stock issuers to identify conflicts of interest.

### Challenge: Free Text Employer Field

Employer names are self-reported, no standardization:

**Same entity, multiple spellings:**
- "Google"
- "Google Inc"
- "Google LLC"
- "Alphabet Inc"
- "Alphabet"
- "GOOGLE" (all caps)

**Abbreviations:**
- "Goldman Sachs" vs "GS"
- "Bank of America" vs "BofA" vs "BOA"
- "JPMorgan Chase" vs "JP Morgan" vs "JPMC"

**Generic/uninformative entries:**
- "Self-employed"
- "Retired"
- "Not employed"
- "N/A"
- "None"

**Spelling errors:**
- "Mircosoft" (Microsoft)
- "Goolge" (Google)

### Recommended Approach: Tiered Matching

#### Tier 1: Exact Match (High Confidence)

Normalize both employer and issuer, check for exact match:

**Normalization steps:**
1. Lowercase
2. Remove legal suffixes (Inc, LLC, Corp, Corporation, Company, Co, LP, LLP)
3. Remove punctuation (periods, commas, ampersands)
4. Remove "The" prefix
5. Collapse whitespace

**Examples:**
- "The Goldman Sachs Group, Inc." -> "goldman sachs group"
- "Microsoft Corporation" -> "microsoft"
- "Bank of America, N.A." -> "bank of america"

**Confidence:** 100% (exact match after normalization)

#### Tier 2: Fuzzy Match (Medium Confidence)

Use Jaro-Winkler or Levenshtein distance for near matches:

**Jaro-Winkler similarity:** 0.0 (no match) to 1.0 (exact match)

**Thresholds:**
- 0.95+ = High confidence (minor spelling variation)
- 0.85-0.94 = Medium confidence (abbreviation or missing word)
- 0.70-0.84 = Low confidence (flag for manual review)
- <0.70 = Reject (too different)

**Example matches:**
- "Google LLC" vs "Alphabet Inc" -> 0.52 (reject, different legal entities)
- "Goldman Sachs" vs "Goldman Sachs Group" -> 0.92 (medium confidence)
- "Microsoft" vs "Mircosoft" -> 0.96 (high confidence, typo)

**Rust crate:** `strsim` (Jaro-Winkler implementation)

#### Tier 3: Manual Seed Data (Explicit Mapping)

Maintain manual mapping for top employers:

**Seed data format (TOML or SQLite):**
```toml
[employer_mappings]
"Google" = ["GOOGL", "GOOG"]
"Google LLC" = ["GOOGL", "GOOG"]
"Alphabet Inc" = ["GOOGL", "GOOG"]
"Goldman Sachs" = ["GS"]
"Goldman Sachs Group" = ["GS"]
"The Goldman Sachs Group Inc" = ["GS"]
"Bank of America" = ["BAC"]
"BofA" = ["BAC"]
```

**Priority:** Check seed data first (most reliable), then fuzzy match for unknowns

**Maintenance:** Export unmatched employers to CSV, manually review top 100 by donation volume, add to seed data

### Output: Flagged Matches, Not Auto-Links

Never auto-link employer to issuer without user confirmation:

**Correlation output format:**
```
Potential Employer-Issuer Matches:

Employer: "Google LLC"
Issuer: "Alphabet Inc. Class A"
Ticker: GOOGL
Confidence: MEDIUM (0.52 fuzzy match)
Total Donations: $45,000
Total Trade Value: $250,000
Match Count: 23 donations, 5 trades
Action: [CONFIRM] [REJECT] [MANUAL_LINK]
```

**User workflow:**
1. Run correlation analysis
2. Review flagged matches (sorted by confidence descending)
3. Confirm/reject each match
4. Store confirmed mappings in `employer_issuer_links` table

**Schema:**
```sql
CREATE TABLE employer_issuer_links (
    employer_normalized TEXT PRIMARY KEY,
    ticker TEXT NOT NULL,
    confidence REAL,
    confirmed_by_user BOOLEAN DEFAULT 0,
    created_at TEXT
);
```

## Sector-Based Donation Analysis

Group employers by industry sector to compare donation sources to trade portfolio sectors.

### Challenge: No Industry Data in FEC API

FEC does not provide industry/sector classification. Employer field is free text.

### Option 1: Manual Sector Mapping (Recommended for MVP)

Seed top 200 employers by donation volume with sector assignments:

**Sector taxonomy (simplified GICS):**
- Communication Services
- Consumer Discretionary
- Consumer Staples
- Energy
- Financials
- Health Care
- Industrials
- Information Technology
- Materials
- Real Estate
- Utilities
- Other (Non-profit, Government, Self-employed, Retired)

**Seed data format:**
```toml
[employer_sectors]
"Google LLC" = "Information Technology"
"Alphabet Inc" = "Information Technology"
"Goldman Sachs" = "Financials"
"JPMorgan Chase" = "Financials"
"Kaiser Permanente" = "Health Care"
"Boeing" = "Industrials"
```

**Coverage:** Top 200 employers cover approximately 60-70% of total donation volume (Pareto principle)

**Fallback:** "Unknown Sector" for unclassified employers

### Option 2: NAICS Code Lookup (Future Enhancement)

Map employer to NAICS (North American Industry Classification System) code:

**NAICS structure:**
- 2-digit: Sector (e.g., 51 = Information)
- 3-digit: Subsector (e.g., 511 = Publishing Industries)
- 4-digit: Industry Group (e.g., 5112 = Software Publishers)
- 6-digit: Industry (e.g., 511210 = Software Publishers)

**Challenge:** Requires employer name -> company legal name -> NAICS lookup
- No free NAICS API with name-based lookup
- Commercial options: Melissa Data, SIC-NAICS API ($$$)
- SEC EDGAR provides NAICS for public companies (CIK-based, not name-based)

**Verdict:** Defer NAICS integration. Manual sector mapping sufficient for MVP.

### Analysis Output

**Top Industries by Total Contributions:**
```
Politician: Nancy Pelosi (2024 cycle)

Industry                    Total      Donors    Avg
Information Technology      $850,000   234       $3,632
Financials                  $620,000   187       $3,316
Health Care                 $410,000   145       $2,828
Real Estate                 $280,000   98        $2,857
Legal Services              $195,000   67        $2,910
Unknown Sector              $245,000   112       $2,188
```

**Compare to portfolio sector exposure:**
```
Donations by Sector vs Portfolio Holdings by Sector:

Sector                  Donations    Portfolio Value    Delta
Information Technology  $850,000     $1.2M             aligned
Financials             $620,000     $50,000           over-donated
Health Care            $410,000     $800,000          under-donated
```

**Insight:** Politician receives $620K from Finance sector but holds only $50K in financial stocks - potential avoidance of appearance of conflict. Or: Politician receives $850K from Tech sector AND holds $1.2M in tech stocks - potential conflict.

## Donation-to-Trade Timing Correlation

Analyze temporal proximity between donations from employers and trades in related stocks.

### Analysis Approach

For each trade, look back N days (30/60/90) for donations from employers matching the issuer:

**Pseudocode:**
```
for trade in trades:
    employer_matches = fuzzy_match(trade.issuer, all_employers)
    lookback_start = trade.trade_date - 60 days
    lookback_end = trade.trade_date

    related_donations = donations.filter(
        employer IN employer_matches,
        contribution_date BETWEEN lookback_start AND lookback_end
    )

    if related_donations.count() > 0:
        flag_correlation(trade, related_donations)
```

**Output:**
```
Timing Correlation Findings:

Trade: 2024-03-20, BUY $100K-$250K Nvidia (NVDA)
Related Donations (60 days prior):
  - 2024-02-15: John Smith, Nvidia Corporation, $2,900 (35 days before)
  - 2024-03-01: Jane Doe, Nvidia, $1,000 (19 days before)
  Total: $3,900 from 2 donors

Trade: 2024-05-10, SELL $50K-$100K Goldman Sachs (GS)
Related Donations (60 days prior):
  - 2024-03-15: Bob Jones, Goldman Sachs, $5,000 (56 days before)
  Total: $5,000 from 1 donor
```

### Performance Considerations

**Dataset size:**
- Major politician: 10,000 donations/cycle, 200 trades/year
- Naive approach: 10,000 x 200 = 2M comparisons per politician

**Optimization strategies:**

1. **Index donation table by date:**
   ```sql
   CREATE INDEX idx_donations_date ON donations(contribution_receipt_date);
   ```

2. **Pre-filter donations to known employers:**
   Only consider donations from employers present in trade issuers list (reduces 10K donations to ~500 relevant)

3. **Use SQL window functions:**
   ```sql
   SELECT t.trade_date, t.ticker, d.contributor_employer, d.amount
   FROM trades t
   JOIN donations d ON fuzzy_match(t.issuer, d.contributor_employer)
   WHERE d.contribution_receipt_date BETWEEN t.trade_date - 60 AND t.trade_date
   ```

4. **In-memory join for small datasets:**
   If politician has <5K donations, load into memory and iterate (faster than SQL for small N)

**Estimate:** Well-optimized query <1 second for typical politician (1K donations, 100 trades)

### False Positive Handling

**Generic employers produce noise:**
- "Google" - common employer, many donations, most unrelated to specific GOOGL trades
- "Retired" - not actually employer-issuer match

**Mitigation:**
- Exclude generic employer values ("Retired", "Self-employed", "Not employed")
- Require minimum employer match confidence (0.85+ Jaro-Winkler)
- Weight by donation amount (flag $5K donation, ignore $50 donation)

**Output includes confidence score:**
```
Trade: BUY $100K GOOGL
Related Donation: John Smith, "Googel Inc", $2,900
Employer Match Confidence: 0.88 (typo correction)
Temporal Proximity: 20 days
Correlation Score: MEDIUM
```

## Table Stakes Features

Features users expect. Missing = product feels incomplete.

| Feature | Why Expected | Complexity | Priority |
|---------|--------------|------------|----------|
| Schedule A individual contributions sync | Core FEC data - without this, no donation data exists. | MEDIUM | P1 |
| Politician-to-committee mapping | Users expect "Nancy Pelosi" not "C00385534". | MEDIUM | P1 |
| Top donors by amount per politician | "Who funds this politician?" - table stakes for any campaign finance tool. | LOW | P1 |
| Total donations by election cycle | Campaign finance is cycle-based (2-year for House). | LOW | P1 |
| Donation date range filtering | "Show donations from 2024" - expected temporal filtering. | LOW | P1 |
| Employer/occupation display | FEC requires these fields for donations >$200. | LOW | P1 |
| Committee type classification | Users need to know if donations went to campaign vs leadership PAC. | MEDIUM | P1 |
| Sync resumability with checkpoints | FEC data is large (10K-50K donations/politician). Must resume after interruption. | MEDIUM | P1 |
| Donation amount aggregation (YTD) | Schedule A includes year-to-date totals per contributor. | LOW | P1 |
| Data staleness indicators | Campaign finance data updated daily. Users need to know data age. | LOW | P1 |

## Differentiator Features

Features that set product apart. Not expected, but valued.

| Feature | Value Proposition | Complexity | Priority |
|---------|-------------------|------------|----------|
| Employer-to-issuer correlation | "Do donations from Goldman Sachs employees correlate with GS trades?" - unique insight linking money to trades. | HIGH | P2 |
| Sector-based donation analysis | "Which industries fund this politician vs which sectors they trade?" - strategic pattern detection. | MEDIUM | P2 |
| Donation-to-trade timing correlation | "Did they buy defense stocks after receiving defense contractor donations?" - suspicious timing patterns. | HIGH | P3 |
| Top industries by total contributions | OpenSecrets-style industry aggregation. "Tech gave $2M, Finance gave $1.5M". | MEDIUM | P2 |
| Individual donor lookup | "Did Elon Musk donate to this politician?" - reverse search from donor name. | LOW | P2 |
| Committee-level donation breakdown | Show donations to campaign vs leadership PAC - reveals fundraising strategy. | LOW | P2 |
| Max-out donor identification | Contributors who gave legal limit ($3,300). Indicates high-value supporters. | LOW | P2 |
| First-time vs repeat donor analysis | "How many new donors vs recurring donors this cycle?" - fundraising health metric. | MEDIUM | P3 |
| Geographic donor concentration | "50% of donations from CA, 30% from NY" - reveals donor base geography. | LOW | P2 |

## Anti-Features

Features to explicitly NOT build.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Full campaign finance platform | Trying to replicate OpenSecrets adds massive scope. FEC has 100+ filing types. | Focus narrowly on Schedule A individual contributions to authorized committees. Link to OpenSecrets for full analysis. |
| Automatic employer-issuer matching | Employer names are messy. Auto-matching produces false positives. | Flag suggested matches with confidence scores. Require user review/confirmation. Export for manual verification. |
| Legal compliance analysis | FEC contribution limits, coordination rules are complex legal domains with liability. | Provide raw donation data only. Explicit disclaimer: "Not legal advice, not for compliance purposes." |
| Real-time donation alerts | FEC data updated daily, not real-time. Polling/webhooks add complexity without value. | Sync on-demand when user runs sync command. Cache 24 hours. Batch CLI tool, not monitoring platform. |
| Donor identity resolution across name variations | "John Smith" vs "John A Smith" - name matching is AI-hard. FEC doesn't provide unique donor IDs. | Display names as reported in filings. Provide fuzzy search for user-driven lookup. Never auto-merge donors. |
| Industry classification automation | Mapping "Software Engineer at Tech Startup" to NAICS requires NLP or massive lookup. | Seed common employers manually (top 200). Provide "Unknown Sector" bucket. Export unclassified for manual review. |
| Expenditure tracking (Schedule B) | Schedule B is disbursements (spending). Different schema, different analysis. | Phase 1 is receipts (Schedule A) only. Document expenditures as future consideration. |
| Historical amendment tracking | Committees file amendments to correct errors. Tracking revision history is complex. | Use most recent filing data from OpenFEC. FEC API already provides amended records. Don't track revision history. |
| Multi-candidate aggregation | "Show all donations to Democratic senators" - cross-politician reporting tool scope. | Focus on per-politician analysis. Users can export JSON/CSV and aggregate externally. |
| FEC filing document parsing | Direct parsing of FEC PDFs is massive complexity. OpenFEC API already does this. | Use OpenFEC API exclusively. Never parse raw filings. If OpenFEC doesn't have it, document as limitation. |

## Feature Dependencies

```
Schedule A Individual Contributions Sync
    └──requires──> Politician-to-Committee Mapping
                       ├──requires──> Candidate ID Lookup (/candidates/search)
                       └──requires──> Committee Lookup (/committees?candidate_id=X)

Top Donors by Amount
    └──requires──> Schedule A Sync (data must exist)
    └──requires──> Donor Name Aggregation (GROUP BY contributor_name)

Employer-to-Issuer Correlation
    ├──requires──> Schedule A Sync (need employer field)
    ├──requires──> Trade Data (need issuer names)
    ├──requires──> Fuzzy String Matching (Jaro-Winkler or Levenshtein)
    └──requires──> Manual Seed Data (top employer mappings)

Sector-based Donation Analysis
    ├──requires──> Schedule A Sync (need employer field)
    ├──requires──> Employer-to-Sector Mapping (manual seed data)
    └──enhances──> Portfolio Sector Analysis (compare donation sectors to trade sectors)

Donation-to-Trade Timing Correlation
    ├──requires──> Schedule A Sync (need donation dates)
    ├──requires──> Employer-to-Issuer Correlation (link employers to tickers)
    ├──requires──> Trade Data (need trade dates and tickers)
    └──requires──> Date Windowing Logic (look-back: 30/60/90 days)

Top Industries by Total Contributions
    ├──requires──> Employer-to-Sector Mapping
    └──requires──> Sector-based Donation Analysis

Committee-level Breakdown
    ├──requires──> Politician-to-Committee Mapping (know all committees)
    └──requires──> Committee Type Classification (committee metadata from /committees)

Max-out Donor Identification
    ├──requires──> Schedule A Sync (need contributor_aggregate_ytd)
    └──requires──> Legal Limit Constants (contribution limits by cycle: $3,300 for 2025-2026)
```

## MVP Recommendation

### Phase 1: Data Sync and Basic Reporting

**Goal:** Sync Schedule A donations to SQLite, query by politician, display donor list with amounts/employers.

**Must-have features:**
1. Schedule A individual contributions sync
2. Politician-to-committee mapping (with caching)
3. Candidate ID lookup (/candidates/search)
4. Top donors by amount per politician
5. Total donations by election cycle
6. Donation date range filtering (--since/--until)
7. Committee type classification
8. Sync resumability with date-based checkpoints
9. Data staleness indicators (synced_at timestamp)
10. Employer/occupation display

**Output formats:** table, JSON, CSV (follow existing output.rs patterns)

**Estimate:** 2-3 weeks implementation

**Success criteria:** User can run `capitoltraders donations --politician "Nancy Pelosi" --cycle 2024` and see top donors with employers/amounts.

### Phase 2: Employer Correlation

**Goal:** Link donation employers to stock issuers, identify potential conflicts.

**Add features:**
- Employer-to-issuer fuzzy matching (Jaro-Winkler, 0.85+ threshold)
- Manual seed data for top 200 employers (TOML config)
- Flagged matches with confidence scores (not auto-linked)
- User confirmation workflow (review/confirm/reject matches)
- Correlation output: employer, ticker, confidence, donation total, trade value

**Estimate:** 2 weeks implementation + 1 week seed data curation

**Success criteria:** User can run `capitoltraders correlate-donations --politician "Nancy Pelosi"` and review suggested employer-issuer matches.

### Phase 3: Advanced Analysis

**Goal:** Sector analysis and timing correlation.

**Add features:**
- Employer-to-sector mapping (manual seed for top 200)
- Top industries by total contributions
- Committee-level donation breakdown
- Donation-to-trade timing correlation (60-day look-back)
- Geographic donor concentration
- Max-out donor identification

**Estimate:** 3-4 weeks implementation

**Success criteria:** User can identify that politician received $500K from Tech sector, then bought $1M in tech stocks 30 days later.

## Implementation Estimates

| Feature | Complexity | Estimate | Rationale |
|---------|------------|----------|-----------|
| Schedule A sync | MEDIUM | 2-3 days | OpenFEC keyset pagination, rate limiting, incremental sync |
| Politician-to-committee mapping | MEDIUM | 2-3 days | Multi-step lookup, name disambiguation, caching |
| Top donors aggregation | LOW | 1 day | SQL GROUP BY contributor_name, ORDER BY sum DESC |
| Date range filtering | LOW | 0.5 day | WHERE contribution_receipt_date BETWEEN |
| Committee type classification | LOW | 1 day | Fetch committee metadata, map codes to labels |
| Sync resumability | MEDIUM | 1-2 days | Track last_contribution_date, use min_date for incremental |
| Employer-to-issuer fuzzy matching | HIGH | 4-5 days | Jaro-Winkler implementation, threshold tuning, normalization |
| Employer-to-sector mapping | MEDIUM | 2-3 days | Manual seed data curation for top 200 employers |
| Timing correlation | HIGH | 3-4 days | Date windowing SQL, indexed queries, performance optimization |

**Total Phase 1:** 10-15 days (2-3 weeks)
**Total Phase 2:** 10-12 days (2 weeks)
**Total Phase 3:** 15-20 days (3-4 weeks)

## Data Volume Estimates

### Per-Politician Data

**Major politicians (Pelosi, McConnell, leadership):**
- 10,000-50,000 donations per 2-year cycle
- 3-5 associated committees
- 500-1,000 unique donors per cycle

**Average House member:**
- 1,000-5,000 donations per cycle
- 1-2 committees
- 200-500 unique donors

**Average Senator:**
- 5,000-15,000 donations per 6-year term (across 3 cycles)
- 2-3 committees
- 500-1,500 unique donors

### API Call Estimates

**Full sync for one politician:**
- Candidate search: 1 call
- Committee lookup: 1 call
- Schedule A pages: 10-500 calls (100 records/page)
- **Total:** 12-502 calls

**Full sync for all 535 members of Congress:**
- Candidate/committee: 1,070 calls
- Schedule A (average 5K donations each): ~30,000 calls
- **Total:** ~31,000 calls
- **Time at 100 calls/hour:** 310 hours = 13 days continuous

**Mitigation:** On-demand sync per politician (not bulk sync), incremental date-based updates

### Storage Estimates

**Donations table (per politician, 2-year cycle):**
- Average politician: 2,000 donations x 1KB/row = 2 MB
- Major politician: 20,000 donations x 1KB/row = 20 MB
- All 535 members: ~1 GB total (assuming average dataset)

**Committee mappings cache:**
- 535 politicians x 3 committees average = ~1,600 entries
- ~100 KB total

**Employer-issuer seed data:**
- 200 manual mappings x 200 bytes = ~40 KB

**Total database size:** 1-2 GB for full congressional dataset (all members, current cycle)

## Known Data Quality Issues

### Issue: ActBlue/WinRed Conduit Contributions

**Problem:** Donations through ActBlue (Democratic) or WinRed (Republican) often show `contributor_id` as the conduit committee, not the individual donor.

**Impact:** `is_individual=true` filter may include conduit contributions, inflating individual donor counts.

**Mitigation:** Filter out known conduit committee IDs (ActBlue = C00401224, WinRed = C00694323).

**OpenFEC issue:** [GitHub Issue #1426 - Handle itemized receipts from individuals with committee contributor IDs](https://github.com/fecgov/openFEC/issues/1426)

### Issue: Employer Name Inconsistency

**Problem:** Same employer reported dozens of ways:
- "Google" (1,234 records)
- "Google Inc" (567 records)
- "Google LLC" (890 records)
- "Alphabet Inc" (345 records)
- "GOOGLE" (123 records)

**Impact:** Aggregating by raw employer name underreports totals.

**Mitigation:** Normalize employer names before aggregation (lowercase, strip legal suffixes, collapse whitespace).

**Research finding:** [FEC standardizer using Random Forest achieves 0.96 F1 score for donor name clustering](https://github.com/cjdd3b/fec-standardizer/wiki/Defining-donor-clusters) but occasionally groups identical names from same ZIP (false positives).

### Issue: Generic Employer Values

**Problem:** FEC requires employer for donations >$200, but many report:
- "Retired" (10-15% of donations)
- "Self-employed" (5-10%)
- "Not employed"
- "N/A"
- "None"

**Impact:** Cannot correlate these to stock issuers.

**Mitigation:** Exclude from employer-issuer matching. Classify as "Non-employment Income" sector.

### Issue: Occupation Ambiguity

**Problem:** Occupation field is free text, no standardization:
- "Software Engineer" at Google vs "Engineer" at Google vs "SWE" at Google
- "Attorney" vs "Lawyer" vs "Legal Counsel"

**Impact:** Occupation-based filtering unreliable.

**Mitigation:** Don't rely on occupation for critical logic. Display as-is, don't aggregate by occupation.

## Regulatory and Compliance Context

### Individual Contribution Limits (2025-2026 Cycle)

- **Per candidate per election:** $3,300
- **Primary vs general:** Separate limits (total $6,600 if both)
- **National party committee per year:** $41,300
- **State/district/local party committee per year:** $10,000
- **PAC (multicandidate) per year:** $5,000
- **PAC (non-multicandidate) per year:** $10,000

**Source:** [FEC Contribution Limits 2025-2026](https://www.fec.gov/help-candidates-and-committees/candidate-taking-receipts/contribution-limits/)

### Reporting Thresholds

- **$200 threshold:** Contributions over $200 (aggregate per calendar year) must be itemized with name, address, employer, occupation.
- **Under $200:** Reported as aggregate total, no itemization.

**Impact:** Schedule A only shows donations >$200. Small donations ($5, $25) not itemized.

### Schedule A vs Schedule B

- **Schedule A:** Itemized receipts (contributions TO the committee)
- **Schedule B:** Itemized disbursements (expenditures BY the committee)

**This research focuses on Schedule A only.** Schedule B (spending) is separate analysis domain.

### Independent Expenditures (Super PACs)

**Out of scope for Phase 1:**
- Super PACs make "independent expenditures" (not coordinated with candidate)
- Unlimited contributions to Super PACs allowed
- Reported separately from candidate committees
- Different analysis (who's spending on behalf of candidate, not who's funding candidate directly)

**Mitigation:** Document that we track authorized committee contributions only. Link to OpenSecrets for Super PAC analysis.

## Sources

### OpenFEC API Documentation (HIGH Confidence)
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) - Official interactive API docs
- [Schedule A column documentation - fecgov/openFEC Wiki](https://github.com/fecgov/openFEC/wiki/Schedule-A-column-documentation) - Field definitions
- [GitHub - fecgov/openFEC](https://github.com/fecgov/openFEC) - Source code and issue tracker
- [18F - OpenFEC API Update - 67 million more records](https://18f.gsa.gov/2015/07/15/openfec-api-update/) - API design rationale
- [OpenFEC makes campaign finance data more accessible - Sunlight Foundation](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/) - Getting started guide

### FEC Filing Requirements (HIGH Confidence)
- [Individual contributions - FEC.gov](https://www.fec.gov/help-candidates-and-committees/filing-reports/individual-contributions/) - Official filing rules
- [Contribution limits for 2025-2026](https://www.fec.gov/help-candidates-and-committees/candidate-taking-receipts/contribution-limits/) - Current limits
- [Committee master file description - FEC.gov](https://www.fec.gov/campaign-finance-data/committee-master-file-description/) - Committee data fields
- [Candidate master file description - FEC.gov](https://www.fec.gov/campaign-finance-data/candidate-master-file-description/) - Candidate ID format

### Candidate-Committee Mapping (MEDIUM Confidence)
- [Differences Between Candidate ID vs Committee ID - FEC](https://ispolitical.com/What-is-the-Difference-Between-FEC-Candidate-IDs-and-Committee-IDs/) - ID format explanation
- [Registering a committee - FEC.gov](https://www.fec.gov/help-candidates-and-committees/filing-reports/registering-committee/) - Committee types
- [OpenFEC (Independent Publisher) - Microsoft Learn](https://learn.microsoft.com/en-us/connectors/openfec/) - API connector documentation

### Data Quality and Normalization (MEDIUM Confidence)
- [Defining donor clusters - fec-standardizer Wiki](https://github.com/cjdd3b/fec-standardizer/wiki/Defining-donor-clusters) - Random Forest donor name matching (0.96 F1)
- [Contributions by individuals file description - FEC.gov](https://www.fec.gov/campaign-finance-data/contributions-individuals-file-description/) - Bulk data format
- [Handle itemized receipts with committee contributor IDs - Issue #1426](https://github.com/fecgov/openFEC/issues/1426) - ActBlue conduit issue
- [Resolve individual vs committee filtering issues - Issue #1779](https://github.com/fecgov/openFEC/issues/1779) - is_individual filter bugs

### Employer-Issuer Correlation (MEDIUM Confidence)
- [SEC EDGAR - Company Search](https://www.sec.gov/edgar/searchedgar/companysearch.html) - Public company lookup
- [SEC EDGAR APIs](https://www.sec.gov/search-filings/edgar-application-programming-interfaces) - Company metadata JSON
- [Yahoo Finance Stocklist Scraper](https://github.com/jaungiers/Yahoo-Finance-Stocklist-Scraper) - Ticker to company name mapping
- [SEC CIK/CUSIP/Ticker service](https://github.com/danielsobrado/edgar-cik-cusip-ticker-sector-service) - Multi-identifier mapping

### Industry Classification (LOW Confidence)
- [OpenSecrets Industry Codes](https://www.opensecrets.org/open-data/api-documentation) - Manual industry categorization
- [OpenSecrets Bulk Data](https://www.opensecrets.org/open-data/bulk-data) - CRP_Categories.txt industry mapping
- [Employer & Industry - Adept ID](https://docs.adept-id.com/docs/employer-industry) - Employer taxonomy concepts
- [BLS Industries by NAICS Code](https://www.bls.gov/iag/tgs/iag_index_naics.htm) - NAICS structure (no employer name mapping)

### API Pagination and Rate Limits (MEDIUM Confidence)
- [18F - OpenFEC API rate limiting](https://github.com/fecgov/openFEC) - 100 calls/hour via API Umbrella
- [OpenFEC API makes new itemized data available - Sunlight Foundation](https://sunlightfoundation.com/2015/08/18/openfec-api-makes-new-itemized-data-available/) - Keyset pagination design

### Campaign Finance Analysis Tools (Context)
- [OpenSecrets](https://www.opensecrets.org/) - Competitive analysis (industry aggregation)
- [OpenSecrets Donor Lookup](https://www.opensecrets.org/donor-lookup) - Individual donor search
- [FEC Campaign Finance Data](https://www.fec.gov/data/browse-data/) - Official FEC web interface

### Congressional Trading and Donations (Context)
- [Congressional Stock Trading Conflicts - Campaign Legal Center](https://campaignlegal.org/update/congressional-stock-trading-continues-raise-conflicts-interest-concerns) - Ethics context
- [InsiderFinance Congress Trades Tracker](https://www.insiderfinance.io/congress-trades) - Competitive landscape

---

**Confidence Assessment:**
- **API Fields and Endpoints:** HIGH (verified via OpenFEC docs and GitHub)
- **Candidate-Committee Mapping:** MEDIUM (documented in API, but multi-step complexity)
- **Employer-Issuer Correlation:** MEDIUM (approach documented in research, but no production implementation reference)
- **Industry Classification:** LOW (no authoritative free API found, manual mapping recommended)
- **Data Quality Issues:** MEDIUM (documented in GitHub issues, community research)

**Research Gaps:**
- No official OpenFEC field-by-field data dictionary found (GitHub wiki partially complete)
- Industry/sector classification not provided by FEC API (must build externally)
- ActBlue conduit contribution filtering not fully documented (known issue, no official solution)
- Employer name normalization best practices scattered across community tools (no single authority)
