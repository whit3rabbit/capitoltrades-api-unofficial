# Phase 10: Donation Sync Pipeline - Research

**Researched:** 2026-02-12
**Domain:** Concurrent data pipeline with keyset pagination and resumable state
**Confidence:** HIGH

## Summary

Phase 10 implements a concurrent donation sync pipeline that fetches FEC Schedule A contribution data for politicians' committees and stores it in SQLite. This phase directly reuses the battle-tested Semaphore + JoinSet + mpsc pattern from price enrichment (Phase 4), with the addition of keyset cursor pagination state management for resumability. The OpenFEC API's Schedule A endpoint uses keyset pagination (not page numbers) via last_index + last_contribution_receipt_date cursors, which map cleanly to our existing donation_sync_meta table (added in Phase 9, schema v4).

The core technical challenge is cursor state persistence for resume-after-interruption. The pattern is straightforward: persist cursor values (last_index, last_contribution_receipt_date) in donation_sync_meta after each successful page fetch, and load them before starting the next page request. This is simpler than price enrichment's deduplication strategy because Schedule A provides stable cursor values from the API response's pagination.last_indexes field.

**Primary recommendation:** Mirror enrich_prices.rs structure exactly (CircuitBreaker, Semaphore, JoinSet, mpsc channel, progress bar) but replace the two-phase deduplication logic with single-phase keyset pagination loops. Store cursor state in donation_sync_meta after each page completes. Use ON CONFLICT IGNORE for sub_id deduplication at the database layer rather than in-memory tracking.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.x (existing) | Async runtime with Semaphore, JoinSet, mpsc | Already in use for enrich_prices, proven for concurrent API pipelines |
| rusqlite | 0.x (existing) | SQLite database operations with transaction support | Existing DB layer, unchecked_transaction for batch writes |
| reqwest | 0.x (existing) | HTTP client for OpenFEC API | Already in OpenFecClient, battle-tested |
| indicatif | 0.x (existing) | Progress bar for sync feedback | Used in enrich_prices for user-facing progress |
| rand | 0.8.5 (existing) | Jittered delay for rate limiting | Used in enrich_prices for randomized backoff |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde_json | 1.x (existing) | JSON serialization for storing committee_ids in fec_mappings | Already used for API response parsing |
| chrono | 0.x (existing) | Date/time parsing for contribution_receipt_date | Already used throughout project for date handling |
| anyhow | 1.x (existing) | Error handling with context | Existing error handling pattern in CLI commands |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| JoinSet | FuturesUnordered | JoinSet is simpler and handles task cancellation better (circuit breaker abort_all) |
| mpsc channel | Direct DB writes from tasks | mpsc enables single-threaded DB writes (SQLite requirement), prevents Connection not Send+Sync errors |
| Cursor persistence in DB | In-memory state | DB persistence enables resume-after-interruption; in-memory loses progress on Ctrl+C |

**Installation:**
No new dependencies required. All libraries already in Cargo.toml from Phases 1-9.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_cli/src/commands/
├── sync_donations.rs    # New: donation sync command (mirrors enrich_prices.rs)
└── enrich_prices.rs     # Existing: pattern to mirror for concurrent pipeline
```

### Pattern 1: Keyset Pagination Loop with Cursor Persistence

**What:** Fetch paginated data using keyset cursors (last_index + last_contribution_receipt_date), persisting cursor state after each successful page to enable resume.

**When to use:** Any API that uses keyset/cursor pagination instead of page numbers, especially when fetching large datasets that may be interrupted.

**Example:**
```rust
// Keyset pagination loop (per committee)
let mut cursor: Option<(i64, String)> = None;

loop {
    // Build query with cursor if resuming
    let mut query = ScheduleAQuery::default()
        .with_committee_id(&committee_id)
        .with_cycle(cycle)
        .with_per_page(batch_size);

    if let Some((last_idx, last_date)) = &cursor {
        query = query
            .with_last_index(*last_idx)
            .with_last_contribution_receipt_date(last_date);
    }

    // Fetch page from API
    let response = client.get_schedule_a(&query).await?;

    // Insert contributions (ON CONFLICT IGNORE for sub_id deduplication)
    for contrib in &response.results {
        db.insert_donation(contrib)?;
    }

    // Persist cursor state for resume
    if let Some(ref indexes) = response.pagination.last_indexes {
        cursor = Some((indexes.last_index, indexes.last_contribution_receipt_date.clone()));
        db.update_sync_cursor(politician_id, &committee_id, cursor)?;
    } else {
        // No more pages, exit loop
        break;
    }
}
```

### Pattern 2: Concurrent Committee Fetch with Semaphore Backpressure

**What:** Spawn concurrent tasks for each committee, bounded by Semaphore to limit concurrency, using mpsc channel for serialized DB writes.

**When to use:** When fetching data for multiple entities (committees) concurrently while respecting rate limits and DB write serialization.

**Example:**
```rust
// Source: Existing enrich_prices.rs pattern, adapted for committees
const CONCURRENCY: usize = 3;  // Lower than price enrichment (5) due to OpenFEC rate limits
let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
let (tx, mut rx) = mpsc::channel::<DonationResult>(CONCURRENCY * 2);
let mut join_set = JoinSet::new();

for committee_id in committee_ids {
    let sem = Arc::clone(&semaphore);
    let sender = tx.clone();
    let client = Arc::clone(&openfec_client);

    join_set.spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");

        // Rate limiting with jittered delay (200-500ms per enrich_prices pattern)
        let delay_ms = rand::thread_rng().gen_range(200..500);
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        // Keyset pagination loop for this committee
        let donations = fetch_donations_for_committee(&client, &committee_id, cycle, batch_size).await;
        let _ = sender.send((committee_id, donations)).await;
    });
}
drop(tx);  // Close channel so receiver knows when all tasks are done

// Single-threaded DB writes from receiver
while let Some((committee_id, result)) = rx.recv().await {
    match result {
        Ok(donations) => {
            for donation in donations {
                db.insert_donation(&donation)?;
            }
        }
        Err(e) => {
            circuit_breaker.record_failure();
            if circuit_breaker.is_tripped() {
                join_set.abort_all();
                break;
            }
        }
    }
}
```

### Pattern 3: CircuitBreaker for 429 Rate Limit Handling

**What:** Track consecutive failures and halt processing after threshold (5 for OpenFEC per requirements) to avoid burning API budget on persistent rate limiting.

**When to use:** Any API integration with rate limits, especially when multiple requests are in flight concurrently.

**Example:**
```rust
// Source: Existing enrich_prices.rs CircuitBreaker struct
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize,
}

impl CircuitBreaker {
    fn new(threshold: usize) -> Self {
        Self { consecutive_failures: 0, threshold }
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

// Usage in receiver loop
match fetch_result {
    Ok(_) => breaker.record_success(),
    Err(OpenFecError::RateLimited) => {
        breaker.record_failure();
        if breaker.is_tripped() {
            pb.println("Circuit breaker tripped after 5 consecutive 429 errors, stopping sync");
            join_set.abort_all();
            break;
        }
    }
}
```

### Pattern 4: Incremental Sync with Min-Date Checkpointing

**What:** On subsequent syncs, use the most recent contribution date from the DB as a starting point, fetching only new donations plus a 90-day overlap window to catch late-arriving data.

**When to use:** When syncing time-series data that may arrive out of order or be backdated.

**Example:**
```rust
// Query most recent donation date for this politician
let min_date = db.get_most_recent_donation_date(politician_id)?;

// Apply min_date filter with 90-day overlap window
let query = ScheduleAQuery::default()
    .with_committee_id(&committee_id)
    .with_cycle(cycle)
    .with_min_date(min_date.map(|d| d - Duration::days(90)));
```

**Note:** OpenFEC Schedule A endpoint supports min_date and max_date filters. Use these to narrow the sync window and avoid refetching all historical data on every run.

### Anti-Patterns to Avoid

- **Storing Connection in Arc directly:** rusqlite::Connection is not Send+Sync. Use Arc<Mutex<Db>> or pass &Db references within synchronous code paths only. For concurrent writes, use mpsc channel pattern.
- **Retrying 429 errors without backoff:** Circuit breaker should trip after threshold, not retry indefinitely. Each retry burns API budget and delays progress.
- **Ignoring pagination.last_indexes == None:** This signals the end of results. Continuing to loop will either repeat the last page or error.
- **Forgetting to drop(tx) before receiver loop:** If all senders aren't dropped, rx.recv() will hang waiting for more messages after tasks complete.
- **Updating cursor state before DB insert completes:** Cursor should only be persisted after donations are successfully written. Otherwise, interruption causes data loss.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Concurrent task management | Manual thread pool or Vec of spawned tasks | tokio::task::JoinSet | Handles task cancellation (abort_all), automatic cleanup, and ergonomic result collection |
| Rate limiting | Manual sleep() + atomic counter | tokio::sync::Semaphore with jittered delay | Prevents thundering herd, provides backpressure, integrates with async runtime |
| Progress reporting | println! with counters | indicatif::ProgressBar | Thread-safe, spinner animation, ETA calculation, proper terminal handling |
| Cursor state serialization | Custom JSON/binary format | Store as TEXT fields in donation_sync_meta | SQLite natively supports compound primary keys and atomic updates |
| Date parsing for contribution_receipt_date | Regex or split() | chrono::NaiveDate::parse_from_str | Handles edge cases (leap years, invalid dates), standardized format |

**Key insight:** The concurrent pipeline pattern has already been validated in production through enrich_prices (Phase 4). Don't reinvent it. The only novelty in Phase 10 is keyset pagination, which is simpler than price enrichment's two-phase deduplication because the API provides cursor values directly in the response.

## Common Pitfalls

### Pitfall 1: Cursor State Desync (Critical)

**What goes wrong:** Cursor state is updated in donation_sync_meta before donations are written to the DB. If the process is interrupted after cursor update but before DB commit, subsequent runs skip those donations.

**Why it happens:** Natural instinct is to update state immediately after receiving the API response, but this creates a transaction ordering problem.

**How to avoid:**
- Wrap donation inserts and cursor update in a single rusqlite transaction
- Pattern: `tx = conn.unchecked_transaction()? -> insert donations -> update cursor -> tx.commit()?`
- Never update cursor before transaction commit

**Warning signs:**
- Gap in donation dates after interrupted sync
- Decreasing total donation counts after resume

### Pitfall 2: Empty Results Cached as "Completed"

**What goes wrong:** Committee with no donations returns empty results on first page. Cursor is not updated (pagination.last_indexes == None). Next sync repeats the same query, hitting API unnecessarily.

**Why it happens:** Empty results are indistinguishable from "sync complete" state in the current schema.

**How to avoid:**
- Store a `completed_at` timestamp in donation_sync_meta when pagination.last_indexes == None
- Check completed_at before starting sync; if recent (e.g., < 24 hours), skip API call
- This prevents repeated empty fetches for committees with no donations

**Warning signs:**
- Identical API requests on every sync run
- Committee with zero donations shows in progress logs every time

### Pitfall 3: Circuit Breaker Threshold Too Low for Concurrent Tasks

**What goes wrong:** With concurrency = 3, three tasks hitting 429 simultaneously can trip circuit breaker after just 2 rounds instead of the expected 5 consecutive failures.

**Why it happens:** "Consecutive" is measured per-committee-task, not globally. If all 3 concurrent tasks fail on round 1 and round 2, that's 6 failures, but circuit breaker only sees 2 per task.

**How to avoid:**
- Track consecutive failures globally (in receiver loop), not per-task
- Each 429 increments global counter; each success resets it
- Circuit breaker threshold = 5 means "5 consecutive 429s from ANY committee"

**Warning signs:**
- Circuit breaker trips with message "after 5 consecutive failures" but logs show only 2-3 actual 429 errors

### Pitfall 4: Sub-ID Collision Across Election Cycles

**What goes wrong:** FEC sub_id is unique per contribution but may be reused across cycles (unconfirmed). Using sub_id as PRIMARY KEY without cycle creates false duplicates.

**Why it happens:** Assumption that sub_id is globally unique forever, but FEC data model is cycle-based.

**How to avoid:**
- Schema already uses sub_id TEXT PRIMARY KEY (from Phase 9)
- Store election_cycle INTEGER on donations table (already present)
- ON CONFLICT IGNORE on sub_id works because true duplicates share sub_id
- If collision confirmed, change PK to (sub_id, election_cycle) in future migration

**Warning signs:**
- Same contributor+employer+amount+date appears twice in different cycles
- Donation counts lower than expected after multi-cycle sync

### Pitfall 5: Not Handling Missing committee_ids on fec_mappings

**What goes wrong:** CommitteeResolver returns committees, but fec_mappings.committee_ids is NULL or empty JSON array. Sync command finds no committees to sync, exits early with "no committees found."

**Why it happens:** Politician was added to DB before Phase 9 committee resolution, or API call failed during previous sync-fec run.

**How to avoid:**
- Check fec_mappings.committee_ids before starting donation sync
- If empty or NULL, call CommitteeResolver.resolve_committees() to populate
- Log informative message: "Resolving committees for {politician}..."

**Warning signs:**
- Sync exits immediately with "no committees found" for valid politicians
- fec_mappings table has rows but committee_ids field is NULL

## Code Examples

Verified patterns from official sources and existing codebase:

### Keyset Pagination Response Handling

```rust
// Source: OpenFEC API ScheduleAResponse type (capitoltraders_lib/src/openfec/types.rs)
// and keyset pagination documentation (https://api.open.fec.gov/developers/)
let response = client.get_schedule_a(&query).await?;

// Process results
for contribution in &response.results {
    db.insert_donation(contribution)?;
    synced_count += 1;
}

// Extract cursor for next page
let cursor = match response.pagination.last_indexes {
    Some(indexes) => Some((
        indexes.last_index,
        indexes.last_contribution_receipt_date.clone(),
    )),
    None => {
        // No more pages, mark sync as completed
        db.mark_sync_completed(politician_id, &committee_id)?;
        break;
    }
};
```

### Transaction-Wrapped Batch Insert with Cursor Update

```rust
// Source: Existing db.rs unchecked_transaction pattern
// Ensures atomicity: either all donations + cursor update succeed, or none
let tx = db.conn().unchecked_transaction()?;

// Insert donations with ON CONFLICT IGNORE for deduplication
let mut stmt = tx.prepare(
    "INSERT OR IGNORE INTO donations (
        sub_id, committee_id, contributor_name, contributor_employer,
        contributor_occupation, contributor_state, contribution_receipt_amount,
        contribution_receipt_date, election_cycle
    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
)?;

for contrib in &contributions {
    stmt.execute(params![
        contrib.sub_id,
        committee_id,
        contrib.contributor_name,
        contrib.contributor_employer,
        contrib.contributor_occupation,
        contrib.contributor_state,
        contrib.contribution_receipt_amount,
        contrib.contribution_receipt_date,
        cycle,
    ])?;
}
drop(stmt);

// Update cursor state (same transaction)
tx.execute(
    "INSERT OR REPLACE INTO donation_sync_meta (
        politician_id, committee_id, last_index, last_contribution_receipt_date,
        last_synced_at, total_synced
    ) VALUES (?1, ?2, ?3, ?4, datetime('now'), ?5)",
    params![
        politician_id,
        committee_id,
        cursor.0,  // last_index
        cursor.1,  // last_contribution_receipt_date
        synced_count,
    ],
)?;

tx.commit()?;
```

### Loading Cursor State for Resume

```rust
// Source: donation_sync_meta table schema (schema/sqlite.sql)
// Query cursor state before starting pagination loop
let cursor: Option<(i64, String)> = db.conn()
    .query_row(
        "SELECT last_index, last_contribution_receipt_date
         FROM donation_sync_meta
         WHERE politician_id = ?1 AND committee_id = ?2",
        params![politician_id, committee_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?;

// Use cursor if resuming, otherwise start from beginning
let mut query = ScheduleAQuery::default()
    .with_committee_id(committee_id)
    .with_cycle(cycle);

if let Some((last_idx, last_date)) = cursor {
    query = query
        .with_last_index(last_idx)
        .with_last_contribution_receipt_date(&last_date);
}
```

### Progress Bar with Multi-Committee Tracking

```rust
// Source: Existing enrich_prices.rs indicatif pattern
use indicatif::{ProgressBar, ProgressStyle};

let total_committees = committees.len();
let pb = ProgressBar::new(total_committees as u64);
pb.set_style(
    ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
    )
    .unwrap(),
);
pb.set_message("syncing donations...");

// Update progress in receiver loop
pb.set_message(format!("{} donations synced", total_donations));
pb.inc(1);  // Increment per committee completed

pb.finish_with_message(format!(
    "Sync complete: {} donations synced for {} committees",
    total_donations, total_committees
));
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Offset pagination (page numbers) | Keyset pagination (cursor values) | FEC API design (2015+) | Stable pagination even when data is updated during sync; no duplicate/missing records |
| Thread pools for concurrency | tokio JoinSet + Semaphore | Tokio 1.x (2021) | Better async/await integration, automatic cleanup, bounded concurrency |
| Manual transaction commit | unchecked_transaction() for reads | rusqlite pattern | Faster read-heavy transactions, explicit commit control |
| Per-task failure tracking | Global circuit breaker | Learned from enrich_prices (Phase 4) | Prevents concurrent tasks from bypassing threshold |

**Deprecated/outdated:**
- **Naive offset pagination for large datasets:** OpenFEC Schedule A endpoint explicitly does NOT support page parameter. Keyset pagination is mandatory.
- **Blocking DB operations in async tasks:** Causes executor starvation. Use mpsc channel to serialize DB writes on dedicated receiver task.
- **Infinite retry on 429 errors:** Burns API budget. Circuit breaker with threshold (5 consecutive) is the modern pattern.

## Open Questions

1. **Should sync filter by contributor state or employer to reduce data volume?**
   - What we know: Schedule A endpoint supports contributor_state and contributor_employer filters
   - What's unclear: Requirements don't specify filtering. Should we sync ALL donations or only from specific states/employers?
   - Recommendation: Start with ALL donations (no filters). Add --state and --employer filters in Phase 11 (donations CLI command) instead. Syncing everything enables ad-hoc analysis later.

2. **How should we handle contributions with NULL sub_id?**
   - What we know: sub_id is Option<String> per OpenFEC API types. NULL sub_id means the record lacks a unique identifier.
   - What's unclear: Can we safely skip these, or do we need a fallback PK strategy (e.g., hash of contributor+date+amount)?
   - Recommendation: Skip NULL sub_id contributions with warning log. These are rare edge cases (data quality issues). If they become common, add fallback PK in Phase 11.

3. **What's the optimal batch size (per_page) for Schedule A queries?**
   - What we know: OpenFEC API documentation doesn't specify max per_page. Existing tests use default (20-100 typical for APIs).
   - What's unclear: Is 100 too small (many round-trips), or is 1000 too large (timeout risk)?
   - Recommendation: Default to 100 (--batch-size flag). This matches the mpsc channel capacity pattern from enrich_prices. Users can adjust if API supports larger batches without timeout.

4. **Should circuit breaker differentiate between 429 (rate limit) and 403 (invalid key)?**
   - What we know: 403 is fatal (bad API key), 429 is transient (rate limit). Both increment circuit breaker currently.
   - What's unclear: Should 403 fail immediately instead of counting toward threshold?
   - Recommendation: Fail immediately on 403 with message "Invalid API key - check OPENFEC_API_KEY in .env". Only count 429 toward circuit breaker threshold. This gives clearer error messages for config problems.

## Sources

### Primary (HIGH confidence)
- [Tokio Semaphore documentation](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore) - Limit parallel outgoing requests pattern
- [Tokio mpsc channel documentation](https://docs.rs/tokio/latest/tokio/sync/mpsc/fn.channel) - Bounded channel with backpressure
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) - Schedule A keyset pagination
- `/capitoltraders_cli/src/commands/enrich_prices.rs` - Existing Semaphore + JoinSet + mpsc pattern
- `/capitoltraders_lib/src/openfec/types.rs` - ScheduleAQuery, ScheduleAPagination, LastIndexes types
- `/schema/sqlite.sql` - donations and donation_sync_meta table schemas

### Secondary (MEDIUM confidence)
- [Keyset pagination: how it works, examples, and pros and cons](https://www.merge.dev/blog/keyset-pagination) - Cursor state management best practices
- [15k inserts/s with Rust and SQLite](https://kerkour.com/high-performance-rust-with-sqlite) - Transaction wrapping for batch inserts
- [Best practices for handling API rate limits and 429 errors](https://help.docebo.com/hc/en-us/articles/31803763436946-Best-practices-for-handling-API-rate-limits-and-429-errors) - Exponential backoff and retry strategies
- [Investigating Rust with SQLite](https://tedspence.com/investigating-rust-with-sqlite-53d1f9a41112) - Performance patterns for SQLite batch operations

### Tertiary (LOW confidence)
- [18F: 67 million more Federal Election Commission records](https://18f.gsa.gov/2015/07/15/openfec-api-update/) - Historical context on keyset pagination adoption (2015)
- [OpenFEC Schedule_e pagination bug](https://github.com/fecgov/openFEC/issues/3396) - Known pagination edge cases (specific to Schedule E, not A)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in use, proven in enrich_prices
- Architecture: HIGH - Direct reuse of enrich_prices pattern, keyset pagination well-documented by OpenFEC
- Pitfalls: HIGH - Cursor state desync and circuit breaker threshold are validated from Phase 4 lessons learned

**Research date:** 2026-02-12
**Valid until:** 2026-03-12 (30 days - stable APIs and patterns)
