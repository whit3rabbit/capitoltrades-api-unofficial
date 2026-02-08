# Technology Stack: Detail-Page Enrichment

**Project:** Capitol Traders -- detail-page scraping enrichment
**Researched:** 2026-02-07
**Mode:** Ecosystem (stack dimension)

## Context

The existing scraper uses reqwest 0.12 for HTTP, regex for parsing Next.js RSC
payloads (embedded JSON in `self.__next_f.push` script tags), and serde_json for
deserialization. Detail-page methods (`trade_detail`, `politician_detail`,
`issuer_detail`) already exist but extract minimal fields. This research covers
what's needed to extend those extractors and run them at scale (thousands of
pages with throttling), without re-researching the existing stack.

## Recommended Stack

### Concurrency Control -- tokio::sync::Semaphore (already included via tokio)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| tokio::sync::Semaphore | 1.x (workspace) | Bounded concurrency for detail-page fetches | Already in the dependency tree. Lighter than adding governor for this use case. The existing retry/backoff logic in `ScrapeClient::with_retry` already handles 429/5xx; what's missing is concurrency bounding, not rate shaping. A semaphore with 3-5 permits is the right primitive. |

**Confidence:** HIGH -- tokio::sync::Semaphore is stable, well-documented, and
already available in the workspace dependency.

**Rationale for not using governor (0.10.2):** Governor implements GCRA (Generic
Cell Rate Algorithm) which is designed for steady-state rate shaping -- ensuring
requests are evenly spaced over time. The project already has exponential backoff
with jitter and Retry-After header parsing. Adding governor would mean two
overlapping rate-control mechanisms. A semaphore caps in-flight requests (e.g., 3
concurrent) while the existing retry logic handles server pushback. This is
simpler and avoids a new dependency for a problem that's already partially
solved.

### Concurrent Stream Processing -- futures::stream (StreamExt)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| futures | 0.3 | `StreamExt::buffer_unordered` for concurrent detail-page fetching | Enables processing detail pages concurrently up to N at a time. Combined with Semaphore, provides clean bounded-concurrency pipeline: stream of IDs -> map to fetch futures -> buffer_unordered(N) -> collect results. Order doesn't matter for enrichment. |

**Confidence:** HIGH -- futures 0.3.31 is the standard async stream library, used
universally with tokio.

**Pattern:**
```rust
use futures::stream::{self, StreamExt};
use tokio::sync::Semaphore;
use std::sync::Arc;

let semaphore = Arc::new(Semaphore::new(concurrency));
let results: Vec<_> = stream::iter(trade_ids)
    .map(|id| {
        let sem = semaphore.clone();
        let scraper = &scraper;
        async move {
            let _permit = sem.acquire().await.unwrap();
            scraper.trade_detail(id).await
        }
    })
    .buffer_unordered(concurrency)
    .collect()
    .await;
```

### Progress Reporting -- indicatif

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| indicatif | 0.17 | Progress bars for long-running detail-page enrichment | Detail enrichment on 1000+ trades at 250ms delay = 4+ minutes of waiting. Progress bars are essential UX. indicatif is the de facto standard for Rust CLIs, works well with tokio, and provides MultiProgress for showing per-entity-type progress. |

**Confidence:** HIGH -- indicatif 0.17.11 is mature, widely used, actively
maintained by the console-rs organization.

**Note on tracing-indicatif:** The project already uses `tracing` and
`tracing-subscriber`. The `tracing-indicatif` crate (bridges tracing spans to
progress bars automatically) is tempting but adds complexity for questionable
benefit here. The sync command has a clear sequential flow (pages then details)
that's easier to manage with explicit `ProgressBar` instances than implicit
span-based bars. Recommend direct indicatif usage.

### HTML Parsing -- NOT recommended: scraper crate

The `scraper` crate (0.25.0, built on html5ever/selectors from Servo) is the
standard Rust HTML parser, but it is **not useful here**. The project does not
parse HTML DOM trees. It extracts RSC (React Server Components) payloads from
`self.__next_f.push([1,"..."])` script blocks, decodes escaped JSON strings, and
then uses serde_json to deserialize structured data. This is fundamentally a
text-extraction + JSON-parsing problem, not an HTML-querying problem.

Adding `scraper` would mean:
1. Parse full HTML into a DOM tree (expensive, unnecessary)
2. Use CSS selectors to find `<script>` tags
3. Still need regex to extract the push payload content
4. Still need serde_json to parse the actual data

The existing approach (string search for the needle, escape-decode, serde_json)
is correct and efficient. The RSC payload contains all the structured data; the
HTML is just a delivery mechanism.

**Confidence:** HIGH -- verified by reading the existing `extract_rsc_payload`
implementation and understanding the RSC format.

### Cookie Handling -- reqwest cookies feature (conditional)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| reqwest (cookies feature) | 0.12 | Session cookie persistence if site starts requiring it | Currently the scraper works without cookies. However, scraping thousands of detail pages increases the chance of hitting anti-bot measures that rely on cookie-based session tracking. Adding `cookies` to reqwest features is zero-cost if unused and provides `.cookie_store(true)` as a fallback. |

**Confidence:** MEDIUM -- not currently needed, but cheap insurance. The cookies
feature is built into reqwest; no new crate required. Only add if scraping
starts failing due to session requirements.

### No New Parsing Libraries Needed

The enrichment work is primarily about:
1. Extending `extract_trade_detail` to pull more fields from the RSC payload
2. Extending `ScrapedTradeDetail` struct with new fields
3. Adding new `extract_*` helper functions following the existing pattern
4. Updating `upsert_scraped_trades` to persist the new fields

All of this uses existing dependencies: regex, serde_json, serde, rusqlite.

## Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| indicatif | 0.17 | Progress bars for long sync operations | Add to `capitoltraders_cli/Cargo.toml` when implementing detail enrichment in sync command |
| futures | 0.3 | StreamExt for concurrent detail fetching | Add to `capitoltraders_lib/Cargo.toml` when converting sequential detail fetching to concurrent |

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Concurrency control | tokio::sync::Semaphore | governor 0.10.2 | Existing retry/backoff handles server pushback; semaphore handles concurrency cap. Governor adds overlapping rate control and a new dependency. |
| HTML parsing | Existing regex + serde_json | scraper 0.25.0 | Site uses RSC payloads, not traditional HTML. DOM parsing adds overhead with no benefit. |
| Progress bars | indicatif 0.17 | tracing-indicatif 0.2 | Explicit progress bars are clearer for this sequential pipeline than span-based auto-bars. |
| Stream concurrency | futures::stream::buffer_unordered | tokio::task::JoinSet | JoinSet works but doesn't provide backpressure naturally. buffer_unordered caps in-flight futures cleanly. |
| Cookie persistence | reqwest cookies feature | reqwest_cookie_store 0.8 | Built-in reqwest cookies are sufficient; external cookie store only needed for cross-process persistence. |

## What NOT to Add

| Library | Why Not |
|---------|---------|
| **scraper** | RSC payloads are JSON-in-script-tags, not DOM-queryable HTML. The existing regex approach is correct. |
| **headless browser (thirtyfour, rust-headless-chrome)** | The site serves RSC payloads in initial HTML responses. No JS execution needed. Would add massive complexity for zero benefit. |
| **governor** | Over-engineered for this use case. Semaphore + existing retry covers the need. |
| **reqwest-middleware** | Middleware layering adds abstraction over the already-working retry logic. Not worth the refactor. |
| **backoff crate** | The existing `RetryConfig` with exponential backoff + jitter + Retry-After parsing is feature-complete. |
| **tower (Service trait)** | Tower's retry/rate-limit middleware is designed for server-side or client-side service meshes. Too heavy for a CLI scraper. |

## Installation

```toml
# In capitoltraders_lib/Cargo.toml -- add futures for stream concurrency
[dependencies]
futures = "0.3"

# In capitoltraders_cli/Cargo.toml -- add indicatif for progress bars
[dependencies]
indicatif = "0.17"

# Optional: enable cookies on existing reqwest if anti-bot measures require it
# In workspace Cargo.toml, add "cookies" to reqwest features:
# reqwest = { version = "0.12", default-features = false, features = ["gzip", "rustls-tls", "cookies"] }
```

## Version Verification

| Crate | Recommended | Latest Known | Verified Via |
|-------|-------------|--------------|-------------|
| futures | 0.3 | 0.3.31 | crates.io search (2026-02-07) |
| indicatif | 0.17 | 0.17.11 | docs.rs (2026-02-07) |
| governor | 0.10 (not recommended) | 0.10.2 | crates.io search (2026-02-07) |
| scraper | 0.25 (not recommended) | 0.25.0 | crates.io search (2026-02-07) |
| tokio | 1 (already used) | 1.x | workspace dependency |
| reqwest | 0.12 (already used) | 0.12.x | workspace dependency |

## Sources

- [tokio::sync::Semaphore docs](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html)
- [futures StreamExt docs](https://docs.rs/futures/latest/futures/stream/trait.StreamExt.html)
- [indicatif crate](https://crates.io/crates/indicatif)
- [governor crate](https://crates.io/crates/governor)
- [scraper crate](https://crates.io/crates/scraper)
- [reqwest cookie module](https://docs.rs/reqwest/latest/reqwest/cookie/index.html)
- [futures-rs on crates.io](https://crates.io/crates/futures)
- [Bounded concurrency patterns in Rust](https://medium.com/@jaderd/you-should-never-do-bounded-concurrency-like-this-in-rust-851971728cfb)
- [RSC Payload format analysis](https://edspencer.net/2024/7/1/decoding-react-server-component-payloads)
