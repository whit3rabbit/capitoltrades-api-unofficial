# Phase 16: Conflict Detection - Research

**Researched:** 2026-02-15
**Domain:** Congressional committee jurisdiction mapping, sector-trade correlation, donation-trade correlation, conflict scoring
**Confidence:** MEDIUM-HIGH

## Summary

Phase 16 implements conflict-of-interest detection by flagging trades where a politician's committee jurisdiction overlaps with the traded security's sector, and where donors' employers match traded issuers. The technical foundation combines static committee-sector jurisdiction mapping (YAML-based, similar to Phase 13 GICS mapping), SQL JOIN-based correlation queries (similar to Phase 12 employer correlation), and new analytics calculations for committee trading scores.

The core challenge is mapping congressional committee jurisdictions (broad legislative domains like "financial services" or "energy and commerce") to GICS sectors (11 standardized classifications). This requires manual curation of committee-sector mappings since no authoritative source exists. The House Energy and Commerce Committee has the broadest jurisdiction (telecommunications, health, energy, environment, consumer protection), overlapping with 8 of 11 GICS sectors.

**Primary recommendation:** Use YAML-based committee-sector mapping (similar to gics_sector_mapping.yml pattern), store committee jurisdiction flags on fec_committees table, compute committee trading score as percentage of closed trades in committee-related sectors, and add donation-trade correlation flag when employer_mappings links a donor's employer to a traded issuer. Include disclaimer "based on current committee assignments" since point-in-time membership tracking is out of scope.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde_yml | 0.0.12 | Committee-sector mapping YAML parsing | Already used for GICS sector mapping (Phase 13) and FEC legislator mapping (v1.2 Phase 7) |
| rusqlite | 0.32 | SQLite DB access for correlation queries | Project standard, window functions for percentile ranking |
| Standard library | - | HashSet for sector membership checks | No external dependency needed for set operations |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| None needed | - | - | All requirements satisfied by existing stack |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Static YAML mapping | Congress.gov API scraping | API has no structured committee jurisdiction data; HTML scraping is brittle and violates robots.txt |
| Manual committee curation | GovTrack.us data | GovTrack provides committee memberships but not jurisdiction-to-sector mapping; still requires manual work |
| Committee name matching | FEC committee_type classification | FEC committees are fundraising entities, not legislative committees; wrong data source |

**Installation:**
No new dependencies required. All functionality uses existing stack from Phase 13 (YAML parsing) and Phase 12 (correlation logic).

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── committee_jurisdiction.rs  # NEW: Committee-sector mapping, validation
├── conflict.rs                # NEW: Conflict scoring, correlation detection
├── db.rs                      # EXTEND: Add conflict query methods
└── lib.rs                     # EXTEND: pub use conflict types

capitoltraders_cli/src/commands/
├── analytics.rs               # EXTEND: Add conflict scoring to leaderboard
└── conflicts.rs               # NEW: Dedicated CLI subcommand for conflict queries

seed_data/
└── committee_sectors.yml      # NEW: Committee-sector jurisdiction mapping
```

### Pattern 1: Committee-Sector Jurisdiction Mapping (Static YAML)
**What:** Define which GICS sectors fall under each congressional committee's legislative jurisdiction
**When to use:** One-time manual curation, validated on load, applied to issuers via JOIN

**Example:**
```yaml
# seed_data/committee_sectors.yml
# Source: House Rules, Senate Rules, committee jurisdiction statements
# Note: Many committees have overlapping jurisdictions (e.g., both Banking and Energy committees oversee some Utilities)

committees:
  - committee_name: "House Financial Services"
    chamber: "House"
    sectors:
      - "Financials"
    notes: "Banking, housing, insurance, securities exchanges"

  - committee_name: "House Energy and Commerce"
    chamber: "House"
    sectors:
      - "Energy"
      - "Utilities"
      - "Communication Services"
      - "Health Care"
      - "Consumer Discretionary"
      - "Consumer Staples"
    notes: "Broadest jurisdiction: telecommunications, health, energy, environment, consumer protection, interstate commerce"

  - committee_name: "House Agriculture"
    chamber: "House"
    sectors:
      - "Consumer Staples"  # Food, agriculture, forestry
    notes: "Agriculture, food, rural development, forestry"

  - committee_name: "House Transportation and Infrastructure"
    chamber: "House"
    sectors:
      - "Industrials"  # Aviation, maritime, railroads
      - "Real Estate"  # Public buildings, infrastructure
    notes: "Transportation, infrastructure, public buildings"

  - committee_name: "House Ways and Means"
    chamber: "House"
    sectors: []  # No sector-specific jurisdiction, tax policy applies to all
    notes: "Tax, trade, Social Security - affects all sectors equally"

  - committee_name: "Senate Banking, Housing, and Urban Affairs"
    chamber: "Senate"
    sectors:
      - "Financials"
    notes: "Banking, insurance, financial markets, securities, housing"

  - committee_name: "Senate Commerce, Science, and Transportation"
    chamber: "Senate"
    sectors:
      - "Communication Services"
      - "Information Technology"
      - "Industrials"
    notes: "Science, technology, telecommunications, transportation, consumer affairs"

  - committee_name: "Senate Energy and Natural Resources"
    chamber: "Senate"
    sectors:
      - "Energy"
      - "Materials"
    notes: "Energy policy, mining, national parks, public lands"

  - committee_name: "Senate Health, Education, Labor, and Pensions"
    chamber: "Senate"
    sectors:
      - "Health Care"
    notes: "Public health, biomedical research, education, labor"
```

```rust
// capitoltraders_lib/src/committee_jurisdiction.rs
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize, Debug, Clone)]
pub struct CommitteeJurisdiction {
    pub committee_name: String,
    pub chamber: String,
    pub sectors: Vec<String>,
    pub notes: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CommitteeJurisdictionFile {
    committees: Vec<CommitteeJurisdiction>,
}

pub fn load_committee_jurisdictions() -> Result<Vec<CommitteeJurisdiction>, SectorMappingError> {
    let yaml_content = include_str!("../../seed_data/committee_sectors.yml");
    let file: CommitteeJurisdictionFile = serde_yml::from_str(yaml_content)?;

    // Validate all sectors against GICS_SECTORS constant (from sector_mapping.rs)
    for committee in &file.committees {
        for sector in &committee.sectors {
            validate_sector(sector)?;  // Reuse from Phase 13
        }
    }

    Ok(file.committees)
}
```

### Pattern 2: Committee Trading Score Calculation
**What:** Compute percentage of politician's closed trades that are in committee-related sectors
**When to use:** Analytics leaderboard, conflict scoring

**Example:**
```rust
// capitoltraders_lib/src/conflict.rs
use std::collections::HashSet;

pub struct CommitteeTradingScore {
    pub politician_id: String,
    pub committee_names: Vec<String>,
    pub total_closed_trades: usize,
    pub committee_related_trades: usize,
    pub committee_trading_pct: f64,
}

/// Calculate committee trading score for a politician
pub fn calculate_committee_trading_score(
    closed_trades: &[ClosedTrade],  // From Phase 15 analytics
    politician_committees: &[String],  // Committee names from fec_committees
    committee_jurisdictions: &[CommitteeJurisdiction],  // From YAML
) -> CommitteeTradingScore {
    // Build set of sectors under politician's committee jurisdictions
    let mut committee_sectors = HashSet::new();
    for committee_name in politician_committees {
        if let Some(jurisdiction) = committee_jurisdictions.iter()
            .find(|j| j.committee_name == *committee_name) {
            for sector in &jurisdiction.sectors {
                committee_sectors.insert(sector.clone());
            }
        }
    }

    // Count trades in committee-related sectors
    let committee_related_count = closed_trades
        .iter()
        .filter(|trade| {
            // Requires issuer.gics_sector to be populated (Phase 13)
            if let Some(sector) = &trade.issuer_sector {
                committee_sectors.contains(sector)
            } else {
                false
            }
        })
        .count();

    let total = closed_trades.len();
    let pct = if total > 0 {
        (committee_related_count as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    CommitteeTradingScore {
        politician_id: closed_trades.first().map(|t| t.politician_id.clone()).unwrap_or_default(),
        committee_names: politician_committees.to_vec(),
        total_closed_trades: total,
        committee_related_trades: committee_related_count,
        committee_trading_pct: pct,
    }
}
```

### Pattern 3: Donation-Trade Correlation Detection
**What:** Flag trades where donor's employer matches traded issuer (via employer_mappings table from Phase 12)
**When to use:** Per-trade conflict flag, correlation summary queries

**Example:**
```rust
// SQL query for donation-trade correlation
const DONATION_TRADE_CORRELATION_SQL: &str = "
    WITH politician_donations AS (
        SELECT DISTINCT
            dsm.politician_id,
            d.contributor_employer,
            em.issuer_ticker,
            em.confidence as mapping_confidence
        FROM donations d
        JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
        JOIN employer_mappings em ON LOWER(d.contributor_employer) = em.normalized_employer
        WHERE d.contributor_employer IS NOT NULL
        AND em.confidence >= 0.85
    )
    SELECT
        ct.politician_id,
        ct.ticker,
        COUNT(DISTINCT pd.contributor_employer) as matching_donor_count,
        AVG(pd.mapping_confidence) as avg_mapping_confidence,
        GROUP_CONCAT(DISTINCT pd.contributor_employer) as donor_employers
    FROM closed_trades ct
    LEFT JOIN politician_donations pd
        ON ct.politician_id = pd.politician_id
        AND ct.ticker = pd.issuer_ticker
    WHERE pd.contributor_employer IS NOT NULL
    GROUP BY ct.politician_id, ct.ticker
    HAVING matching_donor_count > 0
";

pub struct DonationTradeCorrelation {
    pub politician_id: String,
    pub ticker: String,
    pub matching_donor_count: i64,
    pub avg_mapping_confidence: f64,
    pub donor_employers: String,  // Comma-separated list
}
```

### Pattern 4: Committee-Related Trade Flag (Per-Trade)
**What:** Boolean flag on closed trades indicating if issuer sector matches politician's committee jurisdiction
**When to use:** Trade-level display (trades/portfolio output with --show-conflicts flag)

**Example:**
```sql
-- Add computed column to closed trades query (Phase 15 extension)
SELECT
    ct.*,
    CASE
        WHEN i.gics_sector IN (
            SELECT cjs.sector
            FROM committee_jurisdiction_sectors cjs
            JOIN politician_committees pc ON pc.committee = cjs.committee_name
            WHERE pc.politician_id = ct.politician_id
        ) THEN 1
        ELSE 0
    END as is_committee_related
FROM closed_trades ct
JOIN issuers i ON ct.issuer_id = i.issuer_id
```

### Pattern 5: Conflict Query Filters (CLI Subcommand)
**What:** New `capitoltraders conflicts` command with politician/committee/sector filters
**When to use:** User wants to query conflict signals directly

**Example:**
```rust
// capitoltraders_cli/src/commands/conflicts.rs
#[derive(Args)]
pub struct ConflictsArgs {
    /// Filter by politician name
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by committee name
    #[arg(long)]
    pub committee: Option<String>,

    /// Minimum committee trading percentage threshold (0-100)
    #[arg(long, default_value = "50.0")]
    pub min_committee_pct: f64,

    /// Include donation-trade correlations
    #[arg(long)]
    pub include_donations: bool,

    /// Database path (required)
    #[arg(long)]
    pub db: PathBuf,
}

pub async fn run(args: &ConflictsArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;
    db.init()?;

    // Load committee jurisdictions
    let jurisdictions = load_committee_jurisdictions()?;

    // Query politicians with committee assignments
    let politicians = if let Some(ref name) = args.politician {
        db.find_politician_by_name(name)?
    } else {
        db.get_all_politicians_with_committees()?
    };

    // Compute committee trading scores
    let mut scores = Vec::new();
    for politician in politicians {
        let closed_trades = db.query_closed_trades_for_politician(&politician.id)?;
        let committees = db.get_politician_committee_names(&politician.id)?;

        let score = calculate_committee_trading_score(
            &closed_trades,
            &committees,
            &jurisdictions
        );

        if score.committee_trading_pct >= args.min_committee_pct {
            scores.push(score);
        }
    }

    // Output scores
    match format {
        OutputFormat::Table => print_conflict_table(&scores),
        OutputFormat::Json => print_json(&scores)?,
        // ... other formats
    }

    // Optional: donation correlations
    if args.include_donations {
        let correlations = db.query_donation_trade_correlations()?;
        // ... output correlations
    }

    Ok(())
}
```

### Anti-Patterns to Avoid
- **Don't use FEC committee data for legislative committees:** FEC committees are fundraising entities (campaign committees, leadership PACs). Legislative committees (House Financial Services, Senate Banking) are separate. Use politician_committees table from CapitolTrades scrape data, not fec_committees.
- **Don't auto-flag based on committee membership alone:** Membership doesn't imply jurisdiction over specific sector. Use committee-sector mapping to determine relevance.
- **Don't claim causation:** Correlation is not causation. Flag conflicts as "potential conflict of interest" or "committee-related trade", never "insider trading" or "corruption".
- **Don't use point-in-time committee assignments:** Tracking historical committee membership (what committees politician was on at trade time) is Phase 18+ scope. Phase 16 uses current committee assignments with disclaimer.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Committee jurisdiction definitions | Web scraping Congress.gov/House.gov | Manual YAML curation from official House/Senate Rules | No structured API exists; jurisdiction descriptions are prose in rules documents; manual curation is unavoidable |
| Sector-committee mapping | Machine learning classification | Expert manual mapping with notes/sources | Only ~20 committees x 11 sectors = 220 possible mappings, most are obvious (Financial Services = Financials); ML is overkill and less accurate than human judgment |
| Donation-trade correlation | New fuzzy matching logic | Reuse employer_mappings table from Phase 12 | Already built, validated, user-reviewed; don't duplicate employer normalization logic |
| Committee trading score percentiles | Custom ranking logic | Reuse aggregate_politician_metrics pattern from Phase 15 | Window functions, percentile calculation already proven in Phase 15 analytics module |

**Key insight:** Committee jurisdiction mapping is inherently a manual curation task. The House Energy and Commerce Committee's jurisdiction is described as "broadest of any authorizing committee" in prose; no machine-readable taxonomy exists. Accept manual work, focus on making YAML easy to audit and update.

## Common Pitfalls

### Pitfall 1: Confusing FEC Committees with Legislative Committees
**What goes wrong:** Query uses fec_committees table (fundraising entities) to determine legislative jurisdiction. House Financial Services Committee member shows no "Financial Services" committee in FEC data.
**Why it happens:** Both are called "committees" but serve different purposes. FEC tracks fundraising, CapitolTrades tracks legislative committees.
**How to avoid:** Use politician_committees table from CapitolTrades scrape (populated during sync), NOT fec_committees table. Verify with `SELECT DISTINCT committee FROM politician_committees` shows "House Financial Services", not "PELOSI FOR CONGRESS".
**Warning signs:** Committee trading scores all show 0%. No politicians have committee assignments in conflict queries.

### Pitfall 2: Overlapping Committee Jurisdictions (Double-Counting)
**What goes wrong:** Utilities sector appears in both House Energy and Commerce AND House Transportation committees. Single AAPL trade counted twice when politician serves on both.
**Why it happens:** Jurisdictions overlap by design (shared oversight). Set operations don't deduplicate.
**How to avoid:** Build committee_sectors as HashSet (deduplicates sectors automatically). A trade is committee-related if sector IN any committee's jurisdiction, counted once.
**Warning signs:** Committee trading percentages sum to >100%. Single trade shows multiple committee-related flags.

### Pitfall 3: Missing gics_sector Data (NULL Handling)
**What goes wrong:** Phase 13 only mapped top 200 tickers. Lesser-traded stocks have NULL gics_sector. Committee trading score denominator includes these trades but can never flag them as committee-related. Score artificially deflated.
**Why it happens:** GICS mapping is incomplete by design (phase 13 scope was top 200).
**How to avoid:** Filter `WHERE i.gics_sector IS NOT NULL` in closed trades query for committee scoring. Only score trades where sector is known. Document in disclaimer.
**Warning signs:** Committee trading scores consistently low (<10%) for politicians with obvious sector concentration (e.g., House Financial Services member trading only banks).

### Pitfall 4: Donation-Trade Correlation False Positives (Short Ticker Matches)
**What goes wrong:** Employer "F5 Networks" maps to issuer "F" (Ford). Donation-trade correlation flags politician receiving F5 donations trading Ford stock.
**Why it happens:** employer_mappings uses fuzzy matching with confidence threshold (Phase 12). Short tickers increase collision risk.
**How to avoid:** Filter employer_mappings by confidence >= 0.90 for correlation queries (higher than Phase 12's 0.85 threshold). Review top 50 employer mappings for short ticker false positives.
**Warning signs:** Correlation shows unexpected employer-ticker pairs. Manual review finds employer name has no semantic relation to issuer.

### Pitfall 5: Disclaimer Visibility (Point-in-Time Committee Assignments)
**What goes wrong:** User assumes conflict detection accounts for historical committee membership (politician was on Banking when they traded JPM, now on Agriculture). Query uses current committees.
**Why it happens:** Phase 16 scope excludes historical membership tracking (data not available from CapitolTrades or FEC).
**How to avoid:** Include disclaimer in ALL conflict output: "Based on current committee assignments. Historical committee membership not tracked." Prominently display in table headers and JSON metadata.
**Warning signs:** User asks "Why does this trade not show committee-related when I know they were on that committee?" Manual check shows committee change between trade date and present.

## Code Examples

Verified patterns from existing codebase and standard approaches:

### Committee-Sector YAML Schema Validation
```rust
// Source: Extends Phase 13 sector_mapping.rs pattern
use crate::sector_mapping::{validate_sector, SectorMappingError};

pub fn validate_committee_jurisdictions(
    jurisdictions: &[CommitteeJurisdiction]
) -> Result<(), SectorMappingError> {
    for committee in jurisdictions {
        // Validate all sectors are valid GICS sectors
        for sector in &committee.sectors {
            validate_sector(sector)?;
        }

        // Validate chamber is House or Senate
        if committee.chamber != "House" && committee.chamber != "Senate" {
            return Err(SectorMappingError::InvalidSeedData(
                format!("Invalid chamber '{}' for committee '{}'", committee.chamber, committee.committee_name)
            ));
        }
    }
    Ok(())
}
```

### DB Query for Committee-Related Trades
```rust
// Source: Extends Phase 15 query_closed_trades pattern
pub fn query_committee_related_trades(
    &self,
    politician_id: &str,
) -> Result<Vec<CommitteeRelatedTrade>, DbError> {
    let sql = "
        WITH politician_committee_sectors AS (
            SELECT DISTINCT cjs.sector
            FROM politician_committees pc
            JOIN committee_jurisdiction_sectors cjs
                ON pc.committee = cjs.committee_name
            WHERE pc.politician_id = ?1
        )
        SELECT
            ct.tx_id,
            ct.ticker,
            i.gics_sector,
            ct.absolute_return,
            ct.buy_date,
            ct.sell_date,
            CASE
                WHEN i.gics_sector IN (SELECT sector FROM politician_committee_sectors)
                THEN 1 ELSE 0
            END as is_committee_related,
            (
                SELECT GROUP_CONCAT(pc.committee)
                FROM politician_committees pc
                JOIN committee_jurisdiction_sectors cjs ON pc.committee = cjs.committee_name
                WHERE pc.politician_id = ?1
                AND cjs.sector = i.gics_sector
            ) as related_committees
        FROM closed_trades ct
        JOIN issuers i ON ct.ticker = i.issuer_ticker
        WHERE ct.politician_id = ?1
        AND i.gics_sector IS NOT NULL
    ";

    let mut stmt = self.conn.prepare(sql)?;
    let rows = stmt.query_map([politician_id], |row| {
        Ok(CommitteeRelatedTrade {
            tx_id: row.get(0)?,
            ticker: row.get(1)?,
            gics_sector: row.get(2)?,
            absolute_return: row.get(3)?,
            buy_date: row.get(4)?,
            sell_date: row.get(5)?,
            is_committee_related: row.get::<_, i64>(6)? == 1,
            related_committees: row.get::<_, Option<String>>(7)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
}
```

### Donation-Trade Correlation with Confidence Threshold
```rust
// Source: Extends Phase 12 employer_mappings pattern
pub fn query_donation_trade_correlations(
    &self,
    min_confidence: f64,
) -> Result<Vec<DonationTradeCorrelation>, DbError> {
    let sql = "
        SELECT
            ct.politician_id,
            p.first_name || ' ' || p.last_name as politician_name,
            ct.ticker,
            COUNT(DISTINCT d.contributor_employer) as matching_donor_count,
            AVG(em.confidence) as avg_mapping_confidence,
            GROUP_CONCAT(DISTINCT d.contributor_employer, ', ') as donor_employers,
            SUM(d.contribution_receipt_amount) as total_donation_amount
        FROM closed_trades ct
        JOIN politicians p ON ct.politician_id = p.politician_id
        JOIN issuers i ON ct.ticker = i.issuer_ticker
        JOIN employer_mappings em ON i.issuer_ticker = em.issuer_ticker
        JOIN employer_lookup el ON em.normalized_employer = el.normalized_employer
        JOIN donations d ON LOWER(d.contributor_employer) = el.raw_employer_lower
        JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
            AND dsm.politician_id = ct.politician_id
        WHERE em.confidence >= ?1
        GROUP BY ct.politician_id, ct.ticker
        HAVING matching_donor_count > 0
        ORDER BY matching_donor_count DESC, total_donation_amount DESC
    ";

    let mut stmt = self.conn.prepare(sql)?;
    let rows = stmt.query_map([min_confidence], |row| {
        Ok(DonationTradeCorrelation {
            politician_id: row.get(0)?,
            politician_name: row.get(1)?,
            ticker: row.get(2)?,
            matching_donor_count: row.get(3)?,
            avg_mapping_confidence: row.get(4)?,
            donor_employers: row.get(5)?,
            total_donation_amount: row.get(6)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Point-in-time committee assignment tracking | Current assignment with disclaimer | Unavailable data | Historical committee data not in CapitolTrades or FEC APIs; would require Congressional Research Service datasets or archive scraping |
| Unstructured conflict narratives | Quantified scores (committee trading %, correlation counts) | Modern analytics (2020s) | Enables ranking, filtering, statistical analysis vs qualitative descriptions |
| Manual audit spreadsheets | Automated SQL correlation queries | Phase 16 (2026-02) | Real-time conflict detection vs monthly manual reviews |
| Binary conflict flags (yes/no) | Confidence-scored correlations | Phase 12 employer mapping (2026-02) | Allows thresholding (show only 0.90+ confidence) vs all-or-nothing |

**Deprecated/outdated:**
- **House Rules from 117th Congress (2021-2022):** Committee jurisdictions change with each Congress. Use current 119th Congress rules (2025-2026). Verify committee names and jurisdictions haven't changed.
- **Pre-2018 GICS taxonomy (10 sectors):** Communication Services renamed/expanded from Telecommunication Services in 2018. Use 11-sector taxonomy only.

## Open Questions

1. **Should we create committee_jurisdiction_sectors table or keep YAML only?**
   - What we know: YAML is loaded at startup, parsed into Vec in memory. Committee queries would JOIN against this Vec in Rust logic.
   - What's unclear: If we normalize to a committee_jurisdiction_sectors table (committee_name TEXT, sector TEXT), we can use pure SQL JOINs. Tradeoff is schema complexity vs query performance.
   - Recommendation: Start with YAML-only (keep Phase 13 pattern). If conflict queries are slow (>2s for leaderboard), migrate to schema v8 with table. YAML is easier to audit/update.

2. **How to handle Leadership PACs in committee context?**
   - What we know: Phase 9 CommitteeClass distinguishes Campaign vs LeadershipPac committees. Leadership PACs are NOT legislative committees.
   - What's unclear: Should politician_committees table include leadership PACs, or only legislative committees?
   - Recommendation: Filter politician_committees to exclude leadership PACs using CommitteeClass. Leadership PAC trades have no legislative jurisdiction overlap by definition.

3. **What threshold for "committee trading score" to flag as concerning?**
   - What we know: 50% committee-related trades could be coincidence (Financials = 20% of S&P 500). 90% is likely intentional sector focus.
   - What's unclear: What's the "normal" baseline for random trading across sectors?
   - Recommendation: Default --min-committee-pct to 50.0 in CLI. Document that score interpretation is user judgment, not automated threshold. Include sector concentration (HHI) in Phase 17 for comparison baseline.

4. **Should donation-trade correlation count unique donors or total contribution amount?**
   - What we know: Phase 12 employer_mappings links employer to ticker. Donations table has contribution_receipt_amount field.
   - What's unclear: Is 100 donors giving $100 each (total $10K) more significant than 1 donor giving $10K? Or vice versa?
   - Recommendation: Return BOTH: matching_donor_count (unique employers) AND total_donation_amount. Let user sort/filter by either. Document both metrics in output.

## Sources

### Primary (HIGH confidence)
- [House Financial Services Committee Jurisdiction](https://democrats-financialservices.house.gov/about/jurisdiction.htm) - Official jurisdiction statement
- [Senate Banking Committee Jurisdiction](https://www.banking.senate.gov/about/jurisdiction) - Official jurisdiction statement
- [House Energy and Commerce Committee Jurisdiction](https://www.opensecrets.org/cong-cmtes/jurisdiction?cmte=HENE&cmtename=Energy+and+Commerce&cong=117) - OpenSecrets jurisdiction summary
- [Committees of the U.S. Congress](https://www.congress.gov/committees) - Official committee directory

### Secondary (MEDIUM confidence)
- [Congressional Investigations 2026 Priorities](https://www.pillsburylaw.com/en/news-and-insights/congressional-investigations-2026.html) - Expected oversight sectors for 2026
- [Senate Commerce, Science, and Transportation Committee](https://www.congress.gov/committee/senate-commerce-science-and-transportation/ssco00) - Jurisdiction areas
- [House Committee on Oversight Jurisdiction](https://oversightdemocrats.house.gov/about/committee-jurisdiction) - Broad investigative authority

### Tertiary (LOW confidence)
- [United States House Committee on Financial Services - Wikipedia](https://en.wikipedia.org/wiki/United_States_House_Committee_on_Financial_Services) - Historical context, not authoritative for current jurisdiction
- [Congressional Oversight - Wikipedia](https://en.wikipedia.org/wiki/Congressional_oversight) - General framework, no sector mapping

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Reuses proven patterns from Phase 13 (YAML) and Phase 12 (correlation)
- Architecture: MEDIUM-HIGH - Committee jurisdiction mapping is manual curation (no authoritative source), rest follows existing patterns
- Pitfalls: MEDIUM - FEC vs legislative committee confusion is project-specific risk, rest are standard data quality issues
- Committee-sector mapping: MEDIUM - No authoritative source exists; requires manual curation and expert judgment
- Donation-trade correlation: HIGH - Reuses Phase 12 employer_mappings infrastructure

**Research date:** 2026-02-15
**Valid until:** 2027-01-03 (end of 119th Congress, when committee jurisdictions may change with new Congress)
