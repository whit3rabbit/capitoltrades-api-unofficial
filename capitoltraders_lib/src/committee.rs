//! Committee resolution service with three-tier caching.
//!
//! Provides CommitteeResolver for mapping CapitolTrades politician IDs to their
//! authorized FEC committees, using a tiered cache strategy (DashMap -> SQLite -> API).

use dashmap::DashMap;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use thiserror::Error;

use crate::db::{Db, DbError};
use crate::openfec::types::CandidateSearchQuery;
use crate::openfec::{OpenFecClient, OpenFecError};

/// Errors from committee resolution operations.
#[derive(Error, Debug)]
pub enum CommitteeError {
    #[error("Database error: {0}")]
    Database(#[from] DbError),
    #[error("OpenFEC error: {0}")]
    OpenFec(#[from] OpenFecError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Classification of FEC committee types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitteeClass {
    Campaign,
    LeadershipPac,
    JointFundraising,
    Party,
    Pac,
    Other,
}

impl CommitteeClass {
    /// Classify a committee based on designation and committee_type.
    ///
    /// CRITICAL: Designation is checked FIRST because leadership PACs can have
    /// H/S/P committee types but are identified by designation "D".
    ///
    /// Classification rules:
    /// - designation "D" -> LeadershipPac (overrides committee_type)
    /// - designation "J" -> JointFundraising
    /// - committee_type H/S/P with designation A/P -> Campaign
    /// - committee_type X/Y/Z -> Party
    /// - committee_type N/Q/O -> Pac
    /// - Everything else -> Other
    pub fn classify(committee_type: Option<&str>, designation: Option<&str>) -> Self {
        // Check designation first (leadership PACs have designation D regardless of type)
        match designation {
            Some("D") => return Self::LeadershipPac,
            Some("J") => return Self::JointFundraising,
            _ => {}
        }

        // Check committee_type next
        match committee_type {
            Some("H") | Some("S") | Some("P") => {
                // Campaign committee requires A or P designation
                match designation {
                    Some("A") | Some("P") => Self::Campaign,
                    _ => Self::Other,
                }
            }
            Some("X") | Some("Y") | Some("Z") => Self::Party,
            Some("N") | Some("Q") | Some("O") => Self::Pac,
            _ => Self::Other,
        }
    }
}

impl std::fmt::Display for CommitteeClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Campaign => write!(f, "campaign"),
            Self::LeadershipPac => write!(f, "leadership_pac"),
            Self::JointFundraising => write!(f, "joint_fundraising"),
            Self::Party => write!(f, "party"),
            Self::Pac => write!(f, "pac"),
            Self::Other => write!(f, "other"),
        }
    }
}

/// Resolved committee with classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCommittee {
    pub committee_id: String,
    pub name: String,
    pub classification: CommitteeClass,
}

/// Committee resolver with three-tier caching (DashMap -> SQLite -> API).
pub struct CommitteeResolver {
    client: Arc<OpenFecClient>,
    db: Arc<Mutex<Db>>,
    cache: DashMap<String, Vec<ResolvedCommittee>>,
}

impl CommitteeResolver {
    /// Create a new CommitteeResolver.
    pub fn new(client: Arc<OpenFecClient>, db: Arc<Mutex<Db>>) -> Self {
        Self {
            client,
            db,
            cache: DashMap::new(),
        }
    }

    /// Resolve committees for a politician using three-tier cache.
    ///
    /// Tier 1: DashMap in-memory cache
    /// Tier 2: SQLite database
    /// Tier 3: OpenFEC API (with fallback to name search if no FEC IDs exist)
    pub async fn resolve_committees(
        &self,
        politician_id: &str,
    ) -> Result<Vec<ResolvedCommittee>, CommitteeError> {
        // Tier 1: Check memory cache
        if let Some(cached) = self.cache.get(politician_id) {
            return Ok(cached.clone());
        }

        // Tier 2: Check SQLite
        let db = self.db.lock().expect("db mutex poisoned");
        if let Some(committee_ids) = db.get_committees_for_politician(politician_id)? {
            if !committee_ids.is_empty() {
                // Build ResolvedCommittee entries from committee metadata
                let mut resolved = Vec::new();
                for committee_id in &committee_ids {
                    // Query fec_committees table for metadata
                    let metadata: Option<(String, Option<String>, Option<String>)> = db
                        .conn()
                        .query_row(
                            "SELECT name, committee_type, designation FROM fec_committees WHERE committee_id = ?1",
                            [committee_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )
                        .optional()?;

                    let (name, committee_type, designation) = match metadata {
                        Some((n, t, d)) => (n, t, d),
                        None => {
                            // Committee metadata missing, classify as Other
                            (committee_id.clone(), None, None)
                        }
                    };

                    let classification = CommitteeClass::classify(
                        committee_type.as_deref(),
                        designation.as_deref(),
                    );

                    resolved.push(ResolvedCommittee {
                        committee_id: committee_id.clone(),
                        name,
                        classification,
                    });
                }

                // Insert into cache
                self.cache.insert(politician_id.to_string(), resolved.clone());
                drop(db); // Release lock before returning
                return Ok(resolved);
            }
        }

        // Tier 3: Fetch from OpenFEC API
        // Extract data from DB before any async operations
        let fec_ids = db.get_fec_ids_for_politician(politician_id)?;
        let politician_info = if fec_ids.is_empty() {
            db.get_politician_info(politician_id)?
        } else {
            None
        };
        drop(db); // Release lock before async operations

        let committees = if !fec_ids.is_empty() {
            // We have FEC IDs, fetch committees for each
            let mut all_committees = Vec::new();
            for fec_id in &fec_ids {
                let response = self.client.get_candidate_committees(fec_id).await?;
                all_committees.extend(response.results);
            }
            all_committees
        } else {
            // No FEC IDs, fall back to name search
            if let Some((first, last, state)) = politician_info {
                let query = CandidateSearchQuery::default()
                    .with_name(&format!("{} {}", first, last))
                    .with_state(&state);
                let response = self.client.search_candidates(&query).await?;

                if let Some(candidate) = response.results.first() {
                    // Found a candidate, fetch their committees
                    let committee_response = self.client.get_candidate_committees(&candidate.candidate_id).await?;
                    committee_response.results
                } else {
                    // No candidate found
                    tracing::warn!(
                        "Politician {} ({} {} {}) not found in OpenFEC",
                        politician_id, first, last, state
                    );
                    self.cache.insert(politician_id.to_string(), Vec::new());
                    return Ok(Vec::new());
                }
            } else {
                // Politician not found in DB
                tracing::warn!("Politician {} not found in database", politician_id);
                self.cache.insert(politician_id.to_string(), Vec::new());
                return Ok(Vec::new());
            }
        };

        // Classify and build ResolvedCommittee entries
        let resolved: Vec<ResolvedCommittee> = committees
            .iter()
            .map(|c| ResolvedCommittee {
                committee_id: c.committee_id.clone(),
                name: c.name.clone(),
                classification: CommitteeClass::classify(
                    c.committee_type.as_deref(),
                    c.designation.as_deref(),
                ),
            })
            .collect();

        // Store committees in DB (acquire lock again for writes)
        let db = self.db.lock().expect("db mutex poisoned");
        db.upsert_committees(&committees)?;

        // Update politician's committee_ids in fec_mappings
        let committee_ids: Vec<String> = committees.iter().map(|c| c.committee_id.clone()).collect();
        if !committee_ids.is_empty() {
            db.update_politician_committees(politician_id, &committee_ids)?;
        }
        drop(db); // Release lock

        // Insert into cache
        self.cache.insert(politician_id.to_string(), resolved.clone());

        Ok(resolved)
    }

    /// Get the number of cached entries (for testing).
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Clear the memory cache (for testing or cache invalidation).
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_campaign_house() {
        assert_eq!(
            CommitteeClass::classify(Some("H"), Some("A")),
            CommitteeClass::Campaign
        );
    }

    #[test]
    fn test_classify_campaign_senate() {
        assert_eq!(
            CommitteeClass::classify(Some("S"), Some("P")),
            CommitteeClass::Campaign
        );
    }

    #[test]
    fn test_classify_campaign_presidential() {
        assert_eq!(
            CommitteeClass::classify(Some("P"), Some("A")),
            CommitteeClass::Campaign
        );
    }

    #[test]
    fn test_classify_leadership_pac() {
        // Designation D overrides H type
        assert_eq!(
            CommitteeClass::classify(Some("H"), Some("D")),
            CommitteeClass::LeadershipPac
        );
    }

    #[test]
    fn test_classify_leadership_pac_no_type() {
        assert_eq!(
            CommitteeClass::classify(None, Some("D")),
            CommitteeClass::LeadershipPac
        );
    }

    #[test]
    fn test_classify_joint_fundraising() {
        assert_eq!(
            CommitteeClass::classify(Some("N"), Some("J")),
            CommitteeClass::JointFundraising
        );
    }

    #[test]
    fn test_classify_party() {
        assert_eq!(
            CommitteeClass::classify(Some("X"), None),
            CommitteeClass::Party
        );
    }

    #[test]
    fn test_classify_pac() {
        assert_eq!(
            CommitteeClass::classify(Some("Q"), Some("B")),
            CommitteeClass::Pac
        );
    }

    #[test]
    fn test_classify_other_unknown() {
        // W is not a valid committee_type, should map to Other
        assert_eq!(
            CommitteeClass::classify(Some("W"), None),
            CommitteeClass::Other
        );
    }

    #[test]
    fn test_classify_none_none() {
        assert_eq!(
            CommitteeClass::classify(None, None),
            CommitteeClass::Other
        );
    }
}
