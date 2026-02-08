# External Integrations

**Analysis Date:** 2026-02-07

## APIs & External Services

**CapitolTrades Official (Unofficial Access):**
- **Service:** CapitolTrades.com (capitoltrades.com)
- **What it's used for:** Primary data source for congressional stock trading, politician details, and issuer information
- **Access method:** HTML scraping via Next.js RSC payloads (no public API available)
- **Base URL:** `https://www.capitoltrades.com` (configurable via `CAPITOLTRADES_BASE_URL` or `--base-url`)
- **Implementation:** `capitoltraders_lib/src/scrape.rs` with ScrapeClient
- **Endpoints scraped:**
  - `/trades?page={page}` - Paginated list of recent congressional trades
  - `/trades/{trade_id}` - Individual trade detail (filing URL/ID extraction)
  - `/politicians?page={page}` - Paginated politician list with trading activity
  - `/politicians/{politician_id}` - Individual politician detail
  - `/issuers?page={page}` - Paginated issuer (stock/company) list
  - `/issuers/{issuer_id}` - Individual issuer detail with performance data

**BFF (Backend For Frontend) API:**
- **Service:** CapitolTrades internal API (bff.capitoltrades.com)
- **What it's used for:** Structured JSON access to trades, politicians, issuers (used by vendored upstream crate)
- **Base URL:** `https://bff.capitoltrades.com`
- **Implementation:** `capitoltrades_api/src/client.rs` with Client struct
- **Endpoints:**
  - `GET /trades` - Query trades with filters
  - `GET /politicians` - Query politicians with filters
  - `GET /issuers` - Query issuers with filters
  - `GET /issuers/{id}` - Get single issuer by numeric ID
- **Auth:** None required (publicly accessible)
- **Rate Limiting:** Informal (5-10 second delay between requests implemented in CachedClient)

## Data Storage

**Databases:**

**SQLite (Local File):**
- **Provider:** Bundled SQLite 3 via rusqlite crate
- **Connection:** File-based (path configurable via `--db` flag, default: `capitoltraders.db`)
- **Client:** `rusqlite 0.31`
- **ORM/Query:** Raw SQL (no ORM; hand-written SQL in `capitoltraders_lib/src/db.rs`)
- **Configuration:**
  - Foreign keys enabled (`PRAGMA foreign_keys = ON`)
  - Write-ahead logging (`PRAGMA journal_mode = WAL`)
  - Normal synchronous mode (`PRAGMA synchronous = NORMAL`)
- **Schema:** `schema/sqlite.sql` (hand-written DDL)
- **Usage:** `sync` subcommand ingests scraped data into persistent SQLite database
- **Incremental tracking:** `ingest_meta` table stores `last_trade_pub_date` for resumable syncs

**File Storage:**
- No external file storage (cloud or otherwise)
- SQLite database stored locally (single file: `capitoltraders.db`)
- Binary distributed via GitHub Releases (artifacts uploaded from CI)

**Caching:**
- **In-Memory Cache:** DashMap-backed TTL cache (5-minute TTL)
- **Implementation:** `capitoltraders_lib/src/cache.rs` with MemoryCache struct
- **Scope:** Per-process, not shared across instances
- **Used by:** CachedClient to avoid repeated API calls during a session

## Authentication & Identity

**Auth Provider:**
- None required - All data sources are publicly accessible

**CapitolTrades Access:**
- No API key or authentication credentials needed
- Requests include browser-like headers (`Origin`, `Referer`, `User-Agent`) to avoid being blocked
- User-agent rotation: 24 common browser strings (weighted by popularity) selected randomly per request
  - Implementation: `capitoltrades_api/src/user_agent.rs` with WeightedIndex sampling

**Authorization:**
- No user roles, permissions, or access control
- CLI runs with whatever system permissions the user has (for file access to SQLite DB)

## Monitoring & Observability

**Error Tracking:**
- None (no external service integration)
- Errors logged to stderr via tracing crate

**Logs:**
- **Framework:** tracing 0.1 (structured logging)
- **Subscriber:** tracing-subscriber 0.3 with env-filter support
- **Output:** Stderr (unformatted by default)
- **Control:** `RUST_LOG` environment variable (e.g., `RUST_LOG=debug capitoltraders trades`)
- **Tracing Points:**
  - HTTP request failures and retries
  - HTML parsing errors and missing fields
  - Database operations (upserts, schema init)
  - Cache hits/misses (not logged by default, would require code change)

**Metrics:**
- None collected or exported
- No integration with Prometheus, Datadog, or similar

## CI/CD & Deployment

**Hosting:**
- GitHub Releases (for binary distribution)
- GitHub Actions (for build and test automation)

**Build Pipeline (`.github/workflows/release.yml`):**
- Triggered on git tags (`v*`)
- Builds for 5 targets: x86_64-linux, aarch64-linux, x86_64-macos, aarch64-macos, x86_64-windows
- Cross-compilation via `cross` tool for aarch64-linux
- Artifacts: Compressed binaries (tar.gz for Unix, zip for Windows)
- Final step: Create GitHub release with SHA256 checksums

**Sync Pipeline (`.github/workflows/sqlite-sync.yml`):**
- Scheduled daily at 07:00 UTC via cron (`0 7 * * *`)
- Also manually triggerable via `workflow_dispatch`
- Restores previous SQLite from GitHub Actions cache
- Runs: `cargo run -p capitoltraders_cli -- sync --db data/capitoltraders.db`
- Uploads updated database as artifact (30-day retention)
- Cache key: `sqlite-db-${{ github.run_id }}` with fallback to `sqlite-db-*`

**Caching:**
- Cargo dependencies: Swatinem/rust-cache@v2 (per-target)
- SQLite database: GitHub Actions cache (per-run with fallback to latest)

## Environment Configuration

**No External Configuration Files:**
- No `.env`, `.env.local`, or similar
- All configuration via CLI flags and environment variables

**Environment Variables Required:**
- None - All have sensible defaults
- Optional for customization:
  - `CAPITOLTRADES_BASE_URL` - Override scraper URL
  - `CAPITOLTRADES_RETRY_MAX` - Max retry attempts
  - `CAPITOLTRADES_RETRY_BASE_MS` - Retry base delay
  - `CAPITOLTRADES_RETRY_MAX_MS` - Retry max delay
  - `RUST_LOG` - Logging level/filter

**Secrets Location:**
- No secrets in codebase
- GitHub Actions uses native `GITHUB_TOKEN` for release creation (no explicit secret needed)
- No API keys, database passwords, or credentials stored anywhere

## Webhooks & Callbacks

**Incoming:**
- None - CLI is not a server; no webhook listeners

**Outgoing:**
- None - CLI does not trigger external webhooks or callbacks

## Rate Limiting Strategy

**Implemented In-Process:**
- **Location:** `capitoltraders_lib/src/client.rs` (CachedClient)
- **Strategy:** Randomized 5-10 second delay between HTTP requests
- **Rationale:** Unofficial API; added to reduce load on CapitolTrades servers
- **Cache Bypass:** Cache hits (within 5-minute TTL) skip the delay
- **First Request:** No delay for the first request in a session
- **Configurability:** Delay parameters NOT exposed via CLI; hardcoded in implementation

**Scraper Retries (with Exponential Backoff):**
- **Location:** `capitoltraders_lib/src/scrape.rs` (ScrapeClient)
- **Strategy:** Exponential backoff with jitter
- **Configuration via environment variables:**
  - `CAPITOLTRADES_RETRY_MAX` (default: 3)
  - `CAPITOLTRADES_RETRY_BASE_MS` (default: 2000 ms)
  - `CAPITOLTRADES_RETRY_MAX_MS` (default: 30000 ms)
- **Respects Retry-After header:** Parses and honors server's `Retry-After` header if present
- **Maximum timeout:** 30-second per-request timeout enforced globally

---

*Integration audit: 2026-02-07*
