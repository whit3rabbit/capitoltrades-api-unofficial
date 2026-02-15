//! Anomaly detection and scoring calculations.
//!
//! This module provides pure computation functions for detecting unusual trading patterns:
//! - Pre-move trade detection (trades before significant price changes)
//! - Unusual volume detection (trading frequency spikes)
//! - Sector concentration scoring (HHI-based portfolio diversification)
//! - Composite anomaly scoring (weighted combination of all signals)

use chrono::NaiveDate;
use serde::Serialize;
use std::collections::HashMap;

/// Input trade record with future price data for pre-move detection.
#[derive(Debug, Clone)]
pub struct TradeWithFuturePrice {
    pub tx_id: i64,
    pub politician_id: String,
    pub ticker: String,
    pub tx_date: String,
    pub tx_type: String,
    pub trade_price: f64,
    pub price_30d_later: Option<f64>,
}

/// Input trade volume record for unusual activity detection.
#[derive(Debug, Clone)]
pub struct TradeVolumeRecord {
    pub politician_id: String,
    pub tx_date: String,
}

/// Input portfolio position record for HHI concentration calculation.
#[derive(Debug, Clone)]
pub struct PortfolioPositionForHHI {
    pub ticker: String,
    pub gics_sector: Option<String>,
    pub estimated_value: f64,
}

/// Pre-move signal result.
///
/// Represents a trade that preceded a significant price movement.
#[derive(Serialize, Debug, Clone)]
pub struct PreMoveSignal {
    pub tx_id: i64,
    pub politician_id: String,
    pub ticker: String,
    pub tx_date: String,
    pub tx_type: String,
    pub trade_price: f64,
    pub price_30d_later: f64,
    pub price_change_pct: f64,
    pub direction: String,
}

/// Volume anomaly signal.
///
/// Represents unusually high trading frequency for a politician.
#[derive(Serialize, Debug, Clone)]
pub struct VolumeSignal {
    pub recent_trade_count: usize,
    pub historical_avg: f64,
    pub volume_ratio: f64,
    pub is_unusual: bool,
}

/// Sector concentration score (HHI-based).
///
/// Measures portfolio diversification across GICS sectors.
#[derive(Serialize, Debug, Clone)]
pub struct ConcentrationScore {
    pub sector_weights: HashMap<String, f64>,
    pub hhi_score: f64,
    pub dominant_sector: Option<String>,
    pub is_concentrated: bool,
}

/// Composite anomaly score.
///
/// Combines multiple anomaly signals with normalized weights.
#[derive(Serialize, Debug, Clone)]
pub struct AnomalyScore {
    pub pre_move_norm: f64,
    pub volume_norm: f64,
    pub concentration_norm: f64,
    pub composite: f64,
    pub confidence: f64,
}

/// Detect pre-move trades (trades before significant price movements).
///
/// Returns trades where the price changed by more than threshold_pct within 30 days.
/// Excludes trades with no 30-day price data.
pub fn detect_pre_move_trades(
    trades: &[TradeWithFuturePrice],
    threshold_pct: f64,
) -> Vec<PreMoveSignal> {
    let mut signals = Vec::new();

    for trade in trades {
        // Skip trades without 30-day price
        let price_30d = match trade.price_30d_later {
            Some(p) => p,
            None => continue,
        };

        // Calculate price change percentage
        let price_change_pct = ((price_30d - trade.trade_price) / trade.trade_price) * 100.0;

        // Check if absolute change exceeds threshold
        if price_change_pct.abs() > threshold_pct {
            // Determine direction based on tx_type and price movement
            let direction = if trade.tx_type == "buy" {
                if price_change_pct > 0.0 {
                    "buy_before_rise".to_string()
                } else {
                    "buy_before_drop".to_string()
                }
            } else {
                // sell
                if price_change_pct > 0.0 {
                    "sell_before_rise".to_string()
                } else {
                    "sell_before_drop".to_string()
                }
            };

            signals.push(PreMoveSignal {
                tx_id: trade.tx_id,
                politician_id: trade.politician_id.clone(),
                ticker: trade.ticker.clone(),
                tx_date: trade.tx_date.clone(),
                tx_type: trade.tx_type.clone(),
                trade_price: trade.trade_price,
                price_30d_later: price_30d,
                price_change_pct,
                direction,
            });
        }
    }

    signals
}

/// Detect unusual trading volume.
///
/// Compares recent trading frequency to historical baseline. Division-by-zero safe.
pub fn detect_unusual_volume(
    trades: &[TradeVolumeRecord],
    politician_id: &str,
    reference_date: NaiveDate,
    lookback_days: i64,
    baseline_days: i64,
) -> VolumeSignal {
    // Filter trades for this politician
    let politician_trades: Vec<&TradeVolumeRecord> = trades
        .iter()
        .filter(|t| t.politician_id == politician_id)
        .collect();

    // Calculate date boundaries
    let recent_start = reference_date - chrono::Duration::days(lookback_days);
    let baseline_start = recent_start - chrono::Duration::days(baseline_days);

    // Count recent trades (within lookback window)
    let recent_trade_count = politician_trades
        .iter()
        .filter(|t| {
            if let Ok(tx_date) = NaiveDate::parse_from_str(&t.tx_date, "%Y-%m-%d") {
                tx_date >= recent_start && tx_date <= reference_date
            } else {
                false
            }
        })
        .count();

    // Count historical trades (baseline window before recent window)
    let historical_trade_count = politician_trades
        .iter()
        .filter(|t| {
            if let Ok(tx_date) = NaiveDate::parse_from_str(&t.tx_date, "%Y-%m-%d") {
                tx_date >= baseline_start && tx_date < recent_start
            } else {
                false
            }
        })
        .count();

    // Calculate historical average per lookback window
    let num_lookback_windows = baseline_days as f64 / lookback_days as f64;
    let historical_avg = if num_lookback_windows > 0.0 {
        historical_trade_count as f64 / num_lookback_windows
    } else {
        0.0
    };

    // Calculate volume ratio (division-by-zero safe)
    let volume_ratio = if historical_avg > 0.0 {
        recent_trade_count as f64 / historical_avg
    } else {
        0.0
    };

    // Flag as unusual if ratio > 2.0
    let is_unusual = volume_ratio > 2.0;

    VolumeSignal {
        recent_trade_count,
        historical_avg,
        volume_ratio,
        is_unusual,
    }
}

/// Calculate sector concentration score using HHI.
///
/// Excludes positions with no sector or non-positive value. Division-by-zero safe.
pub fn calculate_sector_concentration(positions: &[PortfolioPositionForHHI]) -> ConcentrationScore {
    // Filter positions with valid sector and positive value
    let valid_positions: Vec<&PortfolioPositionForHHI> = positions
        .iter()
        .filter(|p| p.gics_sector.is_some() && p.estimated_value > 0.0)
        .collect();

    // If no valid positions, return zero score
    if valid_positions.is_empty() {
        return ConcentrationScore {
            sector_weights: HashMap::new(),
            hhi_score: 0.0,
            dominant_sector: None,
            is_concentrated: false,
        };
    }

    // Calculate total portfolio value
    let total_value: f64 = valid_positions.iter().map(|p| p.estimated_value).sum();

    // Aggregate values by sector
    let mut sector_values: HashMap<String, f64> = HashMap::new();
    for position in valid_positions {
        if let Some(ref sector) = position.gics_sector {
            *sector_values.entry(sector.clone()).or_insert(0.0) += position.estimated_value;
        }
    }

    // Calculate sector weights as percentages
    let mut sector_weights: HashMap<String, f64> = HashMap::new();
    for (sector, value) in &sector_values {
        let weight_pct = (value / total_value) * 100.0;
        sector_weights.insert(sector.clone(), weight_pct);
    }

    // Calculate HHI using decimal weights (0-1 scale)
    let hhi_score: f64 = sector_values
        .values()
        .map(|value| {
            let weight_decimal = value / total_value;
            weight_decimal * weight_decimal
        })
        .sum();

    // Determine dominant sector (highest weight)
    let dominant_sector = sector_weights
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(sector, _)| sector.clone());

    // Flag as concentrated if HHI > 0.25
    let is_concentrated = hhi_score > 0.25;

    ConcentrationScore {
        sector_weights,
        hhi_score,
        dominant_sector,
        is_concentrated,
    }
}

/// Calculate composite anomaly score from individual signals.
///
/// Normalizes and weights all signals. Confidence reflects data availability.
pub fn calculate_composite_anomaly_score(
    pre_move_count: usize,
    volume_ratio: f64,
    hhi_score: f64,
) -> AnomalyScore {
    // Normalize pre-move count: divide by 10, cap at 1.0
    let pre_move_norm = (pre_move_count as f64 / 10.0).min(1.0);

    // Normalize volume ratio: divide by 5.0, cap at 1.0
    let volume_norm = (volume_ratio / 5.0).min(1.0);

    // Concentration is already 0-1 (HHI directly)
    let concentration_norm = hhi_score.min(1.0);

    // Count available signals (non-zero)
    let mut signal_count = 0;
    if pre_move_norm > 0.0 {
        signal_count += 1;
    }
    if volume_norm > 0.0 {
        signal_count += 1;
    }
    if concentration_norm > 0.0 {
        signal_count += 1;
    }

    // Calculate composite as average of all signals (equal weights 33.3% each)
    let composite = (pre_move_norm + volume_norm + concentration_norm) / 3.0;

    // Confidence is proportion of available signals (0-1)
    let confidence = signal_count as f64 / 3.0;

    AnomalyScore {
        pre_move_norm,
        volume_norm,
        concentration_norm,
        composite,
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pre-move detection tests
    #[test]
    fn test_pre_move_basic_detection() {
        let trades = vec![TradeWithFuturePrice {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_date: "2024-01-01".to_string(),
            tx_type: "buy".to_string(),
            trade_price: 100.0,
            price_30d_later: Some(115.0),
        }];

        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].tx_id, 1);
        assert!((signals[0].price_change_pct - 15.0).abs() < 0.01);
        assert_eq!(signals[0].direction, "buy_before_rise");
    }

    #[test]
    fn test_pre_move_below_threshold() {
        let trades = vec![TradeWithFuturePrice {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_date: "2024-01-01".to_string(),
            tx_type: "buy".to_string(),
            trade_price: 100.0,
            price_30d_later: Some(105.0),
        }];

        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn test_pre_move_none_price_excluded() {
        let trades = vec![TradeWithFuturePrice {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_date: "2024-01-01".to_string(),
            tx_type: "buy".to_string(),
            trade_price: 100.0,
            price_30d_later: None,
        }];

        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn test_pre_move_negative_change() {
        let trades = vec![TradeWithFuturePrice {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_date: "2024-01-01".to_string(),
            tx_type: "sell".to_string(),
            trade_price: 100.0,
            price_30d_later: Some(85.0),
        }];

        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 1);
        assert!((signals[0].price_change_pct - (-15.0)).abs() < 0.01);
        assert_eq!(signals[0].direction, "sell_before_drop");
    }

    #[test]
    fn test_pre_move_empty_input() {
        let trades: Vec<TradeWithFuturePrice> = vec![];
        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn test_pre_move_buy_before_drop() {
        let trades = vec![TradeWithFuturePrice {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_date: "2024-01-01".to_string(),
            tx_type: "buy".to_string(),
            trade_price: 100.0,
            price_30d_later: Some(85.0),
        }];

        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].direction, "buy_before_drop");
    }

    #[test]
    fn test_pre_move_sell_before_rise() {
        let trades = vec![TradeWithFuturePrice {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_date: "2024-01-01".to_string(),
            tx_type: "sell".to_string(),
            trade_price: 100.0,
            price_30d_later: Some(115.0),
        }];

        let signals = detect_pre_move_trades(&trades, 10.0);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].direction, "sell_before_rise");
    }

    // Volume detection tests
    #[test]
    fn test_volume_unusual_spike() {
        let trades = vec![
            // Recent 90 days: 10 trades
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-05".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-10".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-20".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-25".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-02-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-02-10".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-03-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-03-15".to_string(),
            },
            // Older trades (baseline): 20 trades spread over prior 365 days
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-01-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-02-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-03-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-04-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-05-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-06-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-07-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-08-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-09-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-10-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-11-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-12-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-01-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-02-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-03-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-04-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-05-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-06-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-07-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-08-15".to_string(),
            },
        ];

        let reference = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        let signal = detect_unusual_volume(&trades, "P000001", reference, 90, 365);

        assert_eq!(signal.recent_trade_count, 10);
        assert!((signal.historical_avg - 4.93).abs() < 0.1); // 20 / (365/90) = 4.93
        assert!((signal.volume_ratio - 2.03).abs() < 0.1); // 10 / 4.93 = 2.03
        assert!(signal.is_unusual); // ratio > 2.0
    }

    #[test]
    fn test_volume_normal() {
        let trades = vec![
            // Recent 90 days: 3 trades
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-02-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-03-01".to_string(),
            },
            // Older baseline: 20 trades
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-01-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-02-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-03-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-04-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-05-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-06-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-07-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-08-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-09-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-10-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-11-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-12-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-01-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-02-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-03-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-04-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-05-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-06-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-07-15".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-08-15".to_string(),
            },
        ];

        let reference = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        let signal = detect_unusual_volume(&trades, "P000001", reference, 90, 365);

        assert_eq!(signal.recent_trade_count, 3);
        assert!((signal.volume_ratio - 0.61).abs() < 0.1);
        assert!(!signal.is_unusual); // ratio < 2.0
    }

    #[test]
    fn test_volume_no_historical() {
        let trades = vec![
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-01-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-02-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2024-03-01".to_string(),
            },
        ];

        let reference = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        let signal = detect_unusual_volume(&trades, "P000001", reference, 90, 365);

        assert_eq!(signal.recent_trade_count, 3);
        assert_eq!(signal.historical_avg, 0.0);
        assert_eq!(signal.volume_ratio, 0.0);
        assert!(!signal.is_unusual);
    }

    #[test]
    fn test_volume_no_recent() {
        let trades = vec![
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-01-01".to_string(),
            },
            TradeVolumeRecord {
                politician_id: "P000001".to_string(),
                tx_date: "2023-02-01".to_string(),
            },
        ];

        let reference = NaiveDate::from_ymd_opt(2024, 3, 31).unwrap();
        let signal = detect_unusual_volume(&trades, "P000001", reference, 90, 365);

        assert_eq!(signal.recent_trade_count, 0);
        assert_eq!(signal.volume_ratio, 0.0);
        assert!(!signal.is_unusual);
    }

    // HHI concentration tests
    #[test]
    fn test_hhi_single_sector() {
        let positions = vec![
            PortfolioPositionForHHI {
                ticker: "AAPL".to_string(),
                gics_sector: Some("Information Technology".to_string()),
                estimated_value: 10000.0,
            },
            PortfolioPositionForHHI {
                ticker: "MSFT".to_string(),
                gics_sector: Some("Information Technology".to_string()),
                estimated_value: 5000.0,
            },
        ];

        let score = calculate_sector_concentration(&positions);
        assert_eq!(score.sector_weights.len(), 1);
        assert!((score.sector_weights["Information Technology"] - 100.0).abs() < 0.01);
        assert!((score.hhi_score - 1.0).abs() < 0.01);
        assert_eq!(score.dominant_sector, Some("Information Technology".to_string()));
        assert!(score.is_concentrated);
    }

    #[test]
    fn test_hhi_two_equal_sectors() {
        let positions = vec![
            PortfolioPositionForHHI {
                ticker: "AAPL".to_string(),
                gics_sector: Some("Information Technology".to_string()),
                estimated_value: 5000.0,
            },
            PortfolioPositionForHHI {
                ticker: "JPM".to_string(),
                gics_sector: Some("Financials".to_string()),
                estimated_value: 5000.0,
            },
        ];

        let score = calculate_sector_concentration(&positions);
        assert_eq!(score.sector_weights.len(), 2);
        assert!((score.sector_weights["Information Technology"] - 50.0).abs() < 0.01);
        assert!((score.sector_weights["Financials"] - 50.0).abs() < 0.01);
        assert!((score.hhi_score - 0.5).abs() < 0.01); // 0.5^2 + 0.5^2 = 0.5
        assert!(score.is_concentrated); // 0.5 > 0.25
    }

    #[test]
    fn test_hhi_four_equal_sectors() {
        let positions = vec![
            PortfolioPositionForHHI {
                ticker: "AAPL".to_string(),
                gics_sector: Some("Information Technology".to_string()),
                estimated_value: 2500.0,
            },
            PortfolioPositionForHHI {
                ticker: "JPM".to_string(),
                gics_sector: Some("Financials".to_string()),
                estimated_value: 2500.0,
            },
            PortfolioPositionForHHI {
                ticker: "JNJ".to_string(),
                gics_sector: Some("Health Care".to_string()),
                estimated_value: 2500.0,
            },
            PortfolioPositionForHHI {
                ticker: "XOM".to_string(),
                gics_sector: Some("Energy".to_string()),
                estimated_value: 2500.0,
            },
        ];

        let score = calculate_sector_concentration(&positions);
        assert_eq!(score.sector_weights.len(), 4);
        assert!((score.hhi_score - 0.25).abs() < 0.01); // 4 * 0.25^2 = 0.25
        assert!(!score.is_concentrated); // 0.25 == 0.25 (not >)
    }

    #[test]
    fn test_hhi_null_sectors_excluded() {
        let positions = vec![
            PortfolioPositionForHHI {
                ticker: "AAPL".to_string(),
                gics_sector: Some("Information Technology".to_string()),
                estimated_value: 5000.0,
            },
            PortfolioPositionForHHI {
                ticker: "UNKNOWN".to_string(),
                gics_sector: None,
                estimated_value: 5000.0,
            },
        ];

        let score = calculate_sector_concentration(&positions);
        assert_eq!(score.sector_weights.len(), 1);
        assert!((score.sector_weights["Information Technology"] - 100.0).abs() < 0.01);
        assert!((score.hhi_score - 1.0).abs() < 0.01);
        assert!(score.is_concentrated);
    }

    #[test]
    fn test_hhi_empty_positions() {
        let positions: Vec<PortfolioPositionForHHI> = vec![];
        let score = calculate_sector_concentration(&positions);
        assert_eq!(score.sector_weights.len(), 0);
        assert_eq!(score.hhi_score, 0.0);
        assert_eq!(score.dominant_sector, None);
        assert!(!score.is_concentrated);
    }

    #[test]
    fn test_hhi_negative_value_excluded() {
        let positions = vec![
            PortfolioPositionForHHI {
                ticker: "AAPL".to_string(),
                gics_sector: Some("Information Technology".to_string()),
                estimated_value: 5000.0,
            },
            PortfolioPositionForHHI {
                ticker: "BAD".to_string(),
                gics_sector: Some("Financials".to_string()),
                estimated_value: -1000.0,
            },
        ];

        let score = calculate_sector_concentration(&positions);
        assert_eq!(score.sector_weights.len(), 1);
        assert!((score.hhi_score - 1.0).abs() < 0.01);
    }

    // Composite score tests
    #[test]
    fn test_composite_all_signals() {
        let score = calculate_composite_anomaly_score(5, 3.0, 0.4);
        assert!((score.pre_move_norm - 0.5).abs() < 0.01); // 5/10 = 0.5
        assert!((score.volume_norm - 0.6).abs() < 0.01); // 3.0/5.0 = 0.6
        assert!((score.concentration_norm - 0.4).abs() < 0.01); // 0.4 directly
        assert!((score.composite - 0.5).abs() < 0.01); // (0.5 + 0.6 + 0.4)/3 = 0.5
        assert!((score.confidence - 1.0).abs() < 0.01); // 3/3 = 1.0
    }

    #[test]
    fn test_composite_no_signals() {
        let score = calculate_composite_anomaly_score(0, 0.0, 0.0);
        assert_eq!(score.pre_move_norm, 0.0);
        assert_eq!(score.volume_norm, 0.0);
        assert_eq!(score.concentration_norm, 0.0);
        assert_eq!(score.composite, 0.0);
        assert_eq!(score.confidence, 0.0);
    }

    #[test]
    fn test_composite_capped_at_one() {
        let score = calculate_composite_anomaly_score(15, 10.0, 1.0);
        assert!((score.pre_move_norm - 1.0).abs() < 0.01); // 15/10 = 1.5, capped to 1.0
        assert!((score.volume_norm - 1.0).abs() < 0.01); // 10/5 = 2.0, capped to 1.0
        assert!((score.concentration_norm - 1.0).abs() < 0.01); // 1.0 directly
        assert!((score.composite - 1.0).abs() < 0.01); // (1.0 + 1.0 + 1.0)/3 = 1.0
        assert!((score.confidence - 1.0).abs() < 0.01);
    }
}
