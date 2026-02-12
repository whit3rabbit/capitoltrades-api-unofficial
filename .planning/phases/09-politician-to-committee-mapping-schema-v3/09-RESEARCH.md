# Phase 9: Politician-to-Committee Mapping & Schema v3 - Research

**Researched:** 2026-02-12
**Domain:** SQLite schema migrations, OpenFEC committee resolution, three-tier caching
**Confidence:** HIGH

## Summary

Phase 9 extends the database schema (v3 -> v4) to support donation storage and implements a three-tier caching strategy (DashMap -> SQLite -> OpenFEC API) to resolve CapitolTrades politicians to their authorized FEC committees. The core challenge is mapping proprietary politician_id format (e.g., "P000001") to FEC candidate IDs and committee IDs, then classifying committees by type.

**Critical finding:** CapitolTrades politician_id is proprietary format (letter + 6 digits), NOT Bioguide IDs. The congress-legislators crosswalk established in Phase 7 provides the Bioguide ID bridge needed for OpenFEC API fallback.

**Primary recommendation:** Reuse existing schema migration patterns (PRAGMA user_version), store committee IDs as JSON TEXT column (rusqlite serde_json feature), implement three-tier cache using Arc<DashMap> + DB reads + OpenFEC client, classify committees via designation and committee_type fields from OpenFEC API.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.31 | SQLite database with bundled feature | Already in workspace, proven migration pattern |
| serde_json | 1.x | JSON serialization for committee_ids column | Already in workspace, standard Rust JSON library |
| DashMap | 6.x | Concurrent in-memory cache | Already in project (yahoo.rs), thread-safe HashMap for multi-threaded access |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chrono | 0.4 | Timestamp handling for last_synced | Already in workspace, consistent with existing DB code |
| reqwest | 0.12 | HTTP client for OpenFEC API | Already in workspace, used by OpenFecClient from Phase 8 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| JSON TEXT column | Separate fec_committees table with FKs | More normalized, but overkill for read-heavy list storage |
| DashMap | RwLock<HashMap> | DashMap is drop-in replacement with better concurrency |
| multi-tier-cache crate | Custom implementation | Pre-built crate adds dependency, we need only simple pattern |

**Installation:**
```bash
# No new dependencies - all libraries already in workspace
```

## Architecture Patterns

### Recommended Schema v4 Structure

**Blocker Resolution:** Schema v3 already exists from Phase 7 (fec_mappings table). Phase 9 creates schema v4 with two new tables (donations, donation_sync_meta).

```sql
-- Schema v4 migration
CREATE TABLE IF NOT EXISTS donations (
    sub_id TEXT PRIMARY KEY,
    committee_id TEXT NOT NULL,
    contributor_name TEXT,
    contributor_employer TEXT,
    contributor_occupation TEXT,
    contributor_state TEXT,
    contributor_city TEXT,
    contributor_zip TEXT,
    contribution_receipt_amount REAL,
    contribution_receipt_date TEXT,
    election_cycle INTEGER,
    memo_text TEXT,
    receipt_type TEXT,
    FOREIGN KEY (committee_id) REFERENCES fec_committees(committee_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS donation_sync_meta (
    politician_id TEXT NOT NULL,
    committee_id TEXT NOT NULL,
    last_index INTEGER,
    last_contribution_receipt_date TEXT,
    last_synced_at TEXT NOT NULL,
    total_synced INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (politician_id, committee_id),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

-- Add committee_ids JSON column to existing fec_mappings table
ALTER TABLE fec_mappings ADD COLUMN committee_ids TEXT;

-- Add committee classification table
CREATE TABLE IF NOT EXISTS fec_committees (
    committee_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    committee_type TEXT,
    designation TEXT,
    party TEXT,
    state TEXT,
    cycles TEXT,
    last_synced TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_donations_committee ON donations(committee_id);
CREATE INDEX IF NOT EXISTS idx_donations_date ON donations(contribution_receipt_date);
CREATE INDEX IF NOT EXISTS idx_donations_cycle ON donations(election_cycle);
CREATE INDEX IF NOT EXISTS idx_donation_sync_meta_politician ON donation_sync_meta(politician_id);
CREATE INDEX IF NOT EXISTS idx_fec_committees_type ON fec_committees(committee_type);
CREATE INDEX IF NOT EXISTS idx_fec_committees_designation ON fec_committees(designation);
```

### Pattern 1: Schema Versioning with PRAGMA user_version

**What:** Incremental migrations via version checks, idempotent DDL
**When to use:** All schema changes post-v1
**Example:**
```rust
// Source: capitoltraders_lib/src/db.rs (existing pattern)
fn migrate_v4(&self) -> Result<(), DbError> {
    // donations table
    self.conn.execute(
        "CREATE TABLE IF NOT EXISTS donations (
            sub_id TEXT PRIMARY KEY,
            committee_id TEXT NOT NULL,
            contributor_name TEXT,
            contribution_receipt_amount REAL,
            contribution_receipt_date TEXT,
            election_cycle INTEGER,
            FOREIGN KEY (committee_id) REFERENCES fec_committees(committee_id) ON DELETE CASCADE
        )",
        [],
    )?;

    // donation_sync_meta table
    self.conn.execute(
        "CREATE TABLE IF NOT EXISTS donation_sync_meta (
            politician_id TEXT NOT NULL,
            committee_id TEXT NOT NULL,
            last_index INTEGER,
            last_contribution_receipt_date TEXT,
            last_synced_at TEXT NOT NULL,
            total_synced INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (politician_id, committee_id),
            FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
        )",
        [],
    )?;

    // fec_committees table
    self.conn.execute(
        "CREATE TABLE IF NOT EXISTS fec_committees (
            committee_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            committee_type TEXT,
            designation TEXT,
            party TEXT,
            state TEXT,
            cycles TEXT,
            last_synced TEXT NOT NULL
        )",
        [],
    )?;

    // Add committee_ids column to fec_mappings if not exists
    match self.conn.execute(
        "ALTER TABLE fec_mappings ADD COLUMN committee_ids TEXT",
        [],
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("duplicate column name") => {}
        Err(e) => return Err(e.into()),
    }

    // Create indexes
    for sql in &[
        "CREATE INDEX IF NOT EXISTS idx_donations_committee ON donations(committee_id)",
        "CREATE INDEX IF NOT EXISTS idx_donations_date ON donations(contribution_receipt_date)",
        "CREATE INDEX IF NOT EXISTS idx_donations_cycle ON donations(election_cycle)",
        "CREATE INDEX IF NOT EXISTS idx_donation_sync_meta_politician ON donation_sync_meta(politician_id)",
        "CREATE INDEX IF NOT EXISTS idx_fec_committees_type ON fec_committees(committee_type)",
        "CREATE INDEX IF NOT EXISTS idx_fec_committees_designation ON fec_committees(designation)",
    ] {
        self.conn.execute(sql, [])?;
    }

    Ok(())
}

// Wire up in init()
if version < 4 {
    self.migrate_v4()?;
    self.conn.pragma_update(None, "user_version", 4)?;
}
```

### Pattern 2: Three-Tier Cache for Committee Resolution

**What:** Check in-memory cache, fall back to SQLite, fall back to OpenFEC API
**When to use:** Committee ID resolution for donation sync, portfolio enrichment
**Example:**
```rust
// Source: Adapted from capitoltraders_lib/src/yahoo.rs caching pattern
use dashmap::DashMap;
use std::sync::Arc;

pub struct CommitteeResolver {
    client: Arc<OpenFecClient>,
    db: Arc<Db>,
    // Cache: (politician_id) -> Vec<committee_id>
    cache: Arc<DashMap<String, Vec<String>>>,
}

impl CommitteeResolver {
    pub fn new(client: Arc<OpenFecClient>, db: Arc<Db>) -> Self {
        Self {
            client,
            db,
            cache: Arc::new(DashMap::new()),
        }
    }

    /// Resolve politician to committee IDs using three-tier cache
    pub async fn resolve_committees(
        &self,
        politician_id: &str,
    ) -> Result<Vec<String>, anyhow::Error> {
        // Tier 1: In-memory cache
        if let Some(committees) = self.cache.get(politician_id) {
            return Ok(committees.clone());
        }

        // Tier 2: SQLite fec_mappings table
        if let Some(committees) = self.db.get_committees_for_politician(politician_id)? {
            self.cache.insert(politician_id.to_string(), committees.clone());
            return Ok(committees);
        }

        // Tier 3: OpenFEC API
        let committees = self.fetch_from_api(politician_id).await?;

        // Store in DB for next time
        self.db.update_politician_committees(politician_id, &committees)?;

        // Store in cache
        self.cache.insert(politician_id.to_string(), committees.clone());

        Ok(committees)
    }

    async fn fetch_from_api(&self, politician_id: &str) -> Result<Vec<String>, anyhow::Error> {
        // Get FEC candidate IDs via congress-legislators crosswalk
        let fec_candidate_ids = self.db.get_fec_ids_for_politician(politician_id)?;

        if fec_candidate_ids.is_empty() {
            // Fallback: search by name
            let (first, last, state) = self.db.get_politician_name(politician_id)?;
            let query = CandidateSearchQuery::default()
                .with_name(&format!("{} {}", first, last))
                .with_state(&state);
            let response = self.client.search_candidates(&query).await?;

            if let Some(candidate) = response.results.first() {
                fec_candidate_ids.push(candidate.candidate_id.clone());
            } else {
                return Ok(vec![]);
            }
        }

        // Fetch committees for each candidate ID
        let mut all_committees = Vec::new();
        for candidate_id in &fec_candidate_ids {
            let response = self.client.get_candidate_committees(candidate_id).await?;
            for committee in response.results {
                all_committees.push(committee.committee_id);
            }
        }

        Ok(all_committees)
    }
}
```

### Pattern 3: JSON Column Storage with rusqlite

**What:** Store Vec<String> as JSON TEXT column using serde_json
**When to use:** Storing committee IDs list in fec_mappings table
**Example:**
```rust
// Source: rusqlite serde_json feature documentation
use serde_json;

// Writing JSON column
pub fn update_politician_committees(
    &mut self,
    politician_id: &str,
    committee_ids: &[String],
) -> Result<(), DbError> {
    let json = serde_json::to_string(committee_ids)?;
    self.conn.execute(
        "UPDATE fec_mappings SET committee_ids = ?1, last_synced = datetime('now')
         WHERE politician_id = ?2",
        params![json, politician_id],
    )?;
    Ok(())
}

// Reading JSON column
pub fn get_committees_for_politician(
    &self,
    politician_id: &str,
) -> Result<Option<Vec<String>>, DbError> {
    let json: Option<String> = self.conn
        .query_row(
            "SELECT committee_ids FROM fec_mappings WHERE politician_id = ?1 LIMIT 1",
            params![politician_id],
            |row| row.get(0),
        )
        .optional()?;

    match json {
        Some(json_str) if !json_str.is_empty() => {
            let committees: Vec<String> = serde_json::from_str(&json_str)?;
            Ok(Some(committees))
        }
        _ => Ok(None),
    }
}
```

### Pattern 4: Committee Type Classification

**What:** Classify committees using designation and committee_type fields from OpenFEC
**When to use:** Filtering campaign committees vs leadership PACs vs joint fundraising
**Example:**
```rust
// Source: FEC.gov committee type code descriptions
#[derive(Debug, Clone, PartialEq)]
pub enum CommitteeClass {
    Campaign,        // H, S, P with designation A or P
    LeadershipPac,   // designation D
    JointFundraising, // designation J
    Party,           // X, Y, Z
    Pac,             // N, Q, O
    Other,
}

impl CommitteeClass {
    pub fn classify(committee_type: Option<&str>, designation: Option<&str>) -> Self {
        match (committee_type, designation) {
            // Campaign committees (H=House, S=Senate, P=Presidential)
            (Some("H" | "S" | "P"), Some("A" | "P")) => CommitteeClass::Campaign,

            // Leadership PAC (designation D)
            (_, Some("D")) => CommitteeClass::LeadershipPac,

            // Joint fundraising (designation J)
            (_, Some("J")) => CommitteeClass::JointFundraising,

            // Party committees
            (Some("X" | "Y" | "Z"), _) => CommitteeClass::Party,

            // PACs
            (Some("N" | "Q" | "O"), _) => CommitteeClass::Pac,

            _ => CommitteeClass::Other,
        }
    }
}

// Store classification in fec_committees table for filtering
pub fn upsert_committee(
    &mut self,
    committee: &Committee,
) -> Result<(), DbError> {
    let classification = CommitteeClass::classify(
        committee.committee_type.as_deref(),
        committee.designation.as_deref(),
    );

    let cycles_json = serde_json::to_string(&committee.cycles)?;

    self.conn.execute(
        "INSERT INTO fec_committees (committee_id, name, committee_type, designation, party, state, cycles, last_synced)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
         ON CONFLICT(committee_id) DO UPDATE SET
           name = excluded.name,
           committee_type = excluded.committee_type,
           designation = excluded.designation,
           party = excluded.party,
           state = excluded.state,
           cycles = excluded.cycles,
           last_synced = datetime('now')",
        params![
            committee.committee_id,
            committee.name,
            committee.committee_type,
            committee.designation,
            committee.party,
            committee.state,
            cycles_json,
        ],
    )?;
    Ok(())
}
```

### Anti-Patterns to Avoid

- **Separate fec_committees table per politician** - Use single table with politician_id FK, query via JOIN
- **Storing committee IDs as comma-separated string** - Use JSON for structured data, enables serde deserialization
- **Caching committee metadata separately from committee IDs** - Store metadata in fec_committees table, cache only IDs list
- **Synchronous API calls in cache miss** - Use async/await, cache tier must be async-aware

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Concurrent HashMap | RwLock<HashMap> with manual locking | DashMap | Handles sharding, lock-free reads, production-tested |
| JSON serialization | Manual string building or parsing | serde_json with rusqlite feature | Type-safe, handles escaping, widely adopted |
| Multi-tier cache eviction | Custom TTL logic with timestamps | Simple read-through cache (no eviction needed) | Committee IDs rarely change, DB is persistent tier |
| FEC API pagination | Manual offset tracking | OpenFEC keyset pagination (last_index + date) | Schedule A requires keyset, not offset-based |

**Key insight:** SQLite as persistent cache tier eliminates need for complex eviction logic in memory tier.

## Common Pitfalls

### Pitfall 1: Politician ID Format Confusion
**What goes wrong:** Assuming politician_id is Bioguide ID, attempting direct OpenFEC lookups
**Why it happens:** Phase 7 uses Bioguide IDs from congress-legislators, easy to conflate
**How to avoid:**
- CapitolTrades politician_id is proprietary format (e.g., "W000830", "A000055")
- congress-legislators provides bioguide_id stored in fec_mappings table
- Use fec_mappings.fec_candidate_id (not bioguide_id) for OpenFEC API calls
**Warning signs:** OpenFEC 404 errors when passing politician_id as candidate_id

### Pitfall 2: JSON Column Empty String vs NULL
**What goes wrong:** serde_json::from_str("") panics or returns error
**Why it happens:** SQL UPDATE sets column to empty string instead of NULL
**How to avoid:** Check for empty string before deserializing
```rust
match json {
    Some(json_str) if !json_str.is_empty() => {
        let committees: Vec<String> = serde_json::from_str(&json_str)?;
        Ok(Some(committees))
    }
    _ => Ok(None),
}
```
**Warning signs:** JSON parse errors with "EOF while parsing" on empty DB rows

### Pitfall 3: Committee Type Classification Edge Cases
**What goes wrong:** Misclassifying leadership PACs as campaign committees
**Why it happens:** Leadership PACs can have committee_type H/S/P but designation D
**How to avoid:** Check designation field FIRST (D overrides committee_type for leadership PACs)
**Warning signs:** Leadership PAC donations appearing in campaign committee queries

### Pitfall 4: OpenFEC Rate Limiting
**What goes wrong:** HTTP 429 errors during bulk committee resolution
**Why it happens:** 1,000 calls/hour limit with API key, 100/hour without
**How to avoid:**
- Batch politician lookups, cache aggressively
- Implement circuit breaker (consecutive failure threshold)
- Use congress-legislators crosswalk to minimize API calls
**Warning signs:** Sporadic 429 errors during sync operations, escalating to complete failures

### Pitfall 5: Schema v4 vs v3 Confusion
**What goes wrong:** Attempting to add fec_mappings table in v4 migration
**Why it happens:** Phase 7 already created schema v3 with fec_mappings, but without committee_ids column
**How to avoid:**
- v3 (Phase 7): fec_mappings table created with (politician_id, fec_candidate_id, bioguide_id)
- v4 (Phase 9): ALTER TABLE fec_mappings ADD COLUMN committee_ids TEXT + new tables (donations, donation_sync_meta, fec_committees)
**Warning signs:** "table fec_mappings already exists" errors during migration

## Code Examples

Verified patterns from existing codebase and official sources:

### DB Migration with Version Check
```rust
// Source: capitoltraders_lib/src/db.rs migrate_v3 pattern
pub fn init(&self) -> Result<(), DbError> {
    let version: i32 = self
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        self.migrate_v1()?;
        self.conn.pragma_update(None, "user_version", 1)?;
    }

    if version < 2 {
        self.migrate_v2()?;
        self.conn.pragma_update(None, "user_version", 2)?;
    }

    if version < 3 {
        self.migrate_v3()?;
        self.conn.pragma_update(None, "user_version", 3)?;
    }

    if version < 4 {
        self.migrate_v4()?;
        self.conn.pragma_update(None, "user_version", 4)?;
    }

    let schema = include_str!("../../schema/sqlite.sql");
    self.conn.execute_batch(schema)?;

    Ok(())
}
```

### DashMap Cache Initialization
```rust
// Source: capitoltraders_lib/src/yahoo.rs (Arc<DashMap> pattern)
use dashmap::DashMap;
use std::sync::Arc;

pub struct CommitteeResolver {
    cache: Arc<DashMap<String, Vec<String>>>,
}

impl CommitteeResolver {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
        }
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }
}
```

### OpenFEC Committee Fetching
```rust
// Source: capitoltraders_lib/src/openfec/client.rs (existing get_candidate_committees method)
pub async fn get_candidate_committees(
    &self,
    candidate_id: &str,
) -> Result<CommitteeResponse, OpenFecError> {
    let path = format!("/candidate/{}/committees/", candidate_id);
    self.get(&path, &[]).await
}

// Usage in resolver
async fn fetch_committees_for_candidate(
    &self,
    candidate_id: &str,
) -> Result<Vec<Committee>, anyhow::Error> {
    let response = self.client.get_candidate_committees(candidate_id).await?;
    Ok(response.results)
}
```

### Committee Classification Logic
```rust
// Source: FEC.gov committee type code descriptions
pub fn filter_campaign_committees(committees: &[Committee]) -> Vec<&Committee> {
    committees
        .iter()
        .filter(|c| {
            matches!(
                (c.committee_type.as_deref(), c.designation.as_deref()),
                (Some("H" | "S" | "P"), Some("A" | "P"))
            )
        })
        .collect()
}

pub fn filter_leadership_pacs(committees: &[Committee]) -> Vec<&Committee> {
    committees
        .iter()
        .filter(|c| c.designation.as_deref() == Some("D"))
        .collect()
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| rusqlite without serde_json feature | rusqlite with serde_json feature for JSON columns | rusqlite 0.28+ | Type-safe JSON serialization, no manual string building |
| RwLock<HashMap> for caching | DashMap for concurrent caching | DashMap 4.0+ (2020) | Lock-free reads, better concurrency |
| Separate cache tables with TTL | SQLite as persistent cache tier | N/A (architectural pattern) | No eviction logic needed, persistent across restarts |
| Offset pagination for FEC data | Keyset pagination (last_index + date) | OpenFEC API design | Required for Schedule A, more efficient for large datasets |

**Deprecated/outdated:**
- FEC designation Z (National party nonfederal account) - Not permitted after Bipartisan Campaign Reform Act of 2002
- serde_json::Value manual matching - Use typed structs with Deserialize derive
- multi-tier-cache crate - Adds dependency for simple pattern, project already has DashMap

## Open Questions

1. **OpenFEC Rate Limit Verification**
   - What we know: API key enables 1,000 calls/hour (unverified), 100/hour without key
   - What's unclear: Exact limit, X-RateLimit-Limit header presence, burst allowance
   - Recommendation: Implement conservative 100/hour limit, check response headers in Phase 8 integration tests, adjust if X-RateLimit-Limit shows higher threshold

2. **Committee ID Stability**
   - What we know: Committee IDs are stable (e.g., C00000001), cycles array tracks active cycles
   - What's unclear: Do committee IDs persist across election cycles, or new IDs per cycle?
   - Recommendation: Store cycles JSON in fec_committees table, implement refresh logic for stale data (last_synced > 30 days)

3. **Leadership PAC vs Campaign Committee Overlap**
   - What we know: Same candidate can have multiple committees (H0CA05080 campaign + leadership PAC)
   - What's unclear: Do we need to track ALL committees or filter to campaign-only for donation sync?
   - Recommendation: Store ALL committees in fec_committees, use CommitteeClass enum to filter at query time (configurable via CLI flags)

4. **Politician Not Found in FEC Handling**
   - What we know: congress-legislators covers current Congress, may miss recent additions
   - What's unclear: Should we skip gracefully with warning, or attempt OpenFEC name search?
   - Recommendation: Two-tier fallback: (1) congress-legislators crosswalk, (2) OpenFEC search_candidates by name + state, (3) log warning and skip if both fail

## Sources

### Primary (HIGH confidence)
- [rusqlite serde_json feature](https://docs.rs/rusqlite/latest/src/rusqlite/types/serde_json.rs.html) - JSON column storage pattern
- [FEC Committee Type Code Descriptions](https://www.fec.gov/campaign-finance-data/committee-type-code-descriptions/) - Official FEC committee type codes
- [DashMap Documentation](https://docs.rs/dashmap/latest/dashmap/struct.DashMap.html) - Concurrent HashMap API
- capitoltraders_lib/src/db.rs - Existing migration patterns (migrate_v1, migrate_v2, migrate_v3)
- capitoltraders_lib/src/yahoo.rs - Existing DashMap caching pattern with Arc
- capitoltraders_lib/src/openfec/client.rs - OpenFEC API client methods from Phase 8

### Secondary (MEDIUM confidence)
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) - Rate limits mentioned but not detailed
- [FEC Joint Fundraising Documentation](https://www.fec.gov/help-candidates-and-committees/joint-fundraising-candidates-political-committees/) - Designation codes J for joint fundraising
- [Instructions for Statement of Organization (FEC FORM 1)](https://www.fec.gov/resources/cms-content/documents/policy-guidance/fecfrm1i.pdf) - Leadership PAC designation D

### Tertiary (LOW confidence - needs verification)
- [Sunlight Foundation OpenFEC Introduction](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/) - Rate limit mentioned as 1,000/hour with API key (unverified in 2026)
- [multi-tier-cache crate](https://crates.io/crates/multi-tier-cache) - Three-tier cache pattern reference (not using, implementing custom)

## Metadata

**Confidence breakdown:**
- Schema v4 migration pattern: HIGH - Follows established v1/v2/v3 pattern in codebase
- JSON column storage: HIGH - rusqlite serde_json feature is documented and tested
- Committee classification: HIGH - FEC.gov official documentation for codes
- Three-tier cache architecture: MEDIUM - Pattern is sound but not production-verified
- OpenFEC rate limits: LOW - Mentioned in sources but not officially documented, needs empirical testing
- Politician ID format: HIGH - Verified with real database records (W000830, A000055, etc.)

**Research date:** 2026-02-12
**Valid until:** 2026-03-12 (30 days - schema patterns are stable, FEC codes are regulatory)
