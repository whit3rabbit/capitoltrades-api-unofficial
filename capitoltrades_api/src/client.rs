//! HTTP client for the CapitolTrades BFF API.

use std::time::Duration;

use serde::de::DeserializeOwned;
use url::Url;

use crate::{
    query::{IssuerQuery, PoliticianQuery, Query, TradeQuery},
    types::{IssuerDetail, PaginatedResponse, PoliticianDetail, Response, Trade},
    user_agent::get_user_agent,
    Error,
};

/// HTTP client for the CapitolTrades BFF API.
///
/// Sends requests with browser-like headers and a randomized user agent to
/// avoid being blocked. Each request builds a fresh `reqwest::Client` with
/// a 30-second timeout.
pub struct Client {
    /// Base URL for the API. Defaults to `https://bff.capitoltrades.com`.
    base_api_url: String,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Creates a new client pointing at the production CapitolTrades BFF API.
    pub fn new() -> Self {
        Self {
            base_api_url: "https://bff.capitoltrades.com".to_string(),
        }
    }

    /// Creates a new client with a custom base URL. Used for testing with wiremock.
    pub fn with_base_url(base_url: &str) -> Self {
        Self {
            base_api_url: base_url.to_string(),
        }
    }

    fn get_url(&self, path: &str, query: Option<&impl Query>) -> Result<Url, Error> {
        let url = Url::parse(format!("{}{}", &self.base_api_url, path).as_str()).map_err(|e| {
            tracing::error!("Invalid URL constructed: {}", e);
            Error::RequestFailed
        })?;
        Ok(match query {
            Some(query) => query.add_to_url(&url),
            None => url,
        })
    }

    async fn get<T, Q>(&self, path: &str, query: Option<&Q>) -> Result<T, Error>
    where
        T: DeserializeOwned,
        Q: Query,
    {
        let url = self.get_url(path, query)?;
        let client = reqwest::Client::builder()
            .user_agent(get_user_agent())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                tracing::error!("Failed to build HTTP client: {}", e);
                Error::RequestFailed
            })?;
        let resp = client
            .get(url)
            .header("content-type", "application/json")
            .header("origin", "https://www.capitoltrades.com")
            .header("referer", "https://www.capitoltrades.com")
            .header("accept", "application/json, text/plain, */*")
            .header("accept-language", "en-US,en;q=0.9")
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-site")
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to get resource: {}", e);
                Error::RequestFailed
            })?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| {
            tracing::error!("Failed to read response body: {}", e);
            Error::RequestFailed
        })?;

        if !status.is_success() {
            let snippet = truncate_body(&body);
            tracing::error!("Request failed with status {}: {}", status, snippet);
            return Err(Error::HttpStatus {
                status: status.as_u16(),
                body: snippet,
            });
        }

        let parsed = serde_json::from_str::<T>(&body).map_err(|e| {
            let snippet = truncate_body(&body);
            tracing::error!("Failed to parse resource: {} | body: {}", e, snippet);
            Error::RequestFailed
        })?;

        Ok(parsed)
    }

    /// Fetches a paginated list of trades matching the given query.
    pub async fn get_trades(&self, query: &TradeQuery) -> Result<PaginatedResponse<Trade>, Error> {
        self.get::<PaginatedResponse<Trade>, TradeQuery>("/trades", Some(query))
            .await
    }

    /// Fetches a paginated list of politicians matching the given query.
    pub async fn get_politicians(
        &self,
        query: &PoliticianQuery,
    ) -> Result<PaginatedResponse<PoliticianDetail>, Error> {
        self.get::<PaginatedResponse<PoliticianDetail>, PoliticianQuery>(
            "/politicians",
            Some(query),
        )
        .await
    }

    /// Fetches a single issuer by its numeric ID.
    pub async fn get_issuer(&self, issuer_id: i64) -> Result<Response<IssuerDetail>, Error> {
        self.get::<Response<IssuerDetail>, IssuerQuery>(
            format!("/issuers/{}", issuer_id).as_str(),
            None,
        )
        .await
    }

    /// Fetches a paginated list of issuers matching the given query.
    pub async fn get_issuers(
        &self,
        query: &IssuerQuery,
    ) -> Result<PaginatedResponse<IssuerDetail>, Error> {
        self.get::<PaginatedResponse<IssuerDetail>, IssuerQuery>("/issuers", Some(query))
            .await
    }
}

fn truncate_body(body: &str) -> String {
    const MAX: usize = 2000;
    if body.len() <= MAX {
        body.to_string()
    } else {
        format!("{}...[truncated]", &body[..MAX])
    }
}
