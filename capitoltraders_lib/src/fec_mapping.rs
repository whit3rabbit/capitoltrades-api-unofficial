//! FEC candidate ID mapping using the congress-legislators dataset.
//!
//! This module provides functionality to download and parse the unitedstates/congress-legislators
//! dataset, and match legislators to existing CapitolTrades politicians via (last_name, state)
//! composite key matching.

use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FecMappingError {
    #[error("Failed to download congress-legislators dataset: {0}")]
    Download(String),
    #[error("Failed to parse YAML: {0}")]
    YamlParse(#[from] serde_yml::Error),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("No FEC IDs found for politician: {0}")]
    NoFecIds(String),
}

/// A legislator from the congress-legislators dataset
#[derive(Deserialize, Debug, Clone)]
pub struct Legislator {
    pub id: LegislatorId,
    pub name: LegislatorName,
    pub terms: Vec<Term>,
}

/// Identifier fields for a legislator
#[derive(Deserialize, Debug, Clone)]
pub struct LegislatorId {
    pub bioguide: String,
    pub fec: Option<Vec<String>>,
    // Other ID fields are optional and not needed for Phase 7
}

/// Name fields for a legislator
#[derive(Deserialize, Debug, Clone)]
pub struct LegislatorName {
    pub first: String,
    pub last: String,
    pub official_full: Option<String>,
}

/// Congressional term information
#[derive(Deserialize, Debug, Clone)]
pub struct Term {
    #[serde(rename = "type")]
    pub term_type: String,
    pub start: String,
    pub end: Option<String>,
    pub state: String,
    pub party: Option<String>,
}

/// Result of matching a legislator to a politician
#[derive(Debug, Clone)]
pub struct FecMapping {
    pub politician_id: String,
    pub fec_candidate_id: String,
    pub bioguide_id: String,
}

const CURRENT_LEGISLATORS_URL: &str =
    "https://raw.githubusercontent.com/unitedstates/congress-legislators/main/legislators-current.yaml";
const HISTORICAL_LEGISLATORS_URL: &str =
    "https://raw.githubusercontent.com/unitedstates/congress-legislators/main/legislators-historical.yaml";

/// Download and parse both current and historical legislators from congress-legislators dataset
pub async fn download_legislators(client: &reqwest::Client) -> Result<Vec<Legislator>, FecMappingError> {
    let mut all = Vec::new();

    for url in &[CURRENT_LEGISLATORS_URL, HISTORICAL_LEGISLATORS_URL] {
        let response = client.get(*url).send().await?;
        if !response.status().is_success() {
            return Err(FecMappingError::Download(
                format!("HTTP {} from {}", response.status(), url)
            ));
        }
        let yaml_content = response.text().await?;
        let legislators: Vec<Legislator> = serde_yml::from_str(&yaml_content)?;
        all.extend(legislators);
    }

    Ok(all)
}

/// Match legislators to politicians using (last_name, state) composite key
///
/// # Arguments
/// * `legislators` - Parsed congress-legislators data
/// * `politicians` - List of (politician_id, last_name, state_id) tuples from database
///
/// # Returns
/// Vector of FecMapping entries, one for each FEC ID found for each matched politician
pub fn match_legislators_to_politicians(
    legislators: &[Legislator],
    politicians: &[(String, String, String)], // (politician_id, last_name, state_id)
) -> Vec<FecMapping> {
    // Build lookup: (lowercase_last_name, uppercase_state) -> politician_id
    let mut lookup: HashMap<(String, String), Vec<String>> = HashMap::new();
    for (pol_id, last_name, state) in politicians {
        let key = (last_name.to_lowercase(), state.to_uppercase());
        lookup.entry(key).or_default().push(pol_id.clone());
    }

    let mut mappings = Vec::new();

    for legislator in legislators {
        // Skip legislators with no FEC IDs
        let fec_ids = match &legislator.id.fec {
            Some(ids) if !ids.is_empty() => ids,
            _ => continue,
        };

        // Use most recent term's state for matching
        let state = match legislator.terms.last() {
            Some(term) => term.state.to_uppercase(),
            None => continue,
        };

        let key = (legislator.name.last.to_lowercase(), state);

        if let Some(pol_ids) = lookup.get(&key) {
            if pol_ids.len() > 1 {
                tracing::warn!(
                    "Multiple politicians match ({}, {}): {:?} -- skipping to avoid incorrect mapping",
                    legislator.name.last, key.1, pol_ids
                );
                continue;
            }

            let pol_id = &pol_ids[0];
            for fec_id in fec_ids {
                mappings.push(FecMapping {
                    politician_id: pol_id.clone(),
                    fec_candidate_id: fec_id.clone(),
                    bioguide_id: legislator.id.bioguide.clone(),
                });
            }
        }
    }

    mappings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_yaml() {
        let yaml = r#"
- id:
    bioguide: A000001
    fec:
      - H0AL05080
  name:
    first: John
    last: Smith
  terms:
    - type: rep
      start: "2021-01-03"
      end: "2023-01-03"
      state: AL
      party: Republican
"#;
        let result: Result<Vec<Legislator>, _> = serde_yml::from_str(yaml);
        assert!(result.is_ok());
        let legislators = result.unwrap();
        assert_eq!(legislators.len(), 1);
        assert_eq!(legislators[0].id.bioguide, "A000001");
        assert_eq!(legislators[0].id.fec, Some(vec!["H0AL05080".to_string()]));
    }

    #[test]
    fn test_matching_exact_match() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "B000001".to_string(),
                fec: Some(vec!["H0CA05080".to_string()]),
            },
            name: LegislatorName {
                first: "Jane".to_string(),
                last: "Doe".to_string(),
                official_full: None,
            },
            terms: vec![Term {
                term_type: "rep".to_string(),
                start: "2021-01-03".to_string(),
                end: Some("2023-01-03".to_string()),
                state: "CA".to_string(),
                party: Some("Democrat".to_string()),
            }],
        }];

        let politicians = vec![(
            "P000123".to_string(),
            "Doe".to_string(),
            "CA".to_string(),
        )];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].politician_id, "P000123");
        assert_eq!(mappings[0].fec_candidate_id, "H0CA05080");
        assert_eq!(mappings[0].bioguide_id, "B000001");
    }

    #[test]
    fn test_matching_no_fec_ids() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "C000001".to_string(),
                fec: None,
            },
            name: LegislatorName {
                first: "Bob".to_string(),
                last: "Smith".to_string(),
                official_full: None,
            },
            terms: vec![Term {
                term_type: "sen".to_string(),
                start: "2021-01-03".to_string(),
                end: Some("2027-01-03".to_string()),
                state: "TX".to_string(),
                party: Some("Republican".to_string()),
            }],
        }];

        let politicians = vec![(
            "P000456".to_string(),
            "Smith".to_string(),
            "TX".to_string(),
        )];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 0, "Should produce no mappings when FEC IDs are None");
    }

    #[test]
    fn test_matching_empty_fec_ids() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "D000001".to_string(),
                fec: Some(vec![]),
            },
            name: LegislatorName {
                first: "Alice".to_string(),
                last: "Johnson".to_string(),
                official_full: None,
            },
            terms: vec![Term {
                term_type: "rep".to_string(),
                start: "2021-01-03".to_string(),
                end: Some("2023-01-03".to_string()),
                state: "NY".to_string(),
                party: Some("Democrat".to_string()),
            }],
        }];

        let politicians = vec![(
            "P000789".to_string(),
            "Johnson".to_string(),
            "NY".to_string(),
        )];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 0, "Should produce no mappings when FEC IDs is empty vec");
    }

    #[test]
    fn test_matching_multiple_fec_ids() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "E000001".to_string(),
                fec: Some(vec![
                    "H0FL01080".to_string(),
                    "H0FL01120".to_string(),
                    "H0FL01140".to_string(),
                ]),
            },
            name: LegislatorName {
                first: "Maria".to_string(),
                last: "Garcia".to_string(),
                official_full: None,
            },
            terms: vec![Term {
                term_type: "rep".to_string(),
                start: "2015-01-03".to_string(),
                end: Some("2023-01-03".to_string()),
                state: "FL".to_string(),
                party: Some("Republican".to_string()),
            }],
        }];

        let politicians = vec![(
            "P000999".to_string(),
            "Garcia".to_string(),
            "FL".to_string(),
        )];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 3, "Should produce one mapping per FEC ID");
        assert_eq!(mappings[0].fec_candidate_id, "H0FL01080");
        assert_eq!(mappings[1].fec_candidate_id, "H0FL01120");
        assert_eq!(mappings[2].fec_candidate_id, "H0FL01140");
        assert!(mappings.iter().all(|m| m.politician_id == "P000999"));
        assert!(mappings.iter().all(|m| m.bioguide_id == "E000001"));
    }

    #[test]
    fn test_matching_name_collision_skips() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "F000001".to_string(),
                fec: Some(vec!["H0OH05080".to_string()]),
            },
            name: LegislatorName {
                first: "John".to_string(),
                last: "Brown".to_string(),
                official_full: None,
            },
            terms: vec![Term {
                term_type: "rep".to_string(),
                start: "2021-01-03".to_string(),
                end: Some("2023-01-03".to_string()),
                state: "OH".to_string(),
                party: Some("Democrat".to_string()),
            }],
        }];

        // Two politicians with same last name and state
        let politicians = vec![
            ("P001111".to_string(), "Brown".to_string(), "OH".to_string()),
            ("P002222".to_string(), "Brown".to_string(), "OH".to_string()),
        ];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 0, "Should skip matches when multiple politicians have same (last_name, state)");
    }

    #[test]
    fn test_matching_no_match_in_db() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "G000001".to_string(),
                fec: Some(vec!["H0WA05080".to_string()]),
            },
            name: LegislatorName {
                first: "Sarah".to_string(),
                last: "Wilson".to_string(),
                official_full: None,
            },
            terms: vec![Term {
                term_type: "rep".to_string(),
                start: "2021-01-03".to_string(),
                end: Some("2023-01-03".to_string()),
                state: "WA".to_string(),
                party: Some("Democrat".to_string()),
            }],
        }];

        let politicians = vec![(
            "P003333".to_string(),
            "Different".to_string(),
            "WA".to_string(),
        )];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 0, "Should produce no mappings when legislator not in politician list");
    }

    #[test]
    fn test_matching_case_insensitive() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "H000001".to_string(),
                fec: Some(vec!["H0MA05080".to_string()]),
            },
            name: LegislatorName {
                first: "Michael".to_string(),
                last: "KENNEDY".to_string(), // uppercase in dataset
                official_full: None,
            },
            terms: vec![Term {
                term_type: "rep".to_string(),
                start: "2021-01-03".to_string(),
                end: Some("2023-01-03".to_string()),
                state: "ma".to_string(), // lowercase state
                party: Some("Democrat".to_string()),
            }],
        }];

        let politicians = vec![(
            "P004444".to_string(),
            "kennedy".to_string(), // lowercase in db
            "MA".to_string(),      // uppercase state
        )];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 1, "Should match case-insensitively");
        assert_eq!(mappings[0].politician_id, "P004444");
    }

    #[test]
    fn test_matching_no_terms_skips() {
        let legislators = vec![Legislator {
            id: LegislatorId {
                bioguide: "I000001".to_string(),
                fec: Some(vec!["H0TX05080".to_string()]),
            },
            name: LegislatorName {
                first: "David".to_string(),
                last: "Lee".to_string(),
                official_full: None,
            },
            terms: vec![], // No terms
        }];

        let politicians = vec![("P005555".to_string(), "Lee".to_string(), "TX".to_string())];

        let mappings = match_legislators_to_politicians(&legislators, &politicians);

        assert_eq!(mappings.len(), 0, "Should skip legislators with no terms");
    }
}
