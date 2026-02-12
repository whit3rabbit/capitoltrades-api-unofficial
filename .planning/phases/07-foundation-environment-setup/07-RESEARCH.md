# Phase 7: Foundation & Environment Setup - Research

**Researched:** 2026-02-11
**Domain:** Environment configuration, YAML parsing, SQLite schema extension
**Confidence:** HIGH

## Summary

Phase 7 establishes the foundation for FEC donation integration by implementing secure API key management via dotenvy 0.15 and creating a SQLite-backed crosswalk table that maps Capitol Traders politician IDs to FEC candidate IDs. The phase leverages the unitedstates/congress-legislators dataset, a well-maintained public dataset that provides bioguide-to-FEC-ID mappings for current and historical members of Congress.

The implementation follows established patterns from v1.1: versioned schema migrations (progressing to v3), thiserror-based error types, and the existing CLI dispatch model. The .env file loading happens at CLI startup before command dispatch, with clear error messages guiding users to obtain and configure their OpenFEC API key. The crosswalk table handles the one-to-many relationship (one politician may have multiple FEC candidate IDs across election cycles) using a junction table pattern.

**Primary recommendation:** Use dotenvy's `#[dotenvy::load]` attribute macro for automatic .env loading at CLI startup, serde_yml for parsing the congress-legislators dataset, and a dedicated `fec_mappings` table with composite index on (politician_id, fec_candidate_id) to support efficient lookups in both directions.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| dotenvy | 0.15 | .env file loading | Official successor to deprecated dotenv-rs; supports both modifying and non-modifying APIs; High source reputation with 770 code snippets on Context7 |
| serde_yml | latest | YAML parsing | Robust Rust YAML library built on Serde framework; High source reputation with 68 code snippets; actively maintained |
| reqwest | 0.12 | HTTP client for dataset download | Already in workspace dependencies; async-first design matches existing patterns |
| rusqlite | 0.31 | SQLite database | Already in workspace dependencies with "bundled" feature |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| thiserror | 1 | Error type derivation | Already in workspace; use for new FecMappingError type |
| serde | 1 | YAML deserialization | Already in workspace with derive feature |
| chrono | 0.4 | Date parsing (birth dates, term dates) | Already in workspace; not critical for Phase 7 but useful for data validation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| dotenvy | dotenv (deprecated) | dotenvy is the maintained fork; original dotenv crate is no longer maintained |
| serde_yml | serde_yaml | serde_yml is the modern continuation with better error handling and active maintenance |
| Junction table | JSON array in politicians table | Junction table allows indexing, efficient queries in both directions, and follows SQL normalization best practices |

**Installation:**
```toml
# Add to workspace Cargo.toml [workspace.dependencies]
dotenvy = "0.15"
serde_yml = "0.3"

# Add to capitoltraders_lib/Cargo.toml [dependencies]
dotenvy = { workspace = true }
serde_yml = { workspace = true }
```

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── fec_mapping.rs       # New module: FEC ID crosswalk types and logic
├── db.rs                # Extended: add fec_mapping table operations
└── lib.rs               # Export new FecMapping module

capitoltraders_cli/src/
├── main.rs              # Modified: add .env loading at startup
└── commands/
    └── sync_fec.rs      # New command: download and populate FEC mappings (optional Phase 7, or defer to Phase 8)

schema/
└── sqlite.sql           # Extended: add fec_mappings table

.env.example             # New: template for API key configuration
```

### Pattern 1: .env Loading with dotenvy Attribute Macro
**What:** Automatic .env file loading before async runtime initialization
**When to use:** CLI applications that need environment variables available before any async code runs
**Example:**
```rust
// Source: https://github.com/allan2/dotenvy/blob/main/README.md
#[dotenvy::load]
#[tokio::main]
async fn main() -> Result<()> {
    // .env loaded automatically before this code runs
    let api_key = std::env::var("OPENFEC_API_KEY")
        .map_err(|_| anyhow::anyhow!(
            "OPENFEC_API_KEY not found. Get your key at https://api.data.gov/signup/ \
             and add it to a .env file in the project root."
        ))?;

    // Rest of CLI initialization
    let cli = Cli::parse();
    // ...
}
```

**Alternative Pattern:** Non-modifying API with EnvLoader
```rust
// Source: https://github.com/allan2/dotenvy/blob/main/dotenvy/README.md
use dotenvy::EnvLoader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_map = EnvLoader::new().load()?;
    println!("OPENFEC_API_KEY={}", env_map.var("OPENFEC_API_KEY")?);
    Ok(())
}
```

### Pattern 2: Congress Legislators YAML Structure
**What:** Parse unitedstates/congress-legislators dataset to extract FEC IDs
**When to use:** Initial population of fec_mappings table or refresh operations
**Example:**
```rust
// Source: https://raw.githubusercontent.com/unitedstates/congress-legislators/main/legislators-current.yaml
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
struct Legislator {
    id: LegislatorId,
    name: LegislatorName,
    bio: LegislatorBio,
    terms: Vec<Term>,
}

#[derive(Deserialize, Debug)]
struct LegislatorId {
    bioguide: String,
    thomas: Option<String>,
    fec: Option<Vec<String>>,  // Array of FEC candidate IDs
    govtrack: Option<i32>,
    opensecrets: Option<String>,
}

#[derive(Deserialize, Debug)]
struct LegislatorName {
    first: String,
    last: String,
    official_full: Option<String>,
}

#[derive(Deserialize, Debug)]
struct LegislatorBio {
    birthday: String,
    gender: String,
}

#[derive(Deserialize, Debug)]
struct Term {
    #[serde(rename = "type")]
    term_type: String,  // "rep" or "sen"
    start: String,
    end: String,
    state: String,
    party: String,
    class: Option<i32>,      // Senate class (1, 2, or 3)
    district: Option<i32>,   // House district number
}

// Parse YAML
async fn load_legislators() -> Result<Vec<Legislator>, serde_yml::Error> {
    let yaml_content = reqwest::get("https://raw.githubusercontent.com/unitedstates/congress-legislators/main/legislators-current.yaml")
        .await?
        .text()
        .await?;

    serde_yml::from_str(&yaml_content)
}
```

### Pattern 3: FEC Mapping Crosswalk Table Design
**What:** One-to-many junction table mapping politician IDs to FEC candidate IDs
**When to use:** Storing and querying politician-to-FEC-ID relationships
**Example:**
```sql
-- Source: SQLite best practices for one-to-many relationships
CREATE TABLE IF NOT EXISTS fec_mappings (
    politician_id TEXT NOT NULL,
    fec_candidate_id TEXT NOT NULL,
    bioguide_id TEXT NOT NULL,
    election_cycle INTEGER,  -- Optional: year candidate ID was used (e.g. 2020, 2022)
    last_synced TEXT NOT NULL,
    PRIMARY KEY (politician_id, fec_candidate_id),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_fec_mappings_fec_id ON fec_mappings(fec_candidate_id);
CREATE INDEX IF NOT EXISTS idx_fec_mappings_bioguide ON fec_mappings(bioguide_id);
```

**Rust DB methods:**
```rust
// Pattern follows existing db.rs patterns (upsert, query)
impl Db {
    pub fn upsert_fec_mappings(&mut self, mappings: &[(String, String, String)]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;

        {
            let mut stmt = tx.prepare(
                "INSERT INTO fec_mappings (politician_id, fec_candidate_id, bioguide_id, last_synced)
                 VALUES (?1, ?2, ?3, datetime('now'))
                 ON CONFLICT(politician_id, fec_candidate_id) DO UPDATE SET
                   last_synced = datetime('now')"
            )?;

            for (politician_id, fec_id, bioguide_id) in mappings {
                stmt.execute(params![politician_id, fec_id, bioguide_id])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_fec_ids_for_politician(&self, politician_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT fec_candidate_id FROM fec_mappings WHERE politician_id = ?1"
        )?;

        let fec_ids = stmt.query_map([politician_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        Ok(fec_ids)
    }

    pub fn get_politician_id_for_bioguide(&self, bioguide_id: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT politician_id FROM fec_mappings WHERE bioguide_id = ?1 LIMIT 1",
                params![bioguide_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(DbError::from)
    }
}
```

### Pattern 4: Versioned Schema Migration (v2 -> v3)
**What:** Add fec_mappings table via migration while preserving existing data
**When to use:** Extending schema for new features without breaking existing deployments
**Example:**
```rust
// Pattern follows existing migrate_v1, migrate_v2 in db.rs
impl Db {
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
            self.migrate_v3()?;  // NEW
            self.conn.pragma_update(None, "user_version", 3)?;
        }

        let schema = include_str!("../../schema/sqlite.sql");
        self.conn.execute_batch(schema)?;

        Ok(())
    }

    fn migrate_v3(&self) -> Result<(), DbError> {
        // Migration only runs if fec_mappings doesn't exist
        // Base schema.sql includes the table for fresh DBs
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS fec_mappings (
                politician_id TEXT NOT NULL,
                fec_candidate_id TEXT NOT NULL,
                bioguide_id TEXT NOT NULL,
                election_cycle INTEGER,
                last_synced TEXT NOT NULL,
                PRIMARY KEY (politician_id, fec_candidate_id),
                FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
            )",
            [],
        )?;

        for sql in &[
            "CREATE INDEX IF NOT EXISTS idx_fec_mappings_fec_id ON fec_mappings(fec_candidate_id)",
            "CREATE INDEX IF NOT EXISTS idx_fec_mappings_bioguide ON fec_mappings(bioguide_id)",
        ] {
            self.conn.execute(sql, [])?;
        }

        Ok(())
    }
}
```

### Pattern 5: Bioguide-to-Politician-ID Mapping Strategy
**What:** Map bioguide IDs from congress-legislators to politician_id from Capitol Traders
**Challenge:** Capitol Traders uses politician_id (format: "P000197"), congress-legislators uses bioguide_id (different format)
**Solution:** Use name matching with state validation as primary strategy, store bioguide for future reference

**Mapping Algorithm:**
```rust
// Step 1: Load existing politicians from Capitol Traders DB
let politicians = db.get_all_politicians()?;

// Step 2: Create lookup by (last_name, state_id)
let mut politician_lookup: HashMap<(String, String), String> = HashMap::new();
for pol in politicians {
    let key = (pol.last_name.to_lowercase(), pol.state_id.clone());
    politician_lookup.insert(key, pol.politician_id);
}

// Step 3: For each legislator from congress-legislators dataset
for legislator in legislators {
    let key = (legislator.name.last.to_lowercase(), legislator.terms.last().unwrap().state.clone());

    if let Some(politician_id) = politician_lookup.get(&key) {
        // Found match - store FEC IDs
        if let Some(fec_ids) = legislator.id.fec {
            for fec_id in fec_ids {
                mappings.push((politician_id.clone(), fec_id, legislator.id.bioguide.clone()));
            }
        }
    }
}
```

**Note:** This approach assumes politicians are already synced from Capitol Traders. If sync hasn't run, fec_mappings will be empty (graceful degradation). Phase 8 can enhance matching with first name + middle name fuzzy matching if needed.

### Anti-Patterns to Avoid
- **Hardcoding API keys in source code:** Always use environment variables loaded from .env
- **Committing .env to git:** Use .env.example as template, exclude .env via .gitignore (already configured)
- **Storing FEC IDs as JSON array in politicians table:** Violates normalization, prevents efficient indexing
- **Loading .env in every command:** Load once at CLI startup, not per-command
- **Using std::env::var without clear error messages:** Provide actionable guidance on how to obtain and configure keys
- **Silently skipping missing .env:** Fail loudly with helpful error when donation commands are invoked without API key

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| .env file parsing | Custom env file reader | dotenvy 0.15 | Handles edge cases (quoted values, comments, whitespace, variable expansion), well-tested with 770 code examples |
| YAML parsing | Manual YAML parser | serde_yml | Supports full YAML 1.2 spec, nested structures, error reporting, serde integration |
| Environment variable validation | Custom checks in multiple places | Centralized check at CLI startup | Single source of truth, fail-fast before any async code runs |
| Bioguide-to-politician mapping | Complex fuzzy matching | Simple (last_name, state) lookup | Capitol Traders data quality is high; over-engineering matching introduces bugs |
| FEC ID storage | CSV/JSON in TEXT column | Dedicated junction table | Enables foreign keys, indexes, efficient joins, proper normalization |

**Key insight:** Environment configuration and dataset parsing are deceptively complex domains with many edge cases (special characters in .env values, YAML anchors/aliases, date format variations). Using battle-tested libraries prevents bugs and security issues that arise from naive implementations.

## Common Pitfalls

### Pitfall 1: .env File Not Loaded Before Environment Variable Access
**What goes wrong:** Code attempts to read `OPENFEC_API_KEY` before dotenvy has loaded the .env file, resulting in "environment variable not found" errors even when .env exists.
**Why it happens:** Rust doesn't have automatic .env loading like some frameworks; explicit initialization is required.
**How to avoid:** Use `#[dotenvy::load]` attribute macro placed BEFORE `#[tokio::main]` to ensure .env is loaded before async runtime starts. Alternatively, call `dotenvy::dotenv()` as first line in main().
**Warning signs:** Test with a valid .env file and still get "not found" errors; works in production (where env vars are set at OS level) but fails in development.

### Pitfall 2: Assuming One FEC ID Per Politician
**What goes wrong:** Code uses `fec: Option<String>` instead of `fec: Option<Vec<String>>`, causing YAML deserialization failures when a legislator has multiple FEC candidate IDs.
**Why it happens:** The congress-legislators dataset uses arrays for FEC IDs because politicians receive new candidate IDs for each election cycle.
**How to avoid:** Use `Vec<String>` for FEC IDs in deserialization structs and junction table design. Query all FEC IDs for a politician when making donation API calls.
**Warning signs:** YAML parsing fails on specific legislators; error messages mention "expected string, got array".

### Pitfall 3: Missing Bioguide-to-Politician-ID Mapping
**What goes wrong:** FEC mappings table remains empty after loading congress-legislators dataset because bioguide IDs don't match politician_id format from Capitol Traders.
**Why it happens:** Two separate systems use different ID schemes; direct ID matching won't work.
**How to avoid:** Use name-based matching (last_name + state_id as composite key) to link bioguide records to Capitol Traders politician records. Store bioguide_id in fec_mappings for audit trail.
**Warning signs:** Congress-legislators data loads successfully but fec_mappings has 0 rows; lookup by politician_id returns empty results.

### Pitfall 4: Schema Migration Conflicts on Fresh Databases
**What goes wrong:** Fresh database creation fails with "table already exists" errors when migrations try to create tables that base schema also creates.
**Why it happens:** Migrations and base schema.sql both run; without IF NOT EXISTS guards, the second attempt fails.
**How to avoid:** Always use `CREATE TABLE IF NOT EXISTS` in both migrations and base schema. Migrations handle existing databases, base schema handles fresh installs.
**Warning signs:** Development (fresh DB) works fine, but CI/test environments (existing DBs) fail; or vice versa.

### Pitfall 5: .env.example Contains Real Secrets
**What goes wrong:** Developer copies .env to .env.example for documentation, accidentally committing real API key to version control.
**Why it happens:** .env.example is tracked by git (intentionally, as a template), but developer forgets to replace real values with placeholders.
**How to avoid:** Create .env.example manually with placeholder values (e.g., `OPENFEC_API_KEY=your_api_key_here`). Never copy .env to .env.example. Add check to CI that .env.example doesn't contain valid API keys.
**Warning signs:** .env.example has 40+ character alphanumeric strings instead of descriptive placeholders; git history shows .env.example changed after developer obtained API key.

### Pitfall 6: Rate Limiting on OpenFEC API Not Handled
**What goes wrong:** Bulk FEC ID lookup or donation queries hit OpenFEC rate limits (1000 calls/hour for API key holders), causing 429 errors.
**Why it happens:** Phase 7 doesn't implement rate limiting logic; initial sync may make hundreds of API calls.
**How to avoid:** Phase 7 focuses on foundation (API key loading, FEC ID mappings); defer actual OpenFEC API calls to Phase 8-11 where rate limiting will be implemented. For Phase 7, only load congress-legislators dataset (no OpenFEC calls).
**Warning signs:** Initial implementation works with small datasets but fails with full sync; HTTP 429 errors in logs.

## Code Examples

Verified patterns from official sources:

### Download Congress-Legislators Dataset
```rust
// Source: https://rust-lang-nursery.github.io/rust-cookbook/web/clients/download.html
// Source: https://github.com/unitedstates/congress-legislators

async fn download_legislators_yaml() -> Result<String, Box<dyn std::error::Error>> {
    let url = "https://raw.githubusercontent.com/unitedstates/congress-legislators/main/legislators-current.yaml";
    let yaml_content = reqwest::get(url)
        .await?
        .text()
        .await?;

    Ok(yaml_content)
}

async fn download_historical_legislators_yaml() -> Result<String, Box<dyn std::error::Error>> {
    let url = "https://raw.githubusercontent.com/unitedstates/congress-legislators/main/legislators-historical.yaml";
    let yaml_content = reqwest::get(url)
        .await?
        .text()
        .await?;

    Ok(yaml_content)
}
```

### Parse YAML with serde_yml
```rust
// Source: https://docs.rs/crate/serde_yml/latest/source/README

use serde_yml;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
struct Legislator {
    id: LegislatorId,
    name: LegislatorName,
    terms: Vec<Term>,
}

#[derive(Deserialize, Debug)]
struct LegislatorId {
    bioguide: String,
    fec: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct LegislatorName {
    first: String,
    last: String,
    official_full: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Term {
    state: String,
    party: String,
}

fn parse_legislators(yaml_content: &str) -> Result<Vec<Legislator>, serde_yml::Error> {
    serde_yml::from_str(yaml_content)
}
```

### Error Handling Pattern
```rust
// Pattern follows existing YahooError, DbError patterns
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FecMappingError {
    #[error("Failed to download congress-legislators dataset: {0}")]
    DownloadFailed(String),

    #[error("Failed to parse YAML: {0}")]
    YamlParseFailed(#[from] serde_yml::Error),

    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),

    #[error("No FEC IDs found for politician: {0}")]
    NoFecIds(String),
}
```

### CLI Error Message for Missing API Key
```rust
// Pattern follows existing CLI error handling
#[dotenvy::load]
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("capitoltraders=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    // Check for API key when donation-related commands are invoked
    // (defer to Phase 8-11; Phase 7 doesn't call OpenFEC API)

    // ... rest of CLI dispatch
}

// Example error message helper (can be in a separate module)
fn require_openfec_api_key() -> Result<String> {
    std::env::var("OPENFEC_API_KEY").map_err(|_| {
        anyhow::anyhow!(
            "OpenFEC API key not found.\n\n\
             To use donation-related features, you need an API key from api.data.gov:\n\
             1. Sign up at https://api.data.gov/signup/\n\
             2. Check your email for the API key\n\
             3. Create a .env file in the project root:\n\
                echo 'OPENFEC_API_KEY=your_key_here' > .env\n\
             4. See .env.example for a template\n\n\
             Note: .env is gitignored and will not be committed."
        )
    })
}
```

### .env.example Template
```bash
# OpenFEC API Key
# Get your key at https://api.data.gov/signup/
# Check your email after registration for the key
OPENFEC_API_KEY=your_api_key_here

# Optional: Override Capitol Trades base URL for testing
# CAPITOLTRADES_BASE_URL=http://localhost:3000
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| dotenv crate | dotenvy crate | 2022 | Original dotenv crate deprecated; dotenvy is the maintained fork with same API |
| serde_yaml | serde_yml | 2024 | serde_yml is the modern continuation with better error messages and active development |
| Manual env validation in each command | Centralized validation at startup with #[dotenvy::load] | Ongoing best practice | Fail-fast approach prevents partial execution with invalid config |
| String columns for multi-valued data | Junction tables with proper foreign keys | SQL normalization since 1970s | Enables indexing, referential integrity, efficient queries |

**Deprecated/outdated:**
- **dotenv crate**: Use dotenvy instead (direct replacement, same API)
- **serde_yaml**: Prefer serde_yml for new projects (actively maintained fork)
- **Hardcoded API URLs**: Congress-legislators dataset moved from unitedstates/congress-legislators to github.com/unitedstates/congress-legislators (but URL structure unchanged)

## Open Questions

1. **Should Phase 7 include a sync command for FEC mappings, or defer to Phase 8?**
   - What we know: Phase 7 success criteria include "lookup by politician name or Bioguide ID returns correct FEC candidate IDs", implying data must be populated.
   - What's unclear: Whether population happens via dedicated CLI command or as part of first donation query.
   - Recommendation: Add basic `capitoltraders sync-fec` command in Phase 7 to populate fec_mappings table. This satisfies success criteria and provides clear path for testing. Keep it simple (download, parse, insert) without complex retry/rate-limiting logic.

2. **How to handle politicians with zero FEC IDs in congress-legislators dataset?**
   - What we know: Some legislators may not have FEC IDs (e.g., appointed officials who haven't run in elections).
   - What's unclear: Should lookup return empty array or error?
   - Recommendation: Return empty Vec<String> (not an error). Document in code that zero FEC IDs is valid state. Future phases can decide how to handle donation queries for these politicians.

3. **Should bioguide_id be stored in politicians table or only in fec_mappings?**
   - What we know: Bioguide ID is the "best field to use as a primary key" per congress-legislators README.
   - What's unclear: Whether to extend politicians table with bioguide_id column or keep it only in fec_mappings junction table.
   - Recommendation: Store in fec_mappings only for Phase 7 (minimal schema changes). If future phases need bioguide lookups frequently, can add to politicians table in later migration.

4. **How to handle name matching collisions (multiple politicians with same last name in same state)?**
   - What we know: Current implementation uses (last_name, state_id) as lookup key.
   - What's unclear: Are there actual collisions in real data?
   - Recommendation: Start with simple (last_name, state) matching. Log warnings for collisions (match count > 1). If collisions occur in practice, Phase 8 can enhance with first_name or middle_name matching. Real-world congressional data likely has few/no collisions given small number of reps per state.

## Sources

### Primary (HIGH confidence)
- [dotenvy GitHub Repository](https://github.com/allan2/dotenvy) - Official documentation and examples
- [dotenvy Context7 /allan2/dotenvy](https://context7.com) - 10 code snippets, High source reputation
- [dotenvy Context7 /websites/rs_dotenvy](https://context7.com) - 770 code snippets, High source reputation, Benchmark Score 49.1
- [serde_yml Context7 /websites/rs_crate_serde_yml](https://context7.com) - 68 code snippets, High source reputation
- [unitedstates/congress-legislators GitHub](https://github.com/unitedstates/congress-legislators) - Official dataset source
- [unitedstates/congress-legislators README](https://github.com/unitedstates/congress-legislators/blob/main/README.md) - Field documentation
- [SQLite FTS5 Extension Official Docs](https://sqlite.org/fts5.html) - Full-text search documentation
- [Rust Cookbook - Downloads](https://rust-lang-nursery.github.io/rust-cookbook/web/clients/download.html) - reqwest download patterns

### Secondary (MEDIUM confidence)
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) - API structure and authentication
- [Sunlight Foundation OpenFEC Guide](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/) - /candidates/search endpoint usage
- [SQLite Full-Text Search Trigram Matching](https://davidmuraya.com/blog/sqlite-fts5-trigram-name-matching/) - Name matching patterns
- [GitIgnore Best Practices](https://gitignore.pro/guide) - .env file exclusion patterns
- [SQLite One-to-Many Relationships](https://towardsdatascience.com/fundamentals-of-relationships-and-joins-in-sqlite-82ab47806d00/) - Junction table design

### Tertiary (LOW confidence)
- Web search results on OpenFEC candidate endpoint structure (2026) - Confirmed `/candidates/search` endpoint exists but detailed parameters not verified

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries verified via Context7 with High source reputation and substantial code examples
- Architecture: HIGH - Patterns follow existing v1.1 implementation (migrations, error types, CLI dispatch)
- Pitfalls: HIGH - Based on documented issues in dotenvy, serde_yml, and SQLite communities; validated against existing codebase patterns
- Congress-legislators dataset: HIGH - Active GitHub repository with 1.7k stars, maintained by civic tech community
- OpenFEC API: MEDIUM - API documentation accessed but detailed rate limits and candidate search parameters require additional verification

**Research date:** 2026-02-11
**Valid until:** 2026-03-11 (30 days - stable domain with mature libraries)
