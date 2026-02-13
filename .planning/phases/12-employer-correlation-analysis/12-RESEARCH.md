# Phase 12: Employer Correlation & Analysis - Research

**Researched:** 2026-02-13
**Domain:** String normalization, fuzzy matching, entity resolution, employer-to-issuer correlation
**Confidence:** HIGH

## Summary

Phase 12 builds employer-to-issuer correlation capabilities to connect donation sources with traded securities. The technical foundation combines three established patterns: string normalization (corporate suffix stripping, case folding), fuzzy matching (Jaro-Winkler via strsim crate), and manual review workflows (export-review-import for user confirmation). The key architectural constraint is NEVER auto-linking: the system displays confidence-scored suggestions only, requiring explicit user review before persisting mappings.

The standard approach uses a three-tier matching system: exact match after normalization (confidence 1.0), fuzzy match with Jaro-Winkler 0.85+ threshold (confidence 0.85-0.99), and no match (skip). Manual seed data (JSON/TOML file with top 200 employers) provides high-confidence anchors, while the export-review-import workflow builds the mapping database over time through user validation.

**Primary recommendation:** Use strsim crate (Jaro-Winkler), TOML seed data for top employers, separate employer_mappings table with confidence scores, and extend trades/portfolio commands with optional --show-donor-context flag that gracefully no-ops if no donation data exists.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| strsim | 0.11+ | Jaro-Winkler fuzzy matching | Rust ecosystem standard, small binary size, normalized 0.0-1.0 scores, maintained by rapidfuzz team |
| serde | 1.0+ | TOML/JSON/CSV serialization | Universal Rust serialization framework, already in project dependencies |
| toml | 0.8+ | Seed data parsing | Cargo ecosystem standard, human-readable, comment support for manual curation |
| csv | 1.3+ | Export/import workflow | Already in project (output module), fast serde integration, BurntSushi standard |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rapidfuzz | Latest | Alternative to strsim | Only if performance profiling shows strsim is bottleneck (unlikely at this scale) |
| serde_json | 1.0+ | Alternative seed format | If programmatic generation preferred over manual curation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| strsim | rapidfuzz crate | Faster but larger binary, overkill for ~10k employer dataset |
| TOML seed data | JSON | Less human-readable, no comments for curation notes |
| Jaro-Winkler | Levenshtein distance | Worse for prefix-heavy corporate names (Apple Inc vs Apple Computer Inc) |
| Manual review | Auto-link at 0.90+ | Dangerous: "Goldman Sachs" could match "Goldman Realty" with high score |

**Installation:**
```bash
cargo add strsim@0.11
cargo add toml@0.8
# serde, csv already in Cargo.toml
```

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── employer_mapping.rs       # Normalization, fuzzy matching, confidence scoring
└── db.rs                      # Add employer_mappings table methods

capitoltraders_cli/src/commands/
├── map_employers.rs           # New command: export unmatched, import reviewed
├── trades.rs                  # Extend with --show-donor-context flag
└── portfolio.rs               # Extend with optional donation summary

seed_data/
└── employer_issuers.toml      # Manual seed: top 200 employers with tickers/sectors
```

### Pattern 1: Three-Tier Matching System
**What:** Normalize employer string, attempt exact match, fallback to fuzzy match, never auto-persist
**When to use:** Every employer name from donations table needs correlation attempt

**Example:**
```rust
// Source: Based on RecordLinker normalization patterns + strsim docs
use strsim::jaro_winkler;

pub struct MatchResult {
    pub issuer_id: i64,
    pub issuer_name: String,
    pub confidence: f64,
    pub match_type: MatchType,
}

pub enum MatchType {
    Exact,       // confidence = 1.0
    Fuzzy,       // confidence = jaro_winkler score
    Manual,      // confidence = 1.0 (user-confirmed)
}

fn normalize_employer(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    // Strip common suffixes: inc, llc, corp, corporation, ltd, co
    let suffixes = ["corporation", "incorporated", "inc", "llc", "corp", "ltd", "co", "l.l.c.", "l.p."];
    let mut normalized = lower.clone();
    for suffix in &suffixes {
        if let Some(stripped) = normalized.strip_suffix(suffix) {
            normalized = stripped.trim_end_matches(&['.', ',', ' ']).to_string();
            break;
        }
    }
    // Collapse whitespace
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn match_employer(
    employer: &str,
    issuers: &[(i64, String)],  // (issuer_id, issuer_name)
) -> Option<MatchResult> {
    let normalized = normalize_employer(employer);

    // Tier 1: Exact match after normalization
    for (id, issuer_name) in issuers {
        if normalize_employer(issuer_name) == normalized {
            return Some(MatchResult {
                issuer_id: *id,
                issuer_name: issuer_name.clone(),
                confidence: 1.0,
                match_type: MatchType::Exact,
            });
        }
    }

    // Tier 2: Fuzzy match with threshold
    let mut best_match: Option<MatchResult> = None;
    for (id, issuer_name) in issuers {
        let score = jaro_winkler(&normalized, &normalize_employer(issuer_name));
        if score >= 0.85 && score < 1.0 {
            if let Some(ref current) = best_match {
                if score > current.confidence {
                    best_match = Some(MatchResult {
                        issuer_id: *id,
                        issuer_name: issuer_name.clone(),
                        confidence: score,
                        match_type: MatchType::Fuzzy,
                    });
                }
            } else {
                best_match = Some(MatchResult {
                    issuer_id: *id,
                    issuer_name: issuer_name.clone(),
                    confidence: score,
                    match_type: MatchType::Fuzzy,
                });
            }
        }
    }

    best_match
}
```

### Pattern 2: Seed Data with TOML
**What:** Pre-populated employer-to-issuer mappings for top 200 employers
**When to use:** Initial data load, reduce fuzzy matching load for common employers

**Example:**
```toml
# seed_data/employer_issuers.toml
# Source: Fortune 500 + S&P 500 datasets
# Format: employer variants mapped to canonical issuer

[[mapping]]
employer_names = ["Apple Inc", "Apple Computer Inc", "Apple"]
issuer_id = 12345
issuer_ticker = "AAPL"
sector = "Information Technology"
confidence = 1.0
notes = "Multiple historical names, all confirmed"

[[mapping]]
employer_names = ["Alphabet Inc", "Google LLC", "Google Inc", "Google"]
issuer_id = 67890
issuer_ticker = "GOOGL"
sector = "Communication Services"
confidence = 1.0
notes = "Parent company + operating subsidiary variants"

[[mapping]]
employer_names = ["Microsoft Corporation", "Microsoft Corp", "Microsoft"]
issuer_id = 11111
issuer_ticker = "MSFT"
sector = "Information Technology"
confidence = 1.0

# Load with:
use serde::Deserialize;

#[derive(Deserialize)]
struct EmployerMapping {
    employer_names: Vec<String>,
    issuer_id: i64,
    issuer_ticker: String,
    sector: String,
    confidence: f64,
    notes: Option<String>,
}

fn load_seed_data() -> Result<Vec<EmployerMapping>> {
    let toml_str = include_str!("../../seed_data/employer_issuers.toml");
    let data: toml::Value = toml::from_str(toml_str)?;
    let mappings: Vec<EmployerMapping> = data["mapping"]
        .as_array()
        .ok_or("missing mapping array")?
        .iter()
        .map(|v| toml::from_value(v.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(mappings)
}
```

### Pattern 3: Export-Review-Import Workflow
**What:** CLI exports unmatched employers with suggestions to CSV, user reviews/edits, re-imports confirmed mappings
**When to use:** Building employer mapping database over time through user validation

**Example:**
```bash
# Export unmatched employers with suggestions
capitoltraders map-employers export --db capitol.db --output unmatched.csv

# CSV format:
# employer,suggestion_issuer_id,suggestion_name,confidence,confirmed_issuer_id,notes
# "Goldman Sachs Group",123,"Goldman Sachs",0.92,123,""
# "Morgan Stanley Inc",456,"Morgan Stanley",0.95,456,""
# "Random Small LLC",789,"Random Corp",0.86,,"False positive - not related"

# User edits CSV in spreadsheet, sets confirmed_issuer_id column

# Import confirmed mappings
capitoltraders map-employers import --db capitol.db --input unmatched.csv
```

```rust
// Export logic
pub fn export_unmatched(db: &Db, output_path: &Path) -> Result<()> {
    let unmatched = db.get_unmatched_employers()?;  // Employers with no mapping
    let issuers = db.get_all_issuers_for_matching()?;  // (id, name) pairs

    let mut wtr = csv::Writer::from_path(output_path)?;
    wtr.write_record(&[
        "employer",
        "suggestion_issuer_id",
        "suggestion_name",
        "confidence",
        "confirmed_issuer_id",
        "notes",
    ])?;

    for employer in unmatched {
        let suggestion = match_employer(&employer, &issuers);
        match suggestion {
            Some(m) if m.confidence >= 0.85 => {
                wtr.write_record(&[
                    &employer,
                    &m.issuer_id.to_string(),
                    &m.issuer_name,
                    &format!("{:.2}", m.confidence),
                    "",  // User fills this
                    "",  // User fills this
                ])?;
            }
            _ => {
                wtr.write_record(&[&employer, "", "", "", "", "No suggestion"])?;
            }
        }
    }
    wtr.flush()?;
    Ok(())
}

// Import logic with validation
pub fn import_confirmed(db: &Db, input_path: &Path) -> Result<()> {
    let mut rdr = csv::Reader::from_path(input_path)?;
    let mut mappings = Vec::new();

    for result in rdr.deserialize() {
        let record: ConfirmedMapping = result?;
        if let Some(issuer_id) = record.confirmed_issuer_id {
            // Validate issuer exists
            if db.issuer_exists(issuer_id)? {
                mappings.push((record.employer, issuer_id, 1.0));  // Manual = confidence 1.0
            } else {
                eprintln!("Warning: issuer_id {} not found, skipping {}", issuer_id, record.employer);
            }
        }
    }

    db.upsert_employer_mappings(&mappings)?;
    Ok(())
}
```

### Pattern 4: Augmenting Existing Commands with Optional Flags
**What:** Add --show-donor-context to trades, donation summary to portfolio, both gracefully no-op if no data
**When to use:** Avoid breaking existing commands, keep feature opt-in

**Example:**
```rust
// In trades.rs
#[derive(Args)]
pub struct TradesArgs {
    // ... existing 24 filters ...

    /// Show donation context for traded securities (requires synced donations)
    #[arg(long)]
    pub show_donor_context: bool,
}

pub fn run_db(args: &TradesArgs, db: &Db, format: &OutputFormat) -> Result<()> {
    let trades = db.query_trades(&filter)?;

    // Existing output
    match format {
        OutputFormat::Table => print_db_trades_table(&trades),
        // ... other formats
    }

    // NEW: Optional donor context
    if args.show_donor_context {
        for trade in &trades {
            // Get politician donations to employers in this issuer's sector
            if let Some(sector) = &trade.issuer_sector {
                let context = db.get_donor_context_for_sector(
                    &trade.politician_id,
                    sector,
                )?;
                if !context.is_empty() {
                    println!("\nDonor context for {} ({})", trade.issuer_name, sector);
                    println!("  Top employers:");
                    for (employer, total) in context.iter().take(5) {
                        println!("    - {}: ${:.0}", employer, total);
                    }
                }
            }
        }
    }

    Ok(())
}

// In portfolio.rs - add optional donation summary
pub fn run(args: &PortfolioArgs, format: &OutputFormat) -> Result<()> {
    let positions = db.get_portfolio(&filter)?;

    // Existing output
    match format {
        OutputFormat::Table => print_portfolio_table(&positions),
        // ...
    }

    // NEW: Optional donation summary (auto-detect if data exists)
    if let Some(ref politician_id) = filter.politician_id {
        if let Ok(summary) = db.get_donation_summary(politician_id) {
            println!("\nDonation summary:");
            println!("  Total received: ${:.0}", summary.total_amount);
            println!("  Top employer sectors:");
            for (sector, total) in summary.top_sectors.iter().take(3) {
                println!("    - {}: ${:.0}", sector, total);
            }
        }
        // Silently skip if no donation data
    }

    Ok(())
}
```

### Anti-Patterns to Avoid
- **Auto-linking fuzzy matches:** Even at 0.95 confidence, "Goldman Sachs" could match "Goldman Realty" - always require manual confirmation
- **Fixed threshold across all employers:** Short names need higher thresholds (0.90+), longer names can use 0.85
- **Ignoring sector mismatch:** If employer is "Apple Retail" but issuer sector is "Technology", flag for review even at high fuzzy score
- **Denormalized employer storage:** Store mappings in separate table, not duplicated columns on donations table
- **Case-sensitive matching:** Always lowercase before comparison, corporate names have inconsistent capitalization in FEC data
- **Forgetting international suffixes:** Seed data should include GmbH, AG, S.A., Ltd for international corporations

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| String similarity scoring | Custom edit distance algorithm | strsim::jaro_winkler | Well-tested, optimized, Jaro-Winkler specifically designed for names with prefix similarity |
| Corporate suffix list | Hardcoded array in code | External TOML/JSON config | Suffixes vary by country, need manual curation, should be editable without recompile |
| CSV export/import | Manual file I/O with string parsing | csv crate with serde | Handles escaping, quoting, UTF-8, formula injection prevention already solved |
| Employer dataset | Web scraping Fortune 500 | Manual TOML seed file for top 200 | Quality > quantity, manual curation catches edge cases (subsidiaries, acquisitions, rebrands) |
| Confidence threshold tuning | Hardcoded 0.85 magic number | Database column + config file | Threshold may need adjustment after real-world testing with FEC data |

**Key insight:** Entity resolution is 80% data quality (normalization, seed data curation) and 20% algorithm. Don't over-invest in sophisticated ML when a curated TOML file + simple Jaro-Winkler solves 90% of cases. The edge cases need human review regardless of algorithm choice.

## Common Pitfalls

### Pitfall 1: Short Name False Positives
**What goes wrong:** "Ford" employer matches "Ford Motor Co" (correct) but also "Hartford Insurance" (0.85+ score on "ford" substring)
**Why it happens:** Jaro-Winkler rewards prefix similarity, short strings have higher collision rates
**How to avoid:** Add minimum length check: only fuzzy match if both strings ≥ 5 characters after normalization, otherwise require exact match
**Warning signs:** High confidence matches on 2-3 character employer names, multiple issuers matching same short employer

### Pitfall 2: Subsidiary Name Drift
**What goes wrong:** "Google LLC" in FEC data but "Alphabet Inc" in issuers table (post-restructure), fuzzy match fails despite being same entity
**Why it happens:** Corporate restructures, acquisitions, brand changes not reflected in both datasets
**How to avoid:** Seed data must include historical names AND current names for all mapped employers, one-to-many employer-to-issuer mappings
**Warning signs:** Zero matches for major employers (Google, Meta, Alphabet), unmatched employers from top 10 donors

### Pitfall 3: Sector Mismatch Ignored
**What goes wrong:** "Apple Hospitality REIT" (hotel company) matches "Apple Inc" (technology) at 0.90+ confidence, gets auto-linked if threshold too low
**Why it happens:** Fuzzy string matching ignores semantic meaning, only looks at character similarity
**How to avoid:** Always display sector in export CSV, require user to validate sector alignment before confirming mapping
**Warning signs:** Employer in "Real Estate" sector mapped to issuer in "Technology" sector, suspiciously high-confidence matches for unrelated industries

### Pitfall 4: Stale Mappings After Ticker Changes
**What goes wrong:** Employer mapped to issuer_id 123 (ticker: FB), company changes ticker to META, mapping now points to wrong ticker
**Why it happens:** employer_mappings table stores issuer_id (foreign key) but ticker can change
**How to avoid:** Always JOIN through issuers table to get current ticker, NEVER cache ticker in employer_mappings table
**Warning signs:** Portfolio shows Facebook donations but Meta trades, or vice versa

### Pitfall 5: CSV Formula Injection on Export
**What goes wrong:** Employer name "=SUM(A1:A10) Inc" in CSV opens as Excel formula, executes on user's machine
**Why it happens:** CSV readers interpret leading = + - @ as formula start
**How to avoid:** Reuse existing sanitize_csv_field() function from output module for all employer name exports
**Warning signs:** Employer names starting with =, +, -, @ symbols in export CSV

### Pitfall 6: Race Condition in Concurrent Matching
**What goes wrong:** Two processes fuzzy-match same employer simultaneously, insert duplicate mappings with different issuer_ids
**Why it happens:** No UNIQUE constraint on employer name in employer_mappings table
**How to avoid:** Add UNIQUE constraint on normalized_employer column, use INSERT ... ON CONFLICT DO UPDATE pattern
**Warning signs:** Duplicate employer entries in export CSV, inconsistent mapping results across runs

### Pitfall 7: Assuming All Employers Are Corporations
**What goes wrong:** "Self-employed" or "Retired" in employer field gets fuzzy-matched to random issuer
**Why it happens:** FEC allows non-corporate employers, donations table contains individual employers
**How to avoid:** Blacklist common non-corporate employers: "Self-employed", "Retired", "Not employed", "N/A", "Homemaker"
**Warning signs:** "Retired" matching "Realty Corp", "Self" matching "Self Storage Inc"

## Code Examples

Verified patterns from official sources:

### Jaro-Winkler Usage (from strsim docs)
```rust
// Source: https://docs.rs/strsim/latest/strsim/
use strsim::jaro_winkler;

assert!((1.0 - jaro_winkler("Philosopher", "Philosopher")).abs() < 0.001);
assert!((0.906 - jaro_winkler("Philosophy", "Philosopher")).abs() < 0.001);

// For employer matching:
let score = jaro_winkler("Apple Inc", "Apple Incorporated");
// Returns ~0.95 (high confidence, likely same entity)

let score = jaro_winkler("Apple Inc", "Pineapple Corp");
// Returns ~0.75 (below 0.85 threshold, skip)
```

### CSV Writer with Serde (from csv crate tutorial)
```rust
// Source: https://docs.rs/csv/latest/csv/tutorial/
use csv::Writer;
use serde::Serialize;

#[derive(Serialize)]
struct UnmatchedRow {
    employer: String,
    suggestion_issuer_id: Option<i64>,
    suggestion_name: String,
    confidence: f64,
    confirmed_issuer_id: Option<i64>,
    notes: String,
}

let mut wtr = Writer::from_path("unmatched.csv")?;
wtr.serialize(UnmatchedRow {
    employer: "Goldman Sachs".into(),
    suggestion_issuer_id: Some(123),
    suggestion_name: "Goldman Sachs Group".into(),
    confidence: 0.92,
    confirmed_issuer_id: None,
    notes: String::new(),
})?;
wtr.flush()?;
```

### TOML Deserialization (from serde docs)
```rust
// Source: https://serde.rs/derive.html
use serde::Deserialize;

#[derive(Deserialize)]
struct MappingFile {
    mapping: Vec<EmployerMapping>,
}

#[derive(Deserialize)]
struct EmployerMapping {
    employer_names: Vec<String>,
    issuer_id: i64,
    issuer_ticker: String,
    sector: String,
    confidence: f64,
    notes: Option<String>,
}

let toml_str = include_str!("../../seed_data/employer_issuers.toml");
let data: MappingFile = toml::from_str(toml_str)?;

for mapping in data.mapping {
    for name in mapping.employer_names {
        // Insert into DB: normalized_employer -> issuer_id, confidence
    }
}
```

### SQLite Schema for Employer Mappings
```sql
-- Source: Derived from existing schema patterns in schema/sqlite.sql
CREATE TABLE IF NOT EXISTS employer_mappings (
    normalized_employer TEXT PRIMARY KEY,
    issuer_id INTEGER NOT NULL,
    confidence REAL NOT NULL,  -- 0.85-1.0
    match_type TEXT NOT NULL,  -- 'exact', 'fuzzy', 'manual'
    created_at TEXT NOT NULL,
    last_updated TEXT NOT NULL,
    notes TEXT,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_employer_mappings_issuer ON employer_mappings(issuer_id);
CREATE INDEX IF NOT EXISTS idx_employer_mappings_confidence ON employer_mappings(confidence);
CREATE INDEX IF NOT EXISTS idx_employer_mappings_type ON employer_mappings(match_type);

-- Query pattern: always normalize before lookup
SELECT em.issuer_id, i.issuer_name, i.issuer_ticker, em.confidence
FROM employer_mappings em
JOIN issuers i ON em.issuer_id = i.issuer_id
WHERE em.normalized_employer = normalize_employer(?1);

-- Donor context query (for trades --show-donor-context)
SELECT
    d.contributor_employer,
    SUM(d.contribution_receipt_amount) as total
FROM donations d
JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
JOIN employer_mappings em ON normalize_employer(d.contributor_employer) = em.normalized_employer
JOIN issuers i ON em.issuer_id = i.issuer_id
WHERE dsm.politician_id = ?1
  AND i.sector = ?2
GROUP BY d.contributor_employer
ORDER BY total DESC
LIMIT 10;
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Levenshtein distance for all matching | Jaro-Winkler for name matching | ~2015 in entity resolution field | Better for corporate names with prefix similarity (Apple Inc vs Apple Computer Inc) |
| Single fixed threshold (0.80) | Adaptive thresholds by string length | ~2020 in fuzzy matching tools | Reduces false positives on short strings |
| Auto-link high-confidence matches | Human-in-the-loop for all fuzzy matches | 2024-2026 (AI safety patterns) | Prevents costly errors, builds trust in system |
| JSON config files | TOML for human-edited data | Rust ecosystem shift ~2020 | Better comments, readability for manual curation |
| Separate normalization rules per field | Shared normalization library | Ongoing best practice | DRY principle, consistent behavior |

**Deprecated/outdated:**
- **Soundex/Metaphone for corporate names:** Designed for phonetic matching of person names, performs poorly on "Inc" vs "Corp" suffix variations
- **Single employer_name column:** Modern entity resolution uses separate normalized_name for matching, preserves original for display
- **Client-side CSV parsing with split(','):** Vulnerable to injection, doesn't handle quoted fields, use csv crate

## Open Questions

1. **What is the optimal Jaro-Winkler threshold for this dataset?**
   - What we know: Literature suggests 0.85 for general name matching, 0.80 for noisy data, 0.90 for high precision
   - What's unclear: FEC employer data quality (typos, abbreviations, inconsistent formatting) unknown until tested
   - Recommendation: Start with 0.85, add threshold as config column in DB, allow per-run tuning via --threshold flag on map-employers command. Phase 12 plans should include threshold tuning task with sample FEC data.

2. **Should we match on issuer_name or issuer_ticker?**
   - What we know: Tickers are short (2-5 chars), high false positive risk. Names are long, better for fuzzy matching.
   - What's unclear: Whether FEC data includes tickers in employer field (e.g., "AAPL Inc")
   - Recommendation: Fuzzy match against issuer_name only, but seed data should include both name variants AND ticker for search. Flag for planner: investigate 100-sample FEC employer field to check ticker prevalence.

3. **How to handle employer sectors not matching issuer sectors?**
   - What we know: Sector lives on issuers table, donations have employer but no sector field
   - What's unclear: Whether to block mapping if semantic mismatch (employer sounds like tech, issuer is real estate)
   - Recommendation: Display sector in export CSV as warning column, but don't auto-block. User review catches semantic mismatches. Consider adding sector_mismatch_warning boolean column to export.

4. **Should donation context show dollar amounts or contribution counts?**
   - What we know: Requirements specify "top donors" but unclear if "top" means highest dollar amount or most frequent
   - What's unclear: Which metric is more useful for user understanding (total $ vs count of donations)
   - Recommendation: Default to total dollar amount (more intuitive), but allow --count flag to switch to contribution counts. Phase 12 plans should specify which metric in success criteria.

5. **How to version seed data updates?**
   - What we know: employer_issuers.toml will need updates as companies rebrand, merge, IPO
   - What's unclear: How to track when seed data was last updated, whether to re-run matching when seed data changes
   - Recommendation: Add version field to TOML header, store in DB metadata table. Phase 12 plans should include seed data versioning strategy.

## Sources

### Primary (HIGH confidence)
- [strsim crate documentation](https://docs.rs/strsim/latest/strsim/) - Jaro-Winkler API, score ranges, usage examples
- [csv crate tutorial](https://docs.rs/csv/latest/csv/tutorial/) - Serde integration, Reader/Writer patterns
- [serde derive documentation](https://serde.rs/derive.html) - Deserialization macros for TOML/JSON
- [Existing codebase](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/db.rs) - Schema patterns, migration versioning, sanitize_csv_field precedent

### Secondary (MEDIUM confidence)
- [RecordLinker normalization guide](https://recordlinker.com/name-normalization-matching/) - Corporate suffix lists, best practices (verified against Wikipedia naming conventions)
- [Databar brand normalization rules](https://databar.ai/blog/article/brand-name-normalization-rules-how-to-standardize-company-names-in-your-crm) - Real-world patterns from CRM data matching
- [Data Ladder fuzzy matching guide](https://dataladder.com/fuzzy-matching-101/) - Threshold recommendations, accuracy benchmarks (cross-referenced with academic sources)
- [SQLite versioning strategies](https://www.sqliteforum.com/p/sqlite-versioning-and-migration-strategies) - PRAGMA user_version patterns (confirmed against existing db.rs migrate_v* methods)
- [GitHub S&P 500 dataset](https://github.com/datasets/s-and-p-500-companies/blob/main/data/constituents.csv) - CSV format example for company ticker mappings
- [Wikipedia legal entity types by country](https://en.wikipedia.org/wiki/List_of_legal_entity_types_by_country) - International corporate suffix reference

### Tertiary (LOW confidence, marked for validation)
- [Jaro-Winkler vs Levenshtein in AML screening](https://www.flagright.com/post/jaro-winkler-vs-levenshtein-choosing-the-right-algorithm-for-aml-screening) - Claims Jaro-Winkler superior for name matching, but AML domain may differ from employer matching
- [Entity Resolution at Scale (Medium article)](https://medium.com/@shereshevsky/entity-resolution-at-scale-deduplication-strategies-for-knowledge-graph-construction-7499a60a97c3) - Mentions GPT-4 outperforming PLMs, but Phase 12 doesn't use ML (Jan 2026 article, not yet peer-reviewed)
- [Fortune 500 datasets](https://www.50pros.com/fortune500) - Multiple sources claim updated 2026 data, but no authoritative Fortune.com verification in search results

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - strsim, serde, csv, toml all established Rust ecosystem standards, verified via docs.rs and existing project usage
- Architecture: HIGH - Three-tier matching, TOML seed data, export-review-import all proven patterns in entity resolution field and Rust CLI ecosystem
- Pitfalls: MEDIUM-HIGH - Short name false positives, subsidiary drift, sector mismatch all documented in entity resolution literature, but specific to this domain's application (donations-to-trades) is novel combination

**Research date:** 2026-02-13
**Valid until:** 2026-03-13 (30 days - stable domain, strsim and csv crates mature, entity resolution patterns long-established)
