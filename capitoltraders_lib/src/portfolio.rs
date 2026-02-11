//! FIFO portfolio calculator for position tracking and realized P&L.
//!
//! This module provides a pure logic implementation of First-In-First-Out (FIFO)
//! portfolio accounting. It processes chronologically-ordered trades and maintains
//! per-politician per-ticker positions with lot-level cost basis tracking.

use std::collections::{HashMap, VecDeque};

/// Epsilon constant for floating-point comparisons.
const EPSILON: f64 = 0.0001;

/// A single buy lot in a FIFO queue.
#[derive(Debug, Clone)]
pub struct Lot {
    pub shares: f64,
    pub cost_basis: f64,
    pub tx_date: String,
}

/// A position tracking lots and realized P&L for a (politician, ticker) pair.
#[derive(Debug, Clone)]
pub struct Position {
    pub politician_id: String,
    pub ticker: String,
    pub lots: VecDeque<Lot>,
    pub realized_pnl: f64,
}

impl Position {
    pub fn new(politician_id: String, ticker: String) -> Self {
        Self {
            politician_id,
            ticker,
            lots: VecDeque::new(),
            realized_pnl: 0.0,
        }
    }

    pub fn buy(&mut self, shares: f64, price: f64, tx_date: String) {
        // TODO: implement
    }

    pub fn sell(&mut self, shares: f64, price: f64) -> Result<(), String> {
        // TODO: implement
        Err("not implemented".to_string())
    }

    pub fn shares_held(&self) -> f64 {
        // TODO: implement
        0.0
    }

    pub fn avg_cost_basis(&self) -> f64 {
        // TODO: implement
        0.0
    }
}

/// A trade record for FIFO processing.
#[derive(Debug)]
pub struct TradeFIFO {
    pub tx_id: i64,
    pub politician_id: String,
    pub ticker: String,
    pub tx_type: String,
    pub tx_date: String,
    pub estimated_shares: f64,
    pub trade_date_price: f64,
}

/// Calculate positions from chronologically-ordered trades.
pub fn calculate_positions(trades: Vec<TradeFIFO>) -> HashMap<(String, String), Position> {
    // TODO: implement
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_buy() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());

        assert!((pos.shares_held() - 100.0).abs() < EPSILON);
        assert!((pos.avg_cost_basis() - 50.0).abs() < EPSILON);
        assert!((pos.realized_pnl - 0.0).abs() < EPSILON);
    }

    #[test]
    fn test_buy_then_full_sell() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());
        let result = pos.sell(100.0, 75.0);

        assert!(result.is_ok());
        assert!(pos.shares_held() < EPSILON);
        assert!((pos.realized_pnl - 2500.0).abs() < EPSILON); // (75-50)*100
    }

    #[test]
    fn test_buy_then_partial_sell() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());
        let result = pos.sell(40.0, 75.0);

        assert!(result.is_ok());
        assert!((pos.shares_held() - 60.0).abs() < EPSILON);
        assert!((pos.avg_cost_basis() - 50.0).abs() < EPSILON);
        assert!((pos.realized_pnl - 1000.0).abs() < EPSILON); // (75-50)*40
    }

    #[test]
    fn test_multiple_buys_then_sell_fifo() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(50.0, 40.0, "2024-01-01".to_string());
        pos.buy(50.0, 60.0, "2024-01-02".to_string());
        let result = pos.sell(70.0, 80.0);

        assert!(result.is_ok());
        assert!((pos.shares_held() - 30.0).abs() < EPSILON);
        assert!((pos.avg_cost_basis() - 60.0).abs() < EPSILON);
        // First 50 @ 40 sold @ 80: (80-40)*50 = 2000
        // Next 20 @ 60 sold @ 80: (80-60)*20 = 400
        // Total: 2400
        assert!((pos.realized_pnl - 2400.0).abs() < EPSILON);
    }

    #[test]
    fn test_sell_from_empty() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        let result = pos.sell(10.0, 50.0);

        assert!(result.is_err());
        assert!((pos.realized_pnl - 0.0).abs() < EPSILON);
    }

    #[test]
    fn test_oversold_position() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(30.0, 50.0, "2024-01-01".to_string());
        let result = pos.sell(50.0, 70.0);

        assert!(result.is_err());
        // Should have sold the 30 shares before failing
        assert!(pos.shares_held() < EPSILON);
        assert!((pos.realized_pnl - 600.0).abs() < EPSILON); // (70-50)*30
    }

    #[test]
    fn test_receive_adds_shares() {
        let trades = vec![
            TradeFIFO {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "receive".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 45.0,
            },
        ];

        let positions = calculate_positions(trades);
        let pos = positions.get(&("P000001".to_string(), "AAPL".to_string())).unwrap();

        assert!((pos.shares_held() - 100.0).abs() < EPSILON);
        assert!((pos.avg_cost_basis() - 45.0).abs() < EPSILON);
    }

    #[test]
    fn test_exchange_is_noop() {
        let trades = vec![
            TradeFIFO {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
            },
            TradeFIFO {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "exchange".to_string(),
                tx_date: "2024-01-02".to_string(),
                estimated_shares: 50.0,
                trade_date_price: 60.0,
            },
        ];

        let positions = calculate_positions(trades);
        let pos = positions.get(&("P000001".to_string(), "AAPL".to_string())).unwrap();

        // Exchange should not affect shares
        assert!((pos.shares_held() - 100.0).abs() < EPSILON);
    }

    #[test]
    fn test_multiple_politicians_same_ticker() {
        let trades = vec![
            TradeFIFO {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
            },
            TradeFIFO {
                tx_id: 2,
                politician_id: "P000002".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 200.0,
                trade_date_price: 60.0,
            },
        ];

        let positions = calculate_positions(trades);
        assert_eq!(positions.len(), 2);

        let pos1 = positions.get(&("P000001".to_string(), "AAPL".to_string())).unwrap();
        assert!((pos1.shares_held() - 100.0).abs() < EPSILON);

        let pos2 = positions.get(&("P000002".to_string(), "AAPL".to_string())).unwrap();
        assert!((pos2.shares_held() - 200.0).abs() < EPSILON);
    }

    #[test]
    fn test_same_politician_different_tickers() {
        let trades = vec![
            TradeFIFO {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
            },
            TradeFIFO {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "MSFT".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 200.0,
                trade_date_price: 60.0,
            },
        ];

        let positions = calculate_positions(trades);
        assert_eq!(positions.len(), 2);

        let pos1 = positions.get(&("P000001".to_string(), "AAPL".to_string())).unwrap();
        assert!((pos1.shares_held() - 100.0).abs() < EPSILON);

        let pos2 = positions.get(&("P000001".to_string(), "MSFT".to_string())).unwrap();
        assert!((pos2.shares_held() - 200.0).abs() < EPSILON);
    }

    #[test]
    fn test_epsilon_zero_check() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());
        let result = pos.sell(99.99999, 75.0);

        assert!(result.is_ok());
        // Remaining shares should be effectively zero (less than epsilon)
        assert!(pos.shares_held() < EPSILON);
    }

    #[test]
    fn test_avg_cost_basis_when_empty() {
        let pos = Position::new("P000001".to_string(), "AAPL".to_string());
        assert!((pos.avg_cost_basis() - 0.0).abs() < EPSILON);
    }

    #[test]
    fn test_unknown_tx_type_skipped() {
        let trades = vec![
            TradeFIFO {
                tx_id: 1,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "buy".to_string(),
                tx_date: "2024-01-01".to_string(),
                estimated_shares: 100.0,
                trade_date_price: 50.0,
            },
            TradeFIFO {
                tx_id: 2,
                politician_id: "P000001".to_string(),
                ticker: "AAPL".to_string(),
                tx_type: "mystery".to_string(),
                tx_date: "2024-01-02".to_string(),
                estimated_shares: 50.0,
                trade_date_price: 60.0,
            },
        ];

        let positions = calculate_positions(trades);
        let pos = positions.get(&("P000001".to_string(), "AAPL".to_string())).unwrap();

        // Unknown tx_type should be skipped
        assert!((pos.shares_held() - 100.0).abs() < EPSILON);
    }

    #[test]
    fn test_full_lifecycle() {
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());
        pos.buy(50.0, 60.0, "2024-01-02".to_string());
        pos.sell(80.0, 70.0).unwrap();
        pos.sell(30.0, 80.0).unwrap();

        assert!((pos.shares_held() - 40.0).abs() < EPSILON);
        assert!((pos.avg_cost_basis() - 60.0).abs() < EPSILON);
        // First 80 @ 50 sold @ 70: (70-50)*80 = 1600 (consumes first lot fully + 30 from second)
        // Wait, let me recalculate FIFO:
        // Buy 100 @ 50
        // Buy 50 @ 60
        // Sell 80: consumes all 100 from first lot @ 50, partial ERROR - only 100 available in first lot
        // Actually: Sell 80 @ 70 consumes 80 from first lot (100 @ 50): (70-50)*80 = 1600, 20 @ 50 remain
        // Sell 30 @ 80 consumes remaining 20 @ 50 and 10 @ 60:
        //   (80-50)*20 = 600
        //   (80-60)*10 = 200
        // Total realized: 1600 + 600 + 200 = 2400
        // Remaining: 40 @ 60
        assert!((pos.realized_pnl - 2400.0).abs() < EPSILON);
    }
}
