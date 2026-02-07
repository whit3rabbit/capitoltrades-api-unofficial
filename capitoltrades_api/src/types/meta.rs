//! Response envelope types shared across all API endpoints.

use serde::{Deserialize, Serialize};

/// Top-level metadata wrapper containing pagination info.
#[derive(Serialize, Deserialize)]
pub struct Meta {
    /// Pagination details for the current response.
    pub paging: Paging,
}

/// Pagination state for a paginated API response.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paging {
    /// Current page number (1-indexed).
    pub page: i64,
    /// Number of items per page.
    pub size: i64,
    /// Total number of items across all pages.
    pub total_items: i64,
    /// Total number of pages.
    pub total_pages: i64,
}

/// Paginated API response containing a list of items and metadata.
#[derive(Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// Pagination metadata.
    pub meta: Meta,
    /// The page of result items.
    pub data: Vec<T>,
}

/// Non-paginated API response wrapping a single item.
#[derive(Serialize, Deserialize)]
pub struct Response<T> {
    /// The response payload.
    pub data: T,
}
