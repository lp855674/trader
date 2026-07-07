#![forbid(unsafe_code)]

use anyhow::{Result, bail};

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
        bail!("reconciliation gate has no required accounts");
    }

    let mut audits = Vec::new();
    for requirement in &requirements {
        let rows = db
            .list_latest_reconciliation_audits_for_gate(
                &requirement.broker,
                &requirement.account_id,
                requirement.min_successful_audits as i64,
            )
            .await?;
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
            now_ms: chrono::Utc::now().timestamp_millis(),
            requirements,
            audits,
        },
    ))
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
