//! Congressional committee jurisdiction mapping to GICS sectors.
//!
//! This module provides compile-time YAML-based mapping of congressional committee
//! short codes (from CapitolTrades scrape data) to GICS sectors under each committee's
//! legislative jurisdiction.

use serde::Deserialize;
use std::collections::HashSet;

use crate::sector_mapping::{validate_sector, SectorMappingError};

/// A single committee-to-sectors jurisdiction mapping.
#[derive(Deserialize, Debug, Clone)]
pub struct CommitteeJurisdiction {
    pub committee_name: String,
    pub chamber: String,
    pub full_name: String,
    pub sectors: Vec<String>,
    pub notes: Option<String>,
}

/// Top-level structure for committee jurisdiction YAML file.
#[derive(Deserialize, Debug)]
struct CommitteeJurisdictionFile {
    committees: Vec<CommitteeJurisdiction>,
}

/// Parse and validate committee jurisdictions from YAML content.
///
/// # Arguments
/// * `yaml_content` - YAML string to parse
///
/// # Returns
/// * `Ok(Vec<CommitteeJurisdiction>)` - Validated committee mappings with normalized sector names
/// * `Err(SectorMappingError)` - Parse error, invalid sector, or invalid chamber
fn parse_committee_jurisdictions(yaml_content: &str) -> Result<Vec<CommitteeJurisdiction>, SectorMappingError> {
    let file: CommitteeJurisdictionFile = serde_yml::from_str(yaml_content)?;

    let mut validated = Vec::new();

    for committee in file.committees {
        // Validate chamber is House or Senate
        if committee.chamber != "House" && committee.chamber != "Senate" {
            return Err(SectorMappingError::InvalidSector(
                format!(
                    "Invalid chamber '{}' for committee '{}'. Must be 'House' or 'Senate'",
                    committee.chamber, committee.committee_name
                )
            ));
        }

        // Validate and normalize all sectors
        let mut normalized_sectors = Vec::new();
        for sector in &committee.sectors {
            let normalized = validate_sector(sector)?;
            normalized_sectors.push(normalized);
        }

        validated.push(CommitteeJurisdiction {
            committee_name: committee.committee_name,
            chamber: committee.chamber,
            full_name: committee.full_name,
            sectors: normalized_sectors,
            notes: committee.notes,
        });
    }

    Ok(validated)
}

/// Load committee jurisdictions from embedded YAML file at compile time.
///
/// # Returns
/// * `Ok(Vec<CommitteeJurisdiction>)` - Validated committee jurisdiction mappings
/// * `Err(SectorMappingError)` - Parse or validation error
pub fn load_committee_jurisdictions() -> Result<Vec<CommitteeJurisdiction>, SectorMappingError> {
    let yaml_content = include_str!("../../seed_data/committee_sectors.yml");
    parse_committee_jurisdictions(yaml_content)
}

/// Validate committee jurisdictions against GICS sectors and chamber rules.
///
/// # Arguments
/// * `jurisdictions` - Slice of CommitteeJurisdiction to validate
///
/// # Returns
/// * `Ok(())` - All jurisdictions valid
/// * `Err(SectorMappingError)` - Invalid sector or chamber found
pub fn validate_committee_jurisdictions(
    jurisdictions: &[CommitteeJurisdiction],
) -> Result<(), SectorMappingError> {
    for committee in jurisdictions {
        // Validate all sectors are valid GICS sectors
        for sector in &committee.sectors {
            validate_sector(sector)?;
        }

        // Validate chamber is House or Senate
        if committee.chamber != "House" && committee.chamber != "Senate" {
            return Err(SectorMappingError::InvalidSector(
                format!(
                    "Invalid chamber '{}' for committee '{}'",
                    committee.chamber, committee.committee_name
                )
            ));
        }
    }
    Ok(())
}

/// Get all GICS sectors under a politician's committee jurisdictions.
///
/// Deduplicates overlapping jurisdictions using a HashSet. If a politician
/// serves on multiple committees with overlapping sectors, each sector is
/// counted only once.
///
/// # Arguments
/// * `jurisdictions` - All committee jurisdiction mappings
/// * `politician_committees` - Slice of committee short codes the politician serves on
///
/// # Returns
/// * `HashSet<String>` - Deduplicated set of GICS sectors under politician's committees
///
/// # Example
/// ```
/// use capitoltraders_lib::committee_jurisdiction::{load_committee_jurisdictions, get_committee_sectors};
///
/// let jurisdictions = load_committee_jurisdictions().unwrap();
/// let committees = vec!["hsba".to_string(), "hsif".to_string()];
/// let sectors = get_committee_sectors(&jurisdictions, &committees);
/// // sectors contains: Financials, Energy, Utilities, Communication Services, Health Care,
/// // Consumer Discretionary, Consumer Staples (deduplicated)
/// ```
pub fn get_committee_sectors(
    jurisdictions: &[CommitteeJurisdiction],
    politician_committees: &[String],
) -> HashSet<String> {
    let mut sectors = HashSet::new();

    for committee_code in politician_committees {
        if let Some(jurisdiction) = jurisdictions.iter()
            .find(|j| j.committee_name == *committee_code) {
            for sector in &jurisdiction.sectors {
                sectors.insert(sector.clone());
            }
        }
        // Unknown committee codes are silently skipped (no error)
    }

    sectors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_committee_jurisdictions_succeeds() {
        let result = load_committee_jurisdictions();
        assert!(result.is_ok(), "Failed to load: {:?}", result.err());
        let jurisdictions = result.unwrap();
        assert!(
            jurisdictions.len() >= 15,
            "Expected at least 15 committees, got {}",
            jurisdictions.len()
        );
    }

    #[test]
    fn test_all_sectors_valid_gics() {
        let jurisdictions = load_committee_jurisdictions().unwrap();

        use crate::sector_mapping::GICS_SECTORS;

        for committee in &jurisdictions {
            for sector in &committee.sectors {
                assert!(
                    GICS_SECTORS.contains(&sector.as_str()),
                    "Committee '{}' has invalid sector: {}",
                    committee.committee_name,
                    sector
                );
            }
        }
    }

    #[test]
    fn test_get_committee_sectors_single() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["hsba".to_string()]; // House Financial Services

        let sectors = get_committee_sectors(&jurisdictions, &committees);

        assert_eq!(sectors.len(), 1);
        assert!(sectors.contains("Financials"));
    }

    #[test]
    fn test_get_committee_sectors_overlap() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        // House Energy and Commerce (hsif) includes Health Care
        // House Veterans' Affairs (hsvc) includes Health Care
        let committees = vec!["hsif".to_string(), "hsvc".to_string()];

        let sectors = get_committee_sectors(&jurisdictions, &committees);

        // Health Care should appear only once (deduplicated)
        let health_care_count = sectors.iter().filter(|s| *s == "Health Care").count();
        assert_eq!(health_care_count, 1, "Health Care should be deduplicated");

        // Should contain sectors from both committees
        assert!(sectors.contains("Health Care"));
        assert!(sectors.contains("Energy")); // From hsif
    }

    #[test]
    fn test_get_committee_sectors_unknown_committee() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["unknown_code".to_string()];

        let sectors = get_committee_sectors(&jurisdictions, &committees);

        // Unknown committees are silently skipped
        assert_eq!(sectors.len(), 0);
    }

    #[test]
    fn test_empty_sectors_committee() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["hswm".to_string()]; // House Ways and Means (empty sectors)

        let sectors = get_committee_sectors(&jurisdictions, &committees);

        assert_eq!(sectors.len(), 0, "Ways and Means should have no sector-specific jurisdiction");
    }

    #[test]
    fn test_chamber_validation() {
        let yaml = r#"
committees:
  - committee_name: test
    chamber: InvalidChamber
    full_name: Test Committee
    sectors: []
"#;
        let result = parse_committee_jurisdictions(yaml);
        assert!(result.is_err(), "Should reject invalid chamber");
    }

    #[test]
    fn test_validate_committee_jurisdictions_valid() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let result = validate_committee_jurisdictions(&jurisdictions);
        assert!(result.is_ok(), "All loaded jurisdictions should be valid");
    }
}
