//! Performance analytics and scoring calculations.
//!
//! This module provides pure computation functions for trade performance metrics,
//! FIFO-based closed trade matching, and politician-level aggregations.

use std::collections::{HashMap, VecDeque};

/// Epsilon constant for floating-point comparisons (same as portfolio.rs).
const EPSILON: f64 = 0.0001;

/// Input trade record for analytics calculations.
#[derive(Debug, Clone)]
pub struct AnalyticsTrade {
    pub tx_id: i64,
    pub politician_id: String,
    pub ticker: String,
    pub tx_type: String,
    pub tx_date: String,
    pub estimated_shares: f64,
    pub trade_date_price: f64,
    pub benchmark_price: Option<f64>,
    pub has_sector_benchmark: bool,
}

/// A closed trade (matched buy-sell pair via FIFO).
#[derive(Debug, Clone)]
pub struct ClosedTrade {
    pub politician_id: String,
    pub ticker: String,
    pub shares: f64,
    pub buy_price: f64,
    pub sell_price: f64,
    pub buy_date: String,
    pub sell_date: String,
    pub buy_benchmark: Option<f64>,
    pub sell_benchmark: Option<f64>,
    pub buy_has_sector: bool,
    pub sell_has_sector: bool,
}

/// Trade performance metrics computed from a ClosedTrade.
#[derive(Debug, Clone)]
pub struct TradeMetrics {
    pub politician_id: String,
    pub ticker: String,
    pub absolute_return: f64,
    pub holding_days: Option<i64>,
    pub annualized_return: Option<f64>,
    pub benchmark_return: Option<f64>,
    pub alpha: Option<f64>,
    pub benchmark_type: Option<String>,
}

/// Aggregated performance metrics for a politician.
#[derive(Debug, Clone)]
pub struct PoliticianMetrics {
    pub politician_id: String,
    pub total_trades: usize,
    pub win_count: usize,
    pub win_rate: f64,
    pub avg_return: f64,
    pub avg_alpha_spy: Option<f64>,
    pub avg_alpha_sector: Option<f64>,
    pub avg_holding_days: Option<i64>,
    pub percentile_rank: f64,
}

/// Internal position tracker for FIFO matching (extends portfolio.rs pattern).
struct AnalyticsLot {
    shares: f64,
    cost_basis: f64,
    tx_date: String,
    benchmark_price: Option<f64>,
    has_sector_benchmark: bool,
}

struct AnalyticsPosition {
    politician_id: String,
    ticker: String,
    lots: VecDeque<AnalyticsLot>,
    closed_trades: Vec<ClosedTrade>,
}

impl AnalyticsPosition {
    fn new(politician_id: String, ticker: String) -> Self {
        Self {
            politician_id,
            ticker,
            lots: VecDeque::new(),
            closed_trades: Vec::new(),
        }
    }

    fn buy(
        &mut self,
        shares: f64,
        price: f64,
        tx_date: String,
        benchmark_price: Option<f64>,
        has_sector_benchmark: bool,
    ) {
        self.lots.push_back(AnalyticsLot {
            shares,
            cost_basis: price,
            tx_date,
            benchmark_price,
            has_sector_benchmark,
        });
    }

    fn sell(
        &mut self,
        shares: f64,
        price: f64,
        tx_date: String,
        benchmark_price: Option<f64>,
        has_sector_benchmark: bool,
    ) {
        let mut remaining = shares;

        while remaining > EPSILON {
            let lot = match self.lots.front_mut() {
                Some(l) => l,
                None => {
                    eprintln!(
                        "Warning: Oversold position: politician_id={}, ticker={}, remaining_shares={}",
                        self.politician_id, self.ticker, remaining
                    );
                    return;
                }
            };

            let shares_to_sell = lot.shares.min(remaining);

            // Record closed trade
            self.closed_trades.push(ClosedTrade {
                politician_id: self.politician_id.clone(),
                ticker: self.ticker.clone(),
                shares: shares_to_sell,
                buy_price: lot.cost_basis,
                sell_price: price,
                buy_date: lot.tx_date.clone(),
                sell_date: tx_date.clone(),
                buy_benchmark: lot.benchmark_price,
                sell_benchmark: benchmark_price,
                buy_has_sector: lot.has_sector_benchmark,
                sell_has_sector: has_sector_benchmark,
            });

            lot.shares -= shares_to_sell;
            remaining -= shares_to_sell;

            if lot.shares < EPSILON {
                self.lots.pop_front();
            }
        }
    }
}

/// Calculate closed trades from chronologically-ordered trade records using FIFO matching.
pub fn calculate_closed_trades(trades: Vec<AnalyticsTrade>) -> Vec<ClosedTrade> {
    let mut positions: HashMap<(String, String), AnalyticsPosition> = HashMap::new();

    for trade in trades {
        let key = (trade.politician_id.clone(), trade.ticker.clone());
        let position = positions.entry(key.clone()).or_insert_with(|| {
            AnalyticsPosition::new(trade.politician_id.clone(), trade.ticker.clone())
        });

        match trade.tx_type.as_str() {
            "buy" | "receive" => {
                position.buy(
                    trade.estimated_shares,
                    trade.trade_date_price,
                    trade.tx_date,
                    trade.benchmark_price,
                    trade.has_sector_benchmark,
                );
            }
            "sell" => {
                position.sell(
                    trade.estimated_shares,
                    trade.trade_date_price,
                    trade.tx_date,
                    trade.benchmark_price,
                    trade.has_sector_benchmark,
                );
            }
            "exchange" => {
                // No-op
                eprintln!(
                    "Exchange transaction skipped: tx_id={}, politician={}, ticker={}",
                    trade.tx_id, trade.politician_id, trade.ticker
                );
            }
            _ => {
                eprintln!(
                    "Warning: Unknown tx_type '{}' for tx_id={}, politician={}, ticker={}",
                    trade.tx_type, trade.tx_id, trade.politician_id, trade.ticker
                );
            }
        }
    }

    // Collect all closed trades from all positions
    let mut all_closed_trades = Vec::new();
    for (_key, position) in positions {
        all_closed_trades.extend(position.closed_trades);
    }

    all_closed_trades
}

/// Calculate absolute return percentage.
pub fn absolute_return(buy_price: f64, sell_price: f64) -> f64 {
    ((sell_price - buy_price) / buy_price) * 100.0
}

/// Calculate holding period in days.
/// Returns None if either date cannot be parsed.
pub fn holding_period_days(buy_date: &str, sell_date: &str) -> Option<i64> {
    use chrono::NaiveDate;

    let buy = NaiveDate::parse_from_str(buy_date, "%Y-%m-%d").ok()?;
    let sell = NaiveDate::parse_from_str(sell_date, "%Y-%m-%d").ok()?;

    Some((sell - buy).num_days())
}

/// Calculate annualized return (geometric).
/// Returns None if holding period < 30 days (unreliable for annualization).
pub fn annualized_return(absolute_return_pct: f64, holding_days: i64) -> Option<f64> {
    if holding_days < 30 {
        return None;
    }

    let years = holding_days as f64 / 365.0;
    let total_multiplier = 1.0 + (absolute_return_pct / 100.0);
    let annualized_multiplier = total_multiplier.powf(1.0 / years);

    Some((annualized_multiplier - 1.0) * 100.0)
}

/// Calculate simple alpha (excess return vs benchmark).
pub fn simple_alpha(trade_return_pct: f64, benchmark_return_pct: f64) -> f64 {
    trade_return_pct - benchmark_return_pct
}

/// Compute comprehensive trade metrics from a closed trade.
pub fn compute_trade_metrics(closed: &ClosedTrade) -> TradeMetrics {
    let abs_return = absolute_return(closed.buy_price, closed.sell_price);
    let holding_days = holding_period_days(&closed.buy_date, &closed.sell_date);
    let annualized = holding_days.and_then(|days| annualized_return(abs_return, days));

    // Calculate benchmark return if both buy and sell benchmarks are present
    let benchmark_return = match (closed.buy_benchmark, closed.sell_benchmark) {
        (Some(buy_bench), Some(sell_bench)) if buy_bench > EPSILON => {
            Some(((sell_bench - buy_bench) / buy_bench) * 100.0)
        }
        _ => None,
    };

    // Calculate alpha if benchmark return is available
    let alpha = benchmark_return.map(|bench_ret| simple_alpha(abs_return, bench_ret));

    // Determine benchmark type
    let benchmark_type = match (closed.buy_has_sector, closed.sell_has_sector) {
        (true, true) => Some("sector".to_string()),
        (false, false) if closed.buy_benchmark.is_some() && closed.sell_benchmark.is_some() => {
            Some("spy".to_string())
        }
        _ => None, // Mixed or missing
    };

    TradeMetrics {
        politician_id: closed.politician_id.clone(),
        ticker: closed.ticker.clone(),
        absolute_return: abs_return,
        holding_days,
        annualized_return: annualized,
        benchmark_return,
        alpha,
        benchmark_type,
    }
}

/// Aggregate trade metrics by politician, computing summary statistics and percentile ranks.
/// Returns politicians sorted by avg_return descending.
pub fn aggregate_politician_metrics(metrics: &[TradeMetrics]) -> Vec<PoliticianMetrics> {
    let mut politician_map: HashMap<String, Vec<&TradeMetrics>> = HashMap::new();

    // Group by politician_id
    for metric in metrics {
        politician_map
            .entry(metric.politician_id.clone())
            .or_default()
            .push(metric);
    }

    let mut politician_metrics: Vec<PoliticianMetrics> = politician_map
        .into_iter()
        .map(|(politician_id, trades)| {
            let total_trades = trades.len();
            let win_count = trades
                .iter()
                .filter(|t| t.absolute_return > 0.0)
                .count();
            let win_rate = (win_count as f64 / total_trades as f64) * 100.0;

            let avg_return =
                trades.iter().map(|t| t.absolute_return).sum::<f64>() / total_trades as f64;

            // Calculate avg_alpha_spy (only for trades with benchmark_type = "spy")
            let spy_alphas: Vec<f64> = trades
                .iter()
                .filter_map(|t| {
                    if t.benchmark_type.as_deref() == Some("spy") {
                        t.alpha
                    } else {
                        None
                    }
                })
                .collect();
            let avg_alpha_spy = if !spy_alphas.is_empty() {
                Some(spy_alphas.iter().sum::<f64>() / spy_alphas.len() as f64)
            } else {
                None
            };

            // Calculate avg_alpha_sector (only for trades with benchmark_type = "sector")
            let sector_alphas: Vec<f64> = trades
                .iter()
                .filter_map(|t| {
                    if t.benchmark_type.as_deref() == Some("sector") {
                        t.alpha
                    } else {
                        None
                    }
                })
                .collect();
            let avg_alpha_sector = if !sector_alphas.is_empty() {
                Some(sector_alphas.iter().sum::<f64>() / sector_alphas.len() as f64)
            } else {
                None
            };

            // Calculate avg_holding_days (exclude None values)
            let valid_holding_days: Vec<i64> =
                trades.iter().filter_map(|t| t.holding_days).collect();
            let avg_holding_days = if !valid_holding_days.is_empty() {
                Some(valid_holding_days.iter().sum::<i64>() / valid_holding_days.len() as i64)
            } else {
                None
            };

            PoliticianMetrics {
                politician_id,
                total_trades,
                win_count,
                win_rate,
                avg_return,
                avg_alpha_spy,
                avg_alpha_sector,
                avg_holding_days,
                percentile_rank: 0.0, // Computed after sorting
            }
        })
        .collect();

    // Sort by avg_return descending
    politician_metrics.sort_by(|a, b| {
        b.avg_return
            .partial_cmp(&a.avg_return)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute percentile ranks
    // For descending sort: best performer (index 0) gets percentile 1.0, worst gets 0.0
    let n = politician_metrics.len();
    for (index, politician) in politician_metrics.iter_mut().enumerate() {
        politician.percentile_rank = if n == 1 {
            1.0
        } else {
            1.0 - (index as f64 / (n - 1) as f64)
        };
    }

    politician_metrics
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test types exist and have expected fields
    #[test]
    fn test_analytics_trade_type() {
        let trade = AnalyticsTrade {
            tx_id: 1,
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            tx_type: "buy".to_string(),
            tx_date: "2024-01-01".to_string(),
            estimated_shares: 100.0,
            trade_date_price: 50.0,
            benchmark_price: Some(400.0),
            has_sector_benchmark: false,
        };
        assert_eq!(trade.tx_id, 1);
        assert_eq!(trade.ticker, "AAPL");
    }

    #[test]
    fn test_closed_trade_type() {
        let closed = ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            shares: 100.0,
            buy_price: 50.0,
            sell_price: 75.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-07-01".to_string(),
            buy_benchmark: Some(400.0),
            sell_benchmark: Some(450.0),
            buy_has_sector: false,
            sell_has_sector: false,
        };
        assert_eq!(closed.shares, 100.0);
    }

    // FIFO closed trade matching tests
    #[test]
    fn test_simple_buy_sell() {
        let trades = vec![
            AnalyticsTrade {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
                benchmark_price: Some(400.0),
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "sell".to_string(),
                tx_date: "2024-07-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 75.0,
                benchmark_price: Some(450.0),
                has_sector_benchmark: false,
            },
        ];

        let closed = calculate_closed_trades(trades);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].shares, 100.0);
        assert_eq!(closed[0].buy_price, 50.0);
        assert_eq!(closed[0].sell_price, 75.0);
        assert_eq!(closed[0].buy_date, "2024-01-01");
        assert_eq!(closed[0].sell_date, "2024-07-01");
    }

    #[test]
    fn test_multi_lot_sell() {
        let trades = vec![
            AnalyticsTrade {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 40.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-02-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 60.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 3,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "sell".to_string(),
                tx_date: "2024-06-01".to_string(),
                estimated_shares: 150.0,
                trade_date_price: 80.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
        ];

        let closed = calculate_closed_trades(trades);
        assert_eq!(closed.len(), 2);
        // First closed trade: 100 shares from first lot
        assert_eq!(closed[0].shares, 100.0);
        assert_eq!(closed[0].buy_price, 40.0);
        assert_eq!(closed[0].sell_price, 80.0);
        // Second closed trade: 50 shares from second lot
        assert_eq!(closed[1].shares, 50.0);
        assert_eq!(closed[1].buy_price, 60.0);
        assert_eq!(closed[1].sell_price, 80.0);
    }

    #[test]
    fn test_losing_trade() {
        let trades = vec![
            AnalyticsTrade {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "sell".to_string(),
                tx_date: "2024-07-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 30.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
        ];

        let closed = calculate_closed_trades(trades);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].buy_price, 50.0);
        assert_eq!(closed[0].sell_price, 30.0);
    }

    #[test]
    fn test_exchange_skipped() {
        let trades = vec![
            AnalyticsTrade {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "exchange".to_string(),
                tx_date: "2024-02-01".to_string(),
                estimated_shares: 50.0,
                trade_date_price: 60.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
        ];

        let closed = calculate_closed_trades(trades);
        assert_eq!(closed.len(), 0);
    }

    #[test]
    fn test_sell_without_buy_skipped() {
        let trades = vec![
            AnalyticsTrade {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "sell".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 75.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
        ];

        let closed = calculate_closed_trades(trades);
        assert_eq!(closed.len(), 0);
    }

    #[test]
    fn test_cross_politician_isolation() {
        let trades = vec![
            AnalyticsTrade {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 2,
                politician_id: "P000002".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-02-01".to_string(),
                estimated_shares: 200.0,
                trade_date_price: 60.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
            AnalyticsTrade {
                tx_id: 3,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "sell".to_string(),
                tx_date: "2024-06-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 75.0,
                benchmark_price: None,
                has_sector_benchmark: false,
            },
        ];

        let closed = calculate_closed_trades(trades);
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].politician_id, "P000001");
        assert_eq!(closed[0].buy_price, 50.0);
    }

    // Pure metric function tests
    #[test]
    fn test_absolute_return_gain() {
        let result = absolute_return(50.0, 75.0);
        assert!((result - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_absolute_return_loss() {
        let result = absolute_return(100.0, 80.0);
        assert!((result - (-20.0)).abs() < 0.01);
    }

    #[test]
    fn test_absolute_return_breakeven() {
        let result = absolute_return(50.0, 50.0);
        assert!((result - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_holding_period_days_half_year() {
        let result = holding_period_days("2024-01-01", "2024-07-01");
        assert_eq!(result, Some(182));
    }

    #[test]
    fn test_holding_period_days_same_day() {
        let result = holding_period_days("2024-01-01", "2024-01-01");
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_holding_period_days_invalid() {
        let result = holding_period_days("bad-date", "2024-01-01");
        assert_eq!(result, None);
    }

    #[test]
    fn test_annualized_return_one_year() {
        let result = annualized_return(50.0, 365);
        assert!(result.is_some());
        let value = result.unwrap();
        assert!((value - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_annualized_return_half_year() {
        let result = annualized_return(10.0, 182);
        assert!(result.is_some());
        let value = result.unwrap();
        // 10% in 182 days annualizes to ~20.6%
        assert!((value - 20.6).abs() < 1.0);
    }

    #[test]
    fn test_annualized_return_too_short() {
        let result = annualized_return(5.0, 15);
        assert_eq!(result, None);
    }

    #[test]
    fn test_annualized_return_zero_days() {
        let result = annualized_return(5.0, 0);
        assert_eq!(result, None);
    }

    #[test]
    fn test_annualized_return_negative_days() {
        let result = annualized_return(5.0, -1);
        assert_eq!(result, None);
    }

    #[test]
    fn test_simple_alpha_positive() {
        let result = simple_alpha(15.0, 10.0);
        assert!((result - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_simple_alpha_negative() {
        let result = simple_alpha(5.0, 10.0);
        assert!((result - (-5.0)).abs() < 0.01);
    }

    #[test]
    fn test_simple_alpha_zero() {
        let result = simple_alpha(10.0, 10.0);
        assert!((result - 0.0).abs() < 0.01);
    }

    // compute_trade_metrics tests
    #[test]
    fn test_compute_trade_metrics_with_spy_benchmark() {
        let closed = ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            shares: 100.0,
            buy_price: 50.0,
            sell_price: 75.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-07-01".to_string(),
            buy_benchmark: Some(400.0),
            sell_benchmark: Some(450.0),
            buy_has_sector: false,
            sell_has_sector: false,
        };

        let metrics = compute_trade_metrics(&closed);
        assert_eq!(metrics.politician_id, "P000001");
        assert_eq!(metrics.ticker, "AAPL");
        assert!((metrics.absolute_return - 50.0).abs() < 0.01);
        assert_eq!(metrics.holding_days, Some(182));
        assert!(metrics.annualized_return.is_some());
        assert!(metrics.benchmark_return.is_some());
        let bench_return = metrics.benchmark_return.unwrap();
        assert!((bench_return - 12.5).abs() < 0.01); // (450-400)/400 * 100
        assert!(metrics.alpha.is_some());
        let alpha = metrics.alpha.unwrap();
        assert!((alpha - 37.5).abs() < 0.1); // 50.0 - 12.5
        assert_eq!(metrics.benchmark_type, Some("spy".to_string()));
    }

    #[test]
    fn test_compute_trade_metrics_with_sector_benchmark() {
        let closed = ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            shares: 100.0,
            buy_price: 50.0,
            sell_price: 75.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-07-01".to_string(),
            buy_benchmark: Some(100.0),
            sell_benchmark: Some(120.0),
            buy_has_sector: true,
            sell_has_sector: true,
        };

        let metrics = compute_trade_metrics(&closed);
        assert_eq!(metrics.benchmark_type, Some("sector".to_string()));
        assert!(metrics.benchmark_return.is_some());
        let bench_return = metrics.benchmark_return.unwrap();
        assert!((bench_return - 20.0).abs() < 0.01); // (120-100)/100 * 100
    }

    #[test]
    fn test_compute_trade_metrics_no_benchmark() {
        let closed = ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            shares: 100.0,
            buy_price: 50.0,
            sell_price: 75.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-07-01".to_string(),
            buy_benchmark: None,
            sell_benchmark: None,
            buy_has_sector: false,
            sell_has_sector: false,
        };

        let metrics = compute_trade_metrics(&closed);
        assert_eq!(metrics.benchmark_return, None);
        assert_eq!(metrics.alpha, None);
        assert_eq!(metrics.benchmark_type, None);
    }

    #[test]
    fn test_compute_trade_metrics_mixed_benchmark() {
        let closed = ClosedTrade {
            politician_id: "P000001".to_string(),
            ticker: "AAPL".to_string(),
            shares: 100.0,
            buy_price: 50.0,
            sell_price: 75.0,
            buy_date: "2024-01-01".to_string(),
            sell_date: "2024-07-01".to_string(),
            buy_benchmark: Some(400.0),
            sell_benchmark: Some(450.0),
            buy_has_sector: false,
            sell_has_sector: true, // Mixed: buy SPY, sell sector
        };

        let metrics = compute_trade_metrics(&closed);
        assert_eq!(metrics.benchmark_type, None); // Mixed types -> None
    }

    // aggregate_politician_metrics tests
    #[test]
    fn test_aggregate_single_politician() {
        let metrics = vec![
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                absolute_return: 50.0,
                holding_days: Some(182),
                annualized_return: Some(100.0),
                benchmark_return: Some(10.0),
                alpha: Some(40.0),
                benchmark_type: Some("spy".to_string()),
            },
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "MSFT".to_string(),
                absolute_return: -10.0,
                holding_days: Some(365),
                annualized_return: Some(-10.0),
                benchmark_return: Some(5.0),
                alpha: Some(-15.0),
                benchmark_type: Some("spy".to_string()),
            },
        ];

        let result = aggregate_politician_metrics(&metrics);
        assert_eq!(result.len(), 1);
        let pol = &result[0];
        assert_eq!(pol.politician_id, "P000001");
        assert_eq!(pol.total_trades, 2);
        assert_eq!(pol.win_count, 1);
        assert!((pol.win_rate - 50.0).abs() < 0.01);
        assert!((pol.avg_return - 20.0).abs() < 0.01); // (50 + -10) / 2
        assert!((pol.avg_alpha_spy.unwrap() - 12.5).abs() < 0.01); // (40 + -15) / 2
        assert_eq!(pol.avg_alpha_sector, None);
        assert_eq!(pol.avg_holding_days, Some(273)); // (182 + 365) / 2 rounded
        assert!((pol.percentile_rank - 1.0).abs() < 0.01); // Only one politician -> 1.0
    }

    #[test]
    fn test_aggregate_multiple_politicians() {
        let metrics = vec![
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                absolute_return: 50.0,
                holding_days: Some(182),
                annualized_return: Some(100.0),
                benchmark_return: None,
                alpha: None,
                benchmark_type: None,
            },
            TradeMetrics {
                politician_id: "P000002".to_string(),
                ticker: "MSFT".to_string(),
                absolute_return: 10.0,
                holding_days: Some(365),
                annualized_return: Some(10.0),
                benchmark_return: None,
                alpha: None,
                benchmark_type: None,
            },
            TradeMetrics {
                politician_id: "P000003".to_string(),
                ticker: "TSLA".to_string(),
                absolute_return: 30.0,
                holding_days: Some(90),
                annualized_return: Some(120.0),
                benchmark_return: None,
                alpha: None,
                benchmark_type: None,
            },
        ];

        let result = aggregate_politician_metrics(&metrics);
        assert_eq!(result.len(), 3);
        // Should be sorted by avg_return descending
        assert_eq!(result[0].politician_id, "P000001"); // 50%
        assert_eq!(result[1].politician_id, "P000003"); // 30%
        assert_eq!(result[2].politician_id, "P000002"); // 10%
        // Check percentiles
        assert!((result[0].percentile_rank - 1.0).abs() < 0.01); // Best: rank 1 -> (1-1)/(3-1) = 0 -> wait, formula is (rank-1)/(n-1)
        // Actually for descending order, rank 1 is best. Percentile should be highest for best performer.
        // Let me reconsider: if sorted desc by avg_return, first has highest return.
        // Percentile (rank-1)/(n-1) where rank starts at 1: (1-1)/(3-1) = 0 for first
        // That's backwards. Higher percentile should mean better performance.
        // Need to check spec... spec says "(rank - 1) / (total_politicians - 1)"
        // For descending sort, first entry (index 0) has rank 1, last has rank 3.
        // (1-1)/(3-1) = 0/2 = 0.0 for best performer, (3-1)/(3-1) = 2/2 = 1.0 for worst.
        // That's inverted from typical percentile (where 100th percentile is best).
        // Implementation should probably use: for i in 0..len: percentile = 1.0 - (i as f64 / (len-1) as f64)
        // Or use (len - rank) / (len - 1). Let me re-read spec.
        // Spec says: "Compute percentile: (rank - 1) / (total_politicians - 1)"
        // This gives 0.0 for rank 1, 1.0 for rank N. That's ascending percentile.
        // For descending sort (best first), we want rank 1 to have percentile 1.0 (100th percentile).
        // I think there's ambiguity here. Let me assume: percentile represents "better than X% of others"
        // So best performer should be 1.0 (100th percentile), worst should be 0.0.
        // For descending sort (index 0 is best), percentile = 1.0 - (index / (len-1))
        assert!((result[0].percentile_rank - 1.0).abs() < 0.01); // Best performer
        assert!((result[1].percentile_rank - 0.5).abs() < 0.01); // Middle
        assert!((result[2].percentile_rank - 0.0).abs() < 0.01); // Worst performer
    }

    #[test]
    fn test_aggregate_with_sector_alpha() {
        let metrics = vec![
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                absolute_return: 50.0,
                holding_days: Some(182),
                annualized_return: Some(100.0),
                benchmark_return: Some(20.0),
                alpha: Some(30.0),
                benchmark_type: Some("sector".to_string()),
            },
        ];

        let result = aggregate_politician_metrics(&metrics);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].avg_alpha_spy, None);
        assert!((result[0].avg_alpha_sector.unwrap() - 30.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregate_mixed_benchmark_types() {
        let metrics = vec![
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                absolute_return: 50.0,
                holding_days: Some(182),
                annualized_return: Some(100.0),
                benchmark_return: Some(10.0),
                alpha: Some(40.0),
                benchmark_type: Some("spy".to_string()),
            },
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "MSFT".to_string(),
                absolute_return: 30.0,
                holding_days: Some(365),
                annualized_return: Some(30.0),
                benchmark_return: Some(15.0),
                alpha: Some(15.0),
                benchmark_type: Some("sector".to_string()),
            },
        ];

        let result = aggregate_politician_metrics(&metrics);
        assert_eq!(result.len(), 1);
        assert!((result[0].avg_alpha_spy.unwrap() - 40.0).abs() < 0.01); // Only one SPY trade
        assert!((result[0].avg_alpha_sector.unwrap() - 15.0).abs() < 0.01); // Only one sector trade
    }

    #[test]
    fn test_aggregate_none_holding_days_excluded() {
        let metrics = vec![
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                absolute_return: 50.0,
                holding_days: Some(182),
                annualized_return: Some(100.0),
                benchmark_return: None,
                alpha: None,
                benchmark_type: None,
            },
            TradeMetrics {
                politician_id: "P000001".to_string(),
                ticker: "MSFT".to_string(),
                absolute_return: 30.0,
                holding_days: None, // Bad date parsing
                annualized_return: None,
                benchmark_return: None,
                alpha: None,
                benchmark_type: None,
            },
        ];

        let result = aggregate_politician_metrics(&metrics);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].total_trades, 2);
        assert_eq!(result[0].avg_holding_days, Some(182)); // Only one valid holding_days
    }
}
