# Technology Stack

**Analysis Date:** 2026-02-07

## Languages

**Primary:**
- Rust 1.70+ (edition 2021) - All three crates and CLI binary

**Secondary:**
- SQL - SQLite schema and database operations (`schema/sqlite.sql`)
- XML Schema - Output validation (`schema/*.xsd`)
- JSON Schema (draft 2020-12) - Output validation (`schema/*.schema.json`)
- YAML - Snapshot testing fixtures for query builder tests

## Runtime

**Environment:**
- Tokio 1.x (async runtime, full features)
  - Used via workspace dependency with `features = ["full"]`
  - Multi-threaded runtime configured in release CI

**Compiler:**
- Stable Rust (auto-installed via `dtolnay/rust-toolchain@stable` in CI)
- Cross-compilation support via `cross` tool for aarch64-unknown-linux-gnu target

## Frameworks

**Core:**
- reqwest 0.12 - HTTP client (with gzip and rustls-tls, no OpenSSL)
  - Used for scraping capitoltrades.com and calling unofficial BFF API
  - Browser-like headers and randomized user-agent rotation
  - 30-second timeout on all requests

**CLI:**
- clap 4 - Command-line argument parsing (derive feature)
  - Global flags: `--output` (format), `--base-url` (scraper override)
  - Subcommands: `trades`, `politicians`, `issuers`, `sync`
  - Multi-value filters accept comma-separated input

**Output Formatting:**
- tabled 0.17 - Human-readable table and Markdown output
- csv 1.3 - CSV output with formula injection sanitization
- quick-xml 0.37 - XML output serialization (Writer API only, no XML parsing)
- serde_json 1 - JSON serialization/deserialization

**Data & Storage:**
- rusqlite 0.31 - SQLite bindings (with bundled SQLite library)
  - WAL mode, foreign keys, and normal synchronous mode configured
  - Used by `sync` subcommand for incremental ingestion

**Data Processing:**
- chrono 0.4 - Date/time handling (serde feature for serialization)
- serde 1.0 - Serialization framework (derive feature)
- regex 1 - HTML parsing for Next.js RSC payloads
- dashmap 6 - Concurrent, lock-free in-memory cache (TTL-based)
- rand 0.8.5 - Random user-agent selection and retry jitter

**Error Handling:**
- thiserror 1 - Error type derivation
- anyhow 1 - Error propagation (workspace-wide)
- tracing 0.1 - Structured logging throughout
- tracing-subscriber 0.3 - Log filtering via `RUST_LOG` environment variable

## Testing

**Framework:**
- insta 1 - Snapshot testing (YAML format) for query builder URL encoding
- wiremock 0.6 - Mock HTTP server for integration tests
- jsonschema 0.29 - JSON Schema validation in tests (draft202012 module, dev-dependency)

**Test Commands:**
```bash
cargo test --workspace              # Run all 194 tests
cargo test --workspace -- --nocapture  # Show println! output
cargo clippy --workspace            # Lint (all clippy warnings resolved)
```

## Build Configuration

**Workspace Configuration:**
- Resolver: "2" (new Rust dependency resolver)
- All workspace members share dependencies via `workspace.dependencies`
- Three member crates:
  - `capitoltrades_api` - Vendored upstream API client
  - `capitoltraders_lib` - Library layer (cache, scraping, validation, analysis)
  - `capitoltraders_cli` - CLI binary

**Build Targets (from release CI):**
- `x86_64-unknown-linux-gnu` (Linux x86-64)
- `aarch64-unknown-linux-gnu` (Linux ARM64, via cross)
- `x86_64-apple-darwin` (macOS Intel)
- `aarch64-apple-darwin` (macOS ARM64/Apple Silicon)
- `x86_64-pc-windows-msvc` (Windows)

**Cargo Features:**
- reqwest: `gzip`, `rustls-tls` (no OpenSSL dependencies; TLS via rustls)
- tokio: `full` (all features enabled)
- serde: `derive` (macro support)
- chrono: `serde` (date serialization)
- insta: `yaml` (YAML snapshot format)
- tracing-subscriber: `env-filter` (RUST_LOG support)

**Binary Name:**
- `capitoltraders` (no underscore, set in `capitoltraders_cli/Cargo.toml`)

## Platform Requirements

**Development:**
- Rust 1.70+ (for edition 2021 and std library features)
- Tokio multi-threaded runtime (requires `std` feature)
- No platform-specific system dependencies (bundled SQLite)

**Production:**
- Linux, macOS (Intel/ARM), Windows (all tier-1 supported)
- Minimum glibc 2.31 for Linux (standard in modern distributions)
- No runtime dependencies (statically linked via rustls and bundled SQLite)

**CI/CD Environment:**
- GitHub Actions
- Ubuntu latest (Linux builds and sync job)
- macOS 15 Intel for x86_64 (macOS 15)
- macOS latest for aarch64 (macOS ARM64)
- Windows latest
- Cargo caching via Swatinem/rust-cache@v2

## Database

**SQLite 3:**
- Bundled version (via `rusqlite` bundled feature)
- No external installation required
- Database file: typically `capitoltraders.db` (created via `sync` subcommand)
- Incremental sync tracks `last_trade_pub_date` in `ingest_meta` table

**Database Pragmas (set in `capitoltraders_lib/src/db.rs`):**
```sql
PRAGMA foreign_keys = ON;        -- Enforce referential integrity
PRAGMA journal_mode = WAL;       -- Write-ahead logging for concurrent access
PRAGMA synchronous = NORMAL;     -- Balance durability and performance
```

## Configuration

**Environment Variables:**
- `CAPITOLTRADES_BASE_URL` - Override scraper base URL (default: `https://www.capitoltrades.com`)
- `CAPITOLTRADES_RETRY_MAX` - Max HTTP retry attempts (default: 3)
- `CAPITOLTRADES_RETRY_BASE_MS` - Base delay for exponential backoff (default: 2000 ms)
- `CAPITOLTRADES_RETRY_MAX_MS` - Max delay for exponential backoff (default: 30000 ms)
- `RUST_LOG` - Tracing/logging filter (e.g., `RUST_LOG=debug`)

**CLI Flags:**
- `--output` - Format: `table`, `json`, `csv`, `md`, `xml` (global)
- `--base-url` - Override scraper URL (global)
- `--db` - SQLite path for `sync` command (default: `capitoltraders.db`)
- Numerous filter flags for `trades`, `politicians`, `issuers` (see CLAUDE.md for full list)

**No Configuration Files:**
- No TOML, YAML, or config file parsing
- All configuration via CLI flags and environment variables

## Key Transitive Dependencies

**Critical (directly used):**
- tokio 1.x - Async runtime
- reqwest 0.12 - HTTP client
- serde/serde_json - Serialization
- rusqlite 0.31 - SQLite
- dashmap 6 - Concurrent cache
- regex 1 - HTML parsing

**Important (for output/CLI):**
- clap 4 - CLI parsing
- tabled 0.17 - Table formatting
- csv 1.3 - CSV output
- quick-xml 0.37 - XML generation
- chrono 0.4 - Date handling

**Utilities:**
- thiserror 1 - Error macros
- rand 0.8.5 - Random selection
- tracing 0.1, tracing-subscriber 0.3 - Logging

**Testing:**
- insta 1 - Snapshot testing
- wiremock 0.6 - HTTP mocking
- jsonschema 0.29 - Schema validation (dev-only)

---

*Stack analysis: 2026-02-07*
