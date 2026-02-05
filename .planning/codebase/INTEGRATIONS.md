# External Integrations

**Analysis Date:** 2026-02-05

## APIs & External Services

**CapitolTrades BFF API:**
- Service: CapitolTrades (congressional trading data)
- What it's used for: Queries recent trades, politician profiles, issuer information
  - SDK/Client: Custom vendored `capitoltrades_api` crate (fork of https://github.com/TommasoAmici/capitoltrades)
  - Endpoint: `https://bff.capitoltrades.com`
  - Authentication: None - public API

**API Routes:**
- `/trades` - Get paginated trade list with extensive filters
  - Accessed via: `capitoltrades_api::Client::get_trades(query: &TradeQuery)` in `capitoltrades_api/src/client.rs:80`
- `/politicians` - Get politician list with optional filters
  - Accessed via: `capitoltrades_api::Client::get_politicians(query: &PoliticianQuery)` in `capitoltrades_api/src/client.rs:85`
- `/issuers` - Get issuer list with filters
  - Accessed via: `capitoltrades_api::Client::get_issuers(query: &IssuerQuery)` in `capitoltrades_api/src/client.rs:104`
- `/issuers/{id}` - Get single issuer by numeric ID
  - Accessed via: `capitoltrades_api::Client::get_issuer(issuer_id: i64)` in `capitoltrades_api/src/client.rs:96`

**HTTP Client Configuration:**
- Library: reqwest 0.12
- Features: json, gzip
- Headers sent with every request (`capitoltrades_api/src/client.rs:48-61`):
  - content-type: application/json
  - origin: https://www.capitoltrades.com
  - referer: https://www.capitoltrades.com
  - accept: */*
  - accept-language: en-US,en;q=0.9
  - sec-fetch-dest: empty
  - sec-fetch-mode: cors
  - sec-fetch-site: same-site
  - user-agent: Randomly rotated from pool of 24 browser user-agents

**User-Agent Rotation:**
- Implementation: `capitoltrades_api/src/user_agent.rs`
- Pool: 24 user-agent strings weighted by browser popularity distribution
- Rotation: Random selection on each request via `get_user_agent()` function
- Purpose: Avoid blocking/rate-limiting by mimicking browser traffic

## Data Storage

**Databases:**
- None - no persistent database

**Cache:**
- Type: In-memory TTL cache
- Implementation: `capitoltraders_lib/src/cache.rs` - `MemoryCache` struct
- Backend: DashMap 6 (concurrent lock-free hashmap)
- TTL: 300 seconds (5 minutes) - set in `capitoltraders_cli/src/main.rs:54`
- Scope: Process-local only (no distributed caching)
- Cache Keys: Format `{endpoint}:{query_hash}` where query_hash includes all filter parameters
  - Example: `trades:{TradeQuery debug repr}` in `capitoltraders_lib/src/client.rs:33`

**File Storage:**
- Local filesystem only - CLI outputs directly to stdout or user-specified files
- CSV output handled via csv 1.3 crate
- JSON output via serde_json
- Markdown table output via tabled 0.17

## Authentication & Identity

**Auth Provider:**
- None - CapitolTrades BFF API is completely public
- No API keys, tokens, or credentials required
- No authentication layer implemented

## Monitoring & Observability

**Error Tracking:**
- None - no external error tracking service
- Errors returned to CLI via `anyhow::Result` in main

**Logs:**
- Framework: tracing 0.1 with tracing-subscriber 0.3
- Activation: `RUST_LOG` environment variable
- Default level: `capitoltraders=info`
- Configuration: `capitoltraders_cli/src/main.rs:37-43`
- Output: stderr via default formatter
- Emitted at:
  - HTTP client failures: `capitoltrades_api/src/client.rs:52, 68, 74`
  - Query parsing errors (implicit via Result types)

## CI/CD & Deployment

**Hosting:**
- None specified - binary is standalone CLI

**CI Pipeline:**
- None detected - no GitHub Actions, GitLab CI, or similar

**Build Process:**
- Cargo workspace: `cargo build -p capitoltraders_cli --release`
- Result: Single statically-linked binary `capitoltraders`

## Environment Configuration

**Required env vars:**
- None - API is public, no credentials needed

**Optional env vars:**
- `RUST_LOG` - Control log verbosity
  - Example: `RUST_LOG=debug capitoltraders trades`
  - Parsed by: tracing-subscriber EnvFilter in `capitoltraders_cli/src/main.rs:38`

**Runtime Configuration:**
- All runtime behavior controlled via CLI args parsed by clap 4
- Global flag: `--output` (table|json|csv|md) - default: table
- Subcommand-specific filters for trades, politicians, issuers

## Webhooks & Callbacks

**Incoming:**
- None - CLI is pull-only consumer of CapitolTrades BFF

**Outgoing:**
- None - CLI does not trigger any external callbacks

## Test Infrastructure

**Integration Testing:**
- wiremock 0.6 - Mock HTTP server for offline testing
- Tests: `capitoltrades_api/tests/client_integration.rs` (8 tests)
- Mocks CapitolTrades BFF endpoints with controlled responses

**Snapshot Testing:**
- insta 1 - YAML-based snapshots for query builder output
- Tests verify URL parameter encoding matches expected format
- Located in: `capitoltrades_api/src/query/*.rs` (3 snapshot tests)

---

*Integration audit: 2026-02-05*
