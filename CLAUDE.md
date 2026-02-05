# Capitol Traders - Development Guide

## Project Structure

Rust workspace with three crates:

- `capitoltrades_api/` -- Vendored fork of [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). HTTP client for the CapitolTrades BFF API. Modifications from upstream are minimal and documented below.
- `capitoltraders_lib/` -- Library layer: cached client wrapper, in-memory TTL cache, analysis helpers, error types.
- `capitoltraders_cli/` -- CLI binary (`capitoltraders`) using clap. Subcommands: `trades`, `politicians`, `issuers`.

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

Remaining clippy warnings in upstream code are intentionally left alone. Do not "fix" upstream code style without understanding why it exists (Chesterton's Fence).

## Test Inventory (36 tests)

| Suite | Count | Location |
|---|---|---|
| Upstream snapshot (insta) | 3 | `capitoltrades_api/src/query/*.rs` |
| Deserialization | 7 | `capitoltrades_api/tests/deserialization.rs` |
| Query builders | 11 | `capitoltrades_api/tests/query_builders.rs` |
| Wiremock integration | 5 | `capitoltrades_api/tests/client_integration.rs` |
| Cache unit | 5 | `capitoltraders_lib/src/cache.rs` |
| Analysis unit | 5 | `capitoltraders_lib/src/analysis.rs` |

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

## Adding a New CLI Subcommand

1. Create `capitoltraders_cli/src/commands/your_command.rs` with a clap `Args` struct and an `async fn run(...)`.
2. Add it to `commands/mod.rs`.
3. Add the variant to the `Commands` enum in `main.rs`.
4. Wire it into the `match` block in `main`.
