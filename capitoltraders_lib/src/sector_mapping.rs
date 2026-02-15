//! GICS sector mapping for issuers.
//!
//! This module provides compile-time YAML-based mapping of ticker symbols to GICS sectors,
//! with validation against the 11 official GICS sector classifications.

use serde::Deserialize;
use std::collections::HashSet;
use thiserror::Error;

/// Error types for sector mapping operations.
#[derive(Error, Debug)]
pub enum SectorMappingError {
    #[error("Invalid GICS sector: {0}")]
    InvalidSector(String),
    #[error("Failed to parse sector mapping YAML: {0}")]
    YamlParse(#[from] serde_yml::Error),
    #[error("Duplicate ticker in mapping: {0}")]
    DuplicateTicker(String),
}

/// The 11 official GICS sectors.
///
/// Global Industry Classification Standard (GICS) developed by MSCI and S&P.
/// These are the only valid sector names that can be stored in the database.
pub const GICS_SECTORS: &[&str] = &[
    "Communication Services",
    "Consumer Discretionary",
    "Consumer Staples",
    "Energy",
    "Financials",
    "Health Care",
    "Industrials",
    "Information Technology",
    "Materials",
    "Real Estate",
    "Utilities",
];

/// Top-level structure for sector mapping YAML file.
#[derive(Deserialize, Debug)]
pub struct SectorMappingFile {
    pub mappings: Vec<SectorMapping>,
}

/// A single ticker-to-sector mapping.
#[derive(Deserialize, Debug, Clone)]
pub struct SectorMapping {
    pub ticker: String,
    pub sector: String,
}

/// Validate a sector name against the official GICS sectors.
///
/// Performs case-insensitive comparison and returns the official capitalization.
///
/// # Arguments
/// * `sector` - Sector name to validate
///
/// # Returns
/// * `Ok(String)` - Official GICS sector name with correct capitalization
/// * `Err(SectorMappingError::InvalidSector)` - Sector not in GICS list
pub fn validate_sector(sector: &str) -> Result<String, SectorMappingError> {
    let normalized = sector.trim();

    for gics_sector in GICS_SECTORS {
        if gics_sector.eq_ignore_ascii_case(normalized) {
            return Ok(gics_sector.to_string());
        }
    }

    Err(SectorMappingError::InvalidSector(sector.to_string()))
}

/// Parse and validate sector mappings from YAML content.
///
/// # Arguments
/// * `yaml_content` - YAML string to parse
///
/// # Returns
/// * `Ok(Vec<SectorMapping>)` - Validated mappings with normalized sector names
/// * `Err(SectorMappingError)` - Parse error, invalid sector, or duplicate ticker
pub fn parse_sector_mappings(yaml_content: &str) -> Result<Vec<SectorMapping>, SectorMappingError> {
    let file: SectorMappingFile = serde_yml::from_str(yaml_content)?;

    let mut validated = Vec::new();
    let mut seen_tickers = HashSet::new();

    for mapping in file.mappings {
        // Check for duplicate tickers
        if seen_tickers.contains(&mapping.ticker) {
            return Err(SectorMappingError::DuplicateTicker(mapping.ticker.clone()));
        }
        seen_tickers.insert(mapping.ticker.clone());

        // Validate and normalize sector
        let normalized_sector = validate_sector(&mapping.sector)?;

        validated.push(SectorMapping {
            ticker: mapping.ticker,
            sector: normalized_sector,
        });
    }

    Ok(validated)
}

/// Load sector mappings from embedded YAML file at compile time.
///
/// # Returns
/// * `Ok(Vec<SectorMapping>)` - Validated sector mappings
/// * `Err(SectorMappingError)` - Parse or validation error
pub fn load_sector_mappings() -> Result<Vec<SectorMapping>, SectorMappingError> {
    let yaml_content = include_str!("../../seed_data/gics_sector_mapping.yml");
    parse_sector_mappings(yaml_content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gics_sectors_count() {
        assert_eq!(GICS_SECTORS.len(), 11);
    }

    #[test]
    fn test_validate_sector_exact() {
        let result = validate_sector("Information Technology");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Information Technology");
    }

    #[test]
    fn test_validate_sector_case_insensitive() {
        let result = validate_sector("information technology");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Information Technology");
    }

    #[test]
    fn test_validate_sector_invalid() {
        let result = validate_sector("Tech");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SectorMappingError::InvalidSector(_)));
    }

    #[test]
    fn test_parse_minimal_yaml() {
        let yaml = r#"
mappings:
  - ticker: AAPL
    sector: Information Technology
  - ticker: JPM
    sector: Financials
"#;
        let result = parse_sector_mappings(yaml);
        assert!(result.is_ok());
        let mappings = result.unwrap();
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[0].ticker, "AAPL");
        assert_eq!(mappings[0].sector, "Information Technology");
        assert_eq!(mappings[1].ticker, "JPM");
        assert_eq!(mappings[1].sector, "Financials");
    }

    #[test]
    fn test_parse_invalid_sector_rejected() {
        let yaml = r#"
mappings:
  - ticker: AAPL
    sector: FakeStuff
"#;
        let result = parse_sector_mappings(yaml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SectorMappingError::InvalidSector(_)));
    }

    #[test]
    fn test_parse_duplicate_ticker_rejected() {
        let yaml = r#"
mappings:
  - ticker: AAPL
    sector: Information Technology
  - ticker: AAPL
    sector: Financials
"#;
        let result = parse_sector_mappings(yaml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SectorMappingError::DuplicateTicker(_)));
    }

    #[test]
    fn test_load_sector_mappings_succeeds() {
        let result = load_sector_mappings();
        assert!(result.is_ok());
        let mappings = result.unwrap();
        assert!(mappings.len() >= 190, "Expected at least 190 mappings, got {}", mappings.len());

        // Verify all sectors are valid GICS sectors
        for mapping in &mappings {
            assert!(
                GICS_SECTORS.contains(&mapping.sector.as_str()),
                "Sector {} is not a valid GICS sector",
                mapping.sector
            );
        }
    }

    #[test]
    fn test_load_sector_mappings_no_duplicates() {
        let result = load_sector_mappings();
        assert!(result.is_ok());
        let mappings = result.unwrap();

        let mut ticker_set = HashSet::new();
        for mapping in &mappings {
            ticker_set.insert(&mapping.ticker);
        }

        assert_eq!(
            ticker_set.len(),
            mappings.len(),
            "Found duplicate tickers in mapping file"
        );
    }
}
