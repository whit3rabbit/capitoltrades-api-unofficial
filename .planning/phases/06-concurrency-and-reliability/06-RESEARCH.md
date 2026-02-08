# Phase 6: Concurrency and Reliability - Research

**Researched:** 2026-02-08
**Domain:** Async concurrency, progress reporting, failure recovery in Rust/Tokio
**Confidence:** HIGH

## Summary

This phase transforms the sequential enrichment pipeline into a bounded-concurrent pipeline with progress reporting and automatic failure recovery. The current enrichment code (`enrich_trades`, `enrich_issuers`, `enrich_politician_committees` in `sync.rs`) processes items one at a time in a loop with a throttle delay between requests. For large datasets (thousands of trades/issuers), this is hours of wall-clock time.

The critical architectural constraint is that `rusqlite::Connection` is `Send` but NOT `Sync` (verified against v0.31.0 docs). This means `&Connection` is not `Send` and cannot be shared across spawned tasks. The concurrency design must separate HTTP fetching (parallelizable) from DB writing (serialized). The recommended pattern is: spawn N concurrent HTTP fetch tasks, collect results through a channel, and write to the database from the single task that owns the `Db`. This avoids any need for `tokio-rusqlite` or `Mutex<Connection>`.

The three requirements map cleanly to existing, well-supported Rust crates: `tokio::sync::Semaphore` for bounded concurrency (already a transitive dependency), `indicatif` for progress bars, and a hand-rolled circuit breaker using an `AtomicUsize` consecutive-failure counter (simpler and more appropriate than pulling in a full circuit breaker library for this use case).

**Primary recommendation:** Use `tokio::sync::Semaphore` + `tokio::task::JoinSet` for bounded-concurrent HTTP fetches, feed results back to the main task via channel for sequential DB writes, wrap progress in `indicatif::ProgressBar`, and implement a simple consecutive-failure circuit breaker as a ~30-line struct.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.49+ (workspace) | Semaphore, JoinSet, mpsc channel | Already in workspace; Semaphore and JoinSet are stable, first-party concurrency primitives |
| indicatif | 0.18.3 | Progress bars with ETA | De facto standard Rust progress bar library; 35M+ downloads; Send+Sync |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| std::sync::atomic | (stdlib) | AtomicUsize for circuit breaker counter | Consecutive failure tracking without locks |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled circuit breaker | failsafe-rs, circuitbreaker-rs | Overkill -- we only need consecutive-failure counting, not half-open states or time windows. Adding a dependency for ~30 lines of code is not justified. |
| tokio Semaphore + JoinSet | futures::stream::buffer_unordered | buffer_unordered is simpler for pure stream processing but harder to integrate with the DB-write-back pattern and circuit breaker state. JoinSet gives explicit task management. |
| tokio-rusqlite | Mutex\<Connection\> or channel-based writes | tokio-rusqlite requires rusqlite 0.37 (project uses 0.31). Upgrading rusqlite is out of scope for this phase. Channel-based write-back to the owning task is simpler. |

**Installation:**
```bash
# In capitoltraders_cli/Cargo.toml (indicatif is CLI-only)
cargo add indicatif@0.18 -p capitoltraders_cli
```

No new dependencies needed in `capitoltraders_lib` -- tokio (with Semaphore, JoinSet, mpsc) is already a workspace dependency.

## Architecture Patterns

### Recommended Concurrency Architecture

```
Main task (owns Db)
  |
  +-- get_unenriched_ids() --> Vec<i64>
  |
  +-- spawn N fetch tasks via JoinSet + Semaphore(3-5)
  |     |
  |     +-- task 1: acquire permit -> fetch HTML -> parse -> send result via mpsc -> drop permit
  |     +-- task 2: acquire permit -> fetch HTML -> parse -> send result via mpsc -> drop permit
  |     +-- task N: ...
  |
  +-- receive results from mpsc channel
  |     |
  |     +-- on Ok: db.update_trade_detail(id, detail)  [sequential, no contention]
  |     +-- on Err: increment circuit breaker counter
  |     +-- update progress bar
  |     +-- check circuit breaker: if tripped, abort remaining tasks
  |
  +-- progress_bar.finish()
```

### Pattern 1: Semaphore-Bounded JoinSet with Channel Write-Back

**What:** Spawn concurrent HTTP tasks that are throttled by a Semaphore, send results back to the main task through an mpsc channel for sequential DB writes.

**When to use:** When the bottleneck is I/O (HTTP requests) but the sink (DB) must be accessed sequentially.

**Why this pattern:** `rusqlite::Connection` is `Send` but NOT `Sync`. The `Db` struct wraps a `Connection`. Passing `&Db` to spawned tasks would require `Sync`. This pattern keeps `Db` on the main task and only parallelizes the HTTP fetching.

**Example:**
```rust
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

struct FetchResult {
    id: i64,
    result: Result<ScrapedTradeDetail, ScrapeError>,
}

async fn enrich_trades_concurrent(
    scraper: &ScrapeClient,  // ScrapeClient is Clone-able (wraps Arc<reqwest::Client>... need to verify)
    db: &Db,
    queue: Vec<i64>,
    concurrency: usize,      // 3-5
    throttle_ms: u64,
) -> Result<EnrichmentResult> {
    let total = queue.len();
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let (tx, mut rx) = mpsc::channel::<FetchResult>(concurrency * 2);

    let mut join_set = JoinSet::new();

    // Spawn all fetch tasks
    for id in queue {
        let sem = semaphore.clone();
        let sender = tx.clone();
        // scraper needs to be cloneable or wrapped in Arc
        let scraper_clone = scraper.clone(); // <-- ScrapeClient needs Clone

        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            if throttle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
            }

            let result = scraper_clone.trade_detail(id).await;
            let _ = sender.send(FetchResult { id, result }).await;
        });
    }

    // Drop the original sender so rx closes when all tasks complete
    drop(tx);

    let mut enriched = 0usize;
    let mut failed = 0usize;
    let mut circuit_breaker = CircuitBreaker::new(5); // trip after 5 consecutive failures

    while let Some(fetch) = rx.recv().await {
        match fetch.result {
            Ok(detail) => {
                db.update_trade_detail(fetch.id, &detail)?;
                enriched += 1;
                circuit_breaker.record_success();
            }
            Err(err) => {
                eprintln!("  Warning: trade {} failed: {}", fetch.id, err);
                failed += 1;
                circuit_breaker.record_failure();
            }
        }

        if circuit_breaker.is_tripped() {
            eprintln!("Circuit breaker tripped after {} consecutive failures", circuit_breaker.threshold);
            join_set.abort_all();
            break;
        }

        // progress_bar.inc(1);
    }

    Ok(EnrichmentResult { enriched, skipped: 0, failed, total })
}
```

### Pattern 2: Simple Circuit Breaker (Consecutive Failures)

**What:** A lightweight struct that counts consecutive failures and trips after N in a row.

**When to use:** When you want to stop wasting requests against a server that's clearly down or rate-limiting you.

**Example:**
```rust
/// Circuit breaker that trips after N consecutive failures.
/// NOT a full circuit breaker with half-open state -- just a kill switch.
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize,
}

impl CircuitBreaker {
    fn new(threshold: usize) -> Self {
        Self {
            consecutive_failures: 0,
            threshold,
        }
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
    }

    fn is_tripped(&self) -> bool {
        self.consecutive_failures >= self.threshold
    }
}
```

### Pattern 3: Progress Bar with indicatif

**What:** A real terminal progress bar replacing the current `eprintln!` progress logging.

**When to use:** During enrichment loops to show position, total, and ETA.

**Example:**
```rust
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total as u64);
pb.set_style(
    ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}"
    )
    .unwrap()
    .progress_chars("##-"),
);

// In the processing loop:
pb.set_message(format!("{} ok, {} failed", enriched, failed));
pb.inc(1);

// After loop:
pb.finish_with_message(format!("done: {} enriched, {} failed", enriched, failed));
```

### Pattern 4: ScrapeClient Must Be Cloneable

**What:** The current `ScrapeClient` wraps a `reqwest::Client` which is already cheaply cloneable (it uses Arc internally). `ScrapeClient` must derive or implement `Clone` to be shared across spawned tasks.

**Current state:** `ScrapeClient` does NOT implement `Clone`. It has two fields: `base_url: String` and `http: reqwest::Client`. Both are cloneable.

**Fix:** Either `#[derive(Clone)]` on `ScrapeClient`, or wrap it in `Arc<ScrapeClient>`.

**Recommendation:** `#[derive(Clone)]` is simpler since `reqwest::Client` is already `Arc`-backed internally. Cloning `ScrapeClient` is cheap.

### Anti-Patterns to Avoid

- **Wrapping Db in Arc\<Mutex\<Db\>\>:** Tempting but wrong. SQLite with WAL mode does support concurrent reads, but `Mutex<Db>` would serialize all DB access including reads, and adds unnecessary complexity. Keep Db on the main task.
- **Using tokio::spawn with &Db:** Will not compile. `&Db` (which holds `&Connection`) is not Send because Connection is not Sync. Don't fight the borrow checker here; use channels.
- **Spawning a task per item without a Semaphore:** Would launch hundreds/thousands of concurrent requests, overwhelming the target server and getting rate-limited or banned.
- **Using buffer_unordered for the full pipeline:** The `.for_each()` after `buffer_unordered` would need DB access, which requires the Connection. Since Connection is not Sync, the closure can't borrow it across the stream. Use JoinSet + channel instead.
- **Forgetting the throttle delay:** The existing 500ms default throttle is per-request, not per-batch. With concurrency=5, you'd send 5 requests instantly then wait. Consider applying the throttle inside each task BEFORE the request to stagger launches, or accept the burst-then-wait pattern.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Progress bars | Custom eprintln progress | indicatif::ProgressBar | Terminal handling, ETA calculation, rate smoothing, multi-bar support, stderr-safe |
| Bounded concurrency | Custom task counting / manual future polling | tokio::sync::Semaphore + JoinSet | Race conditions, permit leaks on panics, fairness guarantees |
| Full circuit breaker with half-open | Complex state machine with timers | N/A (don't need it) | The requirement is just "stop after N consecutive failures." A full state machine is overkill. |

**Key insight:** The circuit breaker IS worth hand-rolling because the requirement is trivially simple (consecutive failure counter). But progress bars and semaphore-based concurrency are absolutely NOT worth hand-rolling -- they have subtle edge cases that libraries handle correctly.

## Common Pitfalls

### Pitfall 1: rusqlite Connection is not Sync
**What goes wrong:** Attempting to share `&Db` across spawned tasks causes a compile error: "future cannot be sent between threads safely."
**Why it happens:** `rusqlite::Connection` has `unsafe impl Send` but is `!Sync` (contains `RefCell`). A shared reference `&T` is `Send` only if `T: Sync`.
**How to avoid:** Keep `Db` on the main task. Send fetch results back through an mpsc channel. Only the main task touches the database.
**Warning signs:** Compiler error about `Send` bounds on `tokio::spawn`.

### Pitfall 2: Semaphore Permit Leak on Task Panic
**What goes wrong:** If a spawned task panics without dropping its permit, that permit is permanently lost, reducing effective concurrency.
**Why it happens:** `SemaphorePermit` implements `Drop` which releases the permit, but only if the task unwinds normally. If the panic is caught by the JoinSet, the permit's drop runs.
**How to avoid:** Let JoinSet handle panics (it catches them in `join_next`). The permit will be dropped when the task's future is dropped. Don't use `std::mem::forget` on permits.
**Warning signs:** Concurrency gradually decreasing over long runs.

### Pitfall 3: Circuit Breaker Tripping on Transient Errors
**What goes wrong:** A brief network blip causes 5 consecutive failures, tripping the breaker even though the server is fine.
**Why it happens:** The existing retry logic in `ScrapeClient::with_retry` already handles transient errors with exponential backoff. If a request fails AFTER retries, it's genuinely failing.
**How to avoid:** Set threshold to 5+ (not too aggressive). The existing retry logic (3 retries with exponential backoff up to 30s) already handles transient issues. A post-retry failure is a strong signal.
**Warning signs:** Circuit breaker tripping too early in runs.

### Pitfall 4: Throttle Delay Behavior Changes with Concurrency
**What goes wrong:** With sequential processing, a 500ms delay between requests means ~2 req/s. With concurrency=5 and 500ms delay per task, you get ~10 req/s (each of 5 tasks waits 500ms independently).
**Why it happens:** The delay is per-task, not global.
**How to avoid:** Decide whether the delay should be per-task or global. For rate limiting against a web server, a global rate limiter is better. Option: use the Semaphore itself as the throttle (release permits on a timer), or accept the burst-then-stagger pattern.
**Warning signs:** Getting rate-limited (429s) more frequently after adding concurrency.

### Pitfall 5: Progress Bar Output Conflicts with eprintln
**What goes wrong:** `eprintln!` writes to stderr, as does `indicatif::ProgressBar`. Mixing them causes garbled output with progress bars overwriting log lines.
**Why it happens:** indicatif manages its own terminal cursor position. Raw `eprintln!` breaks its assumptions.
**How to avoid:** Use `progress_bar.println()` or `progress_bar.suspend()` for any log output during progress bar display. Replace all `eprintln!` in the enrichment functions with `pb.println()` or `pb.set_message()`.
**Warning signs:** Garbled terminal output, progress bar appearing on multiple lines.

### Pitfall 6: JoinSet Abort Semantics
**What goes wrong:** After `join_set.abort_all()`, you still need to drain the JoinSet to avoid resource leaks. Or the mpsc channel may have buffered results that arrived before abort.
**Why it happens:** `abort_all()` cancels tasks but doesn't remove them from the set. The mpsc channel buffer may still have pending items.
**How to avoid:** After `abort_all()`, continue draining `rx.recv()` until it returns `None` (all senders dropped). Or just drop the JoinSet (its Drop impl aborts remaining tasks).
**Warning signs:** Hanging after circuit breaker trips.

## Code Examples

### Complete Enrichment Function Skeleton

```rust
use std::sync::Arc;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

/// Result of a single fetch operation, sent back through the channel.
struct FetchResult<T> {
    id: i64,
    result: Result<T, ScrapeError>,
}

/// Circuit breaker: trips after N consecutive failures.
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize,
}

impl CircuitBreaker {
    fn new(threshold: usize) -> Self {
        Self { consecutive_failures: 0, threshold }
    }
    fn record_success(&mut self) { self.consecutive_failures = 0; }
    fn record_failure(&mut self) { self.consecutive_failures += 1; }
    fn is_tripped(&self) -> bool { self.consecutive_failures >= self.threshold }
}

async fn enrich_trades(
    scraper: &ScrapeClient,
    db: &Db,
    queue: Vec<i64>,
    concurrency: usize,
    throttle_ms: u64,
    circuit_breaker_threshold: usize,
) -> Result<EnrichmentResult> {
    let total = queue.len();

    // Progress bar
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}"
        )
        .unwrap()
        .progress_chars("##-"),
    );
    pb.set_message("enriching trades...");

    // Concurrency control
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let (tx, mut rx) = mpsc::channel::<FetchResult<ScrapedTradeDetail>>(concurrency * 2);
    let mut join_set = JoinSet::new();

    // Spawn fetch tasks
    for id in queue {
        let sem = semaphore.clone();
        let sender = tx.clone();
        let scraper = scraper.clone(); // ScrapeClient must implement Clone

        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            if throttle_ms > 0 {
                tokio::time::sleep(Duration::from_millis(throttle_ms)).await;
            }
            let result = scraper.trade_detail(id).await;
            let _ = sender.send(FetchResult { id, result }).await;
        });
    }
    drop(tx); // Close sender so rx.recv() returns None when all tasks complete

    // Receive and write results sequentially
    let mut enriched = 0usize;
    let mut failed = 0usize;
    let mut breaker = CircuitBreaker::new(circuit_breaker_threshold);

    while let Some(fetch) = rx.recv().await {
        match fetch.result {
            Ok(detail) => {
                db.update_trade_detail(fetch.id, &detail)?;
                enriched += 1;
                breaker.record_success();
            }
            Err(err) => {
                pb.println(format!("  Warning: trade {} failed: {}", fetch.id, err));
                failed += 1;
                breaker.record_failure();
            }
        }

        pb.set_message(format!("{} ok, {} err", enriched, failed));
        pb.inc(1);

        if breaker.is_tripped() {
            pb.println(format!(
                "Circuit breaker: {} consecutive failures, stopping enrichment",
                breaker.threshold
            ));
            join_set.abort_all();
            break;
        }
    }

    pb.finish_with_message(format!("done: {} enriched, {} failed", enriched, failed));

    Ok(EnrichmentResult {
        enriched,
        skipped: 0,
        failed,
        total,
    })
}
```

### indicatif ProgressStyle Templates

```rust
// Simple enrichment progress bar
"[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}"

// Spinner for indeterminate progress (e.g., committee pages)
"{spinner:.green} [{elapsed_precise}] {msg}"

// Compact for terminal-constrained display
"{bar:30} {pos}/{len} {msg}"
```

### Making ScrapeClient Cloneable

```rust
// In capitoltraders_lib/src/scrape.rs
#[derive(Clone)]  // reqwest::Client is Arc-backed, String is Clone
pub struct ScrapeClient {
    base_url: String,
    http: reqwest::Client,
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| futures::stream::buffer_unordered | tokio::task::JoinSet | tokio 1.20+ (2022) | JoinSet is now preferred for managing collections of spawned tasks; gives abort_all(), join_next() |
| indicatif 0.16 | indicatif 0.18 | 2024 | Template syntax via ProgressStyle::with_template(), improved multi-bar support |
| Custom task tracking with JoinHandle vec | JoinSet | tokio 1.20+ | JoinSet manages the collection internally, handles panics, supports abort_all |

**Deprecated/outdated:**
- `futures::stream::FuturesUnordered` for task management: Still works but JoinSet is the idiomatic Tokio approach for spawned tasks (FuturesUnordered is better for non-spawned futures)
- indicatif 0.16 API: `ProgressBar::set_style` with string format changed to `ProgressStyle::with_template` in 0.17+

## Open Questions

1. **Throttle delay semantics with concurrency**
   - What we know: Current 500ms delay is between sequential requests. With concurrency=5, behavior changes.
   - What's unclear: Should we maintain the same total request rate (~2/s) or allow higher throughput (up to ~10/s with 5 concurrent)?
   - Recommendation: Keep the per-task throttle delay for now (the existing retry/backoff handles 429s). The user can increase `--details-delay-ms` if they get rate-limited. Document the behavior change.

2. **ScrapeClient Clone vs Arc**
   - What we know: `reqwest::Client` is internally `Arc`-backed, so cloning is cheap. `ScrapeClient` has `base_url: String` and `http: reqwest::Client`.
   - What's unclear: Whether adding `#[derive(Clone)]` to `ScrapeClient` is considered modifying vendored code (it's in `capitoltraders_lib`, not `capitoltrades_api`).
   - Recommendation: `ScrapeClient` is in `capitoltraders_lib/src/scrape.rs` (NOT the vendored crate). Adding `Clone` is fine. Alternatively, wrap in `Arc<ScrapeClient>` to avoid any Clone derive, but that's unnecessary overhead.

3. **Committee enrichment concurrency**
   - What we know: Committee enrichment iterates over 48 committees sequentially, takes ~25s total. It's fundamentally different from trade/issuer enrichment (iterates over a fixed known set, not a dynamic queue).
   - What's unclear: Whether it needs concurrency at all given it's already fast enough.
   - Recommendation: Apply Semaphore concurrency only to trade and issuer enrichment. Committee enrichment stays sequential -- it's only 48 requests and concurrency would add complexity for minimal gain. Progress bar is still useful though.

4. **CLI argument for concurrency level**
   - What we know: The requirement says 3-5 parallel requests.
   - What's unclear: Should this be configurable via `--concurrency N` or hardcoded?
   - Recommendation: Add `--concurrency` flag with default 3, range 1-10. This follows the pattern of `--details-delay-ms` and `--batch-size` being configurable.

5. **CLI argument for circuit breaker threshold**
   - What we know: Requirement says "N consecutive failures."
   - What's unclear: What N should be.
   - Recommendation: Default 5, configurable via `--max-consecutive-failures N` or similar. 5 is reasonable given the existing 3-retry exponential backoff -- 5 post-retry failures = 15+ actual HTTP failures.

## Sources

### Primary (HIGH confidence)
- [tokio::sync::Semaphore docs](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html) - API, fairness guarantees, permit semantics. Tokio 1.49.0.
- [tokio::task::JoinSet docs](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html) - spawn, join_next, abort_all. Tokio 1.49.0.
- [indicatif docs](https://docs.rs/indicatif/latest/indicatif/) - ProgressBar, ProgressStyle, template syntax. Version 0.18.3.
- rusqlite 0.31.0 generated docs (local cargo doc) - Confirmed `impl Send for Connection`, `!Sync for Connection`.
- Codebase inspection: `sync.rs`, `scrape.rs`, `db.rs`, `client.rs`, `lib.rs`, workspace `Cargo.toml`.

### Secondary (MEDIUM confidence)
- [rusqlite/rusqlite#1013](https://github.com/rusqlite/rusqlite/issues/1013) - Discussion of Connection thread safety, recommended patterns.
- [tokio-rusqlite docs](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/) - Alternative approach (requires rusqlite 0.37, not usable here).
- [Rust Concurrency Patterns (OneSignal blog)](https://onesignal.com/blog/rust-concurrency-patterns/) - buffer_unordered vs Semaphore comparison.

### Tertiary (LOW confidence)
- [failsafe-rs](https://github.com/dmexe/failsafe-rs) - Full circuit breaker library. Not recommended for this use case but documented as alternative.
- [circuitbreaker-rs](https://docs.rs/circuitbreaker-rs) - Another circuit breaker library. Same assessment.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - tokio Semaphore/JoinSet verified in official docs, indicatif verified on docs.rs/crates.io
- Architecture: HIGH - rusqlite Send/!Sync verified via local cargo doc, concurrency pattern well-established
- Pitfalls: HIGH - Connection thread safety is the #1 pitfall and fully verified; progress bar conflicts documented in indicatif docs

**Research date:** 2026-02-08
**Valid until:** 2026-03-08 (stable libraries, unlikely to change significantly)
