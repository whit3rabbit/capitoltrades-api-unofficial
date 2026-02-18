//! Response types for Tiingo API.

use serde::Deserialize;

/// A single daily price record from the Tiingo end-of-day API.
///
/// The response is a JSON array of these records.
/// We only use `adjClose` (split/dividend-adjusted close price).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TiingoDailyPrice {
    pub date: String,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub open: f64,
    pub volume: i64,
    pub adj_close: f64,
    pub adj_high: f64,
    pub adj_low: f64,
    pub adj_open: f64,
    pub adj_volume: i64,
    pub div_cash: f64,
    pub split_factor: f64,
}
