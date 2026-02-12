# Architecture Patterns: OpenFEC Campaign Donation Integration

**Domain:** FEC Campaign Donation Data Integration
**Researched:** 2026-02-11
**Overall confidence:** HIGH

## Executive Summary

OpenFEC integration follows the established Capitol Traders pattern: thin HTTP client wrapper over reqwest, SQLite for persistence, Semaphore+JoinSet+mpsc for concurrent enrichment. Architecture adds: (1) **openfec module** in capitoltraders_lib mirroring yahoo.rs structure, (2) **donations table + fec_mappings** in Schema v3, (3) **sync-donations subcommand** reusing sync.rs enrichment pipeline, (4) **dotenvy .env loading** in main.rs for API key. No new runtime dependencies - leverages existing reqwest 0.12 + tokio 1.x stack.

Critical architectural decision: **FEC is committee-centric, not politician-centric**. Must map politician -> candidate_id -> committee_ids before donation sync. This 3-tier mapping becomes foundation layer, cached in fec_mappings table to avoid repeated API lookups.

## Module Layout

### New Modules in capitoltraders_lib

```
capitoltraders_lib/src/
  openfec/
    mod.rs           # Public exports: OpenFecClient, types, errors
    client.rs        # OpenFecClient struct, HTTP operations, rate limiting
    types.rs         # ScheduleAContribution, Committee, Candidate, pagination
    error.rs         # OpenFecError enum (thiserror)
    query.rs         # Query builders for Schedule A filters (optional, Phase 2+)

  lib.rs             # Add: pub mod openfec; pub use openfec::*;
  db.rs              # Add: donation upsert, fec_mapping CRUD, query_donations
```

### Pattern Justification

**Mirrors yahoo.rs structure:**
- yahoo.rs: 378 lines, single file with YahooClient struct + YahooError enum + helpers
- openfec/client.rs: Similar scope, HTTP client wrapper, keyset pagination, rate limiting
- openfec/types.rs: Separates data types from client logic (cleaner than yahoo.rs single-file approach)

**Follows existing client pattern:**

```rust
// yahoo.rs pattern
pub struct YahooClient {
    connector: yahoo_finance_api::YahooConnector,
    cache: Arc<DashMap<(String, NaiveDate), Option<f64>>>,
}

// openfec/client.rs pattern (proposed)
pub struct OpenFecClient {
    api_key: String,
    client: reqwest::Client,
    base_url: String,  // For testing injection (wiremock pattern)
    // Cache: politician_id -> Vec<committee_id>
    committee_cache: Arc<DashMap<String, Vec<String>>>,
}
```

**Why separate openfec module vs single file:**
- FEC types are more complex than Yahoo (Candidate, Committee, ScheduleA have 10+ fields each)
- Query builders for Schedule A filters (committee_id, min_date, max_date, per_page, last_index) warrant separate file
- Keyset pagination state management (last_index, last_contribution_receipt_date) needs dedicated logic
- Separating concerns improves testability (mock types without mocking client)

### CLI Command Structure

**New subcommand in capitoltraders_cli/src/commands/:**

```
commands/
  sync_donations.rs   # New: donations sync command (mirrors sync.rs structure)
  donations.rs        # New: query/display donations (mirrors trades.rs structure)
  mod.rs              # Add: pub mod sync_donations; pub mod donations;
```

**CLI main.rs additions:**

```rust
// In Commands enum
#[derive(Subcommand)]
enum Commands {
    // Existing...
    Trades(Box<commands::trades::TradesArgs>),
    Politicians(commands::politicians::PoliticiansArgs),

    // New:
    #[command(about = "Sync campaign donations from OpenFEC")]
    SyncDonations(commands::sync_donations::SyncDonationsArgs),

    #[command(about = "Query and display campaign donations")]
    Donations(commands::donations::DonationsArgs),
}

// In main()
#[tokio::main]
async fn main() -> Result<()> {
    // NEW: Load .env at startup (API key)
    let _ = dotenvy::dotenv();

    // Existing tracing setup...

    match &cli.command {
        // Existing handlers...

        Commands::SyncDonations(args) => {
            let api_key = std::env::var("OPENFEC_API_KEY")
                .map_err(|_| anyhow!("OPENFEC_API_KEY not set in environment"))?;
            commands::sync_donations::run(args, &api_key).await?
        }
        Commands::Donations(args) => {
            commands::donations::run(args, &format)?
        }
    }

    Ok(())
}
```

**Why this structure:**
- Separates sync (write operations, FEC API) from query (read operations, SQLite)
- Mirrors existing trades.rs (query) and sync.rs (ingest) separation
- sync_donations handles politician->committee mapping + Schedule A fetch
- donations handles aggregation and display

## Schema v3 Design

### New Tables DDL

```sql
-- FEC candidate/committee mappings (cached politician resolution)
CREATE TABLE IF NOT EXISTS fec_mappings (
    politician_id TEXT PRIMARY KEY,
    candidate_id TEXT NOT NULL,          -- FEC candidate ID (e.g., "H0CA12345")
    candidate_name TEXT NOT NULL,        -- Verified FEC name
    committee_ids TEXT NOT NULL,         -- JSON array of committee IDs
    committee_types TEXT,                -- JSON object {cmte_id: cmte_type}
    mapped_at TEXT NOT NULL,             -- When mapping was created
    last_verified TEXT,                  -- Last API verification timestamp
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

-- Individual campaign contributions (Schedule A data)
CREATE TABLE IF NOT EXISTS donations (
    donation_id INTEGER PRIMARY KEY AUTOINCREMENT,
    politician_id TEXT NOT NULL,         -- Link to politicians table
    committee_id TEXT NOT NULL,          -- FEC committee receiving donation
    contributor_name TEXT,               -- Full name as reported
    contributor_employer TEXT,           -- Employer (key for correlation)
    contributor_occupation TEXT,         -- Occupation
    contribution_date TEXT NOT NULL,     -- Receipt date (YYYY-MM-DD)
    contribution_amount REAL NOT NULL,   -- Dollar amount
    aggregate_ytd REAL,                  -- Year-to-date total from this contributor
    election_cycle INTEGER NOT NULL,     -- 2-year cycle (2024, 2026, etc.)
    entity_type TEXT,                    -- IND (individual), ORG, etc.
    contributor_city TEXT,
    contributor_state TEXT,
    contributor_zip TEXT,
    fec_sub_id TEXT UNIQUE,              -- OpenFEC submission ID (for deduplication)
    file_number TEXT,                    -- FEC filing number
    synced_at TEXT NOT NULL,             -- When this record was synced
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

-- Donation sync metadata (track resume points per politician)
CREATE TABLE IF NOT EXISTS donation_sync_meta (
    politician_id TEXT PRIMARY KEY,
    committee_id TEXT NOT NULL,
    last_sync_date TEXT NOT NULL,              -- When sync last ran
    last_contribution_date TEXT,               -- Latest contribution_date in DB
    last_index INTEGER,                        -- Keyset pagination: last_index
    total_contributions INTEGER DEFAULT 0,     -- Count for this politician
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_donations_politician ON donations(politician_id);
CREATE INDEX IF NOT EXISTS idx_donations_committee ON donations(committee_id);
CREATE INDEX IF NOT EXISTS idx_donations_date ON donations(contribution_date);
CREATE INDEX IF NOT EXISTS idx_donations_cycle ON donations(election_cycle);
CREATE INDEX IF NOT EXISTS idx_donations_employer ON donations(contributor_employer);
CREATE INDEX IF NOT EXISTS idx_donations_amount ON donations(contribution_amount);
CREATE INDEX IF NOT EXISTS idx_fec_mappings_candidate ON fec_mappings(candidate_id);
```

### Schema Integration with Existing Tables

**No changes to existing tables** - donations are separate dimension from trades.

**Foreign key relationships:**
- donations.politician_id -> politicians.politician_id (CASCADE DELETE)
- fec_mappings.politician_id -> politicians.politician_id (CASCADE DELETE)

**Why politician_id linkage:**
- Enables JOIN queries: "trades by politician X + donations to politician X"
- Natural key for correlation analysis
- Already indexed in existing schema

### Data Flow: Politician -> Donations

```
1. User: capitoltraders sync-donations --politician "Nancy Pelosi"

2. Check fec_mappings:
   - If exists AND last_verified < 30 days ago: use cached committee_ids
   - Else: resolve politician name via OpenFEC

3. Resolve candidate_id (if needed):
   GET /v1/candidates/search/?q=Nancy+Pelosi&office=H&state=CA
   - Parse results, find best match (name similarity + office + state)
   - Store candidate_id = "H0CA12345"

4. Resolve committee_ids (if needed):
   GET /v1/committees/?candidate_id=H0CA12345
   - Filter: committee_type IN ('H', 'P') (authorized committees)
   - Extract: committee_ids = ["C00401224", "C00401232"]
   - Store in fec_mappings.committee_ids as JSON array

5. For each committee_id, fetch Schedule A:
   GET /v1/schedules/schedule_a/?committee_id=C00401224&per_page=100&sort=-contribution_receipt_date
   - Keyset pagination: append &last_index=X&last_contribution_receipt_date=Y
   - Parse: ScheduleAContribution objects

6. Upsert donations:
   - Deduplicate by fec_sub_id (OpenFEC submission ID)
   - Insert new, ignore duplicates
   - Update donation_sync_meta (last_sync_date, last_contribution_date, last_index)

7. Report summary:
   "Synced 1,247 donations for Nancy Pelosi (2024 cycle)"
   "Committee C00401224: 892 contributions, $2.1M total"
   "Committee C00401232: 355 contributions, $780K total"
```

**Why this flow:**
- Step 2-4 (mapping) is one-time per politician (cached for 30 days)
- Step 5-6 (Schedule A sync) runs incrementally on subsequent syncs
- Keyset pagination state in donation_sync_meta enables resume after interruption
- Per-committee fetching allows parallel enrichment (multiple committees via JoinSet)

## FEC Client Design

### OpenFecClient Interface

```rust
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use dashmap::DashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OpenFecError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Rate limit exceeded (HTTP 429)")]
    RateLimitExceeded,

    #[error("Invalid API key (HTTP 403)")]
    InvalidApiKey,

    #[error("Candidate not found")]
    CandidateNotFound,

    #[error("Committee not found")]
    CommitteeNotFound,

    #[error("JSON deserialization failed: {0}")]
    JsonParse(#[from] serde_json::Error),
}

pub struct OpenFecClient {
    api_key: String,
    client: Client,
    base_url: String,
    // Cache politician_id -> committee_ids to avoid repeated lookups
    committee_cache: Arc<DashMap<String, Vec<String>>>,
}

impl OpenFecClient {
    pub fn new(api_key: String) -> Result<Self, OpenFecError> {
        Self::with_base_url(api_key, "https://api.open.fec.gov/v1".to_string())
    }

    // For testing with wiremock
    pub fn with_base_url(api_key: String, base_url: String) -> Result<Self, OpenFecError> {
        if api_key.is_empty() {
            return Err(OpenFecError::InvalidApiKey);
        }

        Ok(Self {
            api_key,
            client: Client::new(),
            base_url,
            committee_cache: Arc::new(DashMap::new()),
        })
    }

    /// Search for candidates by name. Returns candidates matching query.
    pub async fn search_candidates(
        &self,
        name: &str,
        office: Option<&str>,
        state: Option<&str>,
    ) -> Result<Vec<Candidate>, OpenFecError> {
        let url = format!("{}/candidates/search/", self.base_url);
        let mut params = vec![
            ("api_key", self.api_key.as_str()),
            ("q", name),
        ];
        if let Some(o) = office {
            params.push(("office", o));
        }
        if let Some(s) = state {
            params.push(("state", s));
        }

        let response = self.client.get(&url).query(&params).send().await?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let data: OpenFecResponse<Candidate> = response.json().await?;
                Ok(data.results)
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => Err(OpenFecError::RateLimitExceeded),
            reqwest::StatusCode::FORBIDDEN => Err(OpenFecError::InvalidApiKey),
            _ => Err(OpenFecError::Request(
                reqwest::Error::from(response.error_for_status().unwrap_err())
            )),
        }
    }

    /// Get committees for a specific candidate.
    pub async fn get_committees_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Vec<Committee>, OpenFecError> {
        let url = format!("{}/committees/", self.base_url);
        let params = [
            ("api_key", self.api_key.as_str()),
            ("candidate_id", candidate_id),
        ];

        let response = self.client.get(&url).query(&params).send().await?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let data: OpenFecResponse<Committee> = response.json().await?;
                Ok(data.results)
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => Err(OpenFecError::RateLimitExceeded),
            reqwest::StatusCode::FORBIDDEN => Err(OpenFecError::InvalidApiKey),
            _ => Err(OpenFecError::Request(
                reqwest::Error::from(response.error_for_status().unwrap_err())
            )),
        }
    }

    /// Fetch Schedule A contributions for a committee (keyset paginated).
    pub async fn get_schedule_a(
        &self,
        committee_id: &str,
        min_date: Option<&str>,
        max_date: Option<&str>,
        last_index: Option<i64>,
        last_contribution_date: Option<&str>,
        per_page: Option<i32>,
    ) -> Result<ScheduleAPage, OpenFecError> {
        let url = format!("{}/schedules/schedule_a/", self.base_url);
        let mut params = vec![
            ("api_key", self.api_key.as_str()),
            ("committee_id", committee_id),
            ("sort", "-contribution_receipt_date"),  // Most recent first
        ];

        if let Some(min) = min_date {
            params.push(("min_date", min));
        }
        if let Some(max) = max_date {
            params.push(("max_date", max));
        }
        if let Some(pp) = per_page {
            params.push(("per_page", &pp.to_string()));
        }

        // Keyset pagination parameters
        if let Some(idx) = last_index {
            params.push(("last_index", &idx.to_string()));
        }
        if let Some(lcd) = last_contribution_date {
            params.push(("last_contribution_receipt_date", lcd));
        }

        let response = self.client.get(&url).query(&params).send().await?;

        match response.status() {
            reqwest::StatusCode::OK => {
                let data: OpenFecResponse<ScheduleAContribution> = response.json().await?;
                Ok(ScheduleAPage {
                    contributions: data.results,
                    pagination: data.pagination,
                })
            }
            reqwest::StatusCode::TOO_MANY_REQUESTS => Err(OpenFecError::RateLimitExceeded),
            reqwest::StatusCode::FORBIDDEN => Err(OpenFecError::InvalidApiKey),
            _ => Err(OpenFecError::Request(
                reqwest::Error::from(response.error_for_status().unwrap_err())
            )),
        }
    }
}
```

**Design Rationale:**

1. **Separate methods for each endpoint** - candidate search, committee lookup, Schedule A fetch. Matches OpenFEC API structure.

2. **API key via constructor** - not global, not hardcoded. Passed at initialization.

3. **Base URL injection** - with_base_url() for wiremock testing. Same pattern as capitoltrades_api Client.

4. **Committee cache** - DashMap<politician_id, Vec<committee_id>> in-memory cache. Avoids repeated API calls for same politician. Same pattern as yahoo.rs DashMap cache.

5. **Keyset pagination parameters** - last_index + last_contribution_receipt_date optional params. Required for Schedule A large datasets.

6. **Error enum** - thiserror for typed errors. Distinguishes rate limit (429) from auth (403) from not found vs generic request error.

### Pagination Type Definitions

```rust
#[derive(Debug, Deserialize, Serialize)]
pub struct OpenFecResponse<T> {
    pub pagination: Pagination,
    pub results: Vec<T>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Pagination {
    pub count: i64,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub pages: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
    // Keyset pagination (Schedule A/B only)
    #[serde(default)]
    pub last_indexes: Option<LastIndexes>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LastIndexes {
    pub last_index: i64,
    pub last_contribution_receipt_date: String,
}

#[derive(Debug)]
pub struct ScheduleAPage {
    pub contributions: Vec<ScheduleAContribution>,
    pub pagination: Pagination,
}
```

**Why #[serde(default)]:**
- OpenFEC uses different pagination for different endpoints
- Candidate/committee endpoints: page + pages + per_page
- Schedule A endpoint: last_indexes (keyset)
- #[serde(default)] allows both schemas to deserialize into same Pagination struct

### Schedule A Contribution Type

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ScheduleAContribution {
    // Contribution metadata
    pub sub_id: Option<String>,                  // Unique submission ID (for deduplication)
    pub committee_id: Option<String>,            // Receiving committee (cmte_id in API)
    pub file_num: Option<i64>,                   // Filing number

    // Contributor info
    pub contributor_name: Option<String>,        // contbr_nm
    pub contributor_employer: Option<String>,    // contbr_employer
    pub contributor_occupation: Option<String>,  // contbr_occupation
    pub contributor_city: Option<String>,        // contbr_city
    pub contributor_state: Option<String>,       // contbr_st
    pub contributor_zip: Option<String>,         // contbr_zip

    // Contribution details
    pub contribution_receipt_date: Option<String>,  // contb_receipt_dt (YYYY-MM-DD)
    pub contribution_receipt_amount: Option<f64>,   // contb_receipt_amt
    pub contributor_aggregate_ytd: Option<f64>,     // contb_aggregate_ytd

    // Election metadata
    pub fec_election_yr: Option<i32>,            // Election cycle year
    pub entity_tp: Option<String>,               // Entity type (IND, ORG, etc.)
}
```

**Field Notes:**
- All Option<T> because OpenFEC data has missing fields frequently
- Field names match OpenFEC API snake_case (contbr_nm, not contributorName)
- sub_id is primary deduplication key (unique per FEC filing)
- contributor_employer is critical for issuer correlation feature

## Enrichment Pipeline Reuse

### Concurrent Fetch Pattern

**Existing pattern (enrich_prices.rs):**

```rust
// Phase 1: Deduplicate by (ticker, date)
let ticker_date_map: HashMap<(String, NaiveDate), Vec<usize>> = ...;

// Phase 2: Concurrent fetch with Semaphore + JoinSet + mpsc
const CONCURRENCY: usize = 5;
let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
let (tx, mut rx) = mpsc::channel::<Result>(CONCURRENCY * 2);
let mut join_set = JoinSet::new();

for ((ticker, date), indices) in ticker_date_map {
    let sem = Arc::clone(&semaphore);
    let yahoo = Arc::clone(&yahoo_client);
    let tx = tx.clone();

    join_set.spawn(async move {
        let _permit = sem.acquire().await.unwrap();
        let result = yahoo.get_price_on_date(&ticker, date).await;
        let _ = tx.send((ticker, date, indices, result)).await;
    });
}
drop(tx);  // Close sender

// Phase 3: Single-threaded DB writes from channel
while let Some((ticker, date, indices, result)) = rx.recv().await {
    match result {
        Ok(Some(price)) => db.update_trade_price(...),
        Ok(None) => /* ticker not found */,
        Err(e) => circuit_breaker.record_failure(),
    }
    if circuit_breaker.is_tripped() { break; }
}
```

### Adapted Pattern for OpenFEC

**sync_donations.rs enrichment:**

```rust
// Step 1: Map politicians to committees (may already be cached in fec_mappings)
let mut politician_committees: HashMap<String, Vec<String>> = HashMap::new();

for politician_id in politician_ids {
    // Check DB cache first
    let committees = db.get_fec_mapping(&politician_id)?
        .map(|m| serde_json::from_str(&m.committee_ids).unwrap())
        .unwrap_or_else(|| {
            // Resolve via OpenFEC API
            let candidate_id = fec_client.search_candidates(&name, office, state).await?;
            let committees = fec_client.get_committees_for_candidate(&candidate_id).await?;
            db.upsert_fec_mapping(&politician_id, &candidate_id, &committees)?;
            committees.iter().map(|c| c.committee_id.clone()).collect()
        });

    politician_committees.insert(politician_id, committees);
}

// Step 2: Deduplicate by (politician_id, committee_id)
// Each politician may have 2-5 committees
let mut tasks: Vec<(String, String)> = vec![];  // (politician_id, committee_id)
for (pol_id, cmte_ids) in politician_committees {
    for cmte_id in cmte_ids {
        tasks.push((pol_id.clone(), cmte_id));
    }
}

// Step 3: Concurrent fetch with rate limiting
const CONCURRENCY: usize = 3;  // Lower than price enrichment (OpenFEC is 1000/hour)
let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
let (tx, mut rx) = mpsc::channel(CONCURRENCY * 2);
let mut join_set = JoinSet::new();

for (politician_id, committee_id) in tasks {
    let sem = Arc::clone(&semaphore);
    let fec = Arc::clone(&fec_client);
    let tx = tx.clone();

    // Get resume point from donation_sync_meta
    let resume_state = db.get_donation_sync_meta(&politician_id, &committee_id)?;

    join_set.spawn(async move {
        let _permit = sem.acquire().await.unwrap();

        // Keyset pagination loop
        let mut all_contributions = Vec::new();
        let mut last_index = resume_state.last_index;
        let mut last_date = resume_state.last_contribution_date;

        loop {
            let page = fec.get_schedule_a(
                &committee_id,
                resume_state.min_date.as_deref(),
                None,  // max_date
                last_index,
                last_date.as_deref(),
                Some(100),  // per_page
            ).await?;

            all_contributions.extend(page.contributions);

            // Check if more pages
            if let Some(last_indexes) = page.pagination.last_indexes {
                last_index = Some(last_indexes.last_index);
                last_date = Some(last_indexes.last_contribution_receipt_date);
            } else {
                break;  // No more pages
            }
        }

        let _ = tx.send((politician_id, committee_id, Ok(all_contributions))).await;
    });
}
drop(tx);

// Step 4: Single-threaded DB upserts
while let Some((politician_id, committee_id, result)) = rx.recv().await {
    match result {
        Ok(contributions) => {
            db.upsert_donations(&politician_id, &committee_id, &contributions)?;
            db.update_donation_sync_meta(&politician_id, &committee_id, ...)?;
        }
        Err(OpenFecError::RateLimitExceeded) => {
            circuit_breaker.record_failure();
            // Sleep 60 seconds, then retry
        }
        Err(e) => {
            eprintln!("Failed to fetch donations: {}", e);
        }
    }
    if circuit_breaker.is_tripped() { break; }
}
```

**Pattern Differences from Price Enrichment:**

| Aspect | Price Enrichment | Donation Sync |
|--------|------------------|---------------|
| Deduplication key | (ticker, date) | (politician_id, committee_id) |
| Concurrency | 5 (Yahoo is lenient) | 3 (OpenFEC is 1000/hour) |
| Pagination | Single request per ticker/date | Keyset pagination loop per committee |
| Resume state | None (re-enriches all) | donation_sync_meta tracks last_index |
| Circuit breaker threshold | 10 consecutive failures | 5 (OpenFEC rate limits are stricter) |

**Why keyset pagination loop inside spawn:**
- Each committee may have 10K-50K donations across multiple pages
- Cannot paginate outside spawn (blocks other committees)
- Spawn task handles full pagination for one committee, then returns all contributions
- Alternative: yield pages incrementally via channel, but adds complexity

## .env Pattern Implementation

### Loading Mechanism

**main.rs addition:**

```rust
use dotenvy::dotenv;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file at startup (ignore if missing - production uses env vars directly)
    let _ = dotenv();

    // Existing tracing setup...
    tracing_subscriber::fmt()
        .with_env_filter(...)
        .init();

    // CLI parsing...
    let cli = Cli::parse();

    // Command dispatch...
    match &cli.command {
        Commands::SyncDonations(args) => {
            let api_key = std::env::var("OPENFEC_API_KEY")
                .map_err(|_| anyhow::anyhow!(
                    "OPENFEC_API_KEY not set. Add to .env file or environment."
                ))?;

            let fec_client = Arc::new(OpenFecClient::new(api_key)?);
            commands::sync_donations::run(args, &fec_client).await?
        }
        // Other commands...
    }

    Ok(())
}
```

**Why this approach:**
- Load .env once at startup (not per-command)
- Only read API key when SyncDonations command is invoked (not for trades/politicians commands)
- Fail fast with clear error message if key missing
- Follows 12-factor app pattern (config from environment)

### .env File Structure

```bash
# OpenFEC API Key
# Obtain from: https://api.data.gov/signup
OPENFEC_API_KEY=your_api_key_here

# Optional: Override OpenFEC base URL (for testing)
# OPENFEC_BASE_URL=http://localhost:8080
```

**.gitignore addition:**

```
# Environment variables
.env
.env.local
.env.*.local
```

**Documentation in README:**

```markdown
## OpenFEC API Setup

To sync campaign donation data, you need an OpenFEC API key:

1. Sign up at https://api.data.gov/signup
2. Verify your email
3. Create `.env` file in project root:
   ```
   OPENFEC_API_KEY=your_api_key_here
   ```
4. Run: `capitoltraders sync-donations --politician "Nancy Pelosi"`

API key is required only for `sync-donations` command. Other commands (trades, politicians) do not need OpenFEC access.
```

## Integration Points

### Trades Command Enhancement (Optional Phase 2+)

**Context display: "Top donors to this politician"**

```rust
// In trades.rs run_db()
if args.show_donor_context {
    let politician_id = &rows[0].politician_id;
    let top_donors = db.query_top_donors(politician_id, 5)?;  // Top 5

    eprintln!("\nTop 5 Donors to {}:", politician_name);
    for donor in top_donors {
        eprintln!("  {} ({}): ${:.0}",
            donor.contributor_name,
            donor.contributor_employer,
            donor.total_amount
        );
    }
}
```

**CLI flag:**
```
capitoltraders trades --politician "Nancy Pelosi" --db trades.db --show-donor-context
```

**Why optional Phase 2:**
- Requires trades + donations data both synced
- Adds complexity to trades output
- Better as separate feature after core donation sync proven

### Portfolio Command Enhancement (Optional Phase 2+)

**"Funded by" summary:**

```rust
// In portfolio.rs run()
if args.show_funding {
    // For each issuer in portfolio, find donations from related employers
    for position in &positions {
        let related_donations = db.query_donations_by_employer_fuzzy(
            &politician_id,
            &position.issuer_name,  // Fuzzy match employer to issuer
            0.85  // Similarity threshold
        )?;

        if !related_donations.is_empty() {
            eprintln!("  {} ({}) - Donations from related employers: ${:.0}",
                position.ticker,
                position.issuer_name,
                related_donations.iter().map(|d| d.contribution_amount).sum::<f64>()
            );
        }
    }
}
```

**Why optional Phase 2:**
- Requires employer-to-issuer fuzzy matching (complex)
- High false positive risk ("Goldman Sachs" employer vs "Goldman Sachs Group Inc" issuer)
- Defer until employer normalization infrastructure built

### Donations Command (Core - Phase 1)

**Query and display donations:**

```rust
// capitoltraders_cli/src/commands/donations.rs

#[derive(Args)]
pub struct DonationsArgs {
    /// Filter by politician name
    #[arg(long)]
    pub politician: String,

    /// Filter by election cycle (e.g., 2024)
    #[arg(long)]
    pub cycle: Option<i32>,

    /// Filter by minimum contribution amount
    #[arg(long)]
    pub min_amount: Option<f64>,

    /// Filter by employer (fuzzy match)
    #[arg(long)]
    pub employer: Option<String>,

    /// Show top N donors (aggregate by contributor)
    #[arg(long, default_value = "20")]
    pub top: i64,

    /// Group by: contributor (default), employer, industry, state
    #[arg(long, default_value = "contributor")]
    pub group_by: String,

    /// Database path (required)
    #[arg(long)]
    pub db: PathBuf,
}

pub fn run(args: &DonationsArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Get politician_id from name
    let politician_id = db.get_politician_id_by_name(&args.politician)?
        .ok_or_else(|| anyhow!("Politician not found: {}", args.politician))?;

    // Build filter
    let filter = DonationFilter {
        politician_id,
        cycle: args.cycle,
        min_amount: args.min_amount,
        employer: args.employer.as_deref(),
    };

    // Query and aggregate
    let donations = db.query_donations(&filter)?;
    let aggregated = match args.group_by.as_str() {
        "contributor" => aggregate_by_contributor(&donations),
        "employer" => aggregate_by_employer(&donations),
        "state" => aggregate_by_state(&donations),
        _ => bail!("Invalid group_by: {}", args.group_by),
    };

    // Take top N
    let top_n = aggregated.into_iter().take(args.top as usize).collect::<Vec<_>>();

    // Format output
    match format {
        OutputFormat::Table => print_donations_table(&top_n),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&top_n)?),
        OutputFormat::Csv => print_donations_csv(&top_n)?,
        OutputFormat::Markdown => print_donations_markdown(&top_n),
        OutputFormat::Xml => print_donations_xml(&top_n),
    }

    Ok(())
}
```

**Why this design:**
- DB-only (no scrape mode - donations require sync first)
- Aggregation in code vs SQL for flexibility (like portfolio.rs approach)
- group_by parameter allows different analysis views
- Reuses existing output.rs formatting patterns

## Code Pattern Recommendations

### Error Handling

```rust
// OpenFEC-specific errors
#[derive(Error, Debug)]
pub enum OpenFecError {
    #[error("Rate limit exceeded (HTTP 429) - try again in {retry_after} seconds")]
    RateLimitExceeded { retry_after: u64 },

    #[error("Invalid API key (HTTP 403)")]
    InvalidApiKey,

    #[error("Candidate not found: {name}")]
    CandidateNotFound { name: String },

    #[error("Committee not found: {committee_id}")]
    CommitteeNotFound { committee_id: String },

    #[error("HTTP request failed")]
    Request(#[from] reqwest::Error),

    #[error("JSON parse error")]
    JsonParse(#[from] serde_json::Error),
}

// Convert to CapitolTradesError at command layer
impl From<OpenFecError> for CapitolTradesError {
    fn from(e: OpenFecError) -> Self {
        match e {
            OpenFecError::RateLimitExceeded { retry_after } => {
                CapitolTradesError::RateLimit(format!("OpenFEC rate limited, retry in {}s", retry_after))
            }
            _ => CapitolTradesError::Upstream(e.to_string()),
        }
    }
}
```

**Pattern:**
- Domain errors (OpenFecError) in lib layer
- Application errors (CapitolTradesError) in CLI layer
- Convert at boundary (commands/*.rs)
- Preserve context (rate limit includes retry_after)

### Caching Strategy

```rust
// In OpenFecClient
impl OpenFecClient {
    pub async fn resolve_politician_committees(
        &self,
        politician_id: &str,
        db: &Db,
    ) -> Result<Vec<String>, OpenFecError> {
        // Check in-memory cache first
        if let Some(cached) = self.committee_cache.get(politician_id) {
            return Ok(cached.clone());
        }

        // Check DB cache (fec_mappings table)
        if let Some(mapping) = db.get_fec_mapping(politician_id)? {
            let committees: Vec<String> = serde_json::from_str(&mapping.committee_ids)?;
            self.committee_cache.insert(politician_id.to_string(), committees.clone());
            return Ok(committees);
        }

        // Fetch from OpenFEC API (expensive)
        let candidate = self.search_candidates(&politician_name, office, state).await?
            .into_iter().next()
            .ok_or(OpenFecError::CandidateNotFound { name: politician_name.to_string() })?;

        let committees = self.get_committees_for_candidate(&candidate.candidate_id).await?;
        let committee_ids: Vec<String> = committees.iter()
            .map(|c| c.committee_id.clone())
            .collect();

        // Store in DB cache
        db.upsert_fec_mapping(politician_id, &candidate.candidate_id, &committee_ids)?;

        // Store in memory cache
        self.committee_cache.insert(politician_id.to_string(), committee_ids.clone());

        Ok(committee_ids)
    }
}
```

**Three-tier cache:**
1. In-memory (DashMap) - fastest, process lifetime
2. SQLite (fec_mappings) - persistent, cross-session
3. OpenFEC API - source of truth, rate-limited

**Why three tiers:**
- In-memory: Avoid DB hits for same politician in batch operation
- SQLite: Avoid API calls for previously-mapped politicians
- API: Fallback for new politicians or cache miss

### Testing Patterns

**Unit test (deserialization):**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_schedule_a_response() {
        let json = include_str!("../../tests/fixtures/schedule_a_response.json");
        let response: OpenFecResponse<ScheduleAContribution> =
            serde_json::from_str(json).unwrap();

        assert_eq!(response.results.len(), 100);
        assert!(response.pagination.last_indexes.is_some());
    }

    #[test]
    fn deserialize_candidate_search_response() {
        let json = include_str!("../../tests/fixtures/candidate_search.json");
        let response: OpenFecResponse<Candidate> =
            serde_json::from_str(json).unwrap();

        assert!(!response.results.is_empty());
        assert_eq!(response.results[0].office, Some("H".to_string()));
    }
}
```

**Integration test (wiremock):**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, query_param};

    #[tokio::test]
    async fn test_search_candidates() {
        let mock_server = MockServer::start().await;

        let fixture = include_str!("../../tests/fixtures/candidate_search.json");
        Mock::given(method("GET"))
            .and(path("/v1/candidates/search/"))
            .and(query_param("api_key", "test_key"))
            .and(query_param("q", "Nancy Pelosi"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        let client = OpenFecClient::with_base_url(
            "test_key".to_string(),
            mock_server.uri()
        ).unwrap();

        let results = client.search_candidates("Nancy Pelosi", None, None).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_rate_limit_handling() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let client = OpenFecClient::with_base_url(
            "test_key".to_string(),
            mock_server.uri()
        ).unwrap();

        let result = client.search_candidates("Test", None, None).await;
        assert!(matches!(result, Err(OpenFecError::RateLimitExceeded { .. })));
    }
}
```

**DB test (fec_mappings CRUD):**

```rust
#[cfg(test)]
mod db_tests {
    use super::*;

    #[test]
    fn test_upsert_fec_mapping() {
        let db = Db::open_in_memory().unwrap();
        db.init().unwrap();

        let politician_id = "P000001";
        let candidate_id = "H0CA12345";
        let committee_ids = vec!["C00401224".to_string(), "C00401232".to_string()];

        db.upsert_fec_mapping(politician_id, candidate_id, &committee_ids).unwrap();

        let mapping = db.get_fec_mapping(politician_id).unwrap().unwrap();
        assert_eq!(mapping.candidate_id, candidate_id);

        let stored_committees: Vec<String> = serde_json::from_str(&mapping.committee_ids).unwrap();
        assert_eq!(stored_committees, committee_ids);
    }
}
```

**Pattern: Fixture files in tests/fixtures/:**
- schedule_a_response.json (100 contributions with keyset pagination)
- candidate_search.json (search results for "Nancy Pelosi")
- committees.json (committees for candidate H0CA12345)

## Anti-Patterns to Avoid

| Anti-Pattern | Why Bad | Correct Approach |
|--------------|---------|------------------|
| Global API key constant | Hardcoded secrets, testing nightmare | Pass via constructor, load from env |
| Sync blocking calls in async | Blocks tokio thread pool | Use async reqwest, spawn_blocking for heavy CPU |
| Page number pagination for Schedule A | OpenFEC uses keyset, page numbers will miss records | Use last_index + last_contribution_receipt_date |
| Single committee assumption | Politicians have 2-5 committees | Fetch all authorized committees per politician |
| No deduplication by sub_id | Re-syncs will duplicate donations | INSERT OR IGNORE on sub_id (unique constraint) |
| Fetching all donations in one request | Paginated dataset (10K+ records) | Loop keyset pagination until last_indexes is None |
| Mixing donation tables with trade tables | Different schemas, different sync patterns | Separate donations table with politician_id foreign key |
| Employer string exact match | "Google" != "Google LLC" | Fuzzy matching with threshold (Phase 2+) |
| No rate limit handling | 1000 calls/hour hard limit | Concurrent limit = 3, exponential backoff on 429 |
| Storing API responses as JSON blob | Query performance nightmare | Parse into structured donations table with indexes |

## Performance Considerations

### Bottlenecks

| Operation | Bottleneck | Mitigation |
|-----------|-----------|------------|
| Politician name resolution | OpenFEC candidate search API call | Cache in fec_mappings table, in-memory DashMap |
| Committee lookup per politician | OpenFEC committee API call | Cache with politician_id, persist in DB |
| Schedule A pagination | 100 records/request, 10K+ total | Concurrent fetch (3 committees in parallel), keyset pagination |
| Donation upsert | SQLite single-threaded writes | Batch INSERT with ON CONFLICT IGNORE, transaction per committee |
| Employer aggregation query | Full table scan on text field | Index on contributor_employer, normalize common employers |

### Scaling Estimates

**Single politician donation sync:**
- API calls: 1 (candidate search) + 1 (committee lookup) + N (Schedule A pages)
- For politician with 10,000 donations: 1 + 1 + 100 pages = 102 API calls
- At 3 concurrent, 1000/hour limit: 102 calls = ~6 minutes with delays
- DB writes: 10,000 inserts in transaction: <1 second

**Full Congress sync (535 members):**
- API calls: 535 (candidate) + 535 (committee) + ~50,000 (Schedule A pages) = ~51,070 calls
- At 1000/hour limit: 51 hours minimum
- Realistic with delays: 60-72 hours
- **Conclusion:** Full sync is multi-day operation, must be incremental (sync on-demand per politician)

### Optimization Strategies

1. **Incremental sync by politician** - don't sync all 535 members, sync when user queries
2. **Resume from last_contribution_date** - donation_sync_meta tracks progress per committee
3. **Parallel committee fetch** - JoinSet spawns 3 concurrent tasks (rate limit safe)
4. **Batch DB inserts** - transaction per committee, not per donation
5. **Index critical fields** - politician_id, contribution_date, contributor_employer

## Success Criteria

Architecture complete when:

- [ ] OpenFecClient fetches candidate, committee, Schedule A data
- [ ] Keyset pagination loops handle 10K+ donation records
- [ ] Three-tier cache (memory + DB + API) avoids redundant calls
- [ ] Semaphore+JoinSet+mpsc pattern handles concurrent committee fetch
- [ ] Schema v3 tables (donations, fec_mappings, donation_sync_meta) created
- [ ] DB upsert methods deduplicate by sub_id
- [ ] Circuit breaker stops sync after 5 consecutive 429 errors
- [ ] .env file loads OPENFEC_API_KEY at startup
- [ ] sync-donations command maps politician -> donations
- [ ] donations command queries and displays aggregated donor data
- [ ] All output formats (table, JSON, CSV, markdown, XML) supported
- [ ] Tests: 15+ unit tests, 8+ wiremock integration tests, 10+ DB tests
- [ ] Documentation: README API key setup, CLAUDE.md sync patterns

**Quality bar:** Type-safe, testable, resumable, rate-limit safe, follows existing patterns.

## Sources

**OpenFEC API Documentation:**
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/)
- [Schedule A Column Documentation - GitHub Wiki](https://github.com/fecgov/openFEC/wiki/Schedule-A-column-documentation)
- [OpenFEC GitHub Repository](https://github.com/fecgov/openFEC)
- [OpenFEC Postman Collection](https://www.postman.com/api-evangelist/federal-election-commission-fec/documentation/19lr6vr/openfec)

**Pagination & Rate Limiting:**
- [OpenFEC API Update - 67 Million Records - 18F](https://18f.gsa.gov/2015/07/15/openfec-api-update/)
- [OpenFEC Getting Started Guide - Sunlight Foundation](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/)
- [Schedule E Pagination Issue #3396 - GitHub](https://github.com/fecgov/openFEC/issues/3396)

**Candidate/Committee Mapping:**
- [FEC Candidate ID Structure - OpenSecrets](https://www.opensecrets.org/resources/faq)
- [FEC Committee Types - FEC.gov](https://www.fec.gov/campaign-finance-data/committee-type-code-descriptions/)

**Rust Ecosystem:**
- [dotenvy Crate Documentation](https://docs.rs/dotenvy/latest/dotenvy/)
- [dotenvy GitHub Repository - allan2/dotenvy](https://github.com/allan2/dotenvy)
- [reqwest Crate Documentation](https://docs.rs/reqwest/)
- [DashMap Crate Documentation](https://docs.rs/dashmap/)

**Existing Codebase Patterns:**
- Capitol Traders db.rs: upsert patterns, migration v1/v2, transaction handling
- Capitol Traders yahoo.rs: DashMap cache, date handling, error patterns
- Capitol Traders sync.rs: Semaphore+JoinSet+mpsc enrichment pipeline
- Capitol Traders enrich_prices.rs: Circuit breaker, progress bar, batch processing

**Confidence Level: HIGH** - OpenFEC API officially documented, dotenvy actively maintained, existing codebase patterns well-established, no unknowns in critical path.
