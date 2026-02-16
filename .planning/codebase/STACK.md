# Technology Stack

**Analysis Date:** 2026-02-15

## Languages

**Primary:**
- Rust 1.70+ (minimum requirement) - Entire workspace codebase

**Secondary:**
- SQL - SQLite schema in `schema/sqlite.sql` (migrations v1-v7)
- YAML/TOML - For configuration and seed data

## Runtime

**Environment:**
- Cargo (Rust package manager) 1.93.0
- Workspace-based Rust project with three interdependent crates

## Frameworks

**Core:**
- Tokio 1 (full features) - Async runtime
- Reqwest 0.12 - HTTP client
- Clap 4 (with derive) - CLI argument parsing

**Serialization:**
- Serde 1 (with derive) - Data serialization
- Serde JSON 1 - JSON handling
- Serde YML 0.0.12 - YAML parsing (for congress-legislators)
- TOML 0.8 - TOML parsing (for seed data)

**Data Storage:**
- Rusqlite 0.31 (bundled SQLite) - SQLite database
- Chrono 0.4 - Date/time handling
- Time 0.3 - High-precision time for Yahoo Finance integration

**CLI Output:**
- Tabled 0.17 - Table formatting
- Quick-XML 0.37 - XML serialization
- CSV 1.3 - CSV output
- Indicatif 0.17 - Progress bars for pipelines

**Integrations:**
- Yahoo Finance API 4.1.0 - Market data integration
- Strsim 0.11 - String similarity (Jaro-Winkler) for employer mapping
- Dotenvy 0.15 - Environment variable loading (.env support)

**Testing:**
- Wiremock 0.6 - HTTP mock server
- Insta 1 - Snapshot testing
- Jsonschema 0.29 - JSON Schema validation

## Crates

- `capitoltrades_api` - Vendored API client and types
- `capitoltraders_lib` - Core library (scraping, db, yahoo, openfec, mapping, analytics, anomaly, conflict, sector_mapping, committee_jurisdiction)
- `capitoltraders_cli` - CLI binary with 13 subcommands

## Platform Requirements

- Rust 1.70+
- Network access for scraping/API calls
- File system access for SQLite database

---

*Stack analysis: 2026-02-15*
