// Risk report generator

use serde::{Deserialize, Serialize};

// ── DailyRiskReport ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyRiskReport {
    pub date: String,
    pub generated_ts_ms: i64,
    pub total_pnl: f64,
    pub open_positions: u32,
    pub var_95: f64,
    pub max_drawdown: f64,
    pub alerts_triggered: u32,
    pub orders_rejected: u32,
    pub top_risks: Vec<String>,
}

// ── PositionReport ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionReport {
    pub instrument: String,
    pub side: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
    pub unrealised_pnl: f64,
    pub notional: f64,
}

// ── RiskReportBuilder ─────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct RiskReportBuilder {
    pnl: f64,
    positions: Vec<PositionReport>,
    var_95: f64,
    alerts: u32,
    rejections: u32,
    max_drawdown: f64,
    top_risks: Vec<String>,
}

impl RiskReportBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pnl(mut self, pnl: f64) -> Self {
        self.pnl = pnl;
        self
    }

    pub fn with_positions(mut self, positions: Vec<PositionReport>) -> Self {
        // Derive top risks from positions with negative PnL
        self.top_risks = positions
            .iter()
            .filter(|p| p.unrealised_pnl < 0.0)
            .map(|p| format!("{} unrealised loss: {:.2}", p.instrument, p.unrealised_pnl))
            .collect();
        self.positions = positions;
        self
    }

    pub fn with_var(mut self, var_95: f64) -> Self {
        self.var_95 = var_95;
        self
    }

    pub fn with_alerts(mut self, count: u32) -> Self {
        self.alerts = count;
        self
    }

    pub fn with_rejections(mut self, count: u32) -> Self {
        self.rejections = count;
        self
    }

    pub fn build(self, date: &str, ts_ms: i64) -> DailyRiskReport {
        let open_positions = self.positions.len() as u32;
        DailyRiskReport {
            date: date.to_string(),
            generated_ts_ms: ts_ms,
            total_pnl: self.pnl,
            open_positions,
            var_95: self.var_95,
            max_drawdown: self.max_drawdown,
            alerts_triggered: self.alerts,
            orders_rejected: self.rejections,
            top_risks: self.top_risks,
        }
    }
}

// ── ReportExporter ────────────────────────────────────────────────────────────

pub struct ReportExporter;

impl ReportExporter {
    pub fn to_json(report: &DailyRiskReport) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(report)
    }

    pub fn to_csv(report: &DailyRiskReport) -> String {
        let header = "date,generated_ts_ms,total_pnl,open_positions,var_95,max_drawdown,alerts_triggered,orders_rejected";
        let row = format!(
            "{},{},{},{},{},{},{},{}",
            report.date,
            report.generated_ts_ms,
            report.total_pnl,
            report.open_positions,
            report.var_95,
            report.max_drawdown,
            report.alerts_triggered,
            report.orders_rejected,
        );
        format!("{}\n{}", header, row)
    }

    pub fn positions_to_csv(positions: &[PositionReport]) -> String {
        let header = "instrument,side,quantity,entry_price,current_price,unrealised_pnl,notional";
        let rows: Vec<String> = positions
            .iter()
            .map(|p| {
                format!(
                    "{},{},{},{},{},{},{}",
                    p.instrument,
                    p.side,
                    p.quantity,
                    p.entry_price,
                    p.current_price,
                    p.unrealised_pnl,
                    p.notional,
                )
            })
            .collect();
        if rows.is_empty() {
            return format!("{}\n", header);
        }
        format!("{}\n{}", header, rows.join("\n"))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_position() -> PositionReport {
        PositionReport {
            instrument: "BTC-USD".into(),
            side: "Buy".into(),
            quantity: 1.0,
            entry_price: 50_000.0,
            current_price: 52_000.0,
            unrealised_pnl: 2_000.0,
            notional: 52_000.0,
        }
    }

    #[test]
    fn builder_creates_correct_report() {
        let report = RiskReportBuilder::new()
            .with_pnl(1_500.0)
            .with_positions(vec![make_position()])
            .with_var(0.05)
            .with_alerts(2)
            .with_rejections(3)
            .build("2026-04-05", 1_000_000);

        assert_eq!(report.date, "2026-04-05");
        assert_eq!(report.total_pnl, 1_500.0);
        assert_eq!(report.open_positions, 1);
        assert_eq!(report.var_95, 0.05);
        assert_eq!(report.alerts_triggered, 2);
        assert_eq!(report.orders_rejected, 3);
        assert_eq!(report.generated_ts_ms, 1_000_000);
    }

    #[test]
    fn json_round_trip() {
        let report = RiskReportBuilder::new()
            .with_pnl(100.0)
            .with_var(0.03)
            .build("2026-04-05", 0);

        let json = ReportExporter::to_json(&report).expect("JSON serialization failed");
        let deserialized: DailyRiskReport =
            serde_json::from_str(&json).expect("JSON deserialization failed");
        assert!((deserialized.total_pnl - 100.0).abs() < 1e-9);
        assert_eq!(deserialized.date, "2026-04-05");
    }

    #[test]
    fn csv_has_correct_headers() {
        let report = RiskReportBuilder::new().build("2026-04-05", 0);
        let csv = ReportExporter::to_csv(&report);
        assert!(csv.contains("date"), "CSV should contain 'date' header");
        assert!(csv.contains("var_95"), "CSV should contain 'var_95' header");
        assert!(csv.contains("total_pnl"), "CSV should contain 'total_pnl' header");
    }

    #[test]
    fn positions_csv_multi_row() {
        let positions = vec![make_position(), make_position()];
        let csv = ReportExporter::positions_to_csv(&positions);
        let lines: Vec<&str> = csv.lines().collect();
        // header + 2 data rows
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("instrument"));
    }
}
