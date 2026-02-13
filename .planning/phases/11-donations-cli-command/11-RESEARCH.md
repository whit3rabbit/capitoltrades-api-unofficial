# Phase 11: Donations CLI Command - Research

**Researched:** 2026-02-13
**Domain:** CLI query command with SQL aggregations, Rust rusqlite, output formatting
**Confidence:** HIGH

## Summary

Phase 11 implements a donations CLI subcommand that queries synced FEC contribution data from SQLite. This is a DB-only read command similar to `portfolio`, requiring users to have run `sync-fec` and `sync-donations` first. The command supports both individual donation listings (default) and aggregated views (via `--group-by`).

The implementation follows established patterns: clap derive for args, dynamic SQL query building with filter composition, row structs with Tabled + Serialize, and 5-output format support. The key technical challenge is implementing GROUP BY aggregations with SUM/COUNT in rusqlite while maintaining the same output format flexibility as other commands.

**Primary recommendation:** Follow the portfolio.rs and trades DB query patterns. Use separate row structs for individual vs aggregated views, build SQL dynamically based on `--group-by` flag, and leverage existing validation module for all filter inputs.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| clap | 4.x | CLI argument parsing | Derive macros for args, subcommands, validation |
| rusqlite | latest | SQLite database access | Type-safe SQL execution, transaction support |
| tabled | 0.17 | ASCII table formatting | Derive macro for table rows, markdown support |
| serde | latest | Serialization | JSON output, CSV serialization via csv crate |
| csv | 1.3 | CSV output | Writer with formula injection sanitization |
| anyhow | latest | Error handling | Result type for CLI command functions |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| quick-xml | 0.37 | XML output | Via xml_output bridge module |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Dynamic SQL building | ORM/query builder | Project uses raw SQL consistently, no new dependency |
| Separate commands for aggregations | Single command with flag | --group-by flag is more ergonomic than 5+ subcommands |

**Installation:**
No new dependencies needed. All libraries already in workspace.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_cli/src/commands/
├── donations.rs        # New: donations subcommand (THIS PHASE)

capitoltraders_lib/src/
├── db.rs               # Add: query_donations(), aggregation variants
├── validation.rs       # Reuse existing validators

output.rs               # Add: print_donations_*, print_donations_aggregated_*
```

### Pattern 1: DB-Only Command Structure
**What:** Command requires --db flag, no API calls, validates filters, queries DB, formats output
**When to use:** Read-only queries on synced data (portfolio, donations)
**Example:**
```rust
// Source: portfolio.rs pattern
#[derive(Args)]
pub struct DonationsArgs {
    #[arg(long)]
    pub db: PathBuf,  // Required, no Option<>

    #[arg(long)]
    pub politician: Option<String>,

    // ... other filters
}

pub fn run(args: &DonationsArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Validate filters
    let validated = validate_filters(args)?;

    // Query DB
    let results = db.query_donations(&validated)?;

    // Format output
    match format {
        OutputFormat::Table => print_donations_table(&results),
        OutputFormat::Json => print_json(&results),
        // ... etc
    }

    Ok(())
}
```

### Pattern 2: Dynamic SQL Query Building
**What:** Build SQL with conditional WHERE clauses based on provided filters
**When to use:** Commands with multiple optional filters
**Example:**
```rust
// Source: db.rs query_trades() pattern
pub fn query_donations(&self, filter: &DonationFilter) -> Result<Vec<DonationRow>, DbError> {
    let mut sql = String::from("SELECT ... FROM donations d JOIN ...");
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(ref party) = filter.party {
        clauses.push(format!("p.party = ?{}", param_idx));
        params.push(Box::new(party.clone()));
        param_idx += 1;
    }

    let where_clause = if clauses.is_empty() {
        "WHERE 1=1".to_string()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };

    sql.push_str(&where_clause);
    // ... execute and map rows
}
```

### Pattern 3: SQL GROUP BY Aggregations
**What:** Use SQL SUM/COUNT/MAX with GROUP BY for aggregated views
**When to use:** --group-by contributor, employer, state, cycle
**Example:**
```rust
// Aggregated query for --group-by contributor
let sql = "
    SELECT
        d.contributor_name,
        d.contributor_state,
        SUM(d.contribution_receipt_amount) as total_amount,
        COUNT(*) as donation_count,
        MAX(d.contribution_receipt_amount) as max_donation,
        MIN(d.contribution_receipt_date) as first_donation,
        MAX(d.contribution_receipt_date) as last_donation
    FROM donations d
    JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
    WHERE dsm.politician_id = ?1
    GROUP BY d.contributor_name, d.contributor_state
    ORDER BY total_amount DESC
";
```

### Pattern 4: Separate Row Structs for Views
**What:** Define distinct row structs for individual listings vs aggregated views
**When to use:** When output schemas differ significantly (individual vs grouped)
**Example:**
```rust
// Individual donation view
#[derive(Tabled, Serialize)]
struct DonationRow {
    #[tabled(rename = "Date")]
    date: String,
    #[tabled(rename = "Contributor")]
    contributor_name: String,
    #[tabled(rename = "Employer")]
    employer: String,
    #[tabled(rename = "Amount")]
    amount: String,
    // ...
}

// Aggregated view (group-by contributor)
#[derive(Tabled, Serialize)]
struct ContributorAggregation {
    #[tabled(rename = "Contributor")]
    name: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "Total")]
    total_amount: String,
    #[tabled(rename = "Count")]
    count: i64,
    #[tabled(rename = "Avg")]
    avg_amount: String,
}
```

### Pattern 5: Output Format Dispatch
**What:** Match on OutputFormat enum, call format-specific print functions
**When to use:** All CLI commands (consistent 5-format support)
**Example:**
```rust
// Source: portfolio.rs, output.rs patterns
match format {
    OutputFormat::Table => print_donations_table(&donations),
    OutputFormat::Json => print_json(&donations),
    OutputFormat::Csv => print_donations_csv(&donations)?,
    OutputFormat::Markdown => print_donations_markdown(&donations),
    OutputFormat::Xml => print_donations_xml(&donations),
}
```

### Anti-Patterns to Avoid
- **Formatting in DB layer:** Keep DB methods returning raw rows, format in output.rs
- **Unwrap in CLI code:** Use ? operator, let anyhow handle errors
- **SQL injection:** Always use parameterized queries, never string interpolation
- **Missing CSV sanitization:** Always sanitize fields that could contain =+-@ prefixes
- **Inconsistent column names:** Use same casing/naming as existing commands

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Table rendering | ASCII table layout logic | tabled crate + Tabled derive | Handles alignment, borders, markdown mode automatically |
| CSV escaping | Custom quote/escape logic | csv::Writer + serialize | RFC 4180 compliant, handles all edge cases |
| XML generation | String concatenation | xml_output bridge + serde | Well-formed documents, proper escaping |
| Argument parsing | Manual string parsing | clap derive macros | Validation, help text, type safety built-in |
| Filter validation | Ad-hoc string checks | validation module functions | Consistent error messages, reusable logic |
| SQL dynamic params | String interpolation | Box<dyn ToSql> vec pattern | Prevents SQL injection, type-safe |

**Key insight:** The output formatting pipeline (row struct -> Tabled/Serialize -> format-specific printer) is mature and battle-tested across trades/politicians/issuers/portfolio commands. Don't deviate from this pattern.

## Common Pitfalls

### Pitfall 1: GROUP BY Without Aggregates on Non-Grouped Columns
**What goes wrong:** SQLite error "column not in GROUP BY clause or aggregate function"
**Why it happens:** Selecting columns not in GROUP BY without applying SUM/COUNT/MAX/MIN/AVG
**How to avoid:** Every selected column must either be in GROUP BY clause OR wrapped in aggregate function
**Warning signs:** Query works without GROUP BY but fails when adding it
**Example:**
```rust
// WRONG: contributor_city not grouped or aggregated
SELECT contributor_name, contributor_city, SUM(amount)
FROM donations
GROUP BY contributor_name

// CORRECT: Either group by both columns
SELECT contributor_name, contributor_city, SUM(amount)
FROM donations
GROUP BY contributor_name, contributor_city

// OR: Aggregate the non-grouped column
SELECT contributor_name, COUNT(DISTINCT contributor_city), SUM(amount)
FROM donations
GROUP BY contributor_name
```

### Pitfall 2: Mixing Individual and Aggregated Output Formats
**What goes wrong:** Output format printers expect consistent row schema, crash on type mismatch
**Why it happens:** Using same row struct for individual and aggregated queries
**How to avoid:** Define separate row structs (DonationRow vs ContributorAggregation), separate print functions
**Warning signs:** Serialization errors, missing columns in output, tabled derive compile errors

### Pitfall 3: Filter Validation Skipped for DB Commands
**What goes wrong:** Invalid input passes through, causes cryptic SQL errors or incorrect results
**Why it happens:** Assuming DB-only commands don't need validation since no API call
**How to avoid:** Validate ALL filters using validation module before building query
**Warning signs:** Raw user input passed directly to SQL, no trim() calls, lowercase not normalized
**Example:**
```rust
// WRONG: No validation
let party = args.party.as_ref();

// CORRECT: Validate and normalize
let party = match args.party {
    Some(ref val) => Some(validation::validate_party(val.trim())?.to_string()),
    None => None,
};
```

### Pitfall 4: Politician-to-Committee JOIN Missing
**What goes wrong:** Donations query returns empty results even when data exists
**Why it happens:** Donations table only has committee_id, not politician_id (schema design)
**How to avoid:** Always JOIN donations with donation_sync_meta to link committees to politicians
**Warning signs:** count_donations_for_politician() works but new query returns nothing
**Example:**
```rust
// WRONG: Direct politician_id filter (column doesn't exist on donations)
SELECT * FROM donations WHERE politician_id = ?1

// CORRECT: JOIN through donation_sync_meta
SELECT d.* FROM donations d
JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
WHERE dsm.politician_id = ?1
```

### Pitfall 5: Max-Out Donor Logic Ignores Election Cycle
**What goes wrong:** False positives for max-out detection (summing across multiple cycles)
**Why it happens:** Contribution limits are per-election-cycle, not lifetime
**How to avoid:** Filter by election_cycle in aggregation query
**Warning signs:** Donor from 2022 + 2024 flagged as max-out when each cycle under limit
**Example:**
```rust
// WRONG: Sum across all cycles
SELECT contributor_name, SUM(amount)
FROM donations
GROUP BY contributor_name
HAVING SUM(amount) >= 3500

// CORRECT: Group by cycle too
SELECT contributor_name, election_cycle, SUM(amount)
FROM donations
GROUP BY contributor_name, election_cycle
HAVING SUM(amount) >= 3500
```

### Pitfall 6: NULL Handling in Aggregations
**What goes wrong:** COUNT(*) vs COUNT(column) mismatch, empty strings vs NULL confusion
**Why it happens:** Donations may have NULL contributor_name, contributor_employer, etc.
**How to avoid:** Use COALESCE for display, COUNT(*) for total records, COUNT(column) for non-NULL
**Warning signs:** Contributor name shown as empty in output, counts don't add up
**Example:**
```rust
// Display handling
SELECT COALESCE(d.contributor_name, 'Unknown') as contributor_name

// Counting distinct employers (ignore NULL)
SELECT COUNT(DISTINCT contributor_employer) FROM donations

// Total donations (include all rows)
SELECT COUNT(*) FROM donations
```

## Code Examples

Verified patterns from existing codebase:

### Individual Donation Query Pattern
```rust
// Source: db.rs query_trades() adapted for donations
pub fn query_donations(&self, filter: &DonationFilter) -> Result<Vec<DonationRow>, DbError> {
    let mut sql = String::from(
        "SELECT
            d.sub_id,
            d.contributor_name,
            d.contributor_employer,
            d.contributor_occupation,
            d.contributor_state,
            d.contribution_receipt_amount,
            d.contribution_receipt_date,
            d.election_cycle,
            fc.name as committee_name,
            fc.designation,
            p.first_name || ' ' || p.last_name AS politician_name
         FROM donations d
         JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
         JOIN politicians p ON dsm.politician_id = p.politician_id
         LEFT JOIN fec_committees fc ON d.committee_id = fc.committee_id
         WHERE 1=1"
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut clauses = Vec::new();
    let mut param_idx = 1;

    if let Some(ref politician_id) = filter.politician_id {
        clauses.push(format!("dsm.politician_id = ?{}", param_idx));
        params.push(Box::new(politician_id.clone()));
        param_idx += 1;
    }

    if let Some(ref min_amount) = filter.min_amount {
        clauses.push(format!("d.contribution_receipt_amount >= ?{}", param_idx));
        params.push(Box::new(*min_amount));
        param_idx += 1;
    }

    if !clauses.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(&clauses.join(" AND "));
    }

    sql.push_str(" ORDER BY d.contribution_receipt_amount DESC");

    if let Some(limit) = filter.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }

    // Execute and map rows...
}
```

### Aggregation Query Pattern (Group by Contributor)
```rust
// Pattern for --group-by contributor
pub fn query_donations_by_contributor(
    &self,
    filter: &DonationFilter
) -> Result<Vec<ContributorAggregation>, DbError> {
    let sql = "
        SELECT
            COALESCE(d.contributor_name, 'Unknown') as contributor_name,
            d.contributor_state,
            SUM(d.contribution_receipt_amount) as total_amount,
            COUNT(*) as donation_count,
            AVG(d.contribution_receipt_amount) as avg_amount,
            MAX(d.contribution_receipt_amount) as max_donation,
            MIN(d.contribution_receipt_date) as first_donation,
            MAX(d.contribution_receipt_date) as last_donation
        FROM donations d
        JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
        WHERE dsm.politician_id = ?1
        GROUP BY d.contributor_name, d.contributor_state
        ORDER BY total_amount DESC
        LIMIT ?2
    ";

    let mut stmt = self.conn.prepare(sql)?;
    let rows = stmt.query_map(
        params![filter.politician_id, filter.limit.unwrap_or(100)],
        |row| {
            Ok(ContributorAggregation {
                contributor_name: row.get(0)?,
                contributor_state: row.get(1)?,
                total_amount: row.get(2)?,
                donation_count: row.get(3)?,
                avg_amount: row.get(4)?,
                max_donation: row.get(5)?,
                first_donation: row.get(6)?,
                last_donation: row.get(7)?,
            })
        }
    )?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}
```

### CSV Formula Injection Sanitization
```rust
// Source: output.rs existing pattern
fn sanitize_csv_field(s: &str) -> String {
    if s.starts_with('=') || s.starts_with('+')
        || s.starts_with('-') || s.starts_with('@') {
        format!("\t{}", s)
    } else {
        s.to_string()
    }
}

pub fn print_donations_csv(donations: &[DonationRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_donation_rows(donations) {
        row.contributor_name = sanitize_csv_field(&row.contributor_name);
        row.contributor_employer = sanitize_csv_field(&row.contributor_employer);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}
```

### Max-Out Donor Detection
```rust
// Detect contributors who hit individual limit ($3,500 per election for 2025-2026)
pub fn query_maxed_out_donors(
    &self,
    politician_id: &str,
    cycle: i32,
    limit_threshold: f64  // 3500.0 for 2025-2026
) -> Result<Vec<MaxedOutDonor>, DbError> {
    let sql = "
        SELECT
            d.contributor_name,
            d.contributor_state,
            d.election_cycle,
            SUM(d.contribution_receipt_amount) as total_amount,
            COUNT(*) as donation_count
        FROM donations d
        JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
        WHERE dsm.politician_id = ?1
          AND d.election_cycle = ?2
          AND d.contributor_name IS NOT NULL
        GROUP BY d.contributor_name, d.contributor_state, d.election_cycle
        HAVING SUM(d.contribution_receipt_amount) >= ?3
        ORDER BY total_amount DESC
    ";

    // Execute and return results
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single query for all formats | Separate queries for individual/aggregated | N/A (new feature) | Cleaner code, type-safe row structs |
| String interpolation in SQL | Parameterized queries with ToSql | Since project start | SQL injection prevention |
| Manual CSV escaping | csv crate + sanitization | Since v1.0 | RFC 4180 compliance + formula injection protection |
| Hard-coded contribution limits | Parameterized threshold | N/A (new) | Easy to update for future cycles |

**Deprecated/outdated:**
- N/A (new feature, no legacy to replace)

## Open Questions

1. **Should --top N apply before or after aggregation?**
   - What we know: SQL LIMIT applies to final result set
   - What's unclear: User expectation (top 10 contributors vs top 10 from filtered set)
   - Recommendation: Apply LIMIT at SQL level (most efficient), document in help text

2. **Geographic concentration: state-level only or drill down to city/zip?**
   - What we know: Schema has contributor_city and contributor_zip
   - What's unclear: Is city-level useful given data quality (may be sparse/NULL)
   - Recommendation: Start with state-level aggregation, add city as future enhancement if users request it

3. **Committee type breakdown: use designation field or committee_type field?**
   - What we know: fec_committees table has both designation (P/A/etc) and committee_type
   - What's unclear: Which field better represents "campaign vs leadership PAC" distinction
   - Recommendation: Use designation (P=principal, D=leadership PAC) per Phase 9 CommitteeClass pattern

4. **How to display NULL/empty contributor names in aggregated views?**
   - What we know: contributor_name can be NULL per Phase 10 testing
   - What's unclear: Show as "Unknown", skip entirely, or separate category
   - Recommendation: Use COALESCE(contributor_name, 'Unknown') for consistency with other commands

## Sources

### Primary (HIGH confidence)
- Codebase: /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/commands/portfolio.rs - DB-only command pattern
- Codebase: /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/db.rs - Dynamic SQL query building, query_trades() filter composition
- Codebase: /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/output.rs - Row struct patterns, CSV sanitization, 5-format support
- Codebase: /Users/whit3rabbit/Documents/GitHub/capitoltraders/schema/sqlite.sql - donations table schema, indexes
- Codebase: /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/validation.rs - Existing validators

### Secondary (MEDIUM confidence)
- [FEC Contribution Limits 2025-2026](https://www.fec.gov/updates/contribution-limits-for-2025-2026/) - $3,500 per-election limit for individuals
- [SQLite Built-in Aggregate Functions](https://sqlite.org/lang_aggfunc.html) - SUM, COUNT, AVG, MAX, MIN official documentation
- [SQLite GROUP BY Tutorial](https://www.sqlitetutorial.net/sqlite-group-by/) - GROUP BY syntax and examples
- [SQLite Aggregate Functions Guide](https://www.sqlitetutorial.net/sqlite-aggregate-functions/) - Practical aggregation patterns

### Tertiary (LOW confidence)
- N/A

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in workspace, no new libraries needed
- Architecture: HIGH - Existing commands (portfolio, trades DB queries) provide clear patterns
- Pitfalls: HIGH - Identified from schema review, SQL aggregation docs, existing codebase patterns
- Code examples: HIGH - Adapted from verified codebase patterns (query_trades, portfolio output)
- FEC limits: MEDIUM - Official FEC source but may change in future cycles (currently valid for 2025-2026)

**Research date:** 2026-02-13
**Valid until:** 2026-03-15 (30 days for stable patterns, FEC limits stable until 2027 cycle)
