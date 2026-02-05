# Capitol Traders - Development Guide

## Project Structure

Rust workspace with three crates:

- `capitoltrades_api/` -- Vendored fork of [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). HTTP client for the CapitolTrades BFF API. Modifications from upstream are minimal and documented below.
- `capitoltraders_lib/` -- Library layer: cached client wrapper, in-memory TTL cache, analysis helpers, validation, error types.
- `capitoltraders_cli/` -- CLI binary (`capitoltraders`) using clap. Subcommands: `trades`, `politicians`, `issuers`.

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
    validation.rs             # input validation for all CLI filter types
    analysis.rs               # trade analysis helpers (by-party, by-month, top issuers)
    error.rs                  # CapitolTradesError (wraps upstream Error + InvalidInput)

capitoltraders_cli/
  src/
    main.rs                   # CLI entry point, Commands enum, tokio runtime
    output.rs                 # table/JSON formatting (tabled, serde_json)
    commands/
      mod.rs                  # module declarations
      trades.rs               # trades subcommand (TradesArgs + run)
      politicians.rs          # politicians subcommand (PoliticiansArgs + run)
      issuers.rs              # issuers subcommand (IssuersArgs + run)
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

Remaining clippy warnings in upstream code are intentionally left alone. Do not "fix" upstream code style without understanding why it exists (Chesterton's Fence).

## Trade Filter Types

The trades command supports extensive filtering. All multi-value filters accept comma-separated input at the CLI layer.

| CLI Flag | API Param | Type | Accepted Values |
|---|---|---|---|
| `--name` | `search` | `String` | free text (max 100 bytes) |
| `--politician` | `politician` | `PoliticianID` | two-step: searches politicians by name, resolves to IDs |
| `--issuer` | `issuer` | `IssuerID` | two-step: searches issuers by name, resolves to IDs |
| `--issuer-id` | `issuer` | `IssuerID` | numeric ID |
| `--party` | `party` | `Party` | `democrat` (`d`), `republican` (`r`), `other` |
| `--state` | `state` | `String` | 2-letter uppercase US state/territory code |
| `--committee` | `committee` | `String` | abbreviation code (e.g. `ssfi`) or full name |
| `--days` | `pubDate` | `i64` | relative days (e.g. `7` becomes `7d`) |
| `--tx-days` | `txDate` | `i64` | relative days |
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

The CapitolTrades BFF API uses short abbreviation codes for committees, **not** full names. The URL pattern is `?committee=hsag&committee=ssfi` (multiple allowed).

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

## Test Inventory (129 tests)

| Suite | Count | Location |
|---|---|---|
| Upstream snapshot (insta) | 3 | `capitoltrades_api/src/query/*.rs` |
| Deserialization | 7 | `capitoltrades_api/tests/deserialization.rs` |
| Query builders | 31 | `capitoltrades_api/tests/query_builders.rs` |
| Wiremock integration | 8 | `capitoltrades_api/tests/client_integration.rs` |
| Cache unit | 5 | `capitoltraders_lib/src/cache.rs` |
| Analysis unit | 5 | `capitoltraders_lib/src/analysis.rs` |
| Validation unit | 70 | `capitoltraders_lib/src/validation.rs` |

## Key Dependencies

| Crate | Version | Purpose |
|---|---|---|
| reqwest | 0.12 | HTTP client (upstream) |
| wiremock | 0.6 | Mock HTTP in tests |
| dashmap | 6 | Concurrent in-memory cache |
| tabled | 0.17 | Terminal table output |
| clap | 4 | CLI argument parsing (derive) |
| insta | 1 | Snapshot testing (upstream) |

## Conventions

- The CLI binary is named `capitoltraders` (no underscore).
- Cache TTL is 300 seconds (5 minutes), in-memory only via `MemoryCache`.
- Output format is controlled by `--output table|json` (global flag).
- Analysis functions operate on slices of upstream types and return standard collections.
- Error types in the lib layer wrap upstream `capitoltrades_api::Error` rather than re-implementing.
- Two-step lookups (`--politician`, `--issuer`) search an endpoint by name, collect IDs, then filter trades by those IDs.
- Comma-separated CLI values are split, individually validated, then added to the query via builder methods.
- `issuerState` and `country` use lowercase in the API (unlike politician `state` which is uppercase). Validation normalizes accordingly.

## Adding a New CLI Subcommand

1. Create `capitoltraders_cli/src/commands/your_command.rs` with a clap `Args` struct and an `async fn run(...)`.
2. Add it to `commands/mod.rs`.
3. Add the variant to the `Commands` enum in `main.rs`.
4. Wire it into the `match` block in `main`.
