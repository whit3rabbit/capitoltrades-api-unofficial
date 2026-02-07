//! Trade analysis helpers for aggregating and summarizing trade data.
//!
//! All functions operate on slices of upstream `Trade` types and return
//! standard collections. They are designed for use in summary views
//! and do not perform network calls.

use std::collections::{BTreeMap, HashMap};
use capitoltrades_api::types::Trade;

/// Groups trades by political party, returning a map of party name to trades.
pub fn trades_by_party(trades: &[Trade]) -> HashMap<String, Vec<&Trade>> {
    let mut map: HashMap<String, Vec<&Trade>> = HashMap::new();
    for trade in trades {
        let key = trade.politician.party.to_string();
        map.entry(key).or_default().push(trade);
    }
    map
}

/// Groups trades by issuer ticker symbol. Issuers without a ticker are keyed as "unknown".
pub fn trades_by_ticker(trades: &[Trade]) -> HashMap<String, Vec<&Trade>> {
    let mut map: HashMap<String, Vec<&Trade>> = HashMap::new();
    for trade in trades {
        let key = trade.issuer.issuer_ticker.clone().unwrap_or_else(|| "unknown".to_string());
        map.entry(key).or_default().push(trade);
    }
    map
}

/// Returns the most frequently traded issuers, sorted by trade count descending.
pub fn top_traded_issuers(trades: &[Trade], limit: usize) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for trade in trades {
        *counts
            .entry(trade.issuer.issuer_name.clone())
            .or_default() += 1;
    }
    let mut sorted: Vec<(String, usize)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(limit);
    sorted
}

/// Groups trades by month (YYYY-MM format), sorted chronologically.
pub fn trades_by_month(trades: &[Trade]) -> BTreeMap<String, Vec<&Trade>> {
    let mut map: BTreeMap<String, Vec<&Trade>> = BTreeMap::new();
    for trade in trades {
        let key = trade.tx_date.format("%Y-%m").to_string();
        map.entry(key).or_default().push(trade);
    }
    map
}

/// Sums the estimated dollar value of all trades.
pub fn total_volume(trades: &[Trade]) -> i64 {
    trades.iter().map(|t| t.value).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture_trades() -> Vec<Trade> {
        let json = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../capitoltrades_api/tests/fixtures/trades.json")
        ).unwrap();
        let resp: capitoltrades_api::types::PaginatedResponse<Trade> =
            serde_json::from_str(&json).unwrap();
        resp.data
    }

    #[test]
    fn test_trades_by_party() {
        let trades = load_fixture_trades();
        let by_party = trades_by_party(&trades);
        assert!(by_party.contains_key("democrat"));
        assert_eq!(by_party["democrat"].len(), 1);
    }

    #[test]
    fn test_top_traded_issuers() {
        let trades = load_fixture_trades();
        let top = top_traded_issuers(&trades, 10);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].0, "Apple Inc");
        assert_eq!(top[0].1, 1);
    }

    #[test]
    fn test_trades_by_month() {
        let trades = load_fixture_trades();
        let by_month = trades_by_month(&trades);
        assert!(by_month.contains_key("2024-03"));
    }

    #[test]
    fn test_total_volume() {
        let trades = load_fixture_trades();
        let vol = total_volume(&trades);
        assert_eq!(vol, 50000);
    }

    #[test]
    fn test_empty_trades() {
        let trades: Vec<Trade> = vec![];
        assert!(trades_by_party(&trades).is_empty());
        assert!(top_traded_issuers(&trades, 10).is_empty());
        assert!(trades_by_month(&trades).is_empty());
        assert_eq!(total_volume(&trades), 0);
    }
}
