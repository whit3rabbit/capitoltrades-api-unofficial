//! Conflict-of-interest detection and scoring.
//!
//! This module provides types and pure computation functions for detecting potential
//! conflicts of interest in politician trading patterns, including:
//! - Committee trading scores (percentage of trades in committee-related sectors)
//! - Donation-trade correlations (matching donor employers to traded issuers)

use serde::Serialize;

use crate::analytics::ClosedTrade;
use crate::committee_jurisdiction::{get_committee_sectors, CommitteeJurisdiction};

/// Committee trading score for a politician.
///
/// Measures the percentage of a politician's closed trades that are in sectors
/// under their committee jurisdictions.
#[derive(Serialize, Debug, Clone)]
pub struct CommitteeTradingScore {
    pub politician_id: String,
    pub politician_name: String,
    pub committee_names: Vec<String>,
    pub total_scored_trades: usize,
    pub committee_related_trades: usize,
    pub committee_trading_pct: f64,
    pub disclaimer: String,
}

/// Donation-trade correlation result.
///
/// Links a politician's trades to donations from employees of the traded companies.
#[derive(Serialize, Debug, Clone)]
pub struct DonationTradeCorrelation {
    pub politician_id: String,
    pub politician_name: String,
    pub ticker: String,
    pub matching_donor_count: i64,
    pub avg_mapping_confidence: f64,
    pub donor_employers: String,
    pub total_donation_amount: f64,
}

/// Comprehensive conflict summary for a politician.
///
/// Combines committee trading score and donation-trade correlations.
#[derive(Serialize, Debug, Clone)]
pub struct ConflictSummary {
    pub politician_id: String,
    pub politician_name: String,
    pub committee_score: Option<CommitteeTradingScore>,
    pub donation_correlations: Vec<DonationTradeCorrelation>,
    pub disclaimer: String,
}

/// Calculate committee trading score for a politician.
///
/// Computes the percentage of closed trades that are in sectors under the
/// politician's committee jurisdictions. Trades with NULL gics_sector are
/// excluded from both numerator and denominator.
///
/// # Arguments
/// * `closed_trades` - Politician's closed trades
/// * `politician_committees` - Committee short codes the politician serves on
/// * `committee_jurisdictions` - All committee jurisdiction mappings
/// * `politician_id` - Politician ID
/// * `politician_name` - Politician name for display
///
/// # Returns
/// * `CommitteeTradingScore` - Scoring result with percentage
///
/// # Example
/// ```
/// use capitoltraders_lib::conflict::calculate_committee_trading_score;
/// use capitoltraders_lib::committee_jurisdiction::load_committee_jurisdictions;
/// use capitoltraders_lib::analytics::ClosedTrade;
///
/// let jurisdictions = load_committee_jurisdictions().unwrap();
/// let committees = vec!["hsba".to_string()]; // House Financial Services
/// let trades = vec![
///     ClosedTrade {
///         politician_id: "P000001".to_string(),
///         ticker: "JPM".to_string(),
///         shares: 100.0,
///         buy_price: 100.0,
///         sell_price: 150.0,
///         buy_date: "2024-01-01".to_string(),
///         sell_date: "2024-06-01".to_string(),
///         buy_benchmark: None,
///         sell_benchmark: None,
///         buy_has_sector: false,
///         sell_has_sector: false,
///         gics_sector: Some("Financials".to_string()),
///     },
/// ];
///
/// let score = calculate_committee_trading_score(
///     &trades,
///     &committees,
///     &jurisdictions,
///     "P000001".to_string(),
///     "John Doe".to_string(),
/// );
///
/// assert_eq!(score.committee_trading_pct, 100.0);
/// ```
pub fn calculate_committee_trading_score(
    closed_trades: &[ClosedTrade],
    politician_committees: &[String],
    committee_jurisdictions: &[CommitteeJurisdiction],
    politician_id: String,
    politician_name: String,
) -> CommitteeTradingScore {
    // Build set of sectors under politician's committee jurisdictions
    let committee_sectors = get_committee_sectors(committee_jurisdictions, politician_committees);

    // Filter trades to those with known gics_sector
    let trades_with_sector: Vec<&ClosedTrade> = closed_trades
        .iter()
        .filter(|t| t.gics_sector.is_some())
        .collect();

    // Count trades in committee-related sectors
    let committee_related_count = trades_with_sector
        .iter()
        .filter(|t| {
            if let Some(ref sector) = t.gics_sector {
                committee_sectors.contains(sector)
            } else {
                false
            }
        })
        .count();

    let total = trades_with_sector.len();
    let pct = if total > 0 {
        (committee_related_count as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    CommitteeTradingScore {
        politician_id,
        politician_name,
        committee_names: politician_committees.to_vec(),
        total_scored_trades: total,
        committee_related_trades: committee_related_count,
        committee_trading_pct: pct,
        disclaimer: "Based on current committee assignments; may not reflect assignment at trade time".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::committee_jurisdiction::load_committee_jurisdictions;

    #[test]
    fn test_committee_trading_score_basic() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["hsba".to_string()]; // House Financial Services

        let trades = vec![
            ClosedTrade {
                politician_id: "P000001".to_string(),
                ticker: "JPM".to_string(),
                shares: 100.0,
                buy_price: 100.0,
                sell_price: 150.0,
                buy_date: "2024-01-01".to_string(),
                sell_date: "2024-06-01".to_string(),
                buy_benchmark: None,
                sell_benchmark: None,
                buy_has_sector: false,
                sell_has_sector: false,
                gics_sector: Some("Financials".to_string()),
            },
            ClosedTrade {
                politician_id: "P000001".to_string(),
                ticker: "JPM".to_string(),
                shares: 50.0,
                buy_price: 100.0,
                sell_price: 150.0,
                buy_date: "2024-01-01".to_string(),
                sell_date: "2024-06-01".to_string(),
                buy_benchmark: None,
                sell_benchmark: None,
                buy_has_sector: false,
                sell_has_sector: false,
                gics_sector: Some("Financials".to_string()),
            },
            ClosedTrade {
                politician_id: "P000001".to_string(),
                ticker: "XOM".to_string(),
                shares: 100.0,
                buy_price: 80.0,
                sell_price: 90.0,
                buy_date: "2024-02-01".to_string(),
                sell_date: "2024-07-01".to_string(),
                buy_benchmark: None,
                sell_benchmark: None,
                buy_has_sector: false,
                sell_has_sector: false,
                gics_sector: Some("Energy".to_string()),
            },
        ];

        let score = calculate_committee_trading_score(
            &trades,
            &committees,
            &jurisdictions,
            "P000001".to_string(),
            "John Doe".to_string(),
        );

        assert_eq!(score.total_scored_trades, 3);
        assert_eq!(score.committee_related_trades, 2); // 2 Financials trades
        assert!((score.committee_trading_pct - 66.666).abs() < 0.01);
    }

    #[test]
    fn test_committee_trading_score_no_committees() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees: Vec<String> = vec![]; // No committees

        let trades = vec![ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "JPM".to_string(),
            shares: 100.0,
            buy_price: 100.0,
            sell_price: 150.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-06-01".to_string(),
            buy_benchmark: None,
            sell_benchmark: None,
            buy_has_sector: false,
            sell_has_sector: false,
            gics_sector: Some("Financials".to_string()),
        }];

        let score = calculate_committee_trading_score(
            &trades,
            &committees,
            &jurisdictions,
            "P000001".to_string(),
            "John Doe".to_string(),
        );

        assert_eq!(score.total_scored_trades, 1);
        assert_eq!(score.committee_related_trades, 0);
        assert_eq!(score.committee_trading_pct, 0.0);
    }

    #[test]
    fn test_committee_trading_score_no_trades() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["hsba".to_string()];
        let trades: Vec<ClosedTrade> = vec![];

        let score = calculate_committee_trading_score(
            &trades,
            &committees,
            &jurisdictions,
            "P000001".to_string(),
            "John Doe".to_string(),
        );

        assert_eq!(score.total_scored_trades, 0);
        assert_eq!(score.committee_related_trades, 0);
        assert_eq!(score.committee_trading_pct, 0.0);
    }

    #[test]
    fn test_committee_trading_score_null_sectors_excluded() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["hsba".to_string()];

        let trades = vec![
            ClosedTrade {
                politician_id: "P000001".to_string(),
                ticker: "JPM".to_string(),
                shares: 100.0,
                buy_price: 100.0,
                sell_price: 150.0,
                buy_date: "2024-01-01".to_string(),
                sell_date: "2024-06-01".to_string(),
                buy_benchmark: None,
                sell_benchmark: None,
                buy_has_sector: false,
                sell_has_sector: false,
                gics_sector: Some("Financials".to_string()),
            },
            ClosedTrade {
                politician_id: "P000001".to_string(),
                ticker: "UNKNOWN".to_string(),
                shares: 100.0,
                buy_price: 100.0,
                sell_price: 150.0,
                buy_date: "2024-01-01".to_string(),
                sell_date: "2024-06-01".to_string(),
                buy_benchmark: None,
                sell_benchmark: None,
                buy_has_sector: false,
                sell_has_sector: false,
                gics_sector: None, // NULL sector
            },
        ];

        let score = calculate_committee_trading_score(
            &trades,
            &committees,
            &jurisdictions,
            "P000001".to_string(),
            "John Doe".to_string(),
        );

        // NULL sector trade excluded from denominator
        assert_eq!(score.total_scored_trades, 1);
        assert_eq!(score.committee_related_trades, 1);
        assert_eq!(score.committee_trading_pct, 100.0);
    }

    #[test]
    fn test_committee_trading_score_overlapping_jurisdictions() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        // hsif (House Energy and Commerce) includes Health Care
        // hsvc (House Veterans' Affairs) includes Health Care
        let committees = vec!["hsif".to_string(), "hsvc".to_string()];

        let trades = vec![ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "JNJ".to_string(),
            shares: 100.0,
            buy_price: 150.0,
            sell_price: 160.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-06-01".to_string(),
            buy_benchmark: None,
            sell_benchmark: None,
            buy_has_sector: false,
            sell_has_sector: false,
            gics_sector: Some("Health Care".to_string()),
        }];

        let score = calculate_committee_trading_score(
            &trades,
            &committees,
            &jurisdictions,
            "P000001".to_string(),
            "John Doe".to_string(),
        );

        // Health Care should be counted once, even though both committees cover it
        assert_eq!(score.total_scored_trades, 1);
        assert_eq!(score.committee_related_trades, 1);
        assert_eq!(score.committee_trading_pct, 100.0);
    }

    #[test]
    fn test_committee_trading_score_disclaimer_present() {
        let jurisdictions = load_committee_jurisdictions().unwrap();
        let committees = vec!["hsba".to_string()];
        let trades: Vec<ClosedTrade> = vec![];

        let score = calculate_committee_trading_score(
            &trades,
            &committees,
            &jurisdictions,
            "P000001".to_string(),
            "John Doe".to_string(),
        );

        assert!(score.disclaimer.contains("current committee assignments"));
    }

    #[test]
    fn test_donation_trade_correlation_type() {
        let correlation = DonationTradeCorrelation {
            politician_id: "P000001".to_string(),
            politician_name: "John Doe".to_string(),
            ticker: "JPM".to_string(),
            matching_donor_count: 5,
            avg_mapping_confidence: 0.92,
            donor_employers: "JPMorgan Chase, JP Morgan".to_string(),
            total_donation_amount: 15000.0,
        };

        assert_eq!(correlation.politician_id, "P000001");
        assert_eq!(correlation.ticker, "JPM");
        assert_eq!(correlation.matching_donor_count, 5);
    }
}
