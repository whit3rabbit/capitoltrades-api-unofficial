# Phase 8: OpenFEC API Client - Research

**Researched:** 2026-02-12
**Domain:** OpenFEC API client implementation in Rust
**Confidence:** MEDIUM

## Summary

The OpenFEC API is a RESTful API for Federal Election Commission data, providing endpoints for candidate search, committee lookup, and Schedule A contributions. The API uses standard HTTP authentication via query parameter, has a confirmed 1,000 calls/hour rate limit, and implements keyset pagination (not page numbers) for large datasets like Schedule A. The project already has strong patterns for HTTP clients (YahooClient), rate limiting (Semaphore), circuit breakers, caching (DashMap), and wiremock testing that can be directly replicated.

**Primary recommendation:** Build openfec module mirroring YahooClient structure (client.rs with reqwest + DashMap cache, types.rs with serde models, error.rs with thiserror), use Semaphore for concurrency limiting (3 concurrent), implement simple consecutive-failure circuit breaker matching enrich_prices pattern, and write comprehensive wiremock tests following capitoltrades_api/tests/client_integration.rs patterns.

**Critical findings:**
- Rate limit is 1,000 calls/hour with API key (confirmed via multiple sources), not 100 as older docs suggested
- Schedule A uses keyset pagination with last_index + last_contribution_receipt_date cursor, NOT page numbers
- API key passed as query parameter ?api_key=... (not header)
- All endpoints are v1: https://api.open.fec.gov/v1/...
- X-RateLimit headers were historically inconsistent (removed X-RateLimit-Remaining), so circuit breaker should rely on 429 status codes

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| reqwest | 0.12+ | HTTP client | Already in project, async, connection pooling, widely used for government APIs |
| serde | 1.0 | JSON deserialization | Standard Rust JSON handling, already in project |
| thiserror | 1.0 | Error types | Project convention for library errors (see YahooError) |
| tokio | 1.0 | Async runtime | Project runtime, provides Semaphore for rate limiting |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| dashmap | 6.0 | Concurrent cache | Already used in YahooClient for caching, prevents duplicate API calls |
| wiremock | 0.6 | HTTP mocking | Project standard for integration tests (capitoltrades_api) |
| anyhow | 1.0 | Application errors | CLI layer error handling (see enrich_prices.rs) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| DashMap cache | http-cache-reqwest | More overhead, HTTP-compliant caching; DashMap simpler for in-memory cache |
| governor crate | Manual Semaphore | Governor provides token bucket; Semaphore simpler for fixed concurrency = 3 |
| Custom circuit breaker | resilience4j-like crate | No mature Rust equivalent; project uses simple consecutive-failure pattern successfully |

**Installation:**
```bash
# All dependencies already in Cargo.toml except potentially url crate for query building
# Check if url crate needed for complex query parameter handling
```

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/openfec/
├── mod.rs           # Module exports
├── client.rs        # OpenFecClient with reqwest + DashMap
├── types.rs         # Candidate, Committee, ScheduleA, Pagination
└── error.rs         # OpenFecError enum
```

### Pattern 1: Client Structure (Mirror YahooClient)
**What:** Wrapper around reqwest::Client with DashMap cache, builder pattern for queries
**When to use:** All OpenFEC API interactions
**Example:**
```rust
// Source: Project pattern from capitoltraders_lib/src/yahoo.rs
pub struct OpenFecClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    cache: Arc<DashMap<String, CachedResponse>>, // Cache by query URL
}

impl OpenFecClient {
    pub fn new(api_key: String) -> Result<Self, OpenFecError> {
        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: "https://api.open.fec.gov/v1".to_string(),
            cache: Arc::new(DashMap::new()),
        })
    }

    // Cache wrapper pattern
    async fn get_with_cache(&self, url: &str) -> Result<Response, OpenFecError> {
        if let Some(cached) = self.cache.get(url) {
            return Ok(cached.clone());
        }
        // Fetch and cache
    }
}
```

### Pattern 2: Query Parameter Building
**What:** Builder pattern for constructing API calls with optional filters
**When to use:** Candidate search, committee lookup (Schedule A uses keyset pagination)
**Example:**
```rust
// Source: Project pattern from capitoltrades_api query builders
pub struct CandidateSearchQuery {
    pub name: Option<String>,
    pub office: Option<String>, // H, S, P
    pub state: Option<String>,
    pub party: Option<String>,
    pub cycle: Option<i32>,
    pub page: Option<i32>,
    pub per_page: Option<i32>,
}

impl CandidateSearchQuery {
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn to_query_params(&self, api_key: &str) -> Vec<(&str, String)> {
        let mut params = vec![("api_key", api_key.to_string())];
        if let Some(ref name) = self.name {
            params.push(("name", name.clone()));
        }
        // ... other params
        params
    }
}
```

### Pattern 3: Keyset Pagination for Schedule A
**What:** Cursor-based pagination using last_index + last_contribution_receipt_date, not page numbers
**When to use:** Fetching Schedule A contributions (large datasets)
**Example:**
```rust
// Source: OpenFEC API documentation pattern
pub struct ScheduleAPagination {
    pub last_index: Option<i64>,
    pub last_contribution_receipt_date: Option<String>, // ISO date
}

impl ScheduleAQuery {
    pub fn with_cursor(mut self, cursor: ScheduleAPagination) -> Self {
        self.last_index = cursor.last_index;
        self.last_contribution_receipt_date = cursor.last_contribution_receipt_date;
        self
    }
}

// Response includes pagination object with next cursor values
pub struct ScheduleAResponse {
    pub results: Vec<Contribution>,
    pub pagination: PaginationInfo,
}

pub struct PaginationInfo {
    pub last_indexes: Option<LastIndexes>, // Contains next cursor values
    pub count: i32,
    pub per_page: i32,
}

pub struct LastIndexes {
    pub last_index: i64,
    pub last_contribution_receipt_date: String,
}
```

### Pattern 4: Rate Limiting with Semaphore
**What:** Concurrency = 3 with Semaphore to respect 1,000 calls/hour limit
**When to use:** Batch operations fetching multiple candidates/committees
**Example:**
```rust
// Source: Project pattern from capitoltraders_cli/src/commands/enrich_prices.rs
let semaphore = Arc::new(Semaphore::new(3)); // Concurrency = 3
let (tx, mut rx) = mpsc::channel(6); // Buffer = concurrency * 2
let mut join_set = JoinSet::new();

for candidate_id in candidate_ids {
    let permit = semaphore.clone().acquire_owned().await?;
    let client = Arc::clone(&openfec_client);
    let tx = tx.clone();

    join_set.spawn(async move {
        let result = client.get_candidate_committees(&candidate_id).await;
        let _ = tx.send((candidate_id, result)).await;
        drop(permit);
    });
}
drop(tx);

// Single-threaded DB writes from receiver
while let Some((id, result)) = rx.recv().await {
    match result {
        Ok(committees) => { /* process */ },
        Err(e) => circuit_breaker.record_failure(),
    }
}
```

### Pattern 5: Circuit Breaker for 429 Errors
**What:** Simple consecutive-failure counter, trip after 5 consecutive 429s
**When to use:** Batch operations to prevent cascade failures
**Example:**
```rust
// Source: Project pattern from capitoltraders_cli/src/commands/enrich_prices.rs
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize, // 5 for OpenFEC
}

impl CircuitBreaker {
    fn new(threshold: usize) -> Self {
        Self { consecutive_failures: 0, threshold }
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
    }

    fn is_tripped(&self) -> bool {
        self.consecutive_failures >= self.threshold
    }
}

// Usage in fetch loop
match result {
    Ok(_) => circuit_breaker.record_success(),
    Err(OpenFecError::RateLimited) => {
        circuit_breaker.record_failure();
        if circuit_breaker.is_tripped() {
            bail!("Circuit breaker tripped after {} consecutive 429 errors", threshold);
        }
    }
    Err(e) => { /* other errors */ }
}
```

### Anti-Patterns to Avoid
- **Using page numbers for Schedule A:** Schedule A does NOT support page parameter, only keyset cursors (last_index + last_contribution_receipt_date)
- **API key in Authorization header:** OpenFEC uses query parameter ?api_key=..., not header-based auth
- **Relying on X-RateLimit-Remaining header:** Header was removed from API responses, use 429 status code detection
- **Cloning reqwest::Client:** Client has Arc internally, cheap to clone, but wrap in Arc<OpenFecClient> for sharing cached state
- **Overwriting cached data:** Cache TTL should consider data staleness; candidate info is relatively static, contributions update frequently

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HTTP caching | Custom cache invalidation logic | DashMap with simple TTL or no TTL for static data | OpenFEC data rarely changes mid-session; simple cache sufficient |
| Rate limiting | Custom token bucket algorithm | tokio::sync::Semaphore with fixed concurrency | 1,000 calls/hour = ~16/min = 3 concurrent requests well within limit |
| Retry logic | Exponential backoff for 429 | Circuit breaker after 5 consecutive failures | OpenFEC rate limits are hourly windows; backoff won't help, failing fast better |
| Query parameter encoding | Manual URL building | reqwest::Url or serde_urlencoded | Edge cases (special chars, arrays) already handled |
| Async HTTP client | Custom reqwest wrapper | Direct reqwest::Client with connection pooling | Reqwest already handles keep-alive, TLS, connection reuse |

**Key insight:** OpenFEC API is simple and well-behaved. Over-engineering with sophisticated retry/backoff/caching logic adds complexity without benefit. Project's existing simple patterns (YahooClient, CircuitBreaker) are exactly right level of sophistication.

## Common Pitfalls

### Pitfall 1: Schedule A Pagination Confusion
**What goes wrong:** Attempting to use page=2 or page=3 for Schedule A, getting inconsistent/missing results
**Why it happens:** Schedule A documentation mentions "pagination" but uses keyset cursors, not page numbers
**How to avoid:**
- NEVER pass page parameter to /schedules/schedule_a
- ALWAYS use last_index + last_contribution_receipt_date from previous response's pagination.last_indexes
- Extract cursor from response: response.pagination.last_indexes.{last_index, last_contribution_receipt_date}
- Pass both cursor values together (paired)
**Warning signs:**
- Duplicate records across pages
- Missing records between pages
- API returns same results for different page numbers

### Pitfall 2: Rate Limit Header Dependency
**What goes wrong:** Code breaks when X-RateLimit-Remaining header is missing or returns -1
**Why it happens:** OpenFEC historically removed/changed rate limit headers (GitHub issue #25 in pyopenfec)
**How to avoid:**
- Detect rate limits via 429 status code, not headers
- Do NOT parse X-RateLimit-* headers for logic decisions
- Circuit breaker should count consecutive 429s, not predict via headers
- Consider 429 as transient error requiring backoff, not immediate retry
**Warning signs:**
- KeyError or None when accessing rate limit headers
- Code assumes X-RateLimit-Remaining exists and is accurate

### Pitfall 3: API Key Exposure
**What goes wrong:** API key logged in URLs, committed to git, exposed in error messages
**Why it happens:** Query parameter ?api_key=... appears in URLs, reqwest debug output, logs
**How to avoid:**
- Load from .env via dotenvy (project already does this in v1.2)
- Never log full URLs with query parameters
- reqwest error messages include URLs - sanitize in Display/Debug implementations
- Use require_openfec_api_key() helper from Phase 7 patterns
**Warning signs:**
- API key visible in terminal output
- reqwest::Error debug output contains ?api_key=...

### Pitfall 4: Candidate ID Format Assumptions
**What goes wrong:** Parsing candidate_id as structured data, failing on unexpected formats
**Why it happens:** Candidate ID format (H0IL01087) looks parseable but is opaque identifier
**How to avoid:**
- Treat candidate_id as opaque String, not structured data
- Do NOT parse office/state/district from ID - use response fields instead
- Validate format if needed (9 chars starting with H/S/P), but don't extract meaning
- API responses include office, state, district as separate fields
**Warning signs:**
- Regex parsing candidate_id
- Assumptions about ID structure (e.g., "characters 3-4 are state")

### Pitfall 5: Committee Endpoint Confusion
**What goes wrong:** Using /committees?candidate_id=... instead of /candidate/{id}/committees
**Why it happens:** Both endpoints exist, different purposes
**How to avoid:**
- /candidate/{id}/committees - committees authorized by specific candidate (use this for Phase 8)
- /committees - search all committees with filters (broader, different use case)
- REQ-v1.2-003 specifies "committees for a given candidate ID" = /candidate/{id}/committees
**Warning signs:**
- Unexpected number of committees returned
- Committees not linked to expected candidate

## Code Examples

Verified patterns from project and official sources:

### Error Enum Structure
```rust
// Source: Project pattern capitoltraders_lib/src/yahoo.rs
#[derive(Error, Debug)]
pub enum OpenFecError {
    #[error("Rate limited by OpenFEC API (HTTP 429)")]
    RateLimited,
    #[error("Invalid API key (HTTP 403)")]
    InvalidApiKey,
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Failed to parse response: {0}")]
    ParseFailed(String),
    #[error("Network error")]
    Network(#[from] reqwest::Error),
}
```

### Deserialization Types
```rust
// Source: OpenFEC API documentation patterns
#[derive(Debug, Deserialize, Serialize)]
pub struct CandidateSearchResponse {
    pub results: Vec<Candidate>,
    pub pagination: StandardPagination,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Candidate {
    pub candidate_id: String,
    pub name: String,
    pub party: Option<String>,
    pub office: Option<String>, // H, S, P
    pub state: Option<String>,
    pub district: Option<String>,
    pub cycles: Vec<i32>,
    pub candidate_status: Option<String>,
    pub incumbent_challenge: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StandardPagination {
    pub count: i32,
    pub page: Option<i32>,
    pub pages: Option<i32>,
    pub per_page: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScheduleAResponse {
    pub results: Vec<Contribution>,
    pub pagination: ScheduleAPagination,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScheduleAPagination {
    pub count: i32,
    pub per_page: i32,
    pub last_indexes: Option<LastIndexes>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LastIndexes {
    pub last_index: i64,
    pub last_contribution_receipt_date: String, // YYYY-MM-DD
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Contribution {
    pub sub_id: String, // Unique ID
    pub committee: Option<CommitteeInfo>,
    pub contributor_name: Option<String>,
    pub contributor_state: Option<String>,
    pub contribution_receipt_date: Option<String>,
    pub contribution_receipt_amount: Option<f64>,
    // ... many other fields, include based on Phase 9 needs
}
```

### Wiremock Test Pattern (Rate Limit)
```rust
// Source: Project pattern capitoltrades_api/tests/client_integration.rs
#[tokio::test]
async fn candidate_search_rate_limited() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search"))
        .and(query_param("api_key", "test-key"))
        .respond_with(ResponseTemplate::new(429)
            .set_body_json(json!({
                "message": "API rate limit exceeded"
            })))
        .mount(&mock_server)
        .await;

    let client = OpenFecClient::with_base_url(&mock_server.uri(), "test-key".to_string()).unwrap();
    let result = client.search_candidates(&CandidateSearchQuery::default()).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::RateLimited));
}
```

### Wiremock Test Pattern (Invalid API Key)
```rust
#[tokio::test]
async fn candidate_search_invalid_key() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search"))
        .and(query_param("api_key", "bad-key"))
        .respond_with(ResponseTemplate::new(403)
            .set_body_json(json!({
                "error": "Invalid API key"
            })))
        .mount(&mock_server)
        .await;

    let client = OpenFecClient::with_base_url(&mock_server.uri(), "bad-key".to_string()).unwrap();
    let result = client.search_candidates(&CandidateSearchQuery::default()).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::InvalidApiKey));
}
```

### Wiremock Test Pattern (Keyset Pagination)
```rust
#[tokio::test]
async fn schedule_a_pagination_uses_keyset_cursor() {
    let mock_server = MockServer::start().await;

    // First page - no cursor
    Mock::given(method("GET"))
        .and(path("/v1/schedules/schedule_a"))
        .and(query_param("api_key", "test-key"))
        .and(query_param_exists("last_index").not())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"sub_id": "1"}, {"sub_id": "2"}],
            "pagination": {
                "count": 100,
                "per_page": 2,
                "last_indexes": {
                    "last_index": 230880619,
                    "last_contribution_receipt_date": "2024-01-15"
                }
            }
        })))
        .mount(&mock_server)
        .await;

    // Second page - with cursor
    Mock::given(method("GET"))
        .and(path("/v1/schedules/schedule_a"))
        .and(query_param("api_key", "test-key"))
        .and(query_param("last_index", "230880619"))
        .and(query_param("last_contribution_receipt_date", "2024-01-15"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{"sub_id": "3"}, {"sub_id": "4"}],
            "pagination": {
                "count": 100,
                "per_page": 2,
                "last_indexes": null // No more pages
            }
        })))
        .mount(&mock_server)
        .await;

    let client = OpenFecClient::with_base_url(&mock_server.uri(), "test-key".to_string()).unwrap();

    // Fetch first page
    let page1 = client.get_schedule_a(&ScheduleAQuery::default()).await.unwrap();
    assert_eq!(page1.results.len(), 2);
    assert!(page1.pagination.last_indexes.is_some());

    // Use cursor for second page
    let cursor = page1.pagination.last_indexes.unwrap();
    let query = ScheduleAQuery::default()
        .with_last_index(cursor.last_index)
        .with_last_contribution_receipt_date(&cursor.last_contribution_receipt_date);
    let page2 = client.get_schedule_a(&query).await.unwrap();
    assert_eq!(page2.results.len(), 2);
    assert!(page2.pagination.last_indexes.is_none()); // No more pages
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| X-RateLimit-Remaining header | 429 status code detection | ~2016-2018 | Client code must handle missing header gracefully |
| 100 calls/hour default | 1,000 calls/hour with API key | Unknown, current as of 2026 | Higher throughput, but still need rate limiting |
| Page-based pagination for all | Keyset pagination for Schedule A/B | Introduced with Schedule A/B endpoints | More efficient for large datasets, different query pattern |

**Deprecated/outdated:**
- X-RateLimit-Remaining header: Removed from API, don't rely on it (pyopenfec issue #25)
- 100 calls/hour limit: Old documentation, current is 1,000 calls/hour with API key
- DEMO_KEY for production: Only for testing web interface, need real API key from data.gov

## Open Questions

1. **What is the exact per_page default and maximum for Schedule A?**
   - What we know: Other endpoints default to some value, max is 100 per page
   - What's unclear: Schedule A documentation doesn't specify default/max
   - Recommendation: Start with per_page=100 (likely max), verify empirically during Phase 8 implementation

2. **Do 429 responses include Retry-After header?**
   - What we know: 429 status code is returned for rate limit violations
   - What's unclear: Whether Retry-After header provides backoff guidance
   - Recommendation: Inspect response headers during first 429 in dev, adjust circuit breaker if header present

3. **Is candidate search case-sensitive for name parameter?**
   - What we know: Endpoint accepts name parameter for candidate search
   - What's unclear: Whether "pelosi" matches "Pelosi", normalization behavior
   - Recommendation: Test with both cases in integration tests, document behavior

4. **What is the response structure for /candidate/{id}/committees endpoint?**
   - What we know: Endpoint exists and returns committees authorized by candidate
   - What's unclear: Exact JSON structure (array vs object wrapper, pagination, committee fields)
   - Recommendation: Make test API call with valid candidate_id during Phase 8, capture response for fixture

5. **Are there undocumented rate limit tiers based on usage patterns?**
   - What we know: Standard is 1,000 calls/hour, can request 7,200 calls/hour via email
   - What's unclear: Whether API implements soft limits or burst allowances
   - Recommendation: Monitor for 429s with concurrency=3, log when they occur to identify patterns

## Sources

### Primary (HIGH confidence)
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) - Official API reference
- [OpenFEC GitHub Repository](https://github.com/fecgov/openFEC) - Source code and issues
- [FEC Candidate Master File Description](https://www.fec.gov/campaign-finance-data/candidate-master-file-description/) - Candidate ID format specification
- [Sunlight Foundation: OpenFEC Guide](https://sunlightfoundation.com/2015/07/08/openfec-makes-campaign-finance-data-more-accessible-with-new-api-heres-how-to-get-started/) - Initial API overview
- [18F: 67 Million Records](https://18f.gsa.gov/2015/07/15/openfec-api-update/) - Schedule A pagination details

### Secondary (MEDIUM confidence)
- [PyOpenFEC GitHub](https://github.com/jeremyjbowers/pyopenfec) - Python client reference implementation
- [OpenFEC Postman Collection](https://www.postman.com/api-evangelist/federal-election-commission-fec/documentation/19lr6vr/openfec) - Endpoint examples
- [Microsoft OpenFEC Connector](https://learn.microsoft.com/en-us/connectors/openfec/) - Endpoint catalog
- [Go OpenFEC Client](https://pkg.go.dev/github.com/tmc/openfec) - Type definitions reference
- [Wiremock Rust Documentation](https://docs.rs/wiremock/) - Testing patterns
- [Governor Crate Documentation](https://docs.rs/governor) - Rate limiting library (not using, but researched)

### Tertiary (LOW confidence - needs validation)
- [Implementing API Rate Limiting in Rust](https://www.shuttle.dev/blog/2024/02/22/api-rate-limiting-rust) - General patterns, not OpenFEC-specific
- [Rust Circuit Breaker Implementation](https://github.com/mahmudsudo/circuit_breaker) - Example implementation, not production-ready

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - Project already uses all dependencies successfully (reqwest, serde, thiserror, tokio, dashmap, wiremock)
- Architecture: HIGH - YahooClient and CircuitBreaker patterns directly applicable, proven in Phase 4-6
- Pitfalls: MEDIUM - Schedule A keyset pagination confirmed via multiple sources, but haven't tested empirically; rate limit headers issue documented in GitHub
- API endpoint details: MEDIUM - Base URL and authentication confirmed, but specific response structures need empirical validation during Phase 8
- Error handling: HIGH - Status codes 429/403 standard HTTP, thiserror pattern well-established in project

**Research date:** 2026-02-12
**Valid until:** 2026-03-14 (30 days - API is stable, government project with infrequent breaking changes)

**Empirical validation needed:**
1. Schedule A response structure - make test API call with DEMO_KEY or real key
2. /candidate/{id}/committees response structure - capture JSON fixture
3. 429 response headers - verify Retry-After presence
4. per_page defaults/maximums - test boundary conditions
5. Candidate name search case-sensitivity - test with mixed case
