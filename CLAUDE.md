# Capitol Traders - Development Guide

## Project Structure

Rust workspace with three crates:

- `capitoltrades_api/` -- Vendored fork of [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). Contains the legacy HTTP client plus shared types/enums used for serialization and validation.
- `capitoltraders_lib/` -- Library layer: cached client wrapper, in-memory TTL cache, analysis helpers, validation, error types.
- `capitoltraders_cli/` -- CLI binary (`capitoltraders`) using clap. Subcommands: `trades`, `politicians`, `issuers`, `sync`.

### File Layout

```
capitoltrades_api/
  src/
    lib.rs                    # crate root, re-exports Client, Query traits, query types
    client.rs                 # HTTP client (reqwest), get_trades/get_politicians/get_issuers
    errors.rs                 # upstream Error type
    user_agent.rs             # random user-agent rotation
    types/
      mod.rs                  # re-exports all types
      trade.rs                # Trade, Asset, TxType, TradeSize, AssetType, Label, Owner
      politician.rs           # Politician, PoliticianDetail, Chamber, Gender, Party
      issuer.rs               # IssuerDetail, MarketCap, Sector, Performance, EodPrice
      meta.rs                 # Meta, Paging, PaginatedResponse, Response
    query/
      mod.rs                  # re-exports all query types
      common.rs               # Query trait, QueryCommon, SortDirection
      trade.rs                # TradeQuery (22 fields), TradeSortBy
      politician.rs           # PoliticianQuery, PoliticianSortBy
      issuer.rs               # IssuerQuery, IssuerSortBy
  tests/
    deserialization.rs        # JSON fixture deserialization tests
    query_builders.rs         # URL parameter encoding tests
    client_integration.rs     # wiremock integration tests
    fixtures/                 # JSON test fixtures

capitoltraders_lib/
  src/
    lib.rs                    # crate root, re-exports from capitoltrades_api
    client.rs                 # CachedClient wrapper, cache key generation
    cache.rs                  # MemoryCache (DashMap-backed TTL cache)
    db.rs                     # SQLite storage + ingestion helpers
    scrape.rs                 # HTML scraper for trades/politicians/issuers
    validation.rs             # input validation for all CLI filter types
    analysis.rs               # trade analysis helpers (by-party, by-month, top issuers)
    error.rs                  # CapitolTradesError (wraps upstream Error + InvalidInput)

capitoltraders_cli/
  src/
    main.rs                   # CLI entry point, Commands enum, tokio runtime
    output.rs                 # table/JSON/CSV/Markdown/XML formatting
    xml_output.rs             # XML serialization via JSON-to-XML bridge (quick-xml)
    commands/
      mod.rs                  # module declarations
      trades.rs               # trades subcommand (TradesArgs + run)
      politicians.rs          # politicians subcommand (PoliticiansArgs + run)
      issuers.rs              # issuers subcommand (IssuersArgs + run)
      sync.rs                 # sync subcommand (SQLite ingestion)

schema/
  trade.schema.json           # JSON Schema (draft 2020-12) for Trade array
  politician.schema.json      # JSON Schema for PoliticianDetail array
  issuer.schema.json          # JSON Schema for IssuerDetail array
  sqlite.sql                  # SQLite DDL aligned with CLI JSON output
  trades.xsd                  # XML Schema for trades XML output
  politicians.xsd             # XML Schema for politicians XML output
  issuers.xsd                 # XML Schema for issuers XML output
```

## Build & Test

```
cargo check --workspace
cargo test --workspace
cargo clippy --workspace
cargo run -p capitoltraders_cli -- trades --help
```

## Upstream Vendored Crate (`capitoltrades_api`)

Forked from `crates/capitoltrades_api/` in the upstream repo. Our modifications:

1. `Client.base_api_url`: changed from `&'static str` to `String`, added `with_base_url()` constructor (required for wiremock test mocking).
2. `Meta.paging`, `Paging` fields, `PaginatedResponse.meta`: made `pub` (required for display/analysis layers).
3. Added `Default` impl for `Client`, removed unnecessary `mut` in `get_url`.
4. `TradeQuery`: added `parties`, `states`, `committees`, `search` fields + builders + URL encoding.
5. `TradeQuery`: added `genders`, `market_caps`, `asset_types`, `labels`, `sectors`, `tx_types`, `chambers`, `politician_ids`, `issuer_states`, `countries` fields + builders + URL encoding.
6. `PoliticianQuery`: added `states`, `committees` fields + builders + URL encoding.
7. `TxType`: added `Clone`, `Copy`, `Display` derives.
8. `Chamber`, `Gender`: added `Clone`, `Copy`, `Display` derives.
9. `MarketCap`: added `Display` impl (outputs numeric value for API).
10. New enums: `AssetType` (22 variants), `Label` (4 variants) in `trade.rs`.
11. `Commands::Trades` uses `Box<TradesArgs>` to avoid `large_enum_variant` clippy warning.

All clippy warnings in the vendored crate have been resolved.

## Trade Filter Types

The trades command supports extensive filtering. All multi-value filters accept comma-separated input at the CLI layer.
Date filters use either relative days (`--days`, `--tx-days`) or absolute date ranges
(`--since`/`--until`, `--tx-since`/`--tx-until`), but not both simultaneously.
In scrape mode, only filters backed by fields present in the HTML are supported; `--committee`,
`--trade-size`, `--market-cap`, `--asset-type`, and `--label` are rejected.

| CLI Flag | API Param | Type | Accepted Values |
|---|---|---|---|
| `--name` | `search` | `String` | free text (max 100 bytes) |
| `--politician` | `politician` | `PoliticianID` | scrape mode uses client-side name matching |
| `--issuer` | `issuer` | `IssuerID` | scrape mode uses client-side name/ticker matching |
| `--issuer-id` | `issuer` | `IssuerID` | numeric ID |
| `--party` | `party` | `Party` | `democrat` (`d`), `republican` (`r`), `other` |
| `--state` | `state` | `String` | 2-letter uppercase US state/territory code |
| `--committee` | `committee` | `String` | abbreviation code (e.g. `ssfi`) or full name |
| `--days` | `pubDate` | `i64` | relative days (e.g. `7` becomes `7d`) |
| `--tx-days` | `txDate` | `i64` | relative days |
| `--since` | `pubDate` | `YYYY-MM-DD` | absolute lower bound (converted to relative days); conflicts with `--days` |
| `--until` | client-side | `YYYY-MM-DD` | absolute upper bound; conflicts with `--days` |
| `--tx-since` | `txDate` | `YYYY-MM-DD` | absolute lower bound (converted to relative days); conflicts with `--tx-days` |
| `--tx-until` | client-side | `YYYY-MM-DD` | absolute upper bound; conflicts with `--tx-days` |
| `--trade-size` | `tradeSize` | `TradeSize` | `1`-`10` (bracket numbers) |
| `--gender` | `gender` | `Gender` | `female` (`f`), `male` (`m`) |
| `--market-cap` | `mcap` | `MarketCap` | `mega`-`nano` or `1`-`6` |
| `--asset-type` | `assetType` | `AssetType` | 22 kebab-case variants (see below) |
| `--label` | `label` | `Label` | `faang`, `crypto`, `memestock`, `spac` |
| `--sector` | `sector` | `Sector` | 12 kebab-case GICS sectors |
| `--tx-type` | `txType` | `TxType` | `buy`, `sell`, `exchange`, `receive` |
| `--chamber` | `chamber` | `Chamber` | `house` (`h`), `senate` (`s`) |
| `--politician-id` | `politician` | `PoliticianID` | `P` + 6 digits (e.g. `P000197`) |
| `--issuer-state` | `issuerState` | `String` | 2-letter lowercase code |
| `--country` | `country` | `String` | 2-letter lowercase ISO code |

### AssetType Variants

`stock`, `stock-option`, `corporate-bond`, `etf`, `etn`, `mutual-fund`, `cryptocurrency`, `pdf`, `municipal-security`, `non-public-stock`, `other`, `reit`, `commodity`, `hedge`, `variable-insurance`, `private-equity`, `closed-end-fund`, `venture`, `index-fund`, `government-bond`, `money-market-fund`, `brokered`

### Sector Variants

`communication-services`, `consumer-discretionary`, `consumer-staples`, `energy`, `financials`, `health-care`, `industrials`, `information-technology`, `materials`, `real-estate`, `utilities`, `other`

## Committee Abbreviation Codes

The CapitolTrades BFF API uses short abbreviation codes for committees, **not** full names. Committee filtering
is not supported in scrape mode, but the mapping is retained for validation and future use. The URL pattern is
`?committee=hsag&committee=ssfi` (multiple allowed).

The full code-to-name mapping lives in `capitoltraders_lib/src/validation.rs` as `COMMITTEE_MAP`. The CLI accepts either the code or the full name and resolves to the code before sending to the API.

Examples:

| Code | Full Name |
|------|-----------|
| `hsag` | House - Agriculture |
| `hsba` | House - Financial Services |
| `hsju` | House - Judiciary |
| `ssfi` | Senate - Finance |
| `ssbk` | Senate - Banking, Housing & Urban Affairs |
| `ssas` | Senate - Armed Services |
| `spag` | Senate - Aging |

See the complete list (48 committees) in `COMMITTEE_MAP`.

## Validation Module (`capitoltraders_lib/src/validation.rs`)

All user input is validated before being passed to the API layer. Each validator returns a typed result or `CapitolTradesError::InvalidInput`.

| Function | Input | Returns | Notes |
|---|---|---|---|
| `validate_search` | free text | `String` | max 100 bytes, strips control chars, trims |
| `validate_state` | state code | `String` (uppercase) | 50 states + DC + territories |
| `validate_party` | party name | `Party` | shorthand `d`/`r` supported |
| `validate_committee` | code or name | `String` (code) | resolves full names to API codes |
| `validate_page` | number | `i64` | must be >= 1 |
| `validate_page_size` | number | `i64` | must be 1-100 |
| `validate_gender` | gender | `Gender` | shorthand `f`/`m` supported |
| `validate_market_cap` | name or number | `MarketCap` | `mega`-`nano` or `1`-`6` |
| `validate_asset_type` | kebab-case string | `AssetType` | 22 variants |
| `validate_label` | label name | `Label` | 4 variants |
| `validate_sector` | kebab-case string | `Sector` | 12 GICS sectors |
| `validate_tx_type` | tx type | `TxType` | `buy`/`sell`/`exchange`/`receive` |
| `validate_chamber` | chamber | `Chamber` | shorthand `h`/`s` supported |
| `validate_politician_id` | ID string | `String` | `P` + 6 digits |
| `validate_country` | ISO code | `String` (lowercase) | 2-letter alpha |
| `validate_issuer_state` | state code | `String` (lowercase) | 2-letter alpha |
| `validate_trade_size` | bracket number | `TradeSize` | `1`-`10` |
| `validate_date` | date string | `NaiveDate` | YYYY-MM-DD format (chrono) |
| `validate_days` | number | `i64` | 1-3650 range |
| `date_to_relative_days` | `NaiveDate` | `Option<i64>` | days from today, `None` if future |

## Test Inventory (188 tests)

| Suite | Count | Location |
|---|---|---|
| Upstream snapshot (insta) | 3 | `capitoltrades_api/src/query/*.rs` |
| Deserialization | 7 | `capitoltrades_api/tests/deserialization.rs` |
| Query builders | 36 | `capitoltrades_api/tests/query_builders.rs` |
| Wiremock integration | 8 | `capitoltrades_api/tests/client_integration.rs` |
| Cache unit | 5 | `capitoltraders_lib/src/cache.rs` |
| Analysis unit | 5 | `capitoltraders_lib/src/analysis.rs` |
| Validation unit | 83 | `capitoltraders_lib/src/validation.rs` |
| XML output | 12 | `capitoltraders_cli/src/xml_output.rs` |
| Output unit | 20 | `capitoltraders_cli/src/output.rs` |
| Schema validation | 9 | `capitoltraders_cli/tests/schema_validation.rs` |

## Key Dependencies

| Crate | Version | Purpose |
|---|---|---|
| reqwest | 0.12 | HTTP client (upstream) |
| wiremock | 0.6 | Mock HTTP in tests |
| dashmap | 6 | Concurrent in-memory cache |
| tabled | 0.17 | Terminal table output |
| clap | 4 | CLI argument parsing (derive) |
| insta | 1 | Snapshot testing (upstream) |
| quick-xml | 0.37 | XML output serialization (Writer API) |
| jsonschema | 0.29 | JSON Schema validation in tests (dev-dependency) |
| rusqlite | 0.31 | SQLite storage for `sync` ingestion |
| regex | 1 | Parsing HTML payloads for politician cards |

## Conventions

- The CLI binary is named `capitoltraders` (no underscore).
- Cache TTL is 300 seconds (5 minutes), in-memory only via `MemoryCache`.
- Output format is controlled by `--output table|json|csv|md|xml` (global flag).
- Analysis functions operate on slices of upstream types and return standard collections.
- Error types in the lib layer wrap upstream `capitoltrades_api::Error` rather than re-implementing.
- `trades`, `politicians`, and `issuers` scrape `capitoltrades.com`; filters are applied client-side and some flags are unsupported (see Scraping).
- Scraped listing pages are fixed at 12 results; `--page-size` is ignored for those commands.
- Comma-separated CLI values are split and individually validated before filtering.
- `issuerState` and `country` are lowercase in scraped data (unlike politician `state` which is uppercase). Validation normalizes accordingly.
- `CachedClient` still enforces a randomized 5-10 second delay between API requests for any legacy usage (not used by CLI scraping).

## Scraping

There is no public API. All CLI commands (`trades`, `politicians`, `issuers`, `sync`) scrape
`capitoltrades.com` and parse the embedded Next.js RSC payloads. `ScrapeClient` exposes
`trades_page`, `trade_detail`, `politicians_page`, `politician_detail`, `issuers_page`, and
`issuer_detail`. Missing fields not present in the HTML are populated with safe defaults
(e.g., unknown asset type, empty committees/labels). Aggregated politician/issuer stats are computed
from scraped trades in `sync`. Use `--with-trade-details` to hit per-trade pages and capture filing
URLs/IDs (slow; adds one request per trade). Senate filings often use UUID-style URLs, so `filing_id`
may be `0` while `filing_url` is populated.

## SQLite Ingestion

The `sync` subcommand writes to SQLite using `schema/sqlite.sql`, which mirrors the CLI JSON output schemas.
Nested arrays are normalized into join tables (`trade_committees`, `trade_labels`, `politician_committees`),
and issuer performance is stored in `issuer_performance` plus `issuer_eod_prices`. Incremental runs track
`ingest_meta.last_trade_pub_date` (YYYY-MM-DD) and request only recent pages; `--since` overrides it.
`--refresh-politicians` and `--refresh-issuers` are ignored in scrape mode.

Commands:

```
capitoltraders sync --db capitoltraders.db --full
capitoltraders sync --db capitoltraders.db
capitoltraders sync --db capitoltraders.db --refresh-issuers --refresh-politicians
capitoltraders sync --db capitoltraders.db --since 2024-01-01
```

## CI

The daily SQLite sync workflow is defined in `.github/workflows/sqlite-sync.yml`.
It restores the previous DB from a GitHub Actions cache, runs `capitoltraders sync`,
and uploads the updated database as a workflow artifact.

## XML Output Format

The `--output xml` flag emits well-formed XML. Implementation uses a JSON-to-XML bridge: types are serialized to `serde_json::Value` first, then walked recursively to emit XML via `quick-xml::Writer`. This avoids modifying the vendored crate.

Key behaviors:
- Null fields are omitted (no empty element emitted)
- Arrays produce a wrapper element with singular child elements (e.g. `<committees><committee>...</committee></committees>`)
- XML declaration: `<?xml version="1.0" encoding="UTF-8"?>`
- Root elements: `<trades>`, `<politicians>`, `<issuers>`
- Empty results produce a self-closing root (e.g. `<trades/>`)
- Special characters (`&`, `<`, `>`) are escaped by quick-xml

The singularization map for array wrapper names:
- `committees` -> `committee`
- `labels` -> `label`
- `eodPrices` -> `priceSet`
- All other arrays use the field name as-is for children

## Schema Files

The `schema/` directory contains JSON Schema (draft 2020-12) and XSD files documenting the structure of CLI output. Schemas describe what `--output json` and `--output xml` actually emit: arrays of data items, not the PaginatedResponse wrapper (which goes to stderr).

| File | Format | Describes |
|------|--------|-----------|
| `schema/trade.schema.json` | JSON Schema | Trade array from `trades --output json` |
| `schema/politician.schema.json` | JSON Schema | PoliticianDetail array from `politicians --output json` |
| `schema/issuer.schema.json` | JSON Schema | IssuerDetail array from `issuers --output json` |
| `schema/sqlite.sql` | SQL DDL | SQLite schema aligned to CLI JSON output |
| `schema/trades.xsd` | XML Schema | Trade XML from `trades --output xml` |
| `schema/politicians.xsd` | XML Schema | PoliticianDetail XML from `politicians --output xml` |
| `schema/issuers.xsd` | XML Schema | IssuerDetail XML from `issuers --output xml` |

All serialized fields are documented, including private Rust fields that are still serde-serialized. Schemas are hand-written (no `schemars` crate) to avoid vendored crate modifications.

## Adding a New CLI Subcommand

1. Create `capitoltraders_cli/src/commands/your_command.rs` with a clap `Args` struct and an `async fn run(...)`.
2. Add it to `commands/mod.rs`.
3. Add the variant to the `Commands` enum in `main.rs`.
4. Wire it into the `match` block in `main`.
