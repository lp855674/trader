use domain::NormalizedBar as _NormalizedBar;

#[derive(Debug, Clone)]
pub struct DepthSnapshot {
    pub ts_ms: i64,
    pub bids: Vec<(f64, f64)>, // (price, qty), best bid first (highest price)
    pub asks: Vec<(f64, f64)>, // (price, qty), best ask first (lowest price)
}

impl DepthSnapshot {
    pub fn best_bid(&self) -> Option<f64> {
        self.bids.first().map(|(p, _)| *p)
    }

    pub fn best_ask(&self) -> Option<f64> {
        self.asks.first().map(|(p, _)| *p)
    }

    pub fn mid_price(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask()) {
            (Some(bid), Some(ask)) => Some((bid + ask) / 2.0),
            _ => None,
        }
    }

    pub fn spread_bps(&self) -> Option<f64> {
        match (self.best_bid(), self.best_ask(), self.mid_price()) {
            (Some(bid), Some(ask), Some(mid)) if mid > 1e-12 => {
                Some((ask - bid) / mid * 10000.0)
            }
            _ => None,
        }
    }

    pub fn bid_depth(&self, n_levels: usize) -> f64 {
        self.bids.iter().take(n_levels).map(|(_, q)| q).sum()
    }

    pub fn ask_depth(&self, n_levels: usize) -> f64 {
        self.asks.iter().take(n_levels).map(|(_, q)| q).sum()
    }

    pub fn imbalance(&self, n_levels: usize) -> f64 {
        let bid = self.bid_depth(n_levels);
        let ask = self.ask_depth(n_levels);
        let total = bid + ask;
        if total < 1e-12 {
            0.0
        } else {
            (bid - ask) / total
        }
    }
}

#[derive(Debug, Clone)]
pub struct DepthMetrics {
    pub avg_spread_bps: f64,
    pub avg_imbalance: f64,
    pub depth_stability: f64,
    pub effective_spread_bps: f64,
}

pub struct MarketDepthAnalyzer;

impl MarketDepthAnalyzer {
    pub fn analyze(snapshots: &[DepthSnapshot]) -> DepthMetrics {
        if snapshots.is_empty() {
            return DepthMetrics {
                avg_spread_bps: 0.0,
                avg_imbalance: 0.0,
                depth_stability: 0.0,
                effective_spread_bps: 0.0,
            };
        }

        let spreads: Vec<f64> = snapshots
            .iter()
            .filter_map(|s| s.spread_bps())
            .collect();
        let avg_spread_bps = if spreads.is_empty() {
            0.0
        } else {
            spreads.iter().sum::<f64>() / spreads.len() as f64
        };

        let imbalances: Vec<f64> = snapshots
            .iter()
            .map(|s| s.imbalance(5))
            .collect();
        let avg_imbalance = imbalances.iter().sum::<f64>() / imbalances.len() as f64;

        // depth_stability = std of imbalance (lower = more stable)
        let imb_variance = imbalances
            .iter()
            .map(|x| (x - avg_imbalance).powi(2))
            .sum::<f64>()
            / imbalances.len() as f64;
        let depth_stability = imb_variance.sqrt();

        let effective_spread_bps = avg_spread_bps * 0.7; // typical effective vs quoted

        DepthMetrics {
            avg_spread_bps,
            avg_imbalance,
            depth_stability,
            effective_spread_bps,
        }
    }

    pub fn detect_order_book_stress(snapshots: &[DepthSnapshot]) -> bool {
        if snapshots.is_empty() {
            return false;
        }
        // Stress if any snapshot has empty bids or asks
        let any_empty = snapshots.iter().any(|s| s.bids.is_empty() || s.asks.is_empty());
        if any_empty {
            return true;
        }
        // Or if avg spread > 50bps
        let spreads: Vec<f64> = snapshots.iter().filter_map(|s| s.spread_bps()).collect();
        let avg = if spreads.is_empty() {
            0.0
        } else {
            spreads.iter().sum::<f64>() / spreads.len() as f64
        };
        avg > 50.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(ts_ms: i64, bid: f64, ask: f64) -> DepthSnapshot {
        DepthSnapshot {
            ts_ms,
            bids: vec![(bid, 100.0), (bid - 0.1, 200.0)],
            asks: vec![(ask, 100.0), (ask + 0.1, 200.0)],
        }
    }

    #[test]
    fn spread_bps_calculation() {
        let s = make_snapshot(0, 99.9, 100.1);
        let spread = s.spread_bps().unwrap();
        // (100.1 - 99.9) / 100.0 * 10000 = 20bps
        assert!((spread - 20.0).abs() < 0.5);
    }

    #[test]
    fn imbalance_balanced() {
        let s = DepthSnapshot {
            ts_ms: 0,
            bids: vec![(99.0, 100.0)],
            asks: vec![(101.0, 100.0)],
        };
        assert!((s.imbalance(5) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn stress_detection_tight_spread_no_stress() {
        // spread: (100.05 - 99.95) / 100.0 * 10000 = 10bps → no stress
        let snapshots = vec![make_snapshot(0, 99.95, 100.05)];
        assert!(!MarketDepthAnalyzer::detect_order_book_stress(&snapshots));
    }

    #[test]
    fn stress_detection_wide_spread_triggers() {
        // spread: (200 - 100) / 150 * 10000 = ~6667bps → stress
        let snapshots = vec![make_snapshot(0, 100.0, 200.0)];
        assert!(MarketDepthAnalyzer::detect_order_book_stress(&snapshots));
    }
}
