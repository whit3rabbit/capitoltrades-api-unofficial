//! OpenFEC API client implementation.

use super::error::OpenFecError;
use super::types::{
    CandidateSearchQuery, CandidateSearchResponse, CommitteeResponse, ScheduleAQuery,
    ScheduleAResponse,
};
use std::time::Duration;

use serde::de::DeserializeOwned;

/// Request timeout for OpenFEC API calls (seconds).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// OpenFEC API client for fetching FEC data.
pub struct OpenFecClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenFecClient {
    /// Create a new OpenFecClient with default base URL.
    pub fn new(api_key: String) -> Result<Self, OpenFecError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| OpenFecError::Network(e))?;
        Ok(Self {
            client,
            api_key,
            base_url: "https://api.open.fec.gov/v1".to_string(),
        })
    }

    /// Create a new OpenFecClient with custom base URL (for testing with wiremock).
    pub fn with_base_url(base_url: &str, api_key: String) -> Result<Self, OpenFecError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| OpenFecError::Network(e))?;
        Ok(Self {
            client,
            api_key,
            base_url: base_url.to_string(),
        })
    }

    /// Internal helper to perform GET requests with query parameters.
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(String, String)],
    ) -> Result<T, OpenFecError> {
        // Build URL
        let url = format!("{}{}", self.base_url, path);

        // Add api_key to params
        let mut all_params = params.to_vec();
        all_params.push(("api_key".to_string(), self.api_key.clone()));

        // Make request
        let response = self
            .client
            .get(&url)
            .query(&all_params)
            .send()
            .await?;

        // Check status code
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(OpenFecError::RateLimited);
        } else if status == reqwest::StatusCode::FORBIDDEN {
            return Err(OpenFecError::InvalidApiKey);
        } else if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            let body_snippet = if body.len() > 200 {
                format!("{}...", &body[..200])
            } else {
                body
            };
            return Err(OpenFecError::InvalidRequest(format!(
                "HTTP {}: {}",
                status, body_snippet
            )));
        }

        // Deserialize JSON
        response.json::<T>().await.map_err(|e| {
            OpenFecError::ParseFailed(format!("Failed to deserialize response: {}", e))
        })
    }

    /// Search for candidates by name and other filters.
    pub async fn search_candidates(
        &self,
        query: &CandidateSearchQuery,
    ) -> Result<CandidateSearchResponse, OpenFecError> {
        let params = query.to_query_pairs();
        self.get("/candidates/search/", &params).await
    }

    /// Get committees authorized by a specific candidate.
    pub async fn get_candidate_committees(
        &self,
        candidate_id: &str,
    ) -> Result<CommitteeResponse, OpenFecError> {
        let path = format!("/candidate/{}/committees/", candidate_id);
        self.get(&path, &[]).await
    }

    /// Get Schedule A contributions with keyset pagination.
    pub async fn get_schedule_a(
        &self,
        query: &ScheduleAQuery,
    ) -> Result<ScheduleAResponse, OpenFecError> {
        let params = query.to_query_pairs();
        self.get("/schedules/schedule_a/", &params).await
    }
}
