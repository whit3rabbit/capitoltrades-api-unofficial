//! Employer-to-issuer mapping with normalization and fuzzy matching.
//!
//! This module provides pure logic for matching employer names from FEC donation data
//! to issuer records. It includes normalization, blacklisting, exact matching, and
//! configurable fuzzy matching via Jaro-Winkler similarity.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error types for employer mapping operations.
#[derive(Error, Debug)]
pub enum EmployerMappingError {
    #[error("TOML parse error: {0}")]
    TomlParse(String),
    #[error("Invalid seed data: {0}")]
    InvalidSeedData(String),
}

/// Type of match found.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum MatchType {
    Exact,
    Fuzzy,
    Manual,
}

/// Result of matching an employer to an issuer.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult {
    pub issuer_id: i64,
    pub issuer_name: String,
    pub issuer_ticker: String,
    pub confidence: f64,
    pub match_type: MatchType,
}

/// Seed mapping from TOML file.
#[derive(Deserialize, Debug, Clone)]
pub struct SeedMapping {
    pub employer_names: Vec<String>,
    pub issuer_ticker: String,
    pub sector: String,
    pub confidence: f64,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Top-level structure for seed TOML file.
#[derive(Deserialize, Debug)]
struct SeedFile {
    mapping: Vec<SeedMapping>,
}

/// Employers that should never be matched (non-corporate).
const EMPLOYER_BLACKLIST: &[&str] = &[
    "self-employed",
    "self employed",
    "retired",
    "not employed",
    "n/a",
    "none",
    "homemaker",
    "student",
    "unemployed",
    "information requested",
    "information requested per best efforts",
];

/// Corporate suffixes to strip during normalization.
/// Sorted by length descending so longer suffixes match first.
const CORPORATE_SUFFIXES: &[&str] = &[
    "information requested per best efforts",
    "corporation",
    "incorporated",
    "partnership",
    "associates",
    "holdings",
    "partners",
    "limited",
    "company",
    "l.l.c.",
    "group",
    "gmbh",
    "corp",
    "inc",
    "llc",
    "ltd",
    "l.p.",
    "plc",
    "n.v.",
    "s.a.",
    "co",
    "ag",
];

/// Check if an employer name is blacklisted (non-corporate).
pub fn is_blacklisted(employer: &str) -> bool {
    let normalized = employer.trim().to_lowercase();

    for blacklisted in EMPLOYER_BLACKLIST {
        if normalized == *blacklisted || normalized.starts_with(blacklisted) {
            return true;
        }
    }

    false
}

/// Normalize an employer name for matching.
///
/// Steps:
/// 1. Trim whitespace
/// 2. Convert to lowercase
/// 3. Strip one trailing corporate suffix (if present)
/// 4. Strip trailing dots, commas, spaces after suffix removal
/// 5. Collapse multiple spaces to single space
///
/// Returns empty string for empty input.
pub fn normalize_employer(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }

    let mut normalized = raw.trim().to_lowercase();

    // Strip one trailing corporate suffix
    for suffix in CORPORATE_SUFFIXES {
        // Check if it ends with the suffix (with optional trailing punctuation/whitespace)
        if let Some(stripped) = normalized.strip_suffix(suffix) {
            normalized = stripped.to_string();
            break; // Only strip one suffix
        }
    }

    // Strip trailing dots, commas, spaces
    normalized = normalized.trim_end_matches(['.', ',', ' ']).to_string();

    // Collapse multiple spaces to single space
    let words: Vec<&str> = normalized.split_whitespace().collect();
    words.join(" ")
}

/// Match an employer name to a list of issuers.
///
/// # Arguments
/// * `employer` - The employer name to match
/// * `issuers` - Slice of (issuer_id, issuer_name, issuer_ticker) tuples
/// * `threshold` - Minimum Jaro-Winkler similarity score for fuzzy matches (typically 0.85)
///
/// # Returns
/// * `Some(MatchResult)` if a match is found
/// * `None` if blacklisted or no match found
///
/// # Matching tiers
/// 1. Blacklist check (returns None if matched)
/// 2. Exact match: normalized employer == normalized issuer_name (confidence 1.0)
/// 3. Fuzzy match: Jaro-Winkler >= threshold, only for employer names >= 5 chars after normalization
pub fn match_employer(
    employer: &str,
    issuers: &[(i64, String, String)],
    threshold: f64,
) -> Option<MatchResult> {
    // Blacklist check
    if is_blacklisted(employer) {
        return None;
    }

    let normalized_employer = normalize_employer(employer);

    // Exact match tier
    for (issuer_id, issuer_name, issuer_ticker) in issuers {
        let normalized_issuer = normalize_employer(issuer_name);

        if normalized_employer == normalized_issuer {
            return Some(MatchResult {
                issuer_id: *issuer_id,
                issuer_name: issuer_name.clone(),
                issuer_ticker: issuer_ticker.clone(),
                confidence: 1.0,
                match_type: MatchType::Exact,
            });
        }
    }

    // Fuzzy match tier (only for names >= 5 chars)
    if normalized_employer.len() >= 5 {
        let mut best_match: Option<(f64, &(i64, String, String))> = None;

        for issuer in issuers {
            let normalized_issuer = normalize_employer(&issuer.1);
            let score = strsim::jaro_winkler(&normalized_employer, &normalized_issuer);

            if score >= threshold {
                if let Some((best_score, _)) = best_match {
                    if score > best_score {
                        best_match = Some((score, issuer));
                    }
                } else {
                    best_match = Some((score, issuer));
                }
            }
        }

        if let Some((score, (issuer_id, issuer_name, issuer_ticker))) = best_match {
            return Some(MatchResult {
                issuer_id: *issuer_id,
                issuer_name: issuer_name.clone(),
                issuer_ticker: issuer_ticker.clone(),
                confidence: score,
                match_type: MatchType::Fuzzy,
            });
        }
    }

    None
}

/// Load seed data from embedded TOML file.
///
/// The TOML file is included at compile time via include_str!.
/// Returns the list of seed mappings or an error if parsing fails.
pub fn load_seed_data() -> Result<Vec<SeedMapping>, EmployerMappingError> {
    let toml_content = include_str!("../../seed_data/employer_issuers.toml");

    let seed_file: SeedFile = toml::from_str(toml_content)
        .map_err(|e| EmployerMappingError::TomlParse(e.to_string()))?;

    Ok(seed_file.mapping)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_basic() {
        assert_eq!(normalize_employer("Apple Inc"), "apple");
    }

    #[test]
    fn test_normalize_llc() {
        assert_eq!(normalize_employer("Google LLC"), "google");
    }

    #[test]
    fn test_normalize_corporation() {
        assert_eq!(normalize_employer("Microsoft Corporation"), "microsoft");
    }

    #[test]
    fn test_normalize_preserves_spaces() {
        assert_eq!(normalize_employer("Goldman Sachs Group"), "goldman sachs");
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(normalize_employer(""), "");
    }

    #[test]
    fn test_normalize_whitespace_collapse() {
        assert_eq!(normalize_employer("  Apple   Inc  "), "apple");
    }

    #[test]
    fn test_normalize_international() {
        assert_eq!(normalize_employer("Siemens AG"), "siemens");
    }

    #[test]
    fn test_blacklisted_retired() {
        assert!(is_blacklisted("Retired"));
    }

    #[test]
    fn test_blacklisted_self_employed() {
        assert!(is_blacklisted("SELF-EMPLOYED"));
    }

    #[test]
    fn test_blacklisted_normal() {
        assert!(!is_blacklisted("Apple Inc"));
    }

    #[test]
    fn test_blacklisted_na() {
        assert!(is_blacklisted("N/A"));
    }

    #[test]
    fn test_match_exact() {
        let issuers = vec![
            (1, "Apple".to_string(), "AAPL".to_string()),
            (2, "Microsoft".to_string(), "MSFT".to_string()),
        ];

        let result = match_employer("Apple", &issuers, 0.85);
        assert!(result.is_some());

        let match_result = result.unwrap();
        assert_eq!(match_result.issuer_ticker, "AAPL");
        assert_eq!(match_result.confidence, 1.0);
        assert_eq!(match_result.match_type, MatchType::Exact);
    }

    #[test]
    fn test_match_fuzzy() {
        let issuers = vec![
            (1, "Apple".to_string(), "AAPL".to_string()),
        ];

        let result = match_employer("Apple Computer", &issuers, 0.85);
        assert!(result.is_some());

        let match_result = result.unwrap();
        assert_eq!(match_result.issuer_ticker, "AAPL");
        assert!(match_result.confidence >= 0.85);
        assert!(match_result.confidence < 1.0);
        assert_eq!(match_result.match_type, MatchType::Fuzzy);
    }

    #[test]
    fn test_match_blacklisted_returns_none() {
        let issuers = vec![
            (1, "Apple".to_string(), "AAPL".to_string()),
        ];

        let result = match_employer("Retired", &issuers, 0.85);
        assert!(result.is_none());
    }

    #[test]
    fn test_match_short_name_no_fuzzy() {
        let issuers = vec![
            (1, "IBMC Corp".to_string(), "IBMC".to_string()),
        ];

        // "IBM" is only 3 chars after normalization, so no fuzzy matching
        let result = match_employer("IBM", &issuers, 0.85);
        assert!(result.is_none());
    }

    #[test]
    fn test_match_no_match() {
        let issuers = vec![
            (1, "Apple".to_string(), "AAPL".to_string()),
            (2, "Microsoft".to_string(), "MSFT".to_string()),
        ];

        let result = match_employer("Random Xyz Company", &issuers, 0.85);
        assert!(result.is_none());
    }

    #[test]
    fn test_load_seed_data() {
        let result = load_seed_data();
        assert!(result.is_ok());

        let mappings = result.unwrap();
        assert!(!mappings.is_empty());

        // Verify structure of first mapping
        if let Some(first) = mappings.first() {
            assert!(!first.employer_names.is_empty());
            assert!(!first.issuer_ticker.is_empty());
            assert!(!first.sector.is_empty());
            assert_eq!(first.confidence, 1.0);
        }
    }
}
