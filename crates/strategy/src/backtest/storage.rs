use super::engine::{BacktestConfig, BacktestState};
use super::performance::PerformanceReport;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub id: String,
    pub config: serde_json::Value,
    pub report: PerformanceReport,
    pub equity_curve: Vec<(i64, f64)>,
    pub created_at_ms: i64,
}

impl BacktestResult {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn equity_curve_csv(&self) -> String {
        let mut rows = String::from("ts_ms,equity\n");
        for &(ts, eq) in &self.equity_curve {
            rows.push_str(&format!("{},{}\n", ts, eq));
        }
        rows
    }

    pub fn summary_csv(&self) -> String {
        let r = &self.report;
        let header = "id,total_return,annualised_return,sharpe_ratio,sortino_ratio,calmar_ratio,max_drawdown,trade_count,win_rate,profit_factor,avg_trade_pnl";
        let profit_factor_str = if r.profit_factor.is_infinite() {
            "inf".to_string()
        } else {
            r.profit_factor.to_string()
        };
        let row = format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            self.id,
            r.total_return,
            r.annualised_return,
            r.sharpe_ratio,
            r.sortino_ratio,
            r.calmar_ratio,
            r.max_drawdown,
            r.trade_count,
            r.win_rate,
            profit_factor_str,
            r.avg_trade_pnl,
        );
        format!("{}\n{}\n", header, row)
    }
}

pub struct ResultStore {
    pub results: HashMap<String, BacktestResult>,
}

impl ResultStore {
    pub fn new() -> Self {
        Self {
            results: HashMap::new(),
        }
    }

    pub fn save(&mut self, result: BacktestResult) {
        self.results.insert(result.id.clone(), result);
    }

    pub fn get(&self, id: &str) -> Option<&BacktestResult> {
        self.results.get(id)
    }

    pub fn list_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.results.keys().cloned().collect();
        ids.sort();
        ids
    }

    pub fn delete(&mut self, id: &str) -> bool {
        self.results.remove(id).is_some()
    }
}

impl Default for ResultStore {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BacktestResultBuilder;

impl BacktestResultBuilder {
    pub fn new(
        strategy_name: &str,
        state: &BacktestState,
        config: &BacktestConfig,
        report: PerformanceReport,
    ) -> BacktestResult {
        let id = format!("{}_{}", strategy_name, config.start_ts_ms);

        let config_value = serde_json::json!({
            "start_ts_ms": config.start_ts_ms,
            "end_ts_ms": config.end_ts_ms,
            "initial_capital": config.initial_capital,
            "granularity_ms": config.granularity_ms,
            "max_positions": config.max_positions,
            "commission_rate": config.commission_rate,
        });

        BacktestResult {
            id,
            config: config_value,
            report,
            equity_curve: state.equity_curve.clone(),
            created_at_ms: state.ts_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::performance::PerformanceCalculator;

    fn make_state_and_config() -> (BacktestState, BacktestConfig) {
        let mut state = BacktestState::new(10_000.0, 0);
        state.equity_curve = vec![(0, 10_000.0), (60_000, 10_200.0), (120_000, 10_150.0)];
        state.trade_count = 2;

        let config = BacktestConfig {
            start_ts_ms: 0,
            end_ts_ms: 120_000,
            initial_capital: 10_000.0,
            instruments: vec![],
            granularity_ms: 60_000,
            max_positions: 5,
            commission_rate: 0.001,
        };
        (state, config)
    }

    #[test]
    fn save_get_delete() {
        let (state, config) = make_state_and_config();
        let report = PerformanceCalculator::calculate(&state, &config);
        let result = BacktestResultBuilder::new("test_strategy", &state, &config, report);

        let mut store = ResultStore::new();
        let id = result.id.clone();
        store.save(result);

        assert!(store.get(&id).is_some());
        assert_eq!(store.list_ids().len(), 1);

        let deleted = store.delete(&id);
        assert!(deleted);
        assert!(store.get(&id).is_none());
    }

    #[test]
    fn json_round_trip() {
        let (state, config) = make_state_and_config();
        let report = PerformanceCalculator::calculate(&state, &config);
        let result = BacktestResultBuilder::new("my_strat", &state, &config, report);

        let json = result.to_json().expect("serialization failed");
        let parsed: BacktestResult = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(parsed.id, result.id);
        assert!((parsed.report.total_return - result.report.total_return).abs() < 1e-10);
    }

    #[test]
    fn equity_curve_csv_format() {
        let (state, config) = make_state_and_config();
        let report = PerformanceCalculator::calculate(&state, &config);
        let result = BacktestResultBuilder::new("strat", &state, &config, report);

        let csv = result.equity_curve_csv();
        assert!(csv.starts_with("ts_ms,equity\n"));
        assert!(csv.contains("0,10000"));
    }

    #[test]
    fn summary_csv_has_header_and_row() {
        let (state, config) = make_state_and_config();
        let report = PerformanceCalculator::calculate(&state, &config);
        let result = BacktestResultBuilder::new("strat", &state, &config, report);

        let csv = result.summary_csv();
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines.len() >= 2);
        assert!(lines[0].contains("total_return"));
        assert!(lines[1].contains("strat_0"));
    }
}
