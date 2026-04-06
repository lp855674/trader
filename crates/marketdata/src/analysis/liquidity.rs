use domain::NormalizedBar;

#[derive(Debug, Clone)]
pub struct LiquidityMetrics {
    pub instrument: String,
    pub avg_volume: f64,
    pub volume_volatility: f64,
    pub bid_ask_spread_bps: f64,
    pub market_impact_bps: f64,
    pub amihud_ratio: f64,
    pub turnover_ratio: f64,
}

pub struct LiquidityRiskCalculator;

impl LiquidityRiskCalculator {
    pub fn calculate(instrument: &str, bars: &[NormalizedBar], spread_bps: f64) -> LiquidityMetrics {
        if bars.is_empty() {
            return LiquidityMetrics {
                instrument: instrument.to_string(),
                avg_volume: 0.0,
                volume_volatility: 0.0,
                bid_ask_spread_bps: spread_bps,
                market_impact_bps: 0.0,
                amihud_ratio: 0.0,
                turnover_ratio: 0.0,
            };
        }

        let volumes: Vec<f64> = bars.iter().map(|b| b.volume).collect();
        let avg_volume = volumes.iter().sum::<f64>() / volumes.len() as f64;

        let vol_variance = if volumes.len() > 1 {
            volumes.iter().map(|v| (v - avg_volume).powi(2)).sum::<f64>() / volumes.len() as f64
        } else {
            0.0
        };
        let vol_std = vol_variance.sqrt();
        let volume_volatility = if avg_volume > 1e-12 { vol_std / avg_volume } else { 0.0 };

        // Amihud ratio: mean(|return| / volume) per bar
        let amihud_values: Vec<f64> = bars
            .windows(2)
            .map(|w| {
                let ret = ((w[1].close - w[0].close) / w[0].close).abs();
                let vol = w[1].volume;
                if vol > 1e-12 { ret / vol } else { 0.0 }
            })
            .collect();
        let amihud_ratio = if amihud_values.is_empty() {
            0.0
        } else {
            amihud_values.iter().sum::<f64>() / amihud_values.len() as f64
        };

        let last_volume = bars.last().map(|b| b.volume).unwrap_or(0.0);
        let turnover_ratio = if avg_volume > 1e-12 { last_volume / avg_volume } else { 0.0 };

        // market_impact proxy: proportional to spread and amihud
        let market_impact_bps = spread_bps * 0.5 + amihud_ratio * 1e6;

        LiquidityMetrics {
            instrument: instrument.to_string(),
            avg_volume,
            volume_volatility,
            bid_ask_spread_bps: spread_bps,
            market_impact_bps,
            amihud_ratio,
            turnover_ratio,
        }
    }

    pub fn liquidity_score(&self, metrics: &LiquidityMetrics) -> f64 {
        // Higher volume, lower spread, lower amihud → higher score (0-1)
        // Normalize components
        let spread_score = (1.0 - (metrics.bid_ask_spread_bps / 100.0).min(1.0)).max(0.0);
        let vol_score = (metrics.avg_volume / (metrics.avg_volume + 1000.0)).min(1.0);
        let amihud_score = (1.0 - (metrics.amihud_ratio * 1e6).min(1.0)).max(0.0);
        (spread_score + vol_score + amihud_score) / 3.0
    }

    pub fn classify(&self, score: f64) -> &'static str {
        if score >= 0.67 {
            "high"
        } else if score >= 0.33 {
            "medium"
        } else {
            "low"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts_ms: i64, close: f64, volume: f64) -> NormalizedBar {
        NormalizedBar { ts_ms, open: close, high: close, low: close, close, volume }
    }

    #[test]
    fn liquidity_score_high_volume_low_spread() {
        let bars: Vec<NormalizedBar> = (0..20).map(|i| bar(i * 1000, 100.0, 50000.0)).collect();
        let calc = LiquidityRiskCalculator;
        let metrics = LiquidityRiskCalculator::calculate("BTC", &bars, 2.0);
        let score = calc.liquidity_score(&metrics);
        assert!(score > 0.5, "Expected high score, got {}", score);
        assert_eq!(calc.classify(score), "high");
    }

    #[test]
    fn liquidity_metrics_empty_bars() {
        let metrics = LiquidityRiskCalculator::calculate("ETH", &[], 5.0);
        assert_eq!(metrics.avg_volume, 0.0);
    }
}
