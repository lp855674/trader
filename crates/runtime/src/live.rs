use crate::CancellationFlag;
use broker::{Broker, BrokerKind, BrokerPositionSide, BrokerStatus, FakeBrokerAdapter};
use rust_decimal::Decimal;
use storage::{
    BrokerPositionSnapshotCommand, Db, LiveRunCommand, PaperPortfolioSnapshotCommand,
    RuntimeEventCommand, SystemLogCommand,
};
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRuntimeSettings {
    pub run_id: String,
    pub broker_kind: BrokerKind,
    pub account_id: String,
    pub base_currency: String,
    pub initial_cash: Decimal,
    pub broker_snapshot_interval_ms: Option<u64>,
}

pub struct LiveRuntime {
    db: Db,
    settings: LiveRuntimeSettings,
}

impl LiveRuntime {
    pub fn new(db: Db, settings: LiveRuntimeSettings) -> Self {
        Self { db, settings }
    }

    pub async fn broker_status(&self) -> anyhow::Result<BrokerStatus> {
        Ok(FakeBrokerAdapter::new(self.settings.broker_kind)
            .status()
            .await?)
    }

    pub async fn run(&self, cancel: CancellationFlag) -> anyhow::Result<()> {
        let started_at_ms = chrono::Utc::now().timestamp_millis();
        self.db
            .start_live_run(LiveRunCommand {
                run_id: self.settings.run_id.clone(),
                started_at_ms,
                config: serde_json::json!({
                    "broker_kind": self.settings.broker_kind
                }),
            })
            .await?;
        self.record_event("live.started").await?;
        self.record_system_log(
            "INFO",
            "runtime.live",
            "live.started",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "broker_kind": self.settings.broker_kind,
                "account_id": &self.settings.account_id,
            }),
        )
        .await?;
        self.record_baseline_snapshot(started_at_ms).await?;

        while !cancel.is_cancelled() {
            if let Some(interval_ms) = self.settings.broker_snapshot_interval_ms {
                self.record_broker_snapshot().await?;
                sleep(Duration::from_millis(interval_ms)).await;
            } else {
                sleep(Duration::from_millis(10)).await;
            }
        }

        let ended_at_ms = chrono::Utc::now().timestamp_millis();
        self.db
            .update_strategy_run_status(&self.settings.run_id, "stopped", Some(ended_at_ms), None)
            .await?;
        self.record_event("live.stopped").await?;
        self.record_system_log(
            "INFO",
            "runtime.live",
            "live.stopped",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "broker_kind": self.settings.broker_kind,
                "account_id": &self.settings.account_id,
            }),
        )
        .await?;
        Ok(())
    }

    async fn record_event(&self, category: &str) -> storage::StorageResult<()> {
        self.db
            .record_runtime_event(RuntimeEventCommand {
                ts_ms: chrono::Utc::now().timestamp_millis(),
                source: self.settings.run_id.clone(),
                category: category.to_string(),
                payload: serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "broker_kind": self.settings.broker_kind
                }),
            })
            .await
    }

    async fn record_baseline_snapshot(&self, ts_ms: i64) -> storage::StorageResult<()> {
        self.record_cash_snapshot(ts_ms, self.settings.initial_cash)
            .await?;
        Ok(())
    }

    async fn record_broker_snapshot(&self) -> anyhow::Result<()> {
        let broker = FakeBrokerAdapter::new(self.settings.broker_kind);
        let snapshot = broker.account_snapshot(&self.settings.account_id).await?;
        self.record_cash_drift_if_needed(snapshot.cash).await?;
        self.record_cash_snapshot(chrono::Utc::now().timestamp_millis(), snapshot.cash)
            .await?;
        self.record_system_log(
            "INFO",
            "runtime.broker_snapshot",
            "broker.snapshot.cash",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "account_id": &self.settings.account_id,
                "currency": &self.settings.base_currency,
                "cash": snapshot.cash.to_string(),
            }),
        )
        .await?;
        for position in broker.position_snapshots(&self.settings.account_id).await? {
            let symbol = position.symbol.clone();
            let position_side = position_side_slug(position.position_side);
            let qty = position.qty;
            self.record_position_drift_if_needed(&symbol, position_side, qty)
                .await?;
            self.db
                .record_broker_position_snapshot(BrokerPositionSnapshotCommand {
                    run_id: self.settings.run_id.clone(),
                    account_id: position.account_id,
                    ts_ms: chrono::Utc::now().timestamp_millis(),
                    exchange: position.exchange,
                    symbol: position.symbol,
                    position_side: position_side.to_string(),
                    qty: position.qty,
                    avg_price: position.avg_price,
                    mark_price: Some(position.avg_price),
                    margin_used: position.margin_used,
                    unrealized_pnl: position.unrealized_pnl,
                    realized_pnl: Decimal::ZERO,
                    currency: self.settings.base_currency.clone(),
                })
                .await?;
            self.record_system_log(
                "INFO",
                "runtime.broker_snapshot",
                "broker.snapshot.position",
                serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "account_id": &self.settings.account_id,
                    "symbol": symbol,
                    "position_side": position_side,
                    "qty": qty.to_string(),
                    "currency": &self.settings.base_currency,
                }),
            )
            .await?;
        }
        Ok(())
    }

    async fn record_cash_snapshot(&self, ts_ms: i64, cash: Decimal) -> storage::StorageResult<()> {
        self.db
            .record_paper_portfolio_snapshot(PaperPortfolioSnapshotCommand {
                run_id: self.settings.run_id.clone(),
                account_id: self.settings.account_id.clone(),
                ts_ms,
                base_currency: self.settings.base_currency.clone(),
                cash,
                market_value: Decimal::ZERO,
                equity: cash,
                realized_pnl: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                positions: Vec::new(),
            })
            .await
    }

    async fn record_cash_drift_if_needed(&self, broker_cash: Decimal) -> anyhow::Result<()> {
        if self
            .db
            .list_risk_events(&self.settings.run_id)
            .await?
            .iter()
            .any(|event| event.risk_type == "reconciliation_drift")
        {
            return Ok(());
        }
        let Some(runtime_cash) = self
            .db
            .get_latest_cash_snapshot(&self.settings.run_id, Some(&self.settings.base_currency))
            .await?
        else {
            return Ok(());
        };
        let runtime_cash = runtime_cash.cash.parse::<Decimal>()?;
        let drift_abs = (runtime_cash - broker_cash).abs();
        if drift_abs == Decimal::ZERO {
            return Ok(());
        }
        self.db
            .record_runtime_event(RuntimeEventCommand {
                ts_ms: chrono::Utc::now().timestamp_millis(),
                source: self.settings.run_id.clone(),
                category: "algorithm.risk.rejected".to_string(),
                payload: serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "account_id": &self.settings.account_id,
                    "risk_type": "reconciliation_drift",
                    "decision": "rejected",
                    "reason": "cash_total_drift",
                    "threshold": "0",
                    "observed_value": drift_abs.to_string(),
                    "runtime_cash": runtime_cash.to_string(),
                    "broker_cash": broker_cash.to_string(),
                    "currency": &self.settings.base_currency
                }),
            })
            .await?;
        self.record_system_log(
            "WARN",
            "runtime.reconciliation",
            "reconciliation.drift",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "account_id": &self.settings.account_id,
                "risk_type": "reconciliation_drift",
                "reason": "cash_total_drift",
                "threshold": "0",
                "observed_value": drift_abs.to_string(),
                "runtime_cash": runtime_cash.to_string(),
                "broker_cash": broker_cash.to_string(),
                "currency": &self.settings.base_currency,
            }),
        )
        .await?;
        Ok(())
    }

    async fn record_position_drift_if_needed(
        &self,
        symbol: &str,
        position_side: &str,
        broker_qty: Decimal,
    ) -> anyhow::Result<()> {
        let runtime_position = self
            .db
            .get_latest_position_snapshot(&self.settings.run_id, symbol, Some(position_side))
            .await?;
        let (reason, observed_value) = match runtime_position {
            Some(runtime_position) => {
                let runtime_qty = runtime_position.qty.parse::<Decimal>()?;
                let drift_qty = (runtime_qty - broker_qty).abs();
                if drift_qty == Decimal::ZERO {
                    return Ok(());
                }
                ("position_qty_drift", drift_qty)
            }
            None => ("position_missing_runtime", broker_qty.abs()),
        };
        if self
            .db
            .list_risk_events(&self.settings.run_id)
            .await?
            .iter()
            .any(|event| {
                event.risk_type == "reconciliation_drift"
                    && event.symbol.as_deref() == Some(symbol)
                    && event.reason.as_deref() == Some(reason)
            })
        {
            return Ok(());
        }

        self.db
            .record_runtime_event(RuntimeEventCommand {
                ts_ms: chrono::Utc::now().timestamp_millis(),
                source: self.settings.run_id.clone(),
                category: "algorithm.risk.rejected".to_string(),
                payload: serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "account_id": &self.settings.account_id,
                    "symbol": symbol,
                    "position_side": position_side,
                    "risk_type": "reconciliation_drift",
                    "decision": "rejected",
                    "reason": reason,
                    "threshold": "0",
                    "observed_value": observed_value.to_string(),
                    "broker_qty": broker_qty.to_string()
                }),
            })
            .await?;
        self.record_system_log(
            "WARN",
            "runtime.reconciliation",
            "reconciliation.drift",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "account_id": &self.settings.account_id,
                "symbol": symbol,
                "position_side": position_side,
                "risk_type": "reconciliation_drift",
                "reason": reason,
                "threshold": "0",
                "observed_value": observed_value.to_string(),
                "broker_qty": broker_qty.to_string(),
            }),
        )
        .await?;
        Ok(())
    }

    async fn record_system_log(
        &self,
        level: &str,
        target: &str,
        message: &str,
        fields: serde_json::Value,
    ) -> storage::StorageResult<()> {
        self.db
            .record_system_log(SystemLogCommand {
                run_id: Some(self.settings.run_id.clone()),
                ts_ms: chrono::Utc::now().timestamp_millis(),
                level: level.to_string(),
                target: target.to_string(),
                message: message.to_string(),
                fields: Some(fields),
            })
            .await
    }
}

fn position_side_slug(side: BrokerPositionSide) -> &'static str {
    match side {
        BrokerPositionSide::Long => "long",
        BrokerPositionSide::Short => "short",
    }
}
