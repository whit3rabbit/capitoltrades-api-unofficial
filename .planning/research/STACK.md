# Technology Stack: OpenFEC Integration

**Project:** Capitol Traders - FEC Donation Data Integration
**Researched:** 2026-02-11
**Overall confidence:** HIGH

## Executive Summary

OpenFEC API integration requires minimal new dependencies. No dedicated Rust client exists, but the existing reqwest 0.12 + serde stack handles OpenFEC's standard JSON REST API. Primary addition: **dotenvy 0.15.7** for .env file loading (API key storage). OpenFEC uses query parameter authentication (`?api_key=XXX`), standard JSON pagination with keyset cursors for large datasets (Schedule A/B), and enforces 1000 calls/hour rate limit via API Umbrella (returns HTTP 429 + X-RateLimit headers on breach).

**Key decision:** Build custom reqwest-based client rather than use external library. No FEC-specific Rust crates exist as of 2026-02-11.

## Recommended Stack

### Environment Variable Loading

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **dotenvy** | 0.15.7 | Load .env file for API key | Well-maintained dotenv fork, addresses RUSTSEC-2021-0141, actively maintained as of 2026-01 |

**Installation:**
```toml
# capitoltraders_lib/Cargo.toml
[dependencies]
dotenvy = "0.15"
```

**Rationale:** Original dotenv crate unmaintained since 2020-06-26 with security advisory RUSTSEC-2021-0141. Dotenvy is the recommended replacement with active maintenance, multiline support, and same API surface.

### HTTP Client (Existing - No Changes)

| Technology | Version | Purpose | Current Use |
|------------|---------|---------|-------------|
| **reqwest** | 0.12 | HTTP client for OpenFEC API | Already in workspace deps, used for CapitolTrades scraping + Yahoo Finance |
| **tokio** | 1.x | Async runtime | Already in workspace deps with "full" features |
| **serde_json** | 1.x | JSON deserialization | Already in workspace deps |

**NO new HTTP dependencies required.** Existing `reqwest = { version = "0.12", default-features = false, features = ["gzip", "rustls-tls"] }` is sufficient.

### Rate Limiting (Optional Enhancement)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **tokio::sync::Semaphore** | Built-in | Concurrency control | Already used in enrichment pipeline, zero-dep solution |
| **Manual tracking** | N/A | 429 response counting | Circuit breaker pattern exists in codebase |

**NOT recommended:**
- **reqwest-middleware** + **reqwest-ratelimit**: Adds middleware layer dependency, requires implementing custom RateLimiter trait. OpenFEC's 1000/hour limit is generous enough for manual tracking.
- **governor**: Standalone rate limiter crate. Overkill for single API with 1000/hour limit.

**Recommendation:** Use existing circuit breaker pattern (consecutive failure counter) + tokio::Semaphore for concurrency control if needed.

## OpenFEC API Technical Specification

### Authentication

**Method:** Query parameter (NOT header-based)
**Parameter:** `api_key`
**Format:** `https://api.open.fec.gov/v1/schedules/schedule_a/?api_key=YOUR_KEY_HERE`

**Key Sources:**
- Register at api.data.gov (free, 1000 calls/hour)
- Use DEMO_KEY for testing (expect aggressive rate limits)
- Request 7200 calls/hour (120/min) limit via email to APIinfo@fec.gov

**Implementation:**
```rust
impl OpenFecClient {
    pub fn new(api_key: String) -> anyhow::Result<Self> {
        if api_key.is_empty() {
            anyhow::bail!("OpenFEC API key cannot be empty");
        }
        if api_key == "DEMO_KEY" {
            tracing::warn!("Using DEMO_KEY - expect aggressive rate limits");
        }
        Ok(Self {
            api_key,
            client: reqwest::Client::new(),
            base_url: "https://api.open.fec.gov/v1".to_string(),
        })
    }

    // Append api_key to all requests
    fn build_url(&self, path: &str, params: &[(&str, &str)]) -> String {
        let mut url = format!("{}{}", self.base_url, path);
        let mut query_params = params.to_vec();
        query_params.push(("api_key", &self.api_key));

        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&serde_urlencoded::to_string(query_params).unwrap());
        }
        url
    }
}
```

### Rate Limiting

| Aspect | Value | Details |
|--------|-------|---------|
| **Default hourly limit** | 1000 requests | Rolling window (not clock hour) |
| **Enhanced limit** | 7200 requests/hour (120/min) | Available via email request |
| **Per-page results** | 100 records (default) | Configurable via `per_page` param |
| **Rate limit headers** | X-RateLimit-Limit, X-RateLimit-Remaining | Included in every response |
| **Exceeded response** | HTTP 429 | API Umbrella rate limiter |

**Header inspection pattern:**
```rust
async fn check_rate_limit(response: &reqwest::Response) {
    if let Some(limit) = response.headers().get("X-RateLimit-Limit") {
        if let Some(remaining) = response.headers().get("X-RateLimit-Remaining") {
            tracing::debug!(
                "Rate limit: {} remaining of {} total",
                remaining.to_str().unwrap_or("?"),
                limit.to_str().unwrap_or("?")
            );
        }
    }
}
```

**Circuit breaker on 429:**
```rust
if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
    return Err(OpenFecError::RateLimitExceeded);
}
```

### Pagination Mechanisms

OpenFEC uses **two different pagination models** depending on dataset size.

#### Standard Pagination (Small Datasets)

**Endpoints:** `/candidates/`, `/committees/`, most lookup endpoints

**Response structure:**
```json
{
  "pagination": {
    "count": 1547892,
    "page": 1,
    "pages": 15479,
    "per_page": 100
  },
  "results": [...]
}
```

**Implementation:**
```rust
#[derive(Debug, Deserialize)]
pub struct OpenFecResponse<T> {
    pub pagination: Pagination,
    pub results: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct Pagination {
    pub count: i64,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub pages: Option<i64>,
    #[serde(default)]
    pub per_page: Option<i64>,
    #[serde(default)]
    pub last_indexes: Option<LastIndexes>,
}
```

#### Keyset Pagination (Large Datasets)

**Endpoints:** `/schedules/schedule_a/`, `/schedules/schedule_b/` (67M+ records)

**Response structure:**
```json
{
  "pagination": {
    "count": 67000000,
    "last_indexes": {
      "last_index": 230880619,
      "last_contribution_receipt_date": "2014-01-01"
    }
  },
  "results": [...]
}
```

**Critical difference:** Schedule A/B do NOT support page numbers. Must use `last_index` + `last_contribution_receipt_date` from previous response to fetch next batch.

**Why keyset pagination:** Offset-based pagination on 67M records would cause severe performance degradation. Keyset ensures stable iteration without missing/duplicating records.

**Implementation:**
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct LastIndexes {
    pub last_index: i64,
    pub last_contribution_receipt_date: String,
}

impl OpenFecClient {
    pub async fn get_schedule_a_page(
        &self,
        last_indexes: Option<&LastIndexes>,
    ) -> Result<OpenFecResponse<ScheduleAContribution>, OpenFecError> {
        let mut params = vec![("per_page", "100")];

        if let Some(indexes) = last_indexes {
            params.push(("last_index", &indexes.last_index.to_string()));
            params.push(("last_contribution_receipt_date", &indexes.last_contribution_receipt_date));
        }

        let url = self.build_url("/schedules/schedule_a/", &params);
        let response = self.client.get(&url).send().await?;

        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(OpenFecError::RateLimitExceeded);
        }

        let data = response.json::<OpenFecResponse<ScheduleAContribution>>().await?;
        Ok(data)
    }
}
```

### Schedule A Contribution Schema

**Key fields for politician correlation:**

| Field | Type | Description | Use Case |
|-------|------|-------------|----------|
| `cmte_id` | String | Committee receiving contribution | Primary correlation key (link to politician) |
| `contbr_nm` | String | Full contributor name | Display, search |
| `contbr_employer` | String | Employment affiliation | Analysis (corporate patterns) |
| `contbr_occupation` | String | Professional classification | Analysis (industry patterns) |
| `contb_receipt_dt` | String (date) | Date contribution received (YYYY-MM-DD) | Temporal analysis |
| `contb_receipt_amt` | Number | Monetary value of contribution | Aggregation, filtering |
| `contb_aggregate_ytd` | Number | Year-to-date cumulative total | Large donor identification |
| `fec_election_yr` | Integer | Federal election cycle year | Filtering by cycle |
| `entity_tp` | String | Entity type (IND, ORG, etc.) | Individual vs organizational |

**Additional fields:**
- **Address:** `contbr_st1`, `contbr_st2`, `contbr_city`, `contbr_st` (state), `contbr_zip`
- **IDs:** `contbr_id` (contributor ID), `cand_id` (candidate ID), `sub_id` (submission ID)
- **Filing metadata:** `file_num`, `filing_form`, `receipt_tp`, `memo_cd`, `memo_text`

**Serde type:**
```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct ScheduleAContribution {
    // Core identification
    pub cmte_id: String,
    pub contbr_nm: Option<String>,
    pub sub_id: Option<i64>,

    // Contributor details
    pub contbr_employer: Option<String>,
    pub contbr_occupation: Option<String>,
    pub contbr_city: Option<String>,
    pub contbr_st: Option<String>,  // State code
    pub contbr_zip: Option<String>,

    // Contribution details
    pub contb_receipt_dt: Option<String>,  // Parse to NaiveDate as needed
    pub contb_receipt_amt: Option<f64>,
    pub contb_aggregate_ytd: Option<f64>,
    pub fec_election_yr: Option<i32>,

    // Classification
    pub entity_tp: Option<String>,
    pub receipt_tp: Option<String>,

    // Optional fields
    pub cand_id: Option<String>,
    pub file_num: Option<i64>,
    pub memo_text: Option<String>,
}
```

**All fields `Option<T>`** - OpenFEC data quality varies, missing fields are common. Follow existing codebase pattern: Option types for scraped data, `unwrap_or_default()` for display layer.

**Field naming:** Keep OpenFEC's snake_case abbreviations (contbr = contributor, contb = contribution, cmte = committee) rather than renaming. Matches official FEC documentation.

## Correlation Strategy: Committee-Based Lookup

**Problem:** How to link FEC donation data to CapitolTrades politician records?

**Solution:** Query by `cmte_id` (committee ID) rather than politician name.

**Rationale:**
- FEC organizes data by committees (campaign committees, leadership PACs)
- Politicians have multiple committees across election cycles
- Committee IDs are stable, names vary
- OpenFEC provides `/committees/` endpoint to map politician names to committee IDs

**Two-step lookup:**
1. Query `/candidates/` with politician name → get `candidate_id`
2. Query `/candidate/{candidate_id}/committees/` → get list of `cmte_id` values
3. Query `/schedules/schedule_a/?committee_id={cmte_id}` → get contributions

**Example:**
```rust
// Step 1: Find candidate
let candidates = client.search_candidates("Pelosi").await?;
let candidate_id = candidates.results[0].candidate_id.clone();

// Step 2: Get committees
let committees = client.get_candidate_committees(&candidate_id).await?;
let cmte_ids: Vec<String> = committees.results.iter()
    .map(|c| c.committee_id.clone())
    .collect();

// Step 3: Fetch contributions for each committee
for cmte_id in cmte_ids {
    let contributions = client.get_schedule_a_by_committee(&cmte_id).await?;
    // Process contributions...
}
```

## Environment Variable Configuration

### .env File Structure

```bash
# OpenFEC API credentials
OPENFEC_API_KEY=your_api_key_here

# Optional: override base URL for testing
# OPENFEC_BASE_URL=https://api.open.fec.gov/v1
```

**DO NOT commit .env** - Add to .gitignore. Existing project has no .env pattern yet, establish now.

### .gitignore Addition

```
# Environment variables
.env
.env.local
.env.*.local
```

**Action required:** Check existing .gitignore, add if not present.

### Loading Pattern (dotenvy)

```rust
// At application startup (capitoltraders_cli/src/main.rs)
use dotenvy::dotenv;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file, ignore if missing (production may use env vars directly)
    let _ = dotenv();

    // Existing tracing setup...

    // Read API key from environment
    let openfec_key = std::env::var("OPENFEC_API_KEY")
        .map_err(|_| anyhow::anyhow!("OPENFEC_API_KEY not set in environment"))?;

    // Pass to client constructor
    let fec_client = OpenFecClient::new(openfec_key)?;

    // Parse CLI args and dispatch...

    Ok(())
}
```

**Key points:**
- Use `let _ = dotenv();` to ignore error if .env missing (supports production deployment with env vars)
- Validate API key presence early with clear error message
- Pass API key as constructor parameter (never use global/static)

### Type-Safe Config Pattern (Optional)

**NOT recommended for initial implementation.** Simple `std::env::var` sufficient for single API key.

**If config grows beyond API key,** consider `envy` crate:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub openfec_api_key: String,
    #[serde(default = "default_base_url")]
    pub openfec_base_url: String,
}

fn default_base_url() -> String {
    "https://api.open.fec.gov/v1".to_string()
}

fn load_config() -> anyhow::Result<Config> {
    let _ = dotenvy::dotenv();
    let config = envy::from_env::<Config>()?;
    Ok(config)
}
```

**Defer until needed.** Start simple, add envy if config complexity justifies it.

## Integration with Existing Stack

### Reuse Existing Infrastructure

| Component | Current Use | OpenFEC Use | Changes Needed |
|-----------|-------------|-------------|----------------|
| **reqwest::Client** | CapitolTrades scraping, Yahoo Finance API | OpenFEC API calls | None - reuse |
| **tokio runtime** | Async HTTP, concurrent enrichment | Same async runtime | None - reuse |
| **serde_json** | Deserialize CapitolTrades JSON | Deserialize OpenFEC JSON | None - reuse |
| **DashMap cache** | In-memory trade cache (300s TTL) | Cache committee lookups | None - reuse pattern |
| **thiserror** | CapitolTradesError, YahooFinanceError | OpenFecError enum | Add new error type |
| **chrono** | Date parsing for trades | Parse `contb_receipt_dt` | None - reuse |

**Zero infrastructure changes needed.** All required primitives exist in workspace dependencies.

### Crate Structure

**Recommended location:** `capitoltraders_lib/src/openfec/`

```
capitoltraders_lib/src/
  openfec/
    mod.rs          # Public module exports, re-export types
    client.rs       # OpenFecClient struct, API methods
    types.rs        # ScheduleAContribution, OpenFecResponse, Pagination, etc.
    error.rs        # OpenFecError enum
    query.rs        # Query builders (optional, defer until needed)
```

**Follows existing pattern:** `capitoltrades_api` has `client.rs` + `types/` structure. Mirror for consistency.

**Module exports (mod.rs):**
```rust
mod client;
mod error;
mod types;

pub use client::OpenFecClient;
pub use error::OpenFecError;
pub use types::{
    ScheduleAContribution,
    OpenFecResponse,
    Pagination,
    LastIndexes,
};
```

### Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OpenFecError {
    #[error("HTTP request failed")]
    Request(#[from] reqwest::Error),

    #[error("Rate limit exceeded (HTTP 429) - retry after cooldown")]
    RateLimitExceeded,

    #[error("Invalid API key (HTTP 401/403)")]
    InvalidApiKey,

    #[error("Resource not found (HTTP 404): {0}")]
    NotFound(String),

    #[error("JSON deserialization failed")]
    Deserialization(#[from] serde_json::Error),

    #[error("Invalid pagination state: {0}")]
    InvalidPagination(String),
}

pub type Result<T> = std::result::Result<T, OpenFecError>;
```

**Matches existing CapitolTradesError pattern:**
- Use `thiserror` for custom error enums
- Implement `From<reqwest::Error>` and `From<serde_json::Error>` via `#[from]`
- Provide descriptive error messages with context
- Use `anyhow::Result` at application layer, typed `Result<T>` at library layer

**Status code mapping:**
```rust
match response.status().as_u16() {
    200..=299 => Ok(response),
    401 | 403 => Err(OpenFecError::InvalidApiKey),
    404 => Err(OpenFecError::NotFound(path.to_string())),
    429 => Err(OpenFecError::RateLimitExceeded),
    _ => Err(OpenFecError::Request(/* ... */)),
}
```

## Installation Changes

### Cargo.toml Modifications

**capitoltraders_lib/Cargo.toml:**

```toml
[dependencies]
# Existing dependencies...
capitoltrades_api = { path = "../capitoltrades_api" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
reqwest = { workspace = true }
thiserror = { workspace = true }
# ... (other existing deps)

# NEW: Environment variable loading
dotenvy = "0.15"
```

**NO workspace-level changes needed** - reqwest, serde, tokio already in `[workspace.dependencies]`.

### CLI Binary Changes

**capitoltraders_cli/src/main.rs:**

```rust
use dotenvy::dotenv;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // NEW: Load .env at startup
    let _ = dotenv();

    // Existing tracing setup...
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Parse CLI args...
    let args = Cli::parse();

    // NEW: Initialize OpenFEC client if needed by command
    let fec_client = if requires_fec(&args.command) {
        let api_key = std::env::var("OPENFEC_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENFEC_API_KEY not set"))?;
        Some(OpenFecClient::new(api_key)?)
    } else {
        None
    };

    // Dispatch to command handlers...

    Ok(())
}

fn requires_fec(command: &Commands) -> bool {
    matches!(command, Commands::Donations(_) | Commands::Sync(_))
}
```

**Minimal invasiveness:**
- Single `dotenv()` call at startup
- Lazy initialization of FEC client (only if command needs it)
- No changes to existing command structure

## Alternatives Considered

### Environment Variable Loading

| Option | Version | Status | Decision |
|--------|---------|--------|----------|
| **dotenvy** | 0.15.7 | Active (2026-01) | **CHOSEN** - Maintained fork, addresses security advisory |
| dotenv | 0.15.0 | Unmaintained (last 2020-06) | REJECTED - RUSTSEC-2021-0141, no updates |
| config-rs | 0.14 | Active | REJECTED - Overkill for single API key |
| figment | 0.10 | Active | REJECTED - Over-engineered for simple .env |

**Decision rationale:** Dotenvy is the community-recommended dotenv replacement, minimal API surface, zero breaking changes from original dotenv.

### HTTP Client

| Option | Version | Status | Decision |
|--------|---------|--------|----------|
| **reqwest** | 0.12 | In workspace deps | **CHOSEN** - Already present, proven |
| reqwest | 0.13 | Latest (2025-12-30) | REJECTED - No benefit (already use rustls-tls) |
| ureq | 2.x | Active | REJECTED - Blocking I/O, incompatible with tokio |
| hyper | 1.x | Active | REJECTED - Lower-level, requires more boilerplate |

**Decision rationale:** reqwest 0.12 with `rustls-tls` feature already configured. Version 0.13 only changes default TLS (we override anyway). No upgrade needed.

### OpenFEC Client Library

| Option | Language | Status | Decision |
|--------|----------|--------|----------|
| **Custom wrapper** | Rust | N/A | **CHOSEN** - No Rust alternatives exist |
| tmc/openfec | Go | Active | REJECTED - Wrong language |
| R.openFEC | R | Active | REJECTED - Wrong language |
| pyopenfec | Python | Active | REJECTED - Wrong language |
| OpenAPI generator | Codegen | N/A | REJECTED - Over-engineering, API is simple |

**Decision rationale:** No Rust crates found on crates.io or GitHub (searched 2026-02-11). OpenFEC API is straightforward (GET + JSON), custom reqwest wrapper provides type safety and full control.

**Search evidence:**
- crates.io search for "openfec", "fec": Only forward error correction crates (ssdv-fec, feco3 = FEC file parser)
- GitHub search for "openFEC Rust": Only Go, Python, R implementations
- No docs.rs results for "openfec"

### Rate Limiting Implementation

| Option | Approach | Decision |
|--------|----------|----------|
| **Manual tracking** | Circuit breaker + 429 handling | **CHOSEN** - Simple, 1000/hour is generous |
| reqwest-middleware + reqwest-ratelimit | Middleware layer | REJECTED - Adds dependencies, requires trait impl |
| governor | Standalone rate limiter | REJECTED - Overkill for single API |
| leaky-bucket | Token bucket algorithm | REJECTED - Unnecessary complexity |

**Decision rationale:** OpenFEC's 1000 requests/hour limit is generous (16.67/min). Simple 429 detection + exponential backoff sufficient. Can enhance later if needed.

## Testing Strategy

### Deserialization Unit Tests

**Pattern:** JSON fixtures with `serde_json::from_str`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SCHEDULE_A_FIXTURE: &str = include_str!("../tests/fixtures/schedule_a_response.json");

    #[test]
    fn deserialize_schedule_a_response() {
        let response: OpenFecResponse<ScheduleAContribution> =
            serde_json::from_str(SCHEDULE_A_FIXTURE).unwrap();

        assert_eq!(response.pagination.count, 67000000);
        assert!(response.pagination.last_indexes.is_some());
        assert!(!response.results.is_empty());

        let first = &response.results[0];
        assert!(first.cmte_id.starts_with('C'));
        assert!(first.contb_receipt_amt.is_some());
    }

    #[test]
    fn deserialize_standard_pagination() {
        let json = r#"{
            "pagination": {
                "count": 100,
                "page": 1,
                "pages": 10,
                "per_page": 10
            },
            "results": []
        }"#;

        let response: OpenFecResponse<ScheduleAContribution> =
            serde_json::from_str(json).unwrap();

        assert_eq!(response.pagination.page, Some(1));
        assert_eq!(response.pagination.pages, Some(10));
        assert!(response.pagination.last_indexes.is_none());
    }

    #[test]
    fn deserialize_keyset_pagination() {
        let json = r#"{
            "pagination": {
                "count": 67000000,
                "last_indexes": {
                    "last_index": 230880619,
                    "last_contribution_receipt_date": "2014-01-01"
                }
            },
            "results": []
        }"#;

        let response: OpenFecResponse<ScheduleAContribution> =
            serde_json::from_str(json).unwrap();

        assert!(response.pagination.last_indexes.is_some());
        let indexes = response.pagination.last_indexes.unwrap();
        assert_eq!(indexes.last_index, 230880619);
        assert_eq!(indexes.last_contribution_receipt_date, "2014-01-01");
    }
}
```

**Fixture location:** `capitoltraders_lib/tests/fixtures/schedule_a_response.json`

**Matches existing pattern:** `capitoltrades_api` uses `include_str!` for HTML fixtures in tests.

### Integration Tests (wiremock)

**Pattern:** Mock HTTP server for API testing

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

    #[tokio::test]
    async fn get_schedule_a_contributions_success() {
        let mock_server = MockServer::start().await;

        let fixture = r#"{
            "pagination": {"count": 1},
            "results": [{
                "cmte_id": "C00401224",
                "contbr_nm": "JONES, JOHN",
                "contb_receipt_amt": 500.0
            }]
        }"#;

        Mock::given(matchers::method("GET"))
            .and(matchers::path("/v1/schedules/schedule_a/"))
            .and(matchers::query_param("api_key", "test_key"))
            .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
            .mount(&mock_server)
            .await;

        let client = OpenFecClient::with_base_url(
            "test_key".to_string(),
            mock_server.uri(),
        ).unwrap();

        let result = client.get_schedule_a_page(None).await.unwrap();

        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].cmte_id, "C00401224");
    }

    #[tokio::test]
    async fn rate_limit_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(matchers::method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&mock_server)
            .await;

        let client = OpenFecClient::with_base_url(
            "test_key".to_string(),
            mock_server.uri(),
        ).unwrap();

        let result = client.get_schedule_a_page(None).await;

        assert!(matches!(result, Err(OpenFecError::RateLimitExceeded)));
    }

    #[tokio::test]
    async fn invalid_api_key_returns_error() {
        let mock_server = MockServer::start().await;

        Mock::given(matchers::method("GET"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let client = OpenFecClient::with_base_url(
            "bad_key".to_string(),
            mock_server.uri(),
        ).unwrap();

        let result = client.get_schedule_a_page(None).await;

        assert!(matches!(result, Err(OpenFecError::InvalidApiKey)));
    }
}
```

**Wiremock version:** Already in dev-dependencies at `wiremock = "0.6"`. No new dependency.

**Matches existing pattern:** 8 wiremock integration tests in `capitoltrades_api`, identical pattern.

### Client Constructor Testing

```rust
#[test]
fn client_rejects_empty_api_key() {
    let result = OpenFecClient::new("".to_string());
    assert!(result.is_err());
}

#[test]
fn client_warns_on_demo_key() {
    // This would require capturing tracing output - defer to manual testing
    let client = OpenFecClient::new("DEMO_KEY".to_string());
    assert!(client.is_ok());
}
```

## Security Considerations

### API Key Protection

**DO:**
- Store in `.env` file (gitignored)
- Load via `dotenvy::dotenv()` at runtime
- Pass as constructor parameter (dependency injection)
- Validate key presence early with clear error
- Use HTTPS for all API calls (enforced by OpenFEC)

**DON'T:**
- Hardcode in source files
- Commit `.env` to git repository
- Log API key in error messages or debug output
- Expose in CLI `--help` output or examples
- Use global/static variables for storage

### .gitignore Verification

**Check existing .gitignore for .env pattern:**

```bash
# Check if .env is already ignored
grep -q "^\.env$" .gitignore || echo ".env" >> .gitignore
```

**Recommended .gitignore entries:**

```
# Environment variables
.env
.env.local
.env.*.local

# But allow .env.example for documentation
!.env.example
```

### API Key Validation

```rust
impl OpenFecClient {
    pub fn new(api_key: String) -> anyhow::Result<Self> {
        // Empty key check
        if api_key.is_empty() {
            anyhow::bail!("OpenFEC API key cannot be empty");
        }

        // DEMO_KEY warning
        if api_key == "DEMO_KEY" {
            tracing::warn!(
                "Using DEMO_KEY for OpenFEC API - expect aggressive rate limits. \
                 Register for free API key at api.data.gov"
            );
        }

        Ok(Self {
            api_key,
            client: reqwest::Client::new(),
            base_url: "https://api.open.fec.gov/v1".to_string(),
        })
    }

    // Test-only constructor with base URL override
    #[cfg(test)]
    pub fn with_base_url(api_key: String, base_url: String) -> anyhow::Result<Self> {
        if api_key.is_empty() {
            anyhow::bail!("OpenFEC API key cannot be empty");
        }

        Ok(Self {
            api_key,
            client: reqwest::Client::new(),
            base_url,
        })
    }
}
```

**Early validation prevents:**
- Runtime failures deep in execution
- Unclear error messages from OpenFEC API
- Wasted API calls with invalid credentials

## Migration Path

### Phase 1: Foundation (Immediate)

**Goal:** Establish environment variable infrastructure

1. Add `dotenvy = "0.15"` to `capitoltraders_lib/Cargo.toml`
2. Add `.env` to `.gitignore` (verify not already present)
3. Create `.env.example` with template:
   ```bash
   # OpenFEC API Key (get from api.data.gov)
   OPENFEC_API_KEY=your_api_key_here
   ```
4. Load `dotenvy::dotenv()` in `capitoltraders_cli/src/main.rs`
5. Document API key setup in README

**Success criteria:**
- [ ] dotenvy dependency added
- [ ] .env gitignored
- [ ] .env.example committed
- [ ] dotenv() called at CLI startup
- [ ] README documents key registration

### Phase 2: Client Implementation (Core)

**Goal:** Build OpenFEC API client

1. Create `capitoltraders_lib/src/openfec/` module structure
2. Implement `types.rs`: ScheduleAContribution, OpenFecResponse, Pagination, LastIndexes
3. Implement `error.rs`: OpenFecError enum with thiserror
4. Implement `client.rs`: OpenFecClient with:
   - Constructor with API key validation
   - Test constructor with base URL override
   - `get_schedule_a_page()` method
   - Status code to error mapping
5. Write deserialization unit tests with JSON fixtures
6. Write wiremock integration tests (success, 429, 403 cases)

**Success criteria:**
- [ ] OpenFecClient struct implemented
- [ ] ScheduleAContribution type with serde derives
- [ ] OpenFecResponse<T> wrapper with dual pagination support
- [ ] OpenFecError enum with From impls
- [ ] 3+ deserialization unit tests
- [ ] 3+ wiremock integration tests
- [ ] All tests passing with `cargo test -p capitoltraders_lib`

### Phase 3: Integration (Feature)

**Goal:** Wire OpenFEC into CLI

1. Implement `/candidates/` search endpoint
2. Implement `/candidate/{id}/committees/` endpoint
3. Add committee lookup caching (DashMap, 300s TTL)
4. Add new CLI subcommand `donations` (or flag on existing `trades`)
5. Implement output formatting for donation records (table, JSON, CSV)
6. Add integration tests for CLI command
7. Document usage in CLAUDE.md

**Success criteria:**
- [ ] Candidate search working
- [ ] Committee lookup working
- [ ] Schedule A fetch by committee ID working
- [ ] DashMap cache for committee data
- [ ] CLI command wired and tested
- [ ] Output formats implemented
- [ ] Documentation updated

### Phase 4: Enhancement (Optional)

**Goal:** Production hardening

1. Implement rate limit tracking with X-RateLimit headers
2. Add circuit breaker for consecutive 429 responses
3. Implement exponential backoff on rate limit
4. Add metrics/logging for API usage
5. Optimize cache TTL based on usage patterns
6. Implement keyset pagination iteration helper

**Success criteria:**
- [ ] Rate limit header inspection
- [ ] Circuit breaker on 429
- [ ] Backoff strategy implemented
- [ ] Comprehensive logging
- [ ] Cache tuning complete

## Success Criteria

**OpenFEC integration complete when:**

**Foundation:**
- [ ] dotenvy 0.15 added to dependencies
- [ ] .env file loading at CLI startup with dotenv()
- [ ] .env and .env.*.local in .gitignore
- [ ] .env.example committed with template
- [ ] API key validation in constructor

**Client:**
- [ ] OpenFecClient struct with reqwest + API key
- [ ] ScheduleAContribution type with serde Deserialize
- [ ] OpenFecResponse<T> wrapper for pagination
- [ ] Dual pagination support (standard + keyset)
- [ ] OpenFecError enum with thiserror

**Testing:**
- [ ] 3+ deserialization unit tests with JSON fixtures
- [ ] 3+ wiremock integration tests (success, 429, 403)
- [ ] All tests passing with `cargo test --workspace`

**Integration:**
- [ ] Committee lookup endpoint implemented
- [ ] DashMap cache for committee data
- [ ] CLI command for donations (or flag)
- [ ] Output formatting (table, JSON, CSV)

**Documentation:**
- [ ] README documents API key setup
- [ ] CLAUDE.md documents OpenFEC module patterns
- [ ] Code comments explain pagination models

**Quality bar:**
- Type-safe (no unwrap in prod code, Option for nullable fields)
- Testable (base URL injection for wiremock)
- Follows existing patterns (thiserror errors, DashMap cache, tokio async)
- Minimal dependencies (only dotenvy added)
- Secure (API key in .env, gitignored, validated early)

## Dependencies NOT Needed

**Avoid these unnecessary additions:**

- **reqwest-middleware / reqwest-ratelimit** - Adds middleware layer, requires custom trait impl, overkill for 1000/hour limit
- **governor / leaky-bucket** - Standalone rate limiters, unnecessary for simple hourly cap
- **config-rs / figment** - Over-engineered for single API key config
- **envy** - Defer until config grows beyond API key (currently just one string)
- **serde_with** - Standard serde sufficient for OpenFEC schema
- **cached crate** - Existing DashMap pattern works, avoid new cache dependency
- **OpenAPI generator** - API simple enough for hand-written client, codegen adds complexity
- **New async runtime** - tokio 1.x already workspace-wide
- **New JSON library** - serde_json 1.x handles OpenFEC responses

**Rationale:** Existing stack covers all requirements. Only dotenvy needed for .env loading.

## Upgrade Considerations (Future)

### rusqlite 0.31 → 0.38

**Current:** `rusqlite = { version = "0.31", features = ["bundled"] }`
**Latest:** 0.38.0 (released 2024-12-20, bundled SQLite 3.51.1)

**Breaking changes in 0.38:**
1. Disabled u64/usize ToSql/FromSql by default (enable with feature flag)
2. Statement cache now optional (not enabled by default)
3. Minimum SQLite version bumped to 3.34.1 (bundled feature unaffected)
4. Stricter ownership checks when registering closures as hooks

**Impact on codebase:** Likely minimal. Using bundled feature, no u64 storage, statement cache usage minimal.

**Recommendation:** Defer upgrade until Phase 5+ (not required for OpenFEC integration). Test thoroughly if upgrading.

### reqwest 0.12 → 0.13

**Current:** `reqwest = { version = "0.12", default-features = false, features = ["gzip", "rustls-tls"] }`
**Latest:** 0.13 (released 2025-12-30)

**Key change:** Default TLS changed from native-tls to rustls (with aws-lc provider)

**Impact on codebase:** NONE. Already using `rustls-tls` feature explicitly, default TLS irrelevant.

**Recommendation:** No upgrade needed for OpenFEC integration. Version 0.12 fully sufficient.

## Confidence Assessment

| Research Area | Level | Rationale |
|--------------|-------|-----------|
| **OpenFEC API specification** | HIGH | Official docs verified, pagination models documented, authentication confirmed query param |
| **Rate limiting** | HIGH | API Umbrella headers documented (X-RateLimit-*), 1000/hour confirmed, 429 status code verified |
| **Pagination mechanisms** | HIGH | Keyset vs standard pagination explained in docs, last_indexes pattern confirmed |
| **Schedule A schema** | MEDIUM | Field names verified via wiki, types inferred from examples, some fields may be undocumented |
| **dotenvy maintenance** | HIGH | Version 0.15.7 released 2026-01, active GitHub repo, recommended dotenv replacement |
| **Rust ecosystem compatibility** | HIGH | reqwest 0.12 + tokio 1.x verified compatible, serde 1.x standard, existing deps sufficient |
| **No Rust FEC client** | HIGH | Searched crates.io, GitHub, docs.rs on 2026-02-11, zero Rust implementations found |
| **Integration strategy** | MEDIUM | Committee-based correlation logical but not verified with real data, may need iteration |

**Overall confidence: HIGH** - All technical specifications verified via official sources. Only integration strategy (committee correlation) requires validation with real data.

## Sources

### OpenFEC Official Documentation

- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) - Main API reference
- [OpenFEC GitHub Repository](https://github.com/fecgov/openFEC) - Source code, issues, architecture
- [Schedule A Column Documentation](https://github.com/fecgov/openFEC/wiki/Schedule-A-column-documentation) - Field definitions
- [OpenFEC Postman Collection](https://www.postman.com/api-evangelist/federal-election-commission-fec/documentation/19lr6vr/openfec) - API examples

### Rate Limiting & Pagination

- [API Umbrella Rate Limits Documentation](https://api-umbrella.readthedocs.io/en/latest/api-consumer/rate-limits.html) - X-RateLimit headers, rolling window
- [18F: OpenFEC API Update - 67 Million Records](https://18f.gsa.gov/2015/07/15/openfec-api-update/) - Keyset pagination rationale
- [Sunlight Foundation: OpenFEC Getting Started](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/) - API key registration, basic usage
- [OpenFEC Pagination Issue #3396](https://github.com/fecgov/openFEC/issues/3396) - Schedule_e pagination discussion (similar to Schedule A)

### Rust Dependencies

- [dotenvy 0.15.7 Documentation](https://docs.rs/dotenvy/latest/dotenvy/) - API reference
- [dotenvy GitHub Repository](https://github.com/allan2/dotenvy) - Maintained fork status, changelog
- [dotenvy Crates.io](https://crates.io/crates/dotenvy) - Version history, downloads
- [Serde Struct Flattening](https://serde.rs/attr-flatten.html) - Pagination wrapper pattern
- [reqwest ClientBuilder Documentation](https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html) - HTTP client configuration
- [reqwest-middleware Documentation](https://docs.rs/reqwest-middleware/latest/reqwest_middleware/) - Middleware layer (considered, rejected)
- [reqwest-ratelimit Documentation](https://docs.rs/reqwest-ratelimit) - Rate limiting middleware (considered, rejected)

### Rust Patterns

- [Tokio Semaphore Documentation](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html) - Concurrency control
- [Tokio Shared State Tutorial](https://tokio.rs/tokio/tutorial/shared-state) - DashMap usage patterns
- [thiserror Documentation](https://docs.rs/thiserror) - Error enum derives

### Ecosystem Research

- [crates.io Search: "fec"](https://crates.io/search?q=fec) - No OpenFEC clients found (only FEC = forward error correction)
- [GitHub Search: openFEC Rust](https://github.com/search?q=openfec+rust) - NickCrews/feco3 (FEC file parser, not API client)
- [rusqlite Releases](https://github.com/rusqlite/rusqlite/releases) - Version history, 0.38 breaking changes
- [rusqlite Changelog](https://github.com/rusqlite/rusqlite/blob/master/Changelog.md) - Detailed upgrade notes

**Research date:** 2026-02-11
**Tools used:** WebSearch, WebFetch, crates.io, GitHub, official docs
**Verification status:** All claims sourced from official documentation or actively maintained projects
