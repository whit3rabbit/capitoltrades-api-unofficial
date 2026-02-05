# Technology Stack

**Analysis Date:** 2026-02-05

## Languages

**Primary:**
- Rust 2021 edition - All three crates

**Secondary:**
- None

## Runtime

**Environment:**
- Rust (no specific version constraint in Cargo.toml, uses stable)

**Package Manager:**
- Cargo (Rust's package manager)
- Lockfile: Cargo.lock (present in git repo)

## Frameworks

**Core:**
- reqwest 0.12 - Async HTTP client for CapitolTrades BFF API calls
- tokio 1 - Async runtime with full feature set (used for async/await in CLI and caching)

**CLI:**
- clap 4 - Command-line argument parsing with derive macros (`capitoltraders_cli/src/main.rs`)
- tabled 0.17 - Terminal table formatting and styling for table/markdown output
- csv 1.3 - CSV output format handling

**Data & Serialization:**
- serde 1 - Serialization framework with derive feature
- serde_json 1 - JSON serialization/deserialization for API responses and cache storage
- chrono 0.4 - Date/time handling with serde support

**Error Handling:**
- thiserror 1 - Error type macros (used in `capitoltrades_api/src/errors.rs`)
- anyhow 1 - Flexible error handling in CLI main (`capitoltraders_cli/src/main.rs`)

**Logging:**
- tracing 0.1 - Structured logging (configured in `capitoltraders_cli/src/main.rs`)
- tracing-subscriber 0.3 - Tracing subscriber with env-filter support

**Testing:**
- insta 1 - Snapshot testing with YAML format (upstream tests in `capitoltrades_api/src/query/*.rs`)
- wiremock 0.6 - HTTP mock server for integration tests

**Utilities:**
- dashmap 6 - Concurrent lock-free hashmap for in-memory TTL cache (`capitoltraders_lib/src/cache.rs`)
- rand 0.8.5 - Random number generation for user-agent rotation (`capitoltrades_api/src/user_agent.rs`)
- url 2 - URL parsing and manipulation

## Key Dependencies

**Critical:**
- reqwest 0.12 - Direct API calls to CapitolTrades BFF at `https://bff.capitoltrades.com`
- dashmap 6 - Powers in-memory cache for 300-second TTL (`capitoltraders_lib/src/cache.rs`)
- tokio 1 - Required for async HTTP and CLI runtime

**Infrastructure:**
- serde/serde_json - Handles all API JSON responses and cache serialization
- clap 4 - Enables multi-subcommand CLI (trades, politicians, issuers)
- tabled 0.17 - Formats output for terminal display (table/markdown)
- csv 1.3 - Provides CSV serialization for structured export

## Configuration

**Environment:**
- RUST_LOG - Controls log level via tracing-subscriber (default: `capitoltraders=info`)
- No API keys or secrets required - CapitolTrades BFF is public with no authentication
- No config files required - all configuration is CLI-driven via clap args

**Build:**
- Workspace root: `/Cargo.toml`
- Individual crates can be built with `cargo build -p <crate>`
- Workspace uses `resolver = "2"` for consistent dependency resolution

## Platform Requirements

**Development:**
- Rust toolchain (1.70+ recommended for 2021 edition features)
- Cargo
- Standard POSIX development tools (make, git)

**Production:**
- Rust binary compiled for target platform (Linux x86_64, macOS, Windows)
- No runtime dependencies beyond libc
- ~5MB binary size (estimate for stripped release build)

## Build Artifacts

**Binary:**
- Name: `capitoltraders` (from `capitoltraders_cli/Cargo.toml` `[[bin]]` section)
- Built from `capitoltraders_cli/src/main.rs`
- Runs against workspace features

**Library:**
- `capitoltraders_lib` - Public API for caching, validation, error handling
- `capitoltrades_api` - Vendored HTTP client library

---

*Stack analysis: 2026-02-05*
