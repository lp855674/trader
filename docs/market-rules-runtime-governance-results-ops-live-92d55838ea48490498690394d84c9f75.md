# Market Rules Runtime Governance Results

## Scope

- Date: 2026-07-11
- Operator: local Codex run
- Git commit: pending
- Run id: `ops-live-92d55838ea48490498690394d84c9f75`
- Database: temporary local SQLite (`sqlite://C:/Users/Hi/AppData/Local/Temp/trader-ops-92d55838ea48490498690394d84c9f75.sqlite`)

This evidence is credential-free local validation only. It covers deterministic SQLite setup, paper runtime enforcement, effective-state readback, and local audit readback. It does not claim live-money readiness, real-broker market-rule validation, production RBAC, SSO/IdP identity, or hosted approvals.

## Commands And Results

```powershell
cargo fmt
cargo test -p storage market_rule_reference_writes_change_audit_events --test runtime_repository_tests
cargo test -p api market_rules_effective_route_returns_runtime_state_and_audits --test api_tests
cargo test -p trader-cli market_rules_commands_print_effective_state_and_audit_events --test cli_tests
cargo test -p paper market_rules
cargo test -p paper trading_session
powershell -ExecutionPolicy Bypass -File .\scripts\ops-smoke.ps1
```

All commands passed.

## Ops Smoke Summary

- `api_cash_snapshots`: 7
- `api_position_snapshots`: 9
- `api_reconciliation`: `drift`
- `api_system_logs`: 37
- `api_reconciliation_alerts`: 2
- `api_reconciliation_alert_deliveries`: 2
- `config_version`: `fnv1a64:369b44710ccb0742`
- `broker_agnostic_snapshot_smoke`: `passed`
- `market_rules_governance_smoke`: `passed`
- `production_required_approvals`: 2
- `production_publish_blocked_before_quorum`: `True`
- `production_approval_state`: `published`

## Remaining Gaps

- No live-money orders were submitted.
- No real-broker market-rule validation was performed.
- No production RBAC/SSO/IdP or hosted approval system was validated.
