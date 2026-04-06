use serde::{Deserialize, Serialize};
use super::engine::{BacktestConfig, BacktestState};

pub struct EquityCurve(pub Vec<(i64, f64)>);

impl EquityCurve {
    pub fn new(data: Vec<(i64, f64)>) -> Self {
        Self(data)
    }

    pub fn returns(&self) -> Vec<f64> {
        if self.0.len() < 2 {
            return Vec::new();
        }
        self.0
            .windows(2)
            .map(|w| {
                let prev = w[0].1;
                let curr = w[1].1;
                if prev == 0.0 { 0.0 } else { (curr - prev) / prev }
            })
            .collect()
    }

    pub fn total_return(&self) -> f64 {
        if self.0.len() < 2 {
            return 0.0;
        }
        let first = self.0.first().unwrap().1;
        let last = self.0.last().unwrap().1;
        if first == 0.0 { 0.0 } else { (last - first) / first }
    }

    pub fn max_drawdown(&self) -> f64 {
        if self.0.is_empty() {
            return 0.0;
        }
        let mut peak = f64::NEG_INFINITY;
        let mut max_dd = 0.0_f64;
        for &(_, equity) in &self.0 {
            if equity > peak {
                peak = equity;
            }
            if peak > 0.0 {
                let dd = (peak - equity) / peak;
                if dd > max_dd {
                    max_dd = dd;
                }
            }
        }
        max_dd
    }

    pub fn peak_equity(&self) -> f64 {
        self.0.iter().map(|&(_, e)| e).fold(f64::NEG_INFINITY, f64::max)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub total_return: f64,
    pub annualised_return: f64,
    pub sharpe_ratio: f64,
    pub sortino_ratio: f64,
    pub calmar_ratio: f64,
    pub max_drawdown: f64,
    pub trade_count: u64,
    pub win_rate: f64,
    pub profit_factor: f64,
    pub avg_trade_pnl: f64,
}

pub struct PerformanceCalculator;

impl PerformanceCalculator {
    pub fn calculate(state: &BacktestState, config: &BacktestConfig) -> PerformanceReport {
        let curve = EquityCurve::new(state.equity_curve.clone());
        let total_return = curve.total_return();
        let max_drawdown = curve.max_drawdown();
        let returns = curve.returns();

        // Duration in ms
        let duration_ms = config.end_ts_ms - config.start_ts_ms;
        let ms_per_year = 365_u64 * 24 * 3600 * 1000;

        let annualised_return = if duration_ms > 0 {
            total_return * (ms_per_year as f64 / duration_ms as f64)
        } else {
            0.0
        };

        // Periods per year based on granularity
        let periods_per_year = if config.granularity_ms > 0 {
            ms_per_year as f64 / config.granularity_ms as f64
        } else {
            252.0
        };

        let sharpe_ratio = Self::sharpe(&returns, periods_per_year);
        let sortino_ratio = Self::sortino(&returns, periods_per_year);

        let calmar_ratio = if max_drawdown.abs() > 1e-10 {
            annualised_return / max_drawdown.abs()
        } else {
            0.0
        };

        // Win rate: approximate from equity increases
        let win_rate = if returns.is_empty() {
            0.0
        } else {
            let wins = returns.iter().filter(|&&r| r > 0.0).count();
            wins as f64 / returns.len() as f64
        };

        // profit_factor from positive vs negative returns (as proxy for gross profit/loss)
        let gross_profit: f64 = returns.iter().filter(|&&r| r > 0.0).sum();
        let gross_loss: f64 = returns.iter().filter(|&&r| r < 0.0).sum::<f64>().abs();
        let profit_factor = if gross_loss < 1e-10 {
            f64::INFINITY
        } else {
            gross_profit / gross_loss
        };

        let avg_trade_pnl = if state.trade_count > 0 {
            let initial = config.initial_capital;
            let final_equity = curve.0.last().map(|&(_, e)| e).unwrap_or(initial);
            (final_equity - initial) / state.trade_count as f64
        } else {
            0.0
        };

        PerformanceReport {
            total_return,
            annualised_return,
            sharpe_ratio,
            sortino_ratio,
            calmar_ratio,
            max_drawdown,
            trade_count: state.trade_count,
            win_rate,
            profit_factor,
            avg_trade_pnl,
        }
    }

    fn sharpe(returns: &[f64], periods_per_year: f64) -> f64 {
        if returns.len() < 2 {
            return 0.0;
        }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>()
            / (returns.len() - 1) as f64;
        let std_dev = variance.sqrt();
        if std_dev < 1e-10 {
            return 0.0;
        }
        mean / std_dev * periods_per_year.sqrt()
    }

    fn sortino(returns: &[f64], periods_per_year: f64) -> f64 {
        if returns.len() < 2 {
            return 0.0;
        }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let negative: Vec<f64> = returns.iter().filter(|&&r| r < 0.0).copied().collect();
        if negative.is_empty() {
            return f64::INFINITY;
        }
        let neg_mean = negative.iter().sum::<f64>() / negative.len() as f64;
        let downside_var = negative.iter().map(|r| (r - neg_mean).powi(2)).sum::<f64>()
            / negative.len() as f64;
        let downside_std = downside_var.sqrt();
        if downside_std < 1e-10 {
            return 0.0;
        }
        mean / downside_std * periods_per_year.sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_curve(values: &[f64]) -> EquityCurve {
        let data = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as i64 * 60_000, v))
            .collect();
        EquityCurve::new(data)
    }

    #[test]
    fn total_return_calculation() {
        let curve = make_curve(&[1000.0, 1100.0, 1050.0, 1200.0]);
        let tr = curve.total_return();
        assert!((tr - 0.2).abs() < 1e-9); // (1200-1000)/1000 = 0.2
    }

    #[test]
    fn max_drawdown_known_curve() {
        // Peak at 1200, then drops to 900 => drawdown = (1200-900)/1200 = 0.25
        let curve = make_curve(&[1000.0, 1100.0, 1200.0, 1050.0, 900.0, 950.0]);
        let dd = curve.max_drawdown();
        assert!((dd - 0.25).abs() < 1e-9);
    }

    #[test]
    fn sharpe_positive_returns() {
        let curve = make_curve(&[1000.0, 1010.0, 1020.0, 1015.0, 1030.0]);
        let returns = curve.returns();
        // Just verify it's a positive number
        let periods_per_year = 252.0 * 24.0 * 60.0; // 1-minute bars
        let sharpe = PerformanceCalculator::sharpe(&returns, periods_per_year);
        // With mostly positive returns, Sharpe should be positive
        assert!(sharpe > 0.0 || sharpe == 0.0); // non-negative
    }

    #[test]
    fn max_drawdown_monotone_increase() {
        let curve = make_curve(&[1000.0, 1100.0, 1200.0, 1300.0]);
        assert!((curve.max_drawdown() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn returns_computation() {
        let curve = make_curve(&[100.0, 110.0, 99.0]);
        let r = curve.returns();
        assert_eq!(r.len(), 2);
        assert!((r[0] - 0.1).abs() < 1e-9);
        assert!((r[1] - (-11.0 / 110.0)).abs() < 1e-9);
    }

    #[test]
    fn performance_report_via_calculator() {
        let mut state = BacktestState::new(10_000.0, 0);
        state.equity_curve = vec![(0, 10_000.0), (60_000, 10_100.0), (120_000, 10_050.0), (180_000, 10_200.0)];
        state.trade_count = 3;

        let config = BacktestConfig {
            start_ts_ms: 0,
            end_ts_ms: 180_000,
            initial_capital: 10_000.0,
            instruments: vec![],
            granularity_ms: 60_000,
            max_positions: 5,
            commission_rate: 0.001,
        };

        let report = PerformanceCalculator::calculate(&state, &config);
        assert!((report.total_return - 0.02).abs() < 1e-9);
        assert!(report.annualised_return > 0.0);
        assert!(report.max_drawdown >= 0.0);
    }
}
