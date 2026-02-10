# Technology Stack

**Analysis Date:** 2026-02-09

## Languages

**Primary:**
- Rust 1.70+ (minimum requirement) - Entire workspace codebase

**Secondary:**
- SQL - SQLite schema in `schema/sqlite.sql`

## Runtime

**Environment:**
- Cargo (Rust package manager) 1.93.0
- Workspace-based Rust project with three interdependent crates

**Package Manager:**
- Cargo (Rust)
- Lockfile: `Cargo.lock` present

## Frameworks

**Core:**
- Tokio 1 (full features) - Async runtime for all async operations
- Reqwest 0.12 - HTTP client for API/scraping requests
- Clap 4 (with derive feature) - CLI argument parsing and subcommand handling

**Serialization:**
- Serde 1 (with derive) - JSON/data serialization framework
- Serde JSON 1 - JSON handling

**Data Storage:**
- Rusqlite 0.31 (with bundled SQLite) - SQLite database access
- Chrono 0.4 (with serde) - Date/time handling and serialization

**CLI Output:**
- Tabled 0.17 - Table formatting for terminal output
- Quick-XML 0.37 - XML serialization (Writer API only)
- CSV 1.3 - CSV output formatting

**Error Handling:**
- Thiserror 1 - Error type macros
- Anyhow 1 - Error context wrapper

**Logging:**
- Tracing 0.1 - Structured logging instrumentation
- Tracing-Subscriber 0.3 (with env-filter) - Logging subscriber and environment filtering

**Testing:**
- Wiremock 0.6 - HTTP mock server for integration tests
- Insta 1 (with yaml feature) - Snapshot testing
- Jsonschema 0.29 (dev-dependency only) - JSON Schema validation in tests

**Utilities:**
- Dashmap 6 - Thread-safe concurrent HashMap for in-memory caching
- Regex 1 - Regular expressions for HTML scraping
- Rand 0.8.5 - Random number generation (for rate limiting delays and jitter)
- URL 2 - URL parsing and construction

## Crates

**Workspace Structure:**
- `capitoltrades_api` - Vendored API client and types (modified version of upstream TommasoAmici/capitoltrades)
- `capitoltraders_lib` - Core library with scraping, caching, validation, and database layers
- `capitoltraders_cli` - CLI binary with commands and output formatters

## Key Dependencies

**Critical:**
- `tokio` (1.x, full features) - Essential for async/await throughout the codebase
- `reqwest` (0.12, with rustls-tls and gzip) - HTTP client for scraping and API calls
- `rusqlite` (0.31, bundled) - SQLite database support for local data storage
- `clap` (4.x, derive) - CLI parsing and subcommand management

**Infrastructure:**
- `dashmap` (6.x) - Concurrent cache for API response deduplication
- `chrono` (0.4, with serde) - Date/time handling and parsing
- `serde` (1.x, with derive) - Serialization framework for JSON, CSV, XML output

**Output Formatting:**
- `tabled` (0.17) - Table layout for terminal output
- `quick-xml` (0.37) - XML serialization via Writer API
- `csv` (1.3) - CSV output with sanitization

**Networking:**
- `reqwest` (0.12) - Built with rustls-tls (no OpenSSL) and gzip compression
- `url` (2.x) - URL parsing for API requests

**Testing:**
- `wiremock` (0.6) - Mock HTTP server for testing
- `insta` (1.x) - Snapshot testing for query builders
- `jsonschema` (0.29) - JSON Schema validation

## Configuration

**Environment Variables:**
- `CAPITOLTRADES_RETRY_MAX` (default: 3) - Maximum retries for failed API requests
- `CAPITOLTRADES_RETRY_BASE_MS` (default: 2000) - Base delay in milliseconds for exponential backoff
- `CAPITOLTRADES_RETRY_MAX_MS` (default: 30000) - Maximum delay in milliseconds between retries
- `CAPITOLTRADES_BASE_URL` (optional) - Override scraping base URL (else https://www.capitoltrades.com)
- `RUST_LOG` (optional) - Tracing log filter (e.g., `RUST_LOG=capitoltraders=debug`)

**Build Configuration:**
- Edition: 2021 (Rust edition across all crates)
- Workspace resolver: 2 (new-style dependency resolution)

**CLI Configuration:**
- Subcommands: `trades`, `politicians`, `issuers`, `sync`
- Global flags: `--output` (format), `--base-url` (override scrape URL)
- Output formats: table, json, csv, md (markdown), xml

## Platform Requirements

**Development:**
- Rust 1.70 or later
- Cargo (comes with Rust)
- macOS/Linux/Windows (platform-agnostic Rust)

**Production:**
- Standalone binary (no runtime dependencies beyond system libraries)
- SQLite bundled within binary
- Network access required for scraping/API calls

## Dependencies Summary

**Total dependency count:** ~80 (including transitive dependencies via Cargo.lock)

**Critical transitive:**
- `hyper` (via reqwest) - HTTP protocol implementation
- `tokio-*` crates (via tokio) - Async runtime components
- `nom` (via quick-xml) - XML parsing

---

*Stack analysis: 2026-02-09*
