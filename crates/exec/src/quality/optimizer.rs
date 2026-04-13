#[derive(Debug, Clone)]
pub struct VenueScore {
    pub venue: String,
    pub score: f64,
    pub expected_slippage_bps: f64,
    pub liquidity_score: f64,
}

#[derive(Debug, Clone)]
pub struct OptimizationResult {
    pub recommended_venue: String,
    pub recommended_slice_qty: f64,
    pub recommended_timing: String,
    pub expected_cost_bps: f64,
}

pub struct ExecutionOptimizer;

impl ExecutionOptimizer {
    pub fn new() -> Self {
        Self
    }

    /// Score venues. Each venue is (name, spread_bps, liquidity_score).
    /// Higher liquidity and lower spread = better score.
    pub fn score_venues(&self, venues: &[(&str, f64, f64)]) -> Vec<VenueScore> {
        venues
            .iter()
            .map(|(name, spread_bps, liquidity)| {
                // Simple score: liquidity penalised by spread
                let score = *liquidity / (1.0 + spread_bps / 100.0);
                VenueScore {
                    venue: name.to_string(),
                    score,
                    expected_slippage_bps: *spread_bps / 2.0,
                    liquidity_score: *liquidity,
                }
            })
            .collect()
    }

    /// Recommend slice size: min(total_qty, adv * max_participation).
    pub fn optimize_order_size(&self, total_qty: f64, adv: f64, max_participation: f64) -> f64 {
        let max_slice = adv * max_participation;
        total_qty.min(max_slice)
    }

    /// Select timing strategy based on volatility.
    /// - Low vol (< 0.1): "immediate"
    /// - High vol (>= 0.1): "twap"
    pub fn select_timing(&self, volatility: f64, _time_of_day_ms: u64) -> &'static str {
        if volatility < 0.1 {
            "immediate"
        } else {
            "twap"
        }
    }

    pub fn full_optimization(
        &self,
        qty: f64,
        adv: f64,
        volatility: f64,
        venues: &[(&str, f64, f64)],
    ) -> OptimizationResult {
        let scored = self.score_venues(venues);
        let best = scored.iter().max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let recommended_venue = best.map(|v| v.venue.clone()).unwrap_or_default();
        let expected_cost_bps = best.map(|v| v.expected_slippage_bps).unwrap_or(0.0);

        let slice_qty = self.optimize_order_size(qty, adv, 0.1);
        let timing = self.select_timing(volatility, 0);

        OptimizationResult {
            recommended_venue,
            recommended_slice_qty: slice_qty,
            recommended_timing: timing.to_string(),
            expected_cost_bps,
        }
    }
}

impl Default for ExecutionOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimizer_recommends_twap_for_high_vol() {
        let opt = ExecutionOptimizer::new();
        let timing = opt.select_timing(0.5, 0);
        assert_eq!(timing, "twap");
    }

    #[test]
    fn optimizer_recommends_immediate_for_low_vol() {
        let opt = ExecutionOptimizer::new();
        let timing = opt.select_timing(0.05, 0);
        assert_eq!(timing, "immediate");
    }

    #[test]
    fn score_venues_prefers_high_liquidity_low_spread() {
        let opt = ExecutionOptimizer::new();
        let venues = vec![
            ("venue_a", 5.0, 100.0),  // lower spread, decent liquidity
            ("venue_b", 20.0, 100.0), // higher spread
        ];
        let scores = opt.score_venues(&venues);
        let a = scores.iter().find(|v| v.venue == "venue_a").unwrap();
        let b = scores.iter().find(|v| v.venue == "venue_b").unwrap();
        assert!(a.score > b.score);
    }

    #[test]
    fn optimize_order_size_respects_participation() {
        let opt = ExecutionOptimizer::new();
        let slice = opt.optimize_order_size(1000.0, 5000.0, 0.1);
        // min(1000, 500) = 500
        assert!((slice - 500.0).abs() < 1e-9);
    }
}
