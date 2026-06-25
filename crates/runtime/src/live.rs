use crate::CancellationFlag;
use broker::{
    Broker, BrokerExecution, BrokerKind, BrokerOpenOrder, BrokerPositionSide, BrokerStatus,
    FakeBrokerAdapter,
};
use events::{LogWriter, LogWriterSettings, SystemLogLayer};
use rust_decimal::Decimal;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use storage::{
    BrokerPositionSnapshotCommand, Db, DbSystemLogSink, LiveRunCommand, NewFill,
    PaperPortfolioSnapshotCommand, RuntimeEventCommand, StoredOrder, SystemLogCommand,
};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::time::{Duration, sleep};
use tracing_subscriber::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRuntimeSettings {
    pub run_id: String,
    pub broker_kind: BrokerKind,
    pub account_id: String,
    pub base_currency: String,
    pub initial_cash: Decimal,
    pub broker_snapshot_interval_ms: Option<u64>,
    pub alert_sink: AlertSinkSettings,
    pub logging: LogWriterSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AlertSinkSettings {
    #[default]
    Noop,
    Multi(Vec<AlertSinkSettings>),
    File {
        path: String,
        cooldown_ms: u64,
    },
    Webhook {
        url: String,
        cooldown_ms: u64,
        timeout_ms: u64,
        max_retries: u32,
        auth_token: Option<String>,
    },
}

pub struct LiveRuntime {
    db: Db,
    settings: LiveRuntimeSettings,
    broker: Arc<dyn Broker>,
    startup_recovery_unmatched_open_orders_policy: StartupRecoveryUnmatchedOpenOrdersPolicy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StartupRecoveryUnmatchedOpenOrdersPolicy {
    #[default]
    Fail,
    WarnOnly,
}

impl LiveRuntime {
    pub fn new(db: Db, settings: LiveRuntimeSettings) -> Self {
        let broker = Arc::new(FakeBrokerAdapter::new(settings.broker_kind));
        Self {
            db,
            settings,
            broker,
            startup_recovery_unmatched_open_orders_policy:
                StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
        }
    }

    pub fn new_with_broker(db: Db, settings: LiveRuntimeSettings, broker: Arc<dyn Broker>) -> Self {
        Self {
            db,
            settings,
            broker,
            startup_recovery_unmatched_open_orders_policy:
                StartupRecoveryUnmatchedOpenOrdersPolicy::Fail,
        }
    }

    pub fn with_startup_recovery_unmatched_open_orders_policy(
        mut self,
        policy: StartupRecoveryUnmatchedOpenOrdersPolicy,
    ) -> Self {
        self.startup_recovery_unmatched_open_orders_policy = policy;
        self
    }

    pub async fn broker_status(&self) -> anyhow::Result<BrokerStatus> {
        Ok(self.broker.status().await?)
    }

    pub async fn run(&self, cancel: CancellationFlag) -> anyhow::Result<()> {
        let log_scope = LiveLogScope::new(
            self.db.clone(),
            self.settings.run_id.clone(),
            self.settings.logging.clone(),
        );
        let started_at_ms = chrono::Utc::now().timestamp_millis();
        tracing::info!(
            run_id = %self.settings.run_id,
            broker_kind = ?self.settings.broker_kind,
            account_id = %self.settings.account_id,
            category = "system",
            "live runtime started"
        );
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
        if let Err(error) = self.recover_startup_orders().await {
            self.record_startup_recovery_failure(&error).await?;
            if let Some(log_scope) = log_scope {
                log_scope.shutdown().await;
            }
            return Err(error);
        }
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
        tracing::info!(
            run_id = %self.settings.run_id,
            broker_kind = ?self.settings.broker_kind,
            account_id = %self.settings.account_id,
            category = "system",
            "live runtime stopped"
        );
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
        if let Some(log_scope) = log_scope {
            log_scope.shutdown().await;
        }
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

    async fn recover_startup_orders(&self) -> anyhow::Result<()> {
        let recoverable = self
            .db
            .list_recoverable_orders(&self.settings.run_id)
            .await?;
        if recoverable.is_empty() {
            self.record_startup_recovery_log(&StartupRecoverySummary::default())
                .await?;
            return Ok(());
        }

        let open_orders = self.broker.open_orders(&self.settings.account_id).await?;
        let symbols = recoverable
            .iter()
            .map(|order| order.symbol.clone())
            .collect::<BTreeSet<_>>();
        let mut executions = Vec::new();
        for symbol in symbols {
            executions.extend(
                self.broker
                    .executions(&self.settings.account_id, Some(&symbol))
                    .await?,
            );
        }
        let existing_fills = self.db.list_fills(&self.settings.run_id).await?;
        let mut existing_fill_ids_by_order = HashMap::<String, HashSet<String>>::new();
        let mut existing_filled_qty_by_order = HashMap::<String, Decimal>::new();
        for fill in existing_fills {
            existing_fill_ids_by_order
                .entry(fill.order_id.clone())
                .or_default()
                .insert(fill.id);
            let qty = fill.qty.parse::<Decimal>()?;
            *existing_filled_qty_by_order
                .entry(fill.order_id)
                .or_default() += qty;
        }

        let mut recovered = 0usize;
        let mut recovered_execution_ids = HashSet::new();
        let mut matched_open_order_ids = HashSet::new();
        let mut matched_execution_ids = HashSet::new();
        for order in &recoverable {
            let open_order = open_orders
                .iter()
                .find(|open_order| broker_order_matches_local(open_order, order));
            let matched_executions = executions
                .iter()
                .filter(|execution| broker_execution_matches_local(execution, order))
                .collect::<Vec<_>>();
            if open_order.is_none() && matched_executions.is_empty() {
                continue;
            }
            if let Some(open_order) = open_order {
                matched_open_order_ids.insert(open_order.broker_order_id.clone());
            }
            matched_execution_ids.extend(
                matched_executions
                    .iter()
                    .map(|execution| execution.trade_id.clone()),
            );

            let broker_order_id = open_order
                .map(|order| order.broker_order_id.as_str())
                .or(order.broker_order_id.as_deref())
                .or_else(|| {
                    matched_executions
                        .first()
                        .map(|execution| execution.broker_order_id.as_str())
                })
                .unwrap_or_default()
                .to_string();
            let existing_fill_ids = existing_fill_ids_by_order
                .get(&order.id)
                .cloned()
                .unwrap_or_default();
            let new_execution_qty = matched_executions
                .iter()
                .filter(|execution| !existing_fill_ids.contains(&execution.trade_id))
                .map(|execution| execution.qty)
                .sum::<Decimal>();
            let mut filled_qty = existing_filled_qty_by_order
                .get(&order.id)
                .copied()
                .unwrap_or_default()
                + new_execution_qty;
            let local_filled_qty = order.filled_qty.parse::<Decimal>()?;
            if local_filled_qty > filled_qty {
                filled_qty = local_filled_qty;
            }
            if let Some(open_order) = open_order
                && open_order.filled_qty > filled_qty
            {
                filled_qty = open_order.filled_qty;
            }
            let matched_execution_count = matched_executions.len();
            let status = recovered_order_status(order, open_order, filled_qty)?;
            let updated_at_ms = chrono::Utc::now().timestamp_millis();
            self.db
                .update_order_execution_by_client_order_id(
                    &order.client_order_id,
                    &broker_order_id,
                    &status,
                    &filled_qty.to_string(),
                    updated_at_ms,
                )
                .await?;

            for execution in matched_executions {
                if !existing_fill_ids.contains(&execution.trade_id)
                    && recovered_execution_ids.insert(execution.trade_id.clone())
                {
                    self.db
                        .insert_fill(NewFill {
                            id: execution.trade_id.clone(),
                            order_id: order.id.clone(),
                            run_id: self.settings.run_id.clone(),
                            symbol: order.symbol.clone(),
                            side: order_side_slug(execution.side).to_string(),
                            price: execution.price.to_string(),
                            qty: execution.qty.to_string(),
                            fee: execution.fee.to_string(),
                            ts_ms: execution.ts_ms,
                        })
                        .await?;
                }
            }
            self.db
                .record_runtime_event(RuntimeEventCommand {
                    ts_ms: updated_at_ms,
                    source: self.settings.run_id.clone(),
                    category: "broker.order.recovered".to_string(),
                    payload: serde_json::json!({
                        "run_id": &self.settings.run_id,
                        "order_id": &order.id,
                        "client_order_id": &order.client_order_id,
                        "broker_order_id": broker_order_id,
                        "account_id": &order.account_id,
                        "symbol": &order.symbol,
                        "status": &status,
                        "filled_qty": filled_qty.to_string(),
                        "executions": matched_execution_count,
                        "recovery_source": "startup",
                        "message": "startup recovery matched broker order state",
                    }),
                })
                .await?;
            recovered += 1;
        }

        let remaining = self
            .db
            .list_recoverable_orders(&self.settings.run_id)
            .await?
            .len();
        let unmatched_open_orders = open_orders
            .iter()
            .filter(|order| !matched_open_order_ids.contains(&order.broker_order_id))
            .map(|order| order.broker_order_id.clone())
            .collect::<Vec<_>>();
        let unmatched_executions = executions
            .iter()
            .filter(|execution| !matched_execution_ids.contains(&execution.trade_id))
            .map(|execution| execution.trade_id.clone())
            .collect::<Vec<_>>();
        let summary = StartupRecoverySummary {
            scanned: recoverable.len(),
            recovered,
            remaining,
            executions: recovered_execution_ids.len(),
            unmatched_open_orders,
            unmatched_executions,
        };
        self.record_startup_recovery_log(&summary).await?;
        if !summary.unmatched_open_orders.is_empty()
            && self.startup_recovery_unmatched_open_orders_policy
                == StartupRecoveryUnmatchedOpenOrdersPolicy::Fail
        {
            anyhow::bail!(
                "unmatched remote open orders during startup recovery: {}",
                summary.unmatched_open_orders.join(",")
            );
        }
        Ok(())
    }

    async fn record_startup_recovery_log(
        &self,
        summary: &StartupRecoverySummary,
    ) -> storage::StorageResult<()> {
        let level = if summary.unmatched_open_orders.is_empty()
            && summary.unmatched_executions.is_empty()
        {
            "INFO"
        } else {
            "WARN"
        };
        self.record_system_log(
            level,
            "runtime.startup_recovery",
            "startup_recovery.orders",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "account_id": &self.settings.account_id,
                "scanned": summary.scanned,
                "recovered": summary.recovered,
                "remaining": summary.remaining,
                "executions": summary.executions,
                "unmatched_open_orders": summary.unmatched_open_orders.len(),
                "unmatched_executions": summary.unmatched_executions.len(),
                "unmatched_open_order_ids": summary.unmatched_open_orders,
                "unmatched_execution_ids": summary.unmatched_executions,
            }),
        )
        .await
    }

    async fn record_startup_recovery_failure(
        &self,
        error: &anyhow::Error,
    ) -> storage::StorageResult<()> {
        let ended_at_ms = chrono::Utc::now().timestamp_millis();
        let error_message = error.to_string();
        self.db
            .update_strategy_run_status(
                &self.settings.run_id,
                "failed",
                Some(ended_at_ms),
                Some(&error_message),
            )
            .await?;
        self.db
            .record_runtime_event(RuntimeEventCommand {
                ts_ms: ended_at_ms,
                source: self.settings.run_id.clone(),
                category: "live.startup_recovery.failed".to_string(),
                payload: serde_json::json!({
                    "run_id": &self.settings.run_id,
                    "broker_kind": self.settings.broker_kind,
                    "account_id": &self.settings.account_id,
                    "error": error_message,
                }),
            })
            .await?;
        self.record_system_log(
            "ERROR",
            "runtime.startup_recovery",
            "startup_recovery.failed",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "broker_kind": self.settings.broker_kind,
                "account_id": &self.settings.account_id,
                "error": error.to_string(),
            }),
        )
        .await
    }

    async fn record_baseline_snapshot(&self, ts_ms: i64) -> storage::StorageResult<()> {
        self.record_cash_snapshot(ts_ms, self.settings.initial_cash)
            .await?;
        Ok(())
    }

    async fn record_broker_snapshot(&self) -> anyhow::Result<()> {
        let snapshot = self
            .broker
            .account_snapshot(&self.settings.account_id)
            .await?;
        self.record_cash_drift_if_needed(snapshot.cash).await?;
        self.record_cash_snapshot(chrono::Utc::now().timestamp_millis(), snapshot.cash)
            .await?;
        tracing::info!(
            run_id = %self.settings.run_id,
            account_id = %self.settings.account_id,
            currency = %self.settings.base_currency,
            cash = %snapshot.cash,
            category = "broker",
            "live broker cash snapshot captured"
        );
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
        for position in self
            .broker
            .position_snapshots(&self.settings.account_id)
            .await?
        {
            let symbol = position.symbol.clone();
            let position_side = position_side_slug(position.position_side);
            let qty = position.qty;
            self.record_position_drift_if_needed(&symbol, position_side, qty)
                .await?;
            tracing::info!(
                run_id = %self.settings.run_id,
                account_id = %self.settings.account_id,
                symbol = %symbol,
                position_side = %position_side,
                qty = %qty,
                currency = %self.settings.base_currency,
                category = "broker",
                "live broker position snapshot captured"
            );
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
        tracing::warn!(
            run_id = %self.settings.run_id,
            account_id = %self.settings.account_id,
            risk_type = "reconciliation_drift",
            reason = "cash_total_drift",
            observed_value = %drift_abs,
            runtime_cash = %runtime_cash,
            broker_cash = %broker_cash,
            currency = %self.settings.base_currency,
            category = "risk",
            "live reconciliation drift detected"
        );
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
        self.record_alert_log(
            "reconciliation_drift.alert",
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

        tracing::warn!(
            run_id = %self.settings.run_id,
            account_id = %self.settings.account_id,
            symbol = %symbol,
            position_side = %position_side,
            risk_type = "reconciliation_drift",
            reason = %reason,
            observed_value = %observed_value,
            broker_qty = %broker_qty,
            category = "risk",
            "live reconciliation drift detected"
        );

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
        self.record_alert_log(
            "reconciliation_drift.alert",
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

    async fn record_alert_log(
        &self,
        message: &str,
        fields: serde_json::Value,
    ) -> storage::StorageResult<()> {
        let now_ts_ms = chrono::Utc::now().timestamp_millis();
        let should_notify = self
            .should_send_alert_notification(message, &fields, now_ts_ms)
            .await
            .unwrap_or(true);
        self.record_system_log("ERROR", "runtime.alert", message, fields.clone())
            .await?;
        if should_notify {
            let deliveries = self
                .send_alert_notification(message, &fields, now_ts_ms)
                .await;
            for delivery in deliveries {
                let _ = self
                    .record_alert_delivery_log(message, &fields, &delivery)
                    .await;
            }
        }
        Ok(())
    }

    async fn should_send_alert_notification(
        &self,
        message: &str,
        fields: &serde_json::Value,
        now_ts_ms: i64,
    ) -> storage::StorageResult<bool> {
        let cooldown_ms = match &self.settings.alert_sink {
            AlertSinkSettings::Noop => return Ok(false),
            AlertSinkSettings::Multi(sinks) if sinks.is_empty() => return Ok(false),
            AlertSinkSettings::Multi(sinks) => sinks
                .iter()
                .map(alert_sink_cooldown_ms)
                .max()
                .unwrap_or_default(),
            AlertSinkSettings::File { cooldown_ms, .. }
            | AlertSinkSettings::Webhook { cooldown_ms, .. } => *cooldown_ms,
        };
        let from_ms = now_ts_ms.saturating_sub(cooldown_ms as i64);
        let recent_logs = self
            .db
            .list_system_logs_filtered(storage::SystemLogFilter {
                run_id: Some(self.settings.run_id.clone()),
                level: None,
                target: Some("runtime.alert".to_string()),
                from_ms: Some(from_ms),
                to_ms: Some(now_ts_ms),
                search: None,
                limit: None,
                offset: None,
            })
            .await?;
        let dedup_key = alert_dedup_key(message, &self.settings.run_id, fields);
        Ok(!recent_logs.into_iter().any(|log| {
            log.message == message
                && log
                    .fields_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .as_ref()
                    .is_some_and(|parsed| {
                        alert_dedup_key(&log.message, log.run_id.as_deref().unwrap_or(""), parsed)
                            == dedup_key
                    })
        }))
    }

    async fn send_alert_notification(
        &self,
        message: &str,
        fields: &serde_json::Value,
        ts_ms: i64,
    ) -> Vec<AlertDeliveryResult> {
        match &self.settings.alert_sink {
            AlertSinkSettings::Multi(sinks) => {
                let mut deliveries = Vec::with_capacity(sinks.len());
                for sink in sinks {
                    deliveries.push(self.send_alert_to_sink(sink, message, fields, ts_ms).await);
                }
                deliveries
            }
            sink => vec![self.send_alert_to_sink(sink, message, fields, ts_ms).await],
        }
    }

    async fn send_alert_to_sink(
        &self,
        sink: &AlertSinkSettings,
        message: &str,
        fields: &serde_json::Value,
        ts_ms: i64,
    ) -> AlertDeliveryResult {
        match sink {
            AlertSinkSettings::Noop => AlertDeliveryResult {
                sink: "noop".to_string(),
                status: "skipped".to_string(),
                attempts: 0,
                http_status: None,
                error: None,
            },
            AlertSinkSettings::Multi(_) => AlertDeliveryResult {
                sink: "multi".to_string(),
                status: "skipped".to_string(),
                attempts: 0,
                http_status: None,
                error: Some("nested multi alert sinks are not supported".to_string()),
            },
            AlertSinkSettings::File { path, .. } => {
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await;
                let dedup_key = alert_dedup_key(message, &self.settings.run_id, fields);
                let payload = serde_json::json!({
                    "ts_ms": ts_ms,
                    "run_id": &self.settings.run_id,
                    "target": "runtime.alert",
                    "message": message,
                    "dedup_key": dedup_key,
                    "fields": fields,
                });
                match file {
                    Ok(mut file) => {
                        let result = async {
                            file.write_all(payload.to_string().as_bytes()).await?;
                            file.write_all(b"\n").await?;
                            file.flush().await
                        }
                        .await;
                        match result {
                            Ok(()) => AlertDeliveryResult {
                                sink: "file".to_string(),
                                status: "sent".to_string(),
                                attempts: 1,
                                http_status: None,
                                error: None,
                            },
                            Err(error) => AlertDeliveryResult {
                                sink: "file".to_string(),
                                status: "failed".to_string(),
                                attempts: 1,
                                http_status: None,
                                error: Some(error.to_string()),
                            },
                        }
                    }
                    Err(error) => AlertDeliveryResult {
                        sink: "file".to_string(),
                        status: "failed".to_string(),
                        attempts: 1,
                        http_status: None,
                        error: Some(error.to_string()),
                    },
                }
            }
            AlertSinkSettings::Webhook {
                url,
                timeout_ms,
                max_retries,
                auth_token,
                ..
            } => {
                let dedup_key = alert_dedup_key(message, &self.settings.run_id, fields);
                let payload = serde_json::json!({
                    "ts_ms": ts_ms,
                    "run_id": &self.settings.run_id,
                    "target": "runtime.alert",
                    "message": message,
                    "dedup_key": dedup_key,
                    "fields": fields,
                });
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_millis(*timeout_ms))
                    .build();
                let client = match client {
                    Ok(client) => client,
                    Err(error) => {
                        return AlertDeliveryResult {
                            sink: "webhook".to_string(),
                            status: "failed".to_string(),
                            attempts: 0,
                            http_status: None,
                            error: Some(error.to_string()),
                        };
                    }
                };
                let mut attempt = 0u32;
                loop {
                    let mut request = client.post(url).json(&payload);
                    if let Some(token) = auth_token.as_deref() {
                        request = request.bearer_auth(token);
                    }
                    match request.send().await {
                        Ok(response) => {
                            let status = response.status();
                            if status.is_success() {
                                return AlertDeliveryResult {
                                    sink: "webhook".to_string(),
                                    status: "sent".to_string(),
                                    attempts: attempt + 1,
                                    http_status: Some(status.as_u16()),
                                    error: None,
                                };
                            }
                            let should_retry = status.is_server_error() && attempt < *max_retries;
                            if !should_retry {
                                return AlertDeliveryResult {
                                    sink: "webhook".to_string(),
                                    status: "failed".to_string(),
                                    attempts: attempt + 1,
                                    http_status: Some(status.as_u16()),
                                    error: Some(format!("http status {}", status.as_u16())),
                                };
                            }
                        }
                        Err(error) => {
                            if attempt >= *max_retries {
                                return AlertDeliveryResult {
                                    sink: "webhook".to_string(),
                                    status: "failed".to_string(),
                                    attempts: attempt + 1,
                                    http_status: None,
                                    error: Some(error.to_string()),
                                };
                            }
                        }
                    }
                    attempt += 1;
                    sleep(Duration::from_millis(50 * i64::from(attempt) as u64)).await;
                }
            }
        }
    }

    async fn record_alert_delivery_log(
        &self,
        message: &str,
        fields: &serde_json::Value,
        delivery: &AlertDeliveryResult,
    ) -> storage::StorageResult<()> {
        let level = if delivery.status == "sent" {
            "INFO"
        } else {
            "WARN"
        };
        self.record_system_log(
            level,
            "runtime.alert_delivery",
            "alert.delivery",
            serde_json::json!({
                "run_id": &self.settings.run_id,
                "message": message,
                "sink": delivery.sink,
                "status": delivery.status,
                "attempts": delivery.attempts,
                "http_status": delivery.http_status,
                "error": delivery.error,
                "dedup_key": alert_dedup_key(message, &self.settings.run_id, fields),
            }),
        )
        .await
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

fn broker_order_matches_local(open_order: &BrokerOpenOrder, local: &StoredOrder) -> bool {
    open_order.client_order_id == local.client_order_id
        || local
            .broker_order_id
            .as_deref()
            .is_some_and(|broker_order_id| broker_order_id == open_order.broker_order_id)
}

fn broker_execution_matches_local(execution: &BrokerExecution, local: &StoredOrder) -> bool {
    execution
        .client_order_id
        .as_deref()
        .is_some_and(|client_order_id| client_order_id == local.client_order_id)
        || local
            .broker_order_id
            .as_deref()
            .is_some_and(|broker_order_id| broker_order_id == execution.broker_order_id)
}

fn recovered_order_status(
    local: &StoredOrder,
    open_order: Option<&BrokerOpenOrder>,
    filled_qty: Decimal,
) -> anyhow::Result<String> {
    if let Some(open_order) = open_order
        && filled_qty == Decimal::ZERO
    {
        return Ok(open_order.status.clone());
    }
    let order_qty = local.qty.parse::<Decimal>()?;
    if filled_qty >= order_qty {
        Ok("FILLED".to_string())
    } else if filled_qty > Decimal::ZERO {
        Ok("PARTIALLY_FILLED".to_string())
    } else if let Some(open_order) = open_order {
        Ok(open_order.status.clone())
    } else {
        Ok(local.status.clone())
    }
}

fn order_side_slug(side: trader_core::OrderSide) -> &'static str {
    match side {
        trader_core::OrderSide::Buy => "BUY",
        trader_core::OrderSide::Sell => "SELL",
    }
}

struct LiveLogScope {
    _guard: tracing::subscriber::DefaultGuard,
    writer: LogWriter<DbSystemLogSink>,
}

impl LiveLogScope {
    fn new(db: Db, run_id: String, settings: LogWriterSettings) -> Option<Self> {
        if !settings.enabled {
            return None;
        }
        let writer = LogWriter::new_with_metrics(
            DbSystemLogSink::new(db),
            settings.buffer_size,
            settings.batch_size,
            settings.flush_interval_ms,
            settings.metrics.clone(),
        );
        let subscriber = tracing_subscriber::registry().with(
            SystemLogLayer::new(writer.sender(), Some(run_id))
                .with_settings(settings)
                .with_metrics(writer.metrics()),
        );
        let guard = tracing::subscriber::set_default(subscriber);
        Some(Self {
            _guard: guard,
            writer,
        })
    }

    async fn shutdown(self) {
        self.writer.shutdown().await;
    }
}

fn alert_dedup_key(message: &str, run_id: &str, fields: &serde_json::Value) -> String {
    let account_id = fields
        .get("account_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let symbol = fields
        .get("symbol")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let reason = fields
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    format!("{message}|{run_id}|{account_id}|{symbol}|{reason}")
}

fn alert_sink_cooldown_ms(sink: &AlertSinkSettings) -> u64 {
    match sink {
        AlertSinkSettings::Noop => 0,
        AlertSinkSettings::Multi(sinks) => {
            sinks.iter().map(alert_sink_cooldown_ms).max().unwrap_or(0)
        }
        AlertSinkSettings::File { cooldown_ms, .. }
        | AlertSinkSettings::Webhook { cooldown_ms, .. } => *cooldown_ms,
    }
}

struct AlertDeliveryResult {
    sink: String,
    status: String,
    attempts: u32,
    http_status: Option<u16>,
    error: Option<String>,
}

#[derive(Debug, Default)]
struct StartupRecoverySummary {
    scanned: usize,
    recovered: usize,
    remaining: usize,
    executions: usize,
    unmatched_open_orders: Vec<String>,
    unmatched_executions: Vec<String>,
}
