//! Types for OpenFEC API requests and responses.

use serde::{Deserialize, Serialize};

// ============================================================================
// Candidate Types
// ============================================================================

/// Response wrapper for candidate search endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CandidateSearchResponse {
    pub results: Vec<Candidate>,
    pub pagination: StandardPagination,
}

/// Candidate record from OpenFEC API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Candidate {
    pub candidate_id: String,
    pub name: String,
    pub party: Option<String>,
    pub office: Option<String>,
    pub state: Option<String>,
    pub district: Option<String>,
    #[serde(default)]
    pub cycles: Vec<i32>,
    pub candidate_status: Option<String>,
    pub incumbent_challenge: Option<String>,
}

/// Standard pagination info (used for candidates and committees).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StandardPagination {
    pub count: i64,
    pub page: Option<i64>,
    pub pages: Option<i64>,
    pub per_page: i64,
}

// ============================================================================
// Committee Types
// ============================================================================

/// Response wrapper for committee endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitteeResponse {
    pub results: Vec<Committee>,
    pub pagination: StandardPagination,
}

/// Committee record from OpenFEC API.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Committee {
    pub committee_id: String,
    pub name: String,
    pub committee_type: Option<String>,
    pub designation: Option<String>,
    pub party: Option<String>,
    pub state: Option<String>,
    #[serde(default)]
    pub cycles: Vec<i32>,
}

// ============================================================================
// Schedule A (Contribution) Types
// ============================================================================

/// Response wrapper for Schedule A endpoint.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleAResponse {
    pub results: Vec<Contribution>,
    pub pagination: ScheduleAPagination,
}

/// Contribution record from Schedule A.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Contribution {
    pub sub_id: Option<String>,
    pub committee: Option<CommitteeRef>,
    pub contributor_name: Option<String>,
    pub contributor_state: Option<String>,
    pub contributor_employer: Option<String>,
    pub contributor_occupation: Option<String>,
    pub contribution_receipt_date: Option<String>,
    pub contribution_receipt_amount: Option<f64>,
}

/// Committee reference nested in contribution records.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommitteeRef {
    pub committee_id: Option<String>,
    pub name: Option<String>,
}

/// Keyset pagination for Schedule A (no page numbers).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleAPagination {
    pub count: i64,
    pub per_page: i64,
    pub last_indexes: Option<LastIndexes>,
}

/// Cursor values for keyset pagination.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LastIndexes {
    pub last_index: i64,
    pub last_contribution_receipt_date: String,
}

// ============================================================================
// Query Builders
// ============================================================================

/// Query builder for candidate search endpoint.
#[derive(Debug, Clone, Default)]
pub struct CandidateSearchQuery {
    pub name: Option<String>,
    pub office: Option<String>,
    pub state: Option<String>,
    pub party: Option<String>,
    pub cycle: Option<i32>,
    pub page: Option<i32>,
    pub per_page: Option<i32>,
}

impl CandidateSearchQuery {
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_office(mut self, office: &str) -> Self {
        self.office = Some(office.to_string());
        self
    }

    pub fn with_state(mut self, state: &str) -> Self {
        self.state = Some(state.to_string());
        self
    }

    pub fn with_party(mut self, party: &str) -> Self {
        self.party = Some(party.to_string());
        self
    }

    pub fn with_cycle(mut self, cycle: i32) -> Self {
        self.cycle = Some(cycle);
        self
    }

    pub fn with_page(mut self, page: i32) -> Self {
        self.page = Some(page);
        self
    }

    pub fn with_per_page(mut self, per_page: i32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    /// Build query parameter pairs (excluding None values).
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();

        if let Some(ref name) = self.name {
            params.push(("name".to_string(), name.clone()));
        }
        if let Some(ref office) = self.office {
            params.push(("office".to_string(), office.clone()));
        }
        if let Some(ref state) = self.state {
            params.push(("state".to_string(), state.clone()));
        }
        if let Some(ref party) = self.party {
            params.push(("party".to_string(), party.clone()));
        }
        if let Some(cycle) = self.cycle {
            params.push(("cycle".to_string(), cycle.to_string()));
        }
        if let Some(page) = self.page {
            params.push(("page".to_string(), page.to_string()));
        }
        if let Some(per_page) = self.per_page {
            params.push(("per_page".to_string(), per_page.to_string()));
        }

        params
    }
}

/// Query builder for Schedule A endpoint (keyset pagination only, NO page parameter).
#[derive(Debug, Clone, Default)]
pub struct ScheduleAQuery {
    pub committee_id: Option<String>,
    pub contributor_name: Option<String>,
    pub two_year_transaction_period: Option<i32>,
    pub per_page: Option<i32>,
    pub last_index: Option<i64>,
    pub last_contribution_receipt_date: Option<String>,
    pub sort: Option<String>,
    pub sort_hide_null: Option<bool>,
}

impl ScheduleAQuery {
    pub fn with_committee_id(mut self, committee_id: &str) -> Self {
        self.committee_id = Some(committee_id.to_string());
        self
    }

    pub fn with_contributor_name(mut self, contributor_name: &str) -> Self {
        self.contributor_name = Some(contributor_name.to_string());
        self
    }

    pub fn with_cycle(mut self, cycle: i32) -> Self {
        self.two_year_transaction_period = Some(cycle);
        self
    }

    pub fn with_per_page(mut self, per_page: i32) -> Self {
        self.per_page = Some(per_page);
        self
    }

    pub fn with_last_index(mut self, last_index: i64) -> Self {
        self.last_index = Some(last_index);
        self
    }

    pub fn with_last_contribution_receipt_date(mut self, date: &str) -> Self {
        self.last_contribution_receipt_date = Some(date.to_string());
        self
    }

    pub fn with_sort(mut self, sort: &str) -> Self {
        self.sort = Some(sort.to_string());
        self
    }

    pub fn with_sort_hide_null(mut self, hide_null: bool) -> Self {
        self.sort_hide_null = Some(hide_null);
        self
    }

    /// Build query parameter pairs (excluding None values).
    /// CRITICAL: Never emits a "page" parameter - Schedule A uses keyset pagination only.
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();

        if let Some(ref committee_id) = self.committee_id {
            params.push(("committee_id".to_string(), committee_id.clone()));
        }
        if let Some(ref contributor_name) = self.contributor_name {
            params.push(("contributor_name".to_string(), contributor_name.clone()));
        }
        if let Some(cycle) = self.two_year_transaction_period {
            params.push((
                "two_year_transaction_period".to_string(),
                cycle.to_string(),
            ));
        }
        if let Some(per_page) = self.per_page {
            params.push(("per_page".to_string(), per_page.to_string()));
        }
        if let Some(last_index) = self.last_index {
            params.push(("last_index".to_string(), last_index.to_string()));
        }
        if let Some(ref date) = self.last_contribution_receipt_date {
            params.push(("last_contribution_receipt_date".to_string(), date.clone()));
        }
        if let Some(ref sort) = self.sort {
            params.push(("sort".to_string(), sort.clone()));
        }
        if let Some(hide_null) = self.sort_hide_null {
            params.push(("sort_hide_null".to_string(), hide_null.to_string()));
        }

        params
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_query_default_empty() {
        let query = CandidateSearchQuery::default();
        let pairs = query.to_query_pairs();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn candidate_query_with_name() {
        let query = CandidateSearchQuery::default().with_name("Pelosi");
        let pairs = query.to_query_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("name".to_string(), "Pelosi".to_string()));
    }

    #[test]
    fn candidate_query_multiple_params() {
        let query = CandidateSearchQuery::default()
            .with_name("Pelosi")
            .with_state("CA")
            .with_party("DEM");
        let pairs = query.to_query_pairs();
        assert_eq!(pairs.len(), 3);
        assert!(pairs.contains(&("name".to_string(), "Pelosi".to_string())));
        assert!(pairs.contains(&("state".to_string(), "CA".to_string())));
        assert!(pairs.contains(&("party".to_string(), "DEM".to_string())));
    }

    #[test]
    fn schedule_a_query_default_empty() {
        let query = ScheduleAQuery::default();
        let pairs = query.to_query_pairs();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn schedule_a_query_with_committee_and_cursor() {
        let query = ScheduleAQuery::default()
            .with_committee_id("C00000001")
            .with_last_index(230880619)
            .with_last_contribution_receipt_date("2024-01-15");
        let pairs = query.to_query_pairs();
        assert_eq!(pairs.len(), 3);
        assert!(pairs.contains(&("committee_id".to_string(), "C00000001".to_string())));
        assert!(pairs.contains(&("last_index".to_string(), "230880619".to_string())));
        assert!(pairs.contains(&(
            "last_contribution_receipt_date".to_string(),
            "2024-01-15".to_string()
        )));
    }

    #[test]
    fn schedule_a_query_never_emits_page_parameter() {
        // Schedule A uses keyset pagination only - verify no page parameter is ever emitted
        let query = ScheduleAQuery::default()
            .with_committee_id("C00000001")
            .with_per_page(100);
        let pairs = query.to_query_pairs();

        // Verify "page" never appears in parameters
        for (key, _) in &pairs {
            assert_ne!(key, "page", "Schedule A query must never emit 'page' parameter");
        }
    }
}
