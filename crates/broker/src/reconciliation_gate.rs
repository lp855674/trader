#![forbid(unsafe_code)]

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateRequirement {
    pub broker: String,
    pub account_id: String,
    pub min_successful_audits: usize,
    pub max_audit_age_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateAudit {
    pub broker: String,
    pub account_id: String,
    pub ts_ms: i64,
    pub cash_drifts: usize,
    pub position_drifts: usize,
    pub open_order_drifts: usize,
    pub execution_drifts: usize,
    pub stale_inputs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationGateStatus {
    Allow,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateFailure {
    pub broker: String,
    pub account_id: String,
    pub reason: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateInput {
    pub now_ms: i64,
    pub requirements: Vec<ReconciliationGateRequirement>,
    pub audits: Vec<ReconciliationGateAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconciliationGateDecision {
    pub status: ReconciliationGateStatus,
    pub failures: Vec<ReconciliationGateFailure>,
}

pub fn evaluate_reconciliation_gate(input: ReconciliationGateInput) -> ReconciliationGateDecision {
    let mut failures = Vec::new();

    for requirement in &input.requirements {
        let matching: Vec<&ReconciliationGateAudit> = input
            .audits
            .iter()
            .filter(|audit| {
                audit.broker == requirement.broker && audit.account_id == requirement.account_id
            })
            .collect();

        if matching.is_empty() {
            failures.push(failure(
                requirement,
                "missing_required_audit",
                "no matching audit",
            ));
            continue;
        }

        let clean_recent = matching
            .iter()
            .filter(|audit| input.now_ms - audit.ts_ms <= requirement.max_audit_age_ms)
            .filter(|audit| {
                audit.cash_drifts == 0
                    && audit.position_drifts == 0
                    && audit.open_order_drifts == 0
                    && audit.execution_drifts == 0
                    && audit.stale_inputs == 0
            })
            .count();

        if clean_recent < requirement.min_successful_audits {
            failures.push(failure(
                requirement,
                "insufficient_clean_recent_audits",
                &format!(
                    "required={} observed={clean_recent}",
                    requirement.min_successful_audits
                ),
            ));
        }

        for audit in matching {
            if input.now_ms - audit.ts_ms > requirement.max_audit_age_ms {
                failures.push(failure(
                    requirement,
                    "audit_too_old",
                    &audit.ts_ms.to_string(),
                ));
            }
            if audit.cash_drifts
                + audit.position_drifts
                + audit.open_order_drifts
                + audit.execution_drifts
                > 0
            {
                failures.push(failure(
                    requirement,
                    "audit_has_drift",
                    &audit.ts_ms.to_string(),
                ));
            }
            if audit.stale_inputs > 0 {
                failures.push(failure(
                    requirement,
                    "audit_has_stale_inputs",
                    &audit.ts_ms.to_string(),
                ));
            }
        }
    }

    ReconciliationGateDecision {
        status: if failures.is_empty() {
            ReconciliationGateStatus::Allow
        } else {
            ReconciliationGateStatus::Block
        },
        failures,
    }
}

fn failure(
    requirement: &ReconciliationGateRequirement,
    reason: &str,
    detail: &str,
) -> ReconciliationGateFailure {
    ReconciliationGateFailure {
        broker: requirement.broker.clone(),
        account_id: requirement.account_id.clone(),
        reason: reason.to_string(),
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(broker: &str, account_id: &str) -> ReconciliationGateRequirement {
        ReconciliationGateRequirement {
            broker: broker.to_string(),
            account_id: account_id.to_string(),
            min_successful_audits: 2,
            max_audit_age_ms: 60_000,
        }
    }

    fn audit(broker: &str, account_id: &str, ts_ms: i64) -> ReconciliationGateAudit {
        ReconciliationGateAudit {
            broker: broker.to_string(),
            account_id: account_id.to_string(),
            ts_ms,
            cash_drifts: 0,
            position_drifts: 0,
            open_order_drifts: 0,
            execution_drifts: 0,
            stale_inputs: 0,
        }
    }

    #[test]
    fn gate_allows_when_each_requirement_has_recent_clean_audits() {
        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![
                requirement("ibkr", "DU****91"),
                requirement("binance", "paper"),
            ],
            audits: vec![
                audit("ibkr", "DU****91", 90_000),
                audit("ibkr", "DU****91", 95_000),
                audit("binance", "paper", 91_000),
                audit("binance", "paper", 96_000),
            ],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Allow);
        assert!(decision.failures.is_empty());
    }

    #[test]
    fn gate_blocks_missing_required_broker_account() {
        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![
                requirement("ibkr", "DU****91"),
                requirement("binance", "paper"),
            ],
            audits: vec![
                audit("ibkr", "DU****91", 95_000),
                audit("ibkr", "DU****91", 96_000),
            ],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Block);
        assert_eq!(decision.failures[0].reason, "missing_required_audit");
        assert_eq!(decision.failures[0].broker, "binance");
    }

    #[test]
    fn gate_blocks_old_audits() {
        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![requirement("ibkr", "DU****91")],
            audits: vec![
                audit("ibkr", "DU****91", 10_000),
                audit("ibkr", "DU****91", 20_000),
            ],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Block);
        assert!(
            decision
                .failures
                .iter()
                .any(|failure| failure.reason == "audit_too_old")
        );
    }

    #[test]
    fn gate_blocks_drift_and_stale_inputs() {
        let mut bad = audit("ibkr", "DU****91", 95_000);
        bad.open_order_drifts = 1;
        bad.stale_inputs = 1;

        let decision = evaluate_reconciliation_gate(ReconciliationGateInput {
            now_ms: 100_000,
            requirements: vec![requirement("ibkr", "DU****91")],
            audits: vec![bad, audit("ibkr", "DU****91", 96_000)],
        });

        assert_eq!(decision.status, ReconciliationGateStatus::Block);
        assert!(
            decision
                .failures
                .iter()
                .any(|failure| failure.reason == "audit_has_drift")
        );
        assert!(
            decision
                .failures
                .iter()
                .any(|failure| failure.reason == "audit_has_stale_inputs")
        );
    }
}
