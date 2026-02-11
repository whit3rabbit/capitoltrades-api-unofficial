//! Dollar range parsing and share estimation logic for trade value calculation.
//!
//! This module provides primitives for estimating share counts from dollar ranges
//! and historical prices. It does NOT validate tickers or run batch processing --
//! those concerns belong to the enrichment pipeline (Phase 4).

/// A dollar range extracted from trade data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeRange {
    pub low: f64,
    pub high: f64,
}

impl TradeRange {
    /// Calculate the midpoint of the range.
    pub fn midpoint(&self) -> f64 {
        (self.low + self.high) / 2.0
    }
}

/// Parse a trade range from size_range_low and size_range_high.
///
/// Returns None if:
/// - Either bound is None (requires both for validation)
/// - low > high (invalid data)
/// - Both bounds are zero
pub fn parse_trade_range(
    size_range_low: Option<i64>,
    size_range_high: Option<i64>,
) -> Option<TradeRange> {
    match (size_range_low, size_range_high) {
        (Some(low), Some(high)) => {
            if low > high {
                return None; // Invalid: inverted range
            }
            if low == 0 && high == 0 {
                return None; // Invalid: zero range
            }
            Some(TradeRange {
                low: low as f64,
                high: high as f64,
            })
        }
        _ => None, // Missing one or both bounds
    }
}

/// The result of share estimation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShareEstimate {
    pub estimated_shares: f64,
    pub estimated_value: f64,
}

/// Estimate shares from a trade range and historical price.
///
/// Calculation:
/// - estimated_shares = range.midpoint() / trade_date_price
/// - estimated_value = estimated_shares * trade_date_price
///
/// Returns None if:
/// - trade_date_price <= 0.0 (division by zero or negative price)
/// - estimated_value falls outside the original range bounds (floating-point edge case)
///
/// Note: estimated_value should mathematically equal the midpoint (shares = mid/price,
/// value = shares*price = mid). The range validation is a sanity check against NaN/Inf.
pub fn estimate_shares(range: &TradeRange, trade_date_price: f64) -> Option<ShareEstimate> {
    if trade_date_price <= 0.0 {
        return None; // Invalid price
    }

    let midpoint = range.midpoint();
    let estimated_shares = midpoint / trade_date_price;
    let estimated_value = estimated_shares * trade_date_price;

    // Sanity check: estimated_value should fall within the original range bounds.
    // This should virtually never fail with correct inputs, but protects against
    // floating-point edge cases (NaN, Inf).
    if estimated_value < range.low || estimated_value > range.high {
        eprintln!(
            "WARNING: Estimated value {} falls outside range [{}, {}]. Skipping share estimation.",
            estimated_value, range.low, range.high
        );
        return None;
    }

    Some(ShareEstimate {
        estimated_shares,
        estimated_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_both_bounds() {
        let range = parse_trade_range(Some(15001), Some(50000)).unwrap();
        assert_eq!(range.low, 15001.0);
        assert_eq!(range.high, 50000.0);
    }

    #[test]
    fn test_parse_range_missing_low() {
        assert!(parse_trade_range(None, Some(50000)).is_none());
    }

    #[test]
    fn test_parse_range_missing_high() {
        assert!(parse_trade_range(Some(15001), None).is_none());
    }

    #[test]
    fn test_parse_range_both_none() {
        assert!(parse_trade_range(None, None).is_none());
    }

    #[test]
    fn test_parse_range_inverted() {
        assert!(parse_trade_range(Some(50000), Some(15001)).is_none());
    }

    #[test]
    fn test_parse_range_zero_bounds() {
        assert!(parse_trade_range(Some(0), Some(0)).is_none());
    }

    #[test]
    fn test_midpoint_calculation() {
        let range = TradeRange {
            low: 15001.0,
            high: 50000.0,
        };
        assert_eq!(range.midpoint(), 32500.5);
    }

    #[test]
    fn test_estimate_shares_normal() {
        let range = TradeRange {
            low: 15001.0,
            high: 50000.0,
        };
        let result = estimate_shares(&range, 150.0).unwrap();
        // midpoint = 32500.5, shares = 32500.5 / 150.0 = 216.67
        assert!((result.estimated_shares - 216.67).abs() < 0.01);
        // value should equal midpoint
        assert!((result.estimated_value - 32500.5).abs() < 0.01);
    }

    #[test]
    fn test_estimate_shares_zero_price() {
        let range = TradeRange {
            low: 15001.0,
            high: 50000.0,
        };
        assert!(estimate_shares(&range, 0.0).is_none());
    }

    #[test]
    fn test_estimate_shares_negative_price() {
        let range = TradeRange {
            low: 15001.0,
            high: 50000.0,
        };
        assert!(estimate_shares(&range, -10.0).is_none());
    }

    #[test]
    fn test_estimate_shares_small_range() {
        let range = TradeRange {
            low: 1001.0,
            high: 15000.0,
        };
        let result = estimate_shares(&range, 25.0).unwrap();
        // midpoint = 8000.5, shares = 8000.5 / 25.0 = 320.02
        assert!((result.estimated_shares - 320.02).abs() < 0.01);
        assert!((result.estimated_value - 8000.5).abs() < 0.01);
    }

    #[test]
    fn test_estimate_shares_large_range() {
        let range = TradeRange {
            low: 500001.0,
            high: 1000000.0,
        };
        let result = estimate_shares(&range, 3500.0).unwrap();
        // midpoint = 750000.5, shares = 750000.5 / 3500.0 = 214.2858...
        assert!((result.estimated_shares - 214.2858).abs() < 0.01);
        assert!((result.estimated_value - 750000.5).abs() < 0.01);
    }

    #[test]
    fn test_estimate_value_matches_midpoint() {
        let range = TradeRange {
            low: 15001.0,
            high: 50000.0,
        };
        let result = estimate_shares(&range, 123.45).unwrap();
        // Verify estimated_value equals midpoint within reasonable tolerance
        let midpoint = range.midpoint();
        // Use 0.01 tolerance for dollar amounts (1 cent precision)
        assert!((result.estimated_value - midpoint).abs() < 0.01);
    }
}
