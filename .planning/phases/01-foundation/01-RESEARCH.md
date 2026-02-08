# Phase 1: Foundation - Research

**Researched:** 2026-02-07
**Domain:** SQLite schema migration, upsert correctness, enrichment tracking
**Confidence:** HIGH

## Summary

Phase 1 is a pure database/schema phase with no HTTP or scraping work. It addresses four requirements: fixing the COALESCE direction in upsert statements so incremental re-syncs do not silently overwrite enriched data with defaults (FOUND-01), adding `enriched_at` timestamp columns to three entity tables (FOUND-02), creating query methods that find rows needing enrichment (FOUND-03), and implementing schema migration for existing databases (FOUND-04).

The existing codebase has a clear and well-documented data corruption risk in `db.rs`. The `upsert_scraped_trades` function (lines 327-376) unconditionally writes `excluded.filing_id`, `excluded.filing_url`, `excluded.size`, `excluded.price`, and other enrichment-target columns on conflict. When a listing-page re-sync inserts a trade with `filing_id=0`, `filing_url=""`, `size=NULL`, these sentinel/default values overwrite any previously enriched values. The same issue exists in the `upsert_trades` function (lines 136-185) for the API-sourced trade path. Some columns (like `asset_ticker`, `issuer_ticker`, `sector`) already use `COALESCE(excluded.X, table.X)` correctly, but the COALESCE direction only protects nullable columns where incoming NULL should not clobber an existing value. For non-nullable sentinel columns (`filing_id=0`, `filing_url=""`, `asset_type="unknown"`), COALESCE does not help because the incoming value is not NULL -- it is a sentinel. These require CASE expressions.

The bundled SQLite version is 3.45.0 (via rusqlite 0.31.0 with libsqlite3-sys). SQLite does not support `ALTER TABLE ADD COLUMN IF NOT EXISTS` in any version. Migration must either catch and ignore the "duplicate column name" error, or use a version-gated approach via `PRAGMA user_version`. The `rusqlite_migration` crate (v1.2.0) is compatible with rusqlite 0.31 and provides a clean versioned migration framework using `PRAGMA user_version`, but for three ALTER TABLE statements the hand-rolled approach is simpler and avoids a new dependency.

**Primary recommendation:** Fix upsert SQL with CASE expressions for sentinel-valued columns, add `enriched_at TEXT` columns via version-gated ALTER TABLE in `Db::init()`, and add query methods that return entity IDs where `enriched_at IS NULL`.

## Standard Stack

### Core (already in workspace)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | 0.31.0 | SQLite access, prepared statements, transactions | Already used. Bundled SQLite 3.45.0. |
| chrono | 0.4 | Timestamp generation for `enriched_at` | Already used for date handling throughout. |
| serde_json | 1 | JSON serialization for the Trade-to-DbTrade conversion | Already used in db.rs for serde round-trip. |
| thiserror | 1 | Error types for DbError | Already used. |

### Supporting (no new dependencies needed)

Phase 1 requires zero new crate dependencies. All work is rusqlite SQL + Rust logic.

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled PRAGMA user_version migration | rusqlite_migration 1.2.0 | Library provides cleaner API but adds a dependency for 3 ALTER TABLE statements. Not worth it at this scale. Would become worth it if migrations grow beyond 5-10 steps. |
| CASE expressions for sentinel protection | Separate enrichment-only UPDATE statements | Separate UPDATEs are cleaner conceptually but require restructuring the sync pipeline (later phases). CASE in the upsert is the minimal fix that protects existing enriched data without changing the sync flow. |
| `enriched_at` column per table | Single `enrichment_status` table | Per-table columns are simpler, avoid JOINs for the common query case, and allow per-entity-type re-enrichment. |

## Architecture Patterns

### Recommended Schema Changes

```sql
-- New columns (applied via migration)
ALTER TABLE trades ADD COLUMN enriched_at TEXT;
ALTER TABLE politicians ADD COLUMN enriched_at TEXT;
ALTER TABLE issuers ADD COLUMN enriched_at TEXT;
```

These columns are nullable TEXT. NULL means "not yet enriched." A non-NULL value is an ISO 8601 timestamp indicating when enrichment completed. This design is unambiguous (unlike sentinel-value detection) and supports future re-enrichment by resetting to NULL.

### Pattern 1: Version-Gated Migration in Db::init()

**What:** Use `PRAGMA user_version` to track schema version. On each `Db::init()` call, check the current version and apply any pending ALTER TABLE statements.

**When to use:** Every time the database is opened.

**Why:** The current `init()` runs `CREATE TABLE IF NOT EXISTS` which is idempotent for new databases but cannot add columns to existing tables. Version-gated migration handles both cases:
- Version 0 (new DB or pre-migration DB): Run full schema creation, then run all ALTER TABLE statements, set version to 1.
- Version 1 (already migrated): Skip ALTER TABLE statements.

**Example:**

```rust
pub fn init(&self) -> Result<(), DbError> {
    let schema = include_str!("../../schema/sqlite.sql");
    self.conn.execute_batch(schema)?;

    let version: i32 = self.conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        // Migration v1: Add enriched_at columns
        // Ignore "duplicate column name" errors for idempotency
        for stmt in &[
            "ALTER TABLE trades ADD COLUMN enriched_at TEXT",
            "ALTER TABLE politicians ADD COLUMN enriched_at TEXT",
            "ALTER TABLE issuers ADD COLUMN enriched_at TEXT",
        ] {
            match self.conn.execute(stmt, []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ffi::ErrorCode::Unknown
                        || err.extended_code == 1 => {}  // "duplicate column name"
                Err(e) => return Err(e.into()),
            }
        }
        self.conn.pragma_update(None, "user_version", &1)?;
    }

    Ok(())
}
```

**Note on error matching:** SQLite returns error code 1 (SQLITE_ERROR) with message "duplicate column name: X" when the column already exists. The rusqlite Error variant is `SqliteFailure`. The code should check for the specific message string to be safe, not just the error code, since code 1 is generic. A safer pattern:

```rust
Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
    if msg.contains("duplicate column name") => {}
```

### Pattern 2: CASE Expressions for Sentinel-Protected Upsert

**What:** Use SQL CASE expressions in ON CONFLICT DO UPDATE to prevent overwriting enriched values with sentinel defaults.

**When to use:** For columns where the incoming data uses a non-NULL default that should not clobber an enriched value. Examples: `filing_id` (default 0), `filing_url` (default ""), `asset_type` (default "unknown").

**Why:** COALESCE only protects against NULL incoming values. When the listing page sends `filing_id=0` (not NULL), `COALESCE(excluded.filing_id, trades.filing_id)` returns 0 because excluded.filing_id is not NULL. The existing row's enriched value (e.g., 12345) is lost.

**Example for trades upsert (upsert_scraped_trades):**

```sql
ON CONFLICT(tx_id) DO UPDATE SET
  -- Always take the incoming value for core fields
  politician_id = excluded.politician_id,
  pub_date = excluded.pub_date,
  tx_date = excluded.tx_date,
  tx_type = excluded.tx_type,

  -- Sentinel-protected: keep existing enriched value if incoming is default
  filing_id = CASE
    WHEN excluded.filing_id > 0 THEN excluded.filing_id
    ELSE trades.filing_id
  END,
  filing_url = CASE
    WHEN excluded.filing_url != '' THEN excluded.filing_url
    ELSE trades.filing_url
  END,
  price = COALESCE(excluded.price, trades.price),
  size = COALESCE(excluded.size, trades.size),
  size_range_high = COALESCE(excluded.size_range_high, trades.size_range_high),
  size_range_low = COALESCE(excluded.size_range_low, trades.size_range_low),

  -- Preserve enriched_at: never overwrite from listing data
  enriched_at = trades.enriched_at
```

**Key rule for enriched_at:** The `enriched_at` column must ALWAYS be preserved in upsert statements. Only dedicated enrichment UPDATE methods should set it.

### Pattern 3: COALESCE for Nullable Columns (Already Correct)

**What:** `COALESCE(excluded.X, table.X)` for columns where NULL means "no data."

**When to use:** For Optional/nullable columns where the API/scraper may omit the value. Already correctly applied in the codebase for `asset_ticker`, `instrument`, `issuer_ticker`, `sector`, `nickname`, etc.

**Why this is correct:** When the listing page sends `issuer_ticker=NULL` and the DB has `issuer_ticker="AAPL"`, COALESCE returns "AAPL" -- the existing value is preserved.

**When this is NOT sufficient:** For columns with non-NULL sentinel defaults (see Pattern 2).

### Pattern 4: Enrichment Query Methods

**What:** Db methods that return IDs of rows needing enrichment.

**When to use:** Before starting an enrichment pass to build the work queue.

```rust
pub fn get_unenriched_trade_ids(&self, limit: Option<i64>) -> Result<Vec<i64>, DbError> {
    let sql = match limit {
        Some(n) => format!(
            "SELECT tx_id FROM trades WHERE enriched_at IS NULL ORDER BY tx_id LIMIT {}",
            n
        ),
        None => "SELECT tx_id FROM trades WHERE enriched_at IS NULL ORDER BY tx_id".to_string(),
    };
    let mut stmt = self.conn.prepare(&sql)?;
    let ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}
```

The same pattern applies for `get_unenriched_politician_ids` (returns `Vec<String>`) and `get_unenriched_issuer_ids` (returns `Vec<i64>`).

### Anti-Patterns to Avoid

- **Modifying `schema/sqlite.sql` to add `enriched_at` columns inline:** This would make `CREATE TABLE IF NOT EXISTS` a no-op for existing databases, which is correct, but it means the ALTER TABLE migration is still needed for pre-existing DBs. Better to keep the base schema clean and add columns via migration. However, for new databases created from scratch, the columns SHOULD be in the schema DDL to avoid unnecessary ALTER TABLE calls. The recommended approach: add the columns to `schema/sqlite.sql` AND have the migration path for existing databases.

- **Using a separate `enrichment_tracking` table instead of per-table columns:** Adds JOIN overhead to every enrichment query, makes the data model more complex, and provides no benefit over a nullable column on the entity table itself.

- **Checking sentinel values (filing_url='', filing_id=0) instead of enriched_at for needs-enrichment detection:** This is ambiguous. Some trades may legitimately have no filing URL (Senate financial disclosure PDFs sometimes use non-parseable URLs). A trade with `filing_url=''` after enrichment (because the detail page had no filing URL) is indistinguishable from a trade that was never enriched. The `enriched_at` column eliminates this ambiguity.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Schema version tracking | Custom version table in SQLite | `PRAGMA user_version` | Built into SQLite, atomic, no table creation needed |
| Duplicate column detection | Parsing PRAGMA table_info results | Catch "duplicate column name" error | Error-catch approach is simpler, fewer moving parts, idempotent |
| Timestamp formatting | Manual string formatting | `chrono::Utc::now().to_rfc3339()` | Already in the dependency tree, handles timezone correctly |

**Key insight:** The migration needs of this phase are minimal (3 ALTER TABLE statements and a version bump). A migration framework would be over-engineering. The PRAGMA user_version approach is the right tool.

## Common Pitfalls

### Pitfall 1: COALESCE Direction Does Not Protect Sentinel Values

**What goes wrong:** Developers see `COALESCE(excluded.X, table.X)` used for some columns and assume adding it to `filing_id` and `filing_url` will fix the overwrite problem. But `filing_id=0` and `filing_url=""` are not NULL, so COALESCE always picks the incoming (sentinel) value.

**Why it happens:** COALESCE returns the first non-NULL argument. Zero and empty string are not NULL.

**How to avoid:** Use CASE expressions for sentinel-valued columns: `CASE WHEN excluded.filing_id > 0 THEN excluded.filing_id ELSE trades.filing_id END`.

**Warning signs:** After `sync --full`, run `SELECT COUNT(*) FROM trades WHERE filing_url != ''`. If this number drops after a subsequent incremental sync, the COALESCE direction is wrong.

### Pitfall 2: Migration Runs on Every init() Call

**What goes wrong:** If the PRAGMA user_version check is not implemented, ALTER TABLE runs every time the database is opened, producing "duplicate column name" errors.

**Why it happens:** `init()` is called on every `Db::open()` + `init()` sequence.

**How to avoid:** Check `PRAGMA user_version` before running ALTER TABLE statements. Only run migrations for versions less than the current target.

**Warning signs:** Error logs showing "duplicate column name: enriched_at" on every run.

### Pitfall 3: enriched_at Gets Clobbered by Listing-Page Upsert

**What goes wrong:** The enriched_at column is added to the trades table, enrichment sets it, but the next listing-page upsert overwrites it with NULL (since listing pages never provide enriched_at).

**Why it happens:** If the ON CONFLICT DO UPDATE for trades includes `enriched_at = excluded.enriched_at`, and the listing-page insert sets enriched_at to NULL (as it should for new rows), the upsert clobbers the existing timestamp.

**How to avoid:** In the ON CONFLICT DO UPDATE clause, explicitly set `enriched_at = trades.enriched_at` (keep existing value) or omit it entirely from the update set.

**Warning signs:** After sync, run `SELECT COUNT(*) FROM trades WHERE enriched_at IS NOT NULL`. If this drops to zero, the upsert is clobbering it.

### Pitfall 4: upsert_trades vs upsert_scraped_trades Divergence

**What goes wrong:** The codebase has TWO trade upsert functions: `upsert_trades` (line 82, for API data) and `upsert_scraped_trades` (line 272, for scraper data). Fixing the COALESCE/CASE logic in one but not the other leaves a corruption path.

**Why it happens:** The two functions were written at different times for different data sources. They handle different sets of columns and have different sentinel defaults.

**How to avoid:** Fix both functions. Apply the same enriched_at preservation and sentinel protection to both upsert paths. Consider whether `upsert_trades` is still used -- if the project has fully moved to scrape-based sync, the API upsert may be dead code, but it should still be correct in case it is used in the future.

**Warning signs:** Tests pass for one code path but not the other.

### Pitfall 5: Foreign Key ON DELETE CASCADE and enriched_at

**What goes wrong:** If a politician or issuer row is deleted (e.g., during a schema reset), the CASCADE deletes all related trades. Re-syncing recreates the trades, but they now have `enriched_at=NULL` and all enrichment work is lost.

**Why it happens:** CASCADE delete is correct for referential integrity but destroys enrichment state along with the row.

**How to avoid:** This is expected behavior -- if the parent entity is deleted, its trades should be re-enriched. The enrichment pipeline handles this naturally because the re-created trades will have `enriched_at IS NULL` and will be picked up by `get_unenriched_trade_ids()`. No code change needed, just awareness.

### Pitfall 6: Asset Table Upsert Uses Unconditional asset_type Overwrite

**What goes wrong:** In `upsert_scraped_trades` (line 279), `asset_type = excluded.asset_type` unconditionally overwrites the asset type. Listing pages insert `asset_type = "unknown"`. If an enrichment pass later populates `asset_type = "stock"`, the next listing-page sync resets it to "unknown".

**Why it happens:** The assets table upsert does not protect `asset_type` the same way it protects `asset_ticker` (which uses COALESCE).

**How to avoid:** Add sentinel protection: `asset_type = CASE WHEN excluded.asset_type != 'unknown' THEN excluded.asset_type ELSE assets.asset_type END`.

## Code Examples

### Example 1: Fixed upsert_scraped_trades with Sentinel Protection

```rust
// Source: Analysis of db.rs lines 272-376
// Fixed ON CONFLICT clause for trades:
let mut stmt_trade = tx.prepare(
    "INSERT INTO trades (
       tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date,
       tx_date, tx_type, tx_type_extended, has_capital_gains, owner,
       chamber, price, size, size_range_high, size_range_low, value,
       filing_id, filing_url, reporting_gap, comment
     )
     VALUES (
       ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
       ?15, ?16, ?17, ?18, ?19, ?20, ?21
     )
     ON CONFLICT(tx_id) DO UPDATE SET
       politician_id = excluded.politician_id,
       asset_id = excluded.asset_id,
       issuer_id = excluded.issuer_id,
       pub_date = excluded.pub_date,
       filing_date = excluded.filing_date,
       tx_date = excluded.tx_date,
       tx_type = excluded.tx_type,
       tx_type_extended = excluded.tx_type_extended,
       has_capital_gains = CASE
         WHEN excluded.has_capital_gains = 1 THEN excluded.has_capital_gains
         ELSE trades.has_capital_gains
       END,
       owner = excluded.owner,
       chamber = excluded.chamber,
       price = COALESCE(excluded.price, trades.price),
       size = COALESCE(excluded.size, trades.size),
       size_range_high = COALESCE(excluded.size_range_high, trades.size_range_high),
       size_range_low = COALESCE(excluded.size_range_low, trades.size_range_low),
       value = excluded.value,
       filing_id = CASE
         WHEN excluded.filing_id > 0 THEN excluded.filing_id
         ELSE trades.filing_id
       END,
       filing_url = CASE
         WHEN excluded.filing_url != '' THEN excluded.filing_url
         ELSE trades.filing_url
       END,
       reporting_gap = excluded.reporting_gap,
       comment = excluded.comment",
)?;
```

### Example 2: Fixed Asset Upsert

```rust
// Source: Analysis of db.rs lines 276-283
let mut stmt_asset = tx.prepare(
    "INSERT INTO assets (asset_id, asset_type, asset_ticker, instrument)
     VALUES (?1, ?2, ?3, ?4)
     ON CONFLICT(asset_id) DO UPDATE SET
       asset_type = CASE
         WHEN excluded.asset_type != 'unknown' THEN excluded.asset_type
         ELSE assets.asset_type
       END,
       asset_ticker = COALESCE(excluded.asset_ticker, assets.asset_ticker),
       instrument = COALESCE(excluded.instrument, assets.instrument)",
)?;
```

### Example 3: Version-Gated Migration

```rust
// Source: rusqlite PRAGMA documentation + project db.rs init() pattern
pub fn init(&self) -> Result<(), DbError> {
    let schema = include_str!("../../schema/sqlite.sql");
    self.conn.execute_batch(schema)?;

    let version: i32 = self
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        self.migrate_v1()?;
        self.conn.pragma_update(None, "user_version", &1)?;
    }

    Ok(())
}

fn migrate_v1(&self) -> Result<(), DbError> {
    for sql in &[
        "ALTER TABLE trades ADD COLUMN enriched_at TEXT",
        "ALTER TABLE politicians ADD COLUMN enriched_at TEXT",
        "ALTER TABLE issuers ADD COLUMN enriched_at TEXT",
    ] {
        match self.conn.execute(sql, []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
```

### Example 4: Enrichment Query Method

```rust
// Source: Pattern derived from existing db.rs query methods
pub fn get_unenriched_trade_ids(&self, limit: Option<i64>) -> Result<Vec<i64>, DbError> {
    let sql = match limit {
        Some(n) => format!(
            "SELECT tx_id FROM trades WHERE enriched_at IS NULL ORDER BY tx_id LIMIT {}",
            n
        ),
        None => "SELECT tx_id FROM trades WHERE enriched_at IS NULL ORDER BY tx_id".to_string(),
    };
    let mut stmt = self.conn.prepare(&sql)?;
    let ids = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}

pub fn get_unenriched_politician_ids(&self, limit: Option<i64>) -> Result<Vec<String>, DbError> {
    let sql = match limit {
        Some(n) => format!(
            "SELECT politician_id FROM politicians WHERE enriched_at IS NULL ORDER BY politician_id LIMIT {}",
            n
        ),
        None => "SELECT politician_id FROM politicians WHERE enriched_at IS NULL ORDER BY politician_id".to_string(),
    };
    let mut stmt = self.conn.prepare(&sql)?;
    let ids = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(ids)
}

pub fn get_unenriched_issuer_ids(&self, limit: Option<i64>) -> Result<Vec<i64>, DbError> {
    let sql = match limit {
        Some(n) => format!(
            "SELECT issuer_id FROM issuers WHERE enriched_at IS NULL ORDER BY issuer_id LIMIT {}",
            n
        ),
        None => "SELECT issuer_id FROM issuers WHERE enriched_at IS NULL ORDER BY issuer_id".to_string(),
    };
    let mut stmt = self.conn.prepare(&sql)?;
    let ids = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}
```

## Upsert Analysis: Column-by-Column Fix Map

This is the complete analysis of every upsert function and what needs to change.

### upsert_scraped_trades (lines 272-441)

**Assets upsert (lines 276-283):**

| Column | Current Behavior | Correct Behavior | Change Needed |
|--------|-----------------|------------------|---------------|
| asset_type | `= excluded.asset_type` | CASE: keep existing if incoming = "unknown" | YES |
| asset_ticker | `COALESCE(excluded, existing)` | Same | No |
| instrument | `COALESCE(excluded, existing)` | Same | No |

**Issuers upsert (lines 284-294):**

| Column | Current Behavior | Correct Behavior | Change Needed |
|--------|-----------------|------------------|---------------|
| issuer_name | `= excluded.issuer_name` | Same (always present) | No |
| issuer_ticker | `COALESCE(excluded, existing)` | Same | No |
| sector | `COALESCE(excluded, existing)` | Same | No |
| state_id | `COALESCE(excluded, existing)` | Same | No |
| c2iq | `COALESCE(excluded, existing)` | Same | No |
| country | `COALESCE(excluded, existing)` | Same | No |

**Politicians upsert (lines 295-326):**

| Column | Current Behavior | Correct Behavior | Change Needed |
|--------|-----------------|------------------|---------------|
| state_id | `= excluded.state_id` | Same (always present from listing) | No |
| party | `= excluded.party` | Same | No |
| first_name | `= excluded.first_name` | Same | No |
| last_name | `= excluded.last_name` | Same | No |
| nickname | `COALESCE(excluded, existing)` | Same | No |
| full_name | `COALESCE(excluded, existing)` | Same | No |
| dob | `= excluded.dob` | Same | No |
| gender | `= excluded.gender` | Same | No |
| chamber | `= excluded.chamber` | Same | No |

**Trades upsert (lines 327-376):**

| Column | Current Behavior | Correct Behavior | Change Needed |
|--------|-----------------|------------------|---------------|
| politician_id | `= excluded` | Same | No |
| asset_id | `= excluded` | Same | No |
| issuer_id | `= excluded` | Same | No |
| pub_date | `= excluded` | Same | No |
| filing_date | `= excluded` | Same | No |
| tx_date | `= excluded` | Same | No |
| tx_type | `= excluded` | Same | No |
| tx_type_extended | `= excluded` | Same | No |
| has_capital_gains | `= excluded` | CASE: keep existing if incoming = 0 | YES |
| owner | `= excluded` | Same | No |
| chamber | `= excluded` | Same | No |
| price | `= excluded` | COALESCE(excluded, existing) | YES |
| size | `= excluded` | COALESCE(excluded, existing) | YES |
| size_range_high | `= excluded` | COALESCE(excluded, existing) | YES |
| size_range_low | `= excluded` | COALESCE(excluded, existing) | YES |
| value | `= excluded` | Same (always present) | No |
| filing_id | `= excluded` | CASE: keep existing if incoming = 0 | YES |
| filing_url | `= excluded` | CASE: keep existing if incoming = '' | YES |
| reporting_gap | `= excluded` | Same | No |
| comment | `= excluded` | Same | No |

### upsert_trades (lines 82-185) -- API data path

The same analysis applies. The trades ON CONFLICT (lines 164-184) has the same unconditional overwrites for:
- price, size, size_range_high, size_range_low (should be COALESCE)
- filing_id, filing_url (should be CASE for sentinels)
- has_capital_gains (should be CASE)

The assets ON CONFLICT (lines 88-92) has the same asset_type issue.

### upsert_politicians (lines 443-556) -- Direct politician API path

This function unconditionally overwrites all fields (`= excluded.*`). This is CORRECT for this function because it receives complete politician detail data (all 16 columns). When you have the full record, you want the latest values. The enriched_at column should still be preserved (`enriched_at = politicians.enriched_at`).

### upsert_issuers (lines 590-743) -- Direct issuer API path

Same as politicians: unconditional overwrite is CORRECT because this receives full issuer detail data. The enriched_at column should be preserved.

### All enriched_at handling

Every upsert function that touches trades, politicians, or issuers must include `enriched_at = {table}.enriched_at` in the ON CONFLICT DO UPDATE clause to prevent clobbering the enrichment timestamp.

## Schema File Update

The `schema/sqlite.sql` file should include the `enriched_at` columns in the CREATE TABLE definitions so that NEW databases get the columns from the start. The migration path via `PRAGMA user_version` handles EXISTING databases.

```sql
-- Add to trades table definition:
    enriched_at TEXT,

-- Add to politicians table definition:
    enriched_at TEXT,

-- Add to issuers table definition:
    enriched_at TEXT
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `= excluded.filing_id` | CASE with sentinel check | Phase 1 (this work) | Prevents enriched data loss on re-sync |
| No schema migration | PRAGMA user_version gated ALTER TABLE | Phase 1 (this work) | Existing databases gain new columns safely |
| Sentinel-based enrichment detection | enriched_at timestamp column | Phase 1 (this work) | Unambiguous enrichment tracking |

## Open Questions

1. **Should upsert_trades (API path) be maintained or deprecated?**
   - What we know: The sync command uses `upsert_scraped_trades` exclusively. The `upsert_trades` function is used in the `trades` CLI command path (via `scraped_trade_to_trade` conversion).
   - What is unclear: Whether both paths will coexist long-term or whether the API path will be removed.
   - Recommendation: Fix both functions now. Divergent behavior between the two paths would be a source of bugs.

2. **Should enriched_at also be added to the assets table?**
   - What we know: The assets table has `asset_type` which defaults to "unknown" from listing pages. Enrichment will update it.
   - What is unclear: Whether tracking enrichment per-asset is needed, or whether trade-level `enriched_at` is sufficient (since assets are derived from trades).
   - Recommendation: Do NOT add enriched_at to assets in Phase 1. Assets are a denormalized side table populated as a byproduct of trade insertion. Track enrichment at the trade level. If a trade is enriched, its associated asset is also enriched.

3. **How to handle the `upsert_politicians` unconditional overwrite when future enrichment adds detail fields?**
   - What we know: `upsert_politicians` receives full data from the politician detail page and unconditionally overwrites. This is correct when you have complete data. But if scrape-based politician sync provides only partial data (e.g., name/party/state but not committees), the unconditional overwrite would clobber detail-enriched fields.
   - What is unclear: Whether scrape-based politician sync will ever call `upsert_politicians` with partial data.
   - Recommendation: The current `upsert_politicians` is fine for Phase 1. Just add `enriched_at = politicians.enriched_at` to preserve the timestamp. Later phases (Phase 4) will add targeted UPDATE methods for enrichment that do not go through the full upsert.

## Sources

### Primary (HIGH confidence)
- Direct analysis of `capitoltraders_lib/src/db.rs` -- all upsert functions, column-by-column
- Direct analysis of `schema/sqlite.sql` -- current table definitions
- Direct analysis of `capitoltraders_cli/src/commands/sync.rs` -- sync pipeline flow
- Bundled SQLite version 3.45.0 confirmed via `target/debug/build/libsqlite3-sys-*/out/bindgen.rs`
- rusqlite 0.31.0 confirmed via `cargo pkgid`

### Secondary (MEDIUM confidence)
- [SQLite ALTER TABLE documentation](https://www.sqlite.org/lang_altertable.html) -- no IF NOT EXISTS for ADD COLUMN
- [rusqlite_migration crate](https://crates.io/crates/rusqlite_migration) -- v1.2.0 compatible with rusqlite 0.31
- [rusqlite releases](https://github.com/rusqlite/rusqlite/releases) -- version history
- [SQLite UPSERT documentation](https://sqlite.org/lang_upsert.html) -- ON CONFLICT behavior

### Tertiary (LOW confidence)
- None. All findings verified against source code and official documentation.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, verified existing versions
- Architecture (upsert fix): HIGH -- direct code analysis, COALESCE behavior is well-defined SQL semantics
- Architecture (migration): HIGH -- PRAGMA user_version is documented SQLite feature, bundled version 3.45.0 is well past the 3.1 where ALTER TABLE ADD COLUMN was introduced
- Pitfalls: HIGH -- all pitfalls verified against actual code paths in db.rs and sync.rs
- Enrichment queries: HIGH -- simple SELECT WHERE IS NULL, standard pattern

**Research date:** 2026-02-07
**Valid until:** 2026-03-07 (stable domain -- SQLite semantics do not change)
