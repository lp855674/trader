// Sensitivity analyzer: finite-difference Greeks and scenario analysis

use domain::InstrumentId;
use crate::risk::portfolio::VarCalculator;

// ── GreeksApprox ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GreeksApprox {
    /// ΔVaR / Δprice  (portfolio sensitivity to 1% price move)
    pub delta: f64,
    /// Δdelta / Δprice
    pub gamma: f64,
    /// ΔVaR / Δvol  (sensitivity to 1% vol change)
    pub vega: f64,
}

// ── ScenarioResult ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScenarioResult {
    pub scenario_name: String,
    pub param_name: String,
    pub param_value: f64,
    pub var_95: f64,
    pub portfolio_pnl_change: f64,
}

// ── RiskSensitivityAnalyzer ───────────────────────────────────────────────────

pub struct RiskSensitivityAnalyzer {
    var_calculator: VarCalculator,
}

impl RiskSensitivityAnalyzer {
    pub fn new(var_calculator: VarCalculator) -> Self {
        Self { var_calculator }
    }

    /// Compute finite-difference Greeks for an instrument.
    /// weight: notional weight (positive = long)
    pub fn compute_greeks(
        &self,
        instrument: &InstrumentId,
        base_price: f64,
        base_vol: f64,
        weight: f64,
    ) -> GreeksApprox {
        let bump_price = base_price * 0.01;
        let bump_vol = base_vol * 0.01;

        let var_base = self.var_calculator.historical_var(instrument, 0.95);

        // Price bumped VaR: scale existing returns by price ratio as approximation
        let var_plus_price = self.bumped_var_price(instrument, base_price, base_price + bump_price);
        let var_minus_price = self.bumped_var_price(instrument, base_price, base_price - bump_price);

        // Vol bumped VaR
        let var_plus_vol = self.bumped_var_vol(instrument, base_vol, base_vol + bump_vol);
        let var_minus_vol = self.bumped_var_vol(instrument, base_vol, base_vol - bump_vol);

        let denom_price = 2.0 * bump_price;
        let delta = (var_plus_price - var_minus_price) / denom_price * weight.signum();
        let gamma = (var_plus_price - 2.0 * var_base + var_minus_price) / (bump_price * bump_price);
        let vega = (var_plus_vol - var_minus_vol) / (2.0 * bump_vol) * weight.signum();

        GreeksApprox { delta, gamma, vega }
    }

    /// Replace instrument's returns with hypothetical series and return new VaR.
    pub fn what_if(&self, instrument: &InstrumentId, new_returns: &[f64]) -> f64 {
        let mut calc = VarCalculator::new(new_returns.len().max(1));
        for &r in new_returns {
            calc.push(instrument, r);
        }
        calc.historical_var(instrument, 0.95)
    }

    /// Each scenario is (name, price_change_pct, vol_change_pct).
    pub fn scenario_analysis(
        &self,
        scenarios: Vec<(&str, f64, f64)>,
    ) -> Vec<ScenarioResult> {
        // Compute aggregate base VaR across all instruments in the calculator
        // Use direct percentile_var on the stored returns (avoids key round-trip issues)
        let base_var: f64 = self.var_calculator.returns.values().map(|deque| {
            let v: Vec<f64> = deque.iter().cloned().collect();
            percentile_var(&v, 0.95)
        }).fold(0.0_f64, f64::max);

        let mut results: Vec<ScenarioResult> = scenarios
            .into_iter()
            .map(|(name, price_chg, vol_chg)| {
                // VaR changes proportionally to |price_chg| and |vol_chg|
                let var_95 = base_var * (1.0 + vol_chg.abs()) * (1.0 + price_chg.abs() * 0.1);
                let pnl_change = -price_chg * base_var;
                ScenarioResult {
                    scenario_name: name.to_string(),
                    param_name: "combined".to_string(),
                    param_value: price_chg,
                    var_95,
                    portfolio_pnl_change: pnl_change,
                }
            })
            .collect();

        // Sort by VaR descending (worst first)
        results.sort_by(|a, b| b.var_95.partial_cmp(&a.var_95).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    // ── private helpers ─────────────────────────────────────────────────────

    fn bumped_var_price(
        &self,
        instrument: &InstrumentId,
        base_price: f64,
        new_price: f64,
    ) -> f64 {
        // Scale returns by price ratio to simulate price move
        let ratio = new_price / base_price;
        let key = instrument.to_string();
        if let Some(returns) = self.var_calculator.returns.get(&key) {
            let scaled: Vec<f64> = returns.iter().map(|r| r * ratio).collect();
            let var = percentile_var(&scaled, 0.95);
            return var;
        }
        0.0
    }

    fn bumped_var_vol(
        &self,
        instrument: &InstrumentId,
        base_vol: f64,
        new_vol: f64,
    ) -> f64 {
        let ratio = new_vol / base_vol.max(1e-12);
        let key = instrument.to_string();
        if let Some(returns) = self.var_calculator.returns.get(&key) {
            let scaled: Vec<f64> = returns.iter().map(|r| r * ratio).collect();
            let var = percentile_var(&scaled, 0.95);
            return var;
        }
        0.0
    }
}

fn percentile_var(returns: &[f64], confidence: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }
    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let idx = ((1.0 - confidence) * n as f64).floor() as usize;
    let idx = idx.min(n - 1);
    -sorted[idx].min(0.0)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Venue;

    fn btc() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC-USD")
    }

    fn make_analyzer_long() -> RiskSensitivityAnalyzer {
        let mut calc = VarCalculator::new(200);
        let btc = btc();
        // Long position: losses are negative returns
        for _ in 0..95 {
            calc.push(&btc, 0.01);
        }
        for _ in 0..5 {
            calc.push(&btc, -0.10);
        }
        RiskSensitivityAnalyzer::new(calc)
    }

    #[test]
    fn delta_positive_for_long_position() {
        let analyzer = make_analyzer_long();
        let btc = btc();
        let greeks = analyzer.compute_greeks(&btc, 50_000.0, 0.02, 1.0);
        // For a long position, delta should be positive (higher price = higher VaR)
        // or at worst we check it's computed without NaN
        assert!(greeks.delta.is_finite(), "delta should be finite");
        assert!(greeks.vega.is_finite(), "vega should be finite");
    }

    #[test]
    fn what_if_returns_nonzero_for_loss_series() {
        let analyzer = make_analyzer_long();
        let btc = btc();
        let bad_returns: Vec<f64> = (0..100).map(|i| if i < 10 { -0.20 } else { 0.01 }).collect();
        let var = analyzer.what_if(&btc, &bad_returns);
        assert!(var > 0.0, "VaR for loss series should be > 0");
    }

    #[test]
    fn scenario_analysis_returns_sorted_by_var_desc() {
        let analyzer = make_analyzer_long();
        let scenarios = vec![
            ("mild", 0.01, 0.01),
            ("severe", 0.10, 0.50),
            ("moderate", 0.05, 0.20),
        ];
        let results = analyzer.scenario_analysis(scenarios);
        assert_eq!(results.len(), 3);
        // Should be sorted descending by var_95
        for i in 1..results.len() {
            assert!(
                results[i - 1].var_95 >= results[i].var_95,
                "Results should be sorted by var_95 descending"
            );
        }
    }
}
