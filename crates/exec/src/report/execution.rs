use serde::Serialize;

use crate::core::position::FillRecord;
use crate::monitor::metrics::MetricsSnapshot;

#[derive(Debug, Clone, Serialize)]
pub struct TradeReport {
    pub order_id: String,
    pub instrument: String,
    pub side: String,
    pub quantity: f64,
    pub avg_price: f64,
    pub commission: f64,
    pub pnl: f64,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyExecutionReport {
    pub date: String,
    pub trades: Vec<TradeReport>,
    pub total_pnl: f64,
    pub total_commission: f64,
    pub fill_rate: f64,
    pub rejection_rate: f64,
}

impl DailyExecutionReport {
    pub fn to_csv(&self) -> String {
        let mut lines = Vec::new();
        lines.push("order_id,instrument,side,quantity,avg_price,commission,pnl,ts_ms".to_string());
        for t in &self.trades {
            lines.push(format!(
                "{},{},{},{},{},{},{},{}",
                t.order_id,
                t.instrument,
                t.side,
                t.quantity,
                t.avg_price,
                t.commission,
                t.pnl,
                t.ts_ms
            ));
        }
        lines.join("\n")
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Position report snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct PositionReport {
    pub instrument: String,
    pub quantity: f64,
    pub avg_cost: f64,
    pub market_value: f64,
    pub unrealised_pnl: f64,
    pub ts_ms: i64,
}

impl PositionReport {
    pub fn new(
        instrument: &str,
        quantity: f64,
        avg_cost: f64,
        market_price: f64,
        ts_ms: i64,
    ) -> Self {
        let market_value = quantity * market_price;
        let unrealised_pnl = (market_price - avg_cost) * quantity;
        Self {
            instrument: instrument.to_string(),
            quantity,
            avg_cost,
            market_value,
            unrealised_pnl,
            ts_ms,
        }
    }

    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{}",
            self.instrument,
            self.quantity,
            self.avg_cost,
            self.market_value,
            self.unrealised_pnl,
            self.ts_ms
        )
    }
}

/// Regulatory report (MiFID II / FCA style trade reporting stub).
#[derive(Debug, Clone, Serialize)]
pub struct RegulatoryTradeReport {
    pub report_id: String,
    pub trade_date: String,
    pub instrument_isin: String,
    pub side: String,
    pub quantity: f64,
    pub price: f64,
    pub notional: f64,
    pub venue: String,
    pub counterparty: String,
    pub commission: f64,
}

impl RegulatoryTradeReport {
    pub fn from_fill(fill: &TradeReport, trade_date: &str, venue: &str, report_id: &str) -> Self {
        Self {
            report_id: report_id.to_string(),
            trade_date: trade_date.to_string(),
            instrument_isin: fill.instrument.clone(),
            side: fill.side.clone(),
            quantity: fill.quantity,
            price: fill.avg_price,
            notional: fill.quantity * fill.avg_price,
            venue: venue.to_string(),
            counterparty: "BROKER".to_string(),
            commission: fill.commission,
        }
    }

    pub fn to_xml(&self) -> String {
        format!(
            "<Trade><Id>{}</Id><Date>{}</Date><Instrument>{}</Instrument><Side>{}</Side>\
            <Qty>{}</Qty><Price>{}</Price><Notional>{}</Notional><Venue>{}</Venue>\
            <Commission>{}</Commission></Trade>",
            self.report_id,
            self.trade_date,
            self.instrument_isin,
            self.side,
            self.quantity,
            self.price,
            self.notional,
            self.venue,
            self.commission
        )
    }
}

pub struct ExecutionReportBuilder;

impl ExecutionReportBuilder {
    pub fn from_fills(
        fills: &[FillRecord],
        metrics: &MetricsSnapshot,
        date: &str,
    ) -> DailyExecutionReport {
        let trades: Vec<TradeReport> = fills
            .iter()
            .map(|f| TradeReport {
                order_id: f.order_id.clone(),
                instrument: f.instrument.to_string(),
                side: format!("{:?}", f.side),
                quantity: f.qty,
                avg_price: f.price,
                commission: f.commission,
                pnl: 0.0, // gross PnL not tracked at fill level without reference price
                ts_ms: f.ts_ms,
            })
            .collect();

        let total_commission: f64 = fills.iter().map(|f| f.commission).sum();
        let total_pnl = 0.0;

        DailyExecutionReport {
            date: date.to_string(),
            trades,
            total_pnl,
            total_commission,
            fill_rate: metrics.fill_rate,
            rejection_rate: metrics.rejection_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{InstrumentId, Side, Venue};

    use super::*;
    use crate::monitor::metrics::ExecutionMetrics;

    fn fill(order_id: &str, side: Side, qty: f64, price: f64) -> FillRecord {
        FillRecord {
            order_id: order_id.to_string(),
            instrument: InstrumentId::new(Venue::Crypto, "BTC-USD"),
            side,
            qty,
            price,
            commission: 1.0,
            ts_ms: 1000,
        }
    }

    #[test]
    fn report_built_from_fills() {
        let fills = vec![
            fill("o1", Side::Buy, 1.0, 100.0),
            fill("o2", Side::Sell, 1.0, 110.0),
        ];
        let mut m = ExecutionMetrics::new();
        m.record_submit(100);
        m.record_submit(100);
        m.record_fill(200);
        m.record_fill(200);
        let snap = m.snapshot(1000);

        let report = ExecutionReportBuilder::from_fills(&fills, &snap, "2026-04-05");
        assert_eq!(report.trades.len(), 2);
        assert!((report.total_commission - 2.0).abs() < 1e-9);
        assert_eq!(report.date, "2026-04-05");
        assert!((report.fill_rate - 1.0).abs() < 1e-9);
    }

    #[test]
    fn csv_has_correct_format() {
        let fills = vec![fill("o1", Side::Buy, 2.0, 100.0)];
        let snap = ExecutionMetrics::new().snapshot(1000);
        let report = ExecutionReportBuilder::from_fills(&fills, &snap, "2026-04-05");
        let csv = report.to_csv();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "order_id,instrument,side,quantity,avg_price,commission,pnl,ts_ms"
        );
        assert!(lines[1].starts_with("o1,"));
        assert!(lines[1].contains("2"));
    }
}
