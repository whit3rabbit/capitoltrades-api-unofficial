//! Ticker alias resolution for price enrichment.
//!
//! Maps raw CapitolTrades tickers to their correct Yahoo Finance equivalents,
//! handling renamed stocks, delistings, and known mismatches that
//! `normalize_ticker_for_yahoo()` cannot resolve via format rules alone.
//!
//! Follows the same compile-time `include_str!` pattern as `sector_mapping.rs`.

use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

/// Error types for ticker alias operations.
#[derive(Error, Debug)]
pub enum TickerAliasError {
    #[error("Failed to parse ticker alias YAML: {0}")]
    YamlParse(#[from] serde_yml::Error),
    #[error("Duplicate 'from' ticker in alias file: {0}")]
    DuplicateFrom(String),
}

/// Top-level structure for ticker alias YAML file.
#[derive(Deserialize, Debug)]
pub struct TickerAliasFile {
    pub aliases: Vec<TickerAlias>,
}

/// A single ticker alias mapping.
///
/// `from` is the raw CapitolTrades ticker (e.g., "ATVI:US").
/// `to` is the Yahoo Finance equivalent, or `None` if the ticker is
/// known to be unenrichable (delisted with no successor, money market funds, etc.).
#[derive(Deserialize, Debug, Clone)]
pub struct TickerAlias {
    pub from: String,
    pub to: Option<String>,
}

/// Parse ticker aliases from YAML content.
///
/// Returns a HashMap where keys are raw CapitolTrades tickers and values are
/// `Some(yahoo_ticker)` for resolvable aliases or `None` for known-unenrichable tickers.
pub fn parse_ticker_aliases(
    yaml_content: &str,
) -> Result<HashMap<String, Option<String>>, TickerAliasError> {
    let file: TickerAliasFile = serde_yml::from_str(yaml_content)?;

    let mut map = HashMap::new();
    for alias in file.aliases {
        if map.contains_key(&alias.from) {
            return Err(TickerAliasError::DuplicateFrom(alias.from));
        }
        map.insert(alias.from, alias.to);
    }

    Ok(map)
}

/// Load ticker aliases from embedded YAML file at compile time.
pub fn load_ticker_aliases() -> Result<HashMap<String, Option<String>>, TickerAliasError> {
    let yaml_content = include_str!("../../seed_data/ticker_aliases.yml");
    parse_ticker_aliases(yaml_content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_aliases() {
        let yaml = r#"
aliases:
  - from: "ATVI:US"
    to: "MSFT"
  - from: "FLT:US"
    to: "CPAY"
"#;
        let result = parse_ticker_aliases(yaml).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("ATVI:US"), Some(&Some("MSFT".to_string())));
        assert_eq!(result.get("FLT:US"), Some(&Some("CPAY".to_string())));
    }

    #[test]
    fn test_parse_null_to_marks_unenrichable() {
        let yaml = r#"
aliases:
  - from: "JTSXX:US"
    to: ~
  - from: "SPX:US"
    to: ~
"#;
        let result = parse_ticker_aliases(yaml).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("JTSXX:US"), Some(&None));
        assert_eq!(result.get("SPX:US"), Some(&None));
    }

    #[test]
    fn test_duplicate_from_rejected() {
        let yaml = r#"
aliases:
  - from: "ATVI:US"
    to: "MSFT"
  - from: "ATVI:US"
    to: "ACTIVISION"
"#;
        let result = parse_ticker_aliases(yaml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TickerAliasError::DuplicateFrom(_)));
    }

    #[test]
    fn test_empty_aliases() {
        let yaml = r#"
aliases: []
"#;
        let result = parse_ticker_aliases(yaml).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_load_ticker_aliases_succeeds() {
        let result = load_ticker_aliases();
        assert!(result.is_ok());
        // Just verify it loads without error; actual entries are data-driven.
    }

    #[test]
    fn test_mixed_aliases_and_nulls() {
        let yaml = r#"
aliases:
  - from: "NCR:US"
    to: "VYX"
  - from: "VMFXX:US"
    to: ~
  - from: "CDAY:US"
    to: "DAY"
"#;
        let result = parse_ticker_aliases(yaml).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.get("NCR:US"), Some(&Some("VYX".to_string())));
        assert_eq!(result.get("VMFXX:US"), Some(&None));
        assert_eq!(result.get("CDAY:US"), Some(&Some("DAY".to_string())));
    }
}
