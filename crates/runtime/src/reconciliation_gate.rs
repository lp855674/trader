#![forbid(unsafe_code)]

use anyhow::{Result, bail};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateAuditLogContext {
    pub run_id: Option<String>,
    pub source: String,
    pub config_path: Option<String>,
    pub config_format: Option<String>,
    pub config_checksum: Option<String>,
    pub config_id: Option<String>,
    pub config_version: Option<String>,
}

pub fn parse_reconciliation_gate_account_requirement(
    value: &str,
) -> Result<broker::ReconciliationGateRequirement> {
    let Some((broker, account_id)) = value.split_once(':') else {
        bail!("expected broker:account_id");
    };
    if broker.trim().is_empty() || account_id.trim().is_empty() {
        bail!("expected broker:account_id");
    }
    Ok(broker::ReconciliationGateRequirement {
        broker: broker.trim().to_string(),
        account_id: account_id.trim().to_string(),
        min_successful_audits: 1,
        max_audit_age_ms: 300_000,
    })
}

pub fn should_enforce_live_reconciliation_gate(app_config: &config::AppConfig) -> bool {
    app_config.live.reconciliation_gate.enabled
        || app_config.broker.mode == config::BrokerMode::Live
}

pub fn should_enforce_reconciliation_gate_block(
    app_config: &config::AppConfig,
    decision: &broker::ReconciliationGateDecision,
) -> bool {
    if decision.status != broker::ReconciliationGateStatus::Block {
        return false;
    }
    if app_config.broker.mode == config::BrokerMode::Live {
        return true;
    }
    decision
        .failures
        .iter()
        .any(|failure| reconciliation_gate_failure_policy(app_config, &failure.reason).is_block())
}

pub fn should_fail_on_reconciliation_gate_log_write_failure(
    app_config: &config::AppConfig,
) -> bool {
    app_config.broker.mode == config::BrokerMode::Live
        || app_config
            .live
            .reconciliation_gate
            .log_write_failure
            .is_block()
}

pub async fn evaluate_live_reconciliation_gate_from_storage(
    app_config: &config::AppConfig,
    db: &storage::Db,
) -> Result<Option<broker::ReconciliationGateDecision>> {
    if !should_enforce_live_reconciliation_gate(app_config) {
        return Ok(None);
    }

    evaluate_reconciliation_gate_from_storage(app_config, db, &[], None, None)
        .await
        .map(Some)
}

pub async fn evaluate_reconciliation_gate_from_storage(
    app_config: &config::AppConfig,
    db: &storage::Db,
    accounts: &[String],
    min_successful_audits: Option<usize>,
    max_audit_age_ms: Option<i64>,
) -> Result<broker::ReconciliationGateDecision> {
    let mut requirements = if accounts.is_empty() {
        app_config
            .live
            .reconciliation_gate
            .required_accounts
            .iter()
            .map(|value| parse_reconciliation_gate_account_requirement(value))
            .collect::<Result<Vec<_>>>()?
    } else {
        accounts
            .iter()
            .map(|value| parse_reconciliation_gate_account_requirement(value))
            .collect::<Result<Vec<_>>>()?
    };

    for requirement in &mut requirements {
        requirement.min_successful_audits = min_successful_audits
            .unwrap_or(app_config.live.reconciliation_gate.min_successful_audits);
        requirement.max_audit_age_ms =
            max_audit_age_ms.unwrap_or(app_config.live.reconciliation_gate.max_audit_age_ms);
    }

    if requirements.is_empty() {
        return Ok(broker::ReconciliationGateDecision {
            status: broker::ReconciliationGateStatus::Block,
            requirements,
            failures: vec![broker::ReconciliationGateFailure {
                broker: String::new(),
                account_id: String::new(),
                reason: "missing_required_accounts".to_string(),
                detail: "reconciliation gate has no required accounts".to_string(),
            }],
        });
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut audits = Vec::new();
    for requirement in &requirements {
        let from_ts_ms = now_ms - requirement.max_audit_age_ms;
        let mut rows = db
            .list_reconciliation_audits_for_gate_since(
                &requirement.broker,
                &requirement.account_id,
                from_ts_ms,
            )
            .await?;
        if rows.is_empty() {
            rows = db
                .list_latest_reconciliation_audits_for_gate(
                    &requirement.broker,
                    &requirement.account_id,
                    1,
                )
                .await?;
        }
        audits.extend(rows.into_iter().map(|row| broker::ReconciliationGateAudit {
            broker: row.broker_kind,
            account_id: row.account_id,
            ts_ms: row.ts_ms,
            cash_drifts: row.cash_drift_count as usize,
            position_drifts: row.position_drift_count as usize,
            open_order_drifts: row.open_order_drift_count as usize,
            execution_drifts: row.execution_drift_count as usize,
            stale_inputs: row.stale_input_count as usize,
        }));
    }

    Ok(broker::evaluate_reconciliation_gate(
        broker::ReconciliationGateInput {
            now_ms,
            requirements,
            audits,
        },
    ))
}

pub async fn record_reconciliation_gate_decision(
    db: &storage::Db,
    app_config: &config::AppConfig,
    decision: &broker::ReconciliationGateDecision,
    context: ReconciliationGateAuditLogContext,
    alert_sink: &crate::AlertSinkSettings,
) -> Result<()> {
    let status = reconciliation_gate_status_label(decision.status);
    let level = match decision.status {
        broker::ReconciliationGateStatus::Allow => "INFO",
        broker::ReconciliationGateStatus::Block => "WARN",
    };
    let requirements = decision
        .requirements
        .iter()
        .map(|requirement| {
            json!({
                "broker": requirement.broker,
                "account_id": requirement.account_id,
                "min_successful_audits": requirement.min_successful_audits,
                "max_audit_age_ms": requirement.max_audit_age_ms,
            })
        })
        .collect::<Vec<_>>();
    let failures = decision
        .failures
        .iter()
        .map(|failure| {
            json!({
                "broker": failure.broker,
                "account_id": failure.account_id,
                "reason": failure.reason,
                "detail": failure.detail,
            })
        })
        .collect::<Vec<_>>();

    let run_id = context.run_id.clone();
    let ts_ms = chrono::Utc::now().timestamp_millis();
    let fields = json!({
        "event_type": "live.reconciliation_gate.decision",
        "status": status,
        "enforcement_action": reconciliation_gate_enforcement_action(app_config, decision),
        "source": context.source,
        "run_id": context.run_id,
        "broker_kind": app_config.broker.kind,
        "broker_mode": app_config.broker.mode,
        "gate_enabled": app_config.live.reconciliation_gate.enabled,
        "required_account_count": requirements.len(),
        "requirements": requirements,
        "failure_count": failures.len(),
        "failures": failures,
        "config_snapshot": {
            "path": context.config_path,
            "format": context.config_format,
            "checksum": context.config_checksum,
            "config_id": context.config_id,
            "version": context.config_version,
        },
        "policy": {
            "missing_required_accounts": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.missing_required_accounts),
            "missing_required_audit": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.missing_required_audit),
            "insufficient_clean_recent_audits": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.insufficient_clean_recent_audits),
            "audit_too_old": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.audit_too_old),
            "audit_has_drift": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.audit_has_drift),
            "audit_has_stale_inputs": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.audit_has_stale_inputs),
            "log_write_failure": reconciliation_gate_policy_label(app_config.live.reconciliation_gate.log_write_failure),
            "live_mode_forces_block": app_config.broker.mode == config::BrokerMode::Live,
        },
    });
    db.record_system_log(storage::SystemLogCommand {
        run_id: run_id.clone(),
        ts_ms,
        level: level.to_string(),
        target: "runtime.reconciliation_gate".to_string(),
        message: format!("reconciliation_gate.{status}"),
        fields: Some(fields.clone()),
    })
    .await?;
    if decision.status == broker::ReconciliationGateStatus::Block {
        let mut alert_fields = fields;
        if let Some(object) = alert_fields.as_object_mut() {
            object.insert(
                "event_type".to_string(),
                json!("live.reconciliation_gate.block_alert"),
            );
            object.insert("reason".to_string(), json!("reconciliation_gate_block"));
        }
        crate::record_runtime_alert(
            db,
            run_id.as_deref(),
            alert_sink,
            "reconciliation_gate.block.alert",
            alert_fields,
        )
        .await?;
    }
    Ok(())
}

fn reconciliation_gate_status_label(status: broker::ReconciliationGateStatus) -> &'static str {
    match status {
        broker::ReconciliationGateStatus::Allow => "allow",
        broker::ReconciliationGateStatus::Block => "block",
    }
}

fn reconciliation_gate_enforcement_action(
    app_config: &config::AppConfig,
    decision: &broker::ReconciliationGateDecision,
) -> &'static str {
    match decision.status {
        broker::ReconciliationGateStatus::Allow => "allow",
        broker::ReconciliationGateStatus::Block => {
            if should_enforce_reconciliation_gate_block(app_config, decision) {
                "block"
            } else {
                "warn_only"
            }
        }
    }
}

fn reconciliation_gate_failure_policy(
    app_config: &config::AppConfig,
    reason: &str,
) -> config::LiveReconciliationGateFailurePolicy {
    let gate = &app_config.live.reconciliation_gate;
    match reason {
        "missing_required_accounts" => gate.missing_required_accounts,
        "missing_required_audit" => gate.missing_required_audit,
        "insufficient_clean_recent_audits" => gate.insufficient_clean_recent_audits,
        "audit_too_old" => gate.audit_too_old,
        "audit_has_drift" => gate.audit_has_drift,
        "audit_has_stale_inputs" => gate.audit_has_stale_inputs,
        _ => config::LiveReconciliationGateFailurePolicy::Block,
    }
}

fn reconciliation_gate_policy_label(
    policy: config::LiveReconciliationGateFailurePolicy,
) -> &'static str {
    match policy {
        config::LiveReconciliationGateFailurePolicy::Block => "block",
        config::LiveReconciliationGateFailurePolicy::WarnOnly => "warn_only",
    }
}

trait ReconciliationGateFailurePolicyExt {
    fn is_block(self) -> bool;
}

impl ReconciliationGateFailurePolicyExt for config::LiveReconciliationGateFailurePolicy {
    fn is_block(self) -> bool {
        self == config::LiveReconciliationGateFailurePolicy::Block
    }
}

pub fn format_reconciliation_gate_failure(failure: &broker::ReconciliationGateFailure) -> String {
    format!(
        "reconciliation gate blocked: broker={} account={} reason={} detail={}",
        failure.broker, failure.account_id, failure.reason, failure.detail
    )
}

pub fn format_reconciliation_gate_failures(
    failures: &[broker::ReconciliationGateFailure],
) -> String {
    failures
        .iter()
        .map(format_reconciliation_gate_failure)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use storage::{NewStrategyRun, ReconciliationAuditCommand};

    fn test_config() -> config::AppConfig {
        config::AppConfig::from_toml_str(
            r#"
            [runtime]
            mode = "paper"
            run_id = "reconciliation-gate-test"

            [database]
            url = "sqlite::memory:"

            [data]
            source = "csv"
            path = "datasets/sample/aapl_1d.csv"

            [strategy]
            name = "moving_average_cross"
            symbols = ["US:NASDAQ:AAPL:EQUITY"]
            fast_window = 20
            slow_window = 60

            [portfolio]
            initial_cash = "100000"
            base_currency = "USD"
            order_qty = "1"
            max_abs_qty = "100"

            [risk]
            max_order_notional = "1000000"
            min_cash_after_order = "0"
            max_exposure = "1000000"
            max_drawdown = "1"
            max_leverage = "10"
            max_margin_used = "0"
            trading_halted = false

            [broker]
            kind = "simulated"
            mode = "paper"

            [paper]
            account_id = "paper"
            slippage_bps = "0"
            fee_bps = "0"

            [live]
            enabled = true

            [live.reconciliation_gate]
            enabled = true
            required_accounts = ["simulated:paper"]
            "#,
        )
        .unwrap()
    }

    async fn test_db() -> storage::Db {
        let db = storage::Db::connect("sqlite::memory:").await.unwrap();
        db.migrate().await.unwrap();
        db.insert_strategy_run(NewStrategyRun {
            id: "reconciliation-gate-test".to_string(),
            name: "moving_average_cross".to_string(),
            mode: "paper".to_string(),
            status: "running".to_string(),
            started_at_ms: 1,
            ended_at_ms: None,
            error: None,
            config_json: "{}".to_string(),
        })
        .await
        .unwrap();
        db
    }

    async fn record_audit(
        db: &storage::Db,
        id: &str,
        ts_ms: i64,
        open_order_drift_count: i64,
        stale_input_count: i64,
    ) {
        db.record_reconciliation_audit(ReconciliationAuditCommand {
            id: id.to_string(),
            run_id: "reconciliation-gate-test".to_string(),
            account_id: "paper".to_string(),
            broker_kind: "simulated".to_string(),
            ts_ms,
            severity: "info".to_string(),
            cash_drift_count: 0,
            position_drift_count: 0,
            open_order_drift_count,
            execution_drift_count: 0,
            stale_input_count,
            payload_json: "{}".to_string(),
        })
        .await
        .unwrap();
    }

    fn has_failure(decision: &broker::ReconciliationGateDecision, reason: &str) -> bool {
        decision
            .failures
            .iter()
            .any(|failure| failure.reason == reason)
    }

    #[tokio::test]
    async fn storage_gate_blocks_recent_drift_before_latest_clean_audit() {
        let db = test_db().await;
        let config = test_config();
        let now_ms = chrono::Utc::now().timestamp_millis();
        record_audit(&db, "recent-drift", now_ms - 1_000, 1, 0).await;
        record_audit(&db, "recent-clean", now_ms - 10, 0, 0).await;

        let decision =
            evaluate_reconciliation_gate_from_storage(&config, &db, &[], Some(1), Some(60_000))
                .await
                .unwrap();

        assert_eq!(decision.status, broker::ReconciliationGateStatus::Block);
        assert!(has_failure(&decision, "audit_has_drift"));
    }

    #[tokio::test]
    async fn storage_gate_ignores_old_drift_when_recent_clean_audit_exists() {
        let db = test_db().await;
        let config = test_config();
        let now_ms = chrono::Utc::now().timestamp_millis();
        record_audit(&db, "old-drift", now_ms - 5_000, 1, 1).await;
        record_audit(&db, "recent-clean", now_ms - 10, 0, 0).await;

        let decision =
            evaluate_reconciliation_gate_from_storage(&config, &db, &[], Some(1), Some(1_000))
                .await
                .unwrap();

        assert_eq!(decision.status, broker::ReconciliationGateStatus::Allow);
        assert!(decision.failures.is_empty());
    }

    #[tokio::test]
    async fn storage_gate_reports_too_old_when_only_old_audit_exists() {
        let db = test_db().await;
        let config = test_config();
        let now_ms = chrono::Utc::now().timestamp_millis();
        record_audit(&db, "old-clean", now_ms - 5_000, 0, 0).await;

        let decision =
            evaluate_reconciliation_gate_from_storage(&config, &db, &[], Some(1), Some(1_000))
                .await
                .unwrap();

        assert_eq!(decision.status, broker::ReconciliationGateStatus::Block);
        assert!(has_failure(&decision, "audit_too_old"));
        assert!(!has_failure(&decision, "missing_required_audit"));
    }
}
