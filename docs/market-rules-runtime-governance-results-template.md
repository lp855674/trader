# Market Rules Runtime Governance Results Template

## Scope

- Date:
- Operator:
- Git commit:
- Run id:
- Database:

This evidence is credential-free local validation only. It covers deterministic SQLite setup, paper runtime enforcement, effective-state readback, and local audit readback. It does not claim live-money readiness, real-broker market-rule validation, production RBAC, SSO/IdP identity, or hosted approvals.

## Commands

```powershell
cargo fmt
cargo test -p storage market_rule
cargo test -p paper market_rules
cargo test -p paper trading_session
cargo test -p api market_rules_effective_route_returns_runtime_state_and_audits
cargo test -p trader-cli market_rules_commands_print_effective_state_and_audit_events
powershell -ExecutionPolicy Bypass -File .\scripts\ops-smoke.ps1
```

## Expected Evidence

- Storage market-rule change audits include lot-size, price-limit, and fee rule records.
- API effective readback returns lot-size, price-limit, fee tiers, calendar, trading sessions, and matching audit events.
- CLI readback prints `market_rule_effective`, rule records, trading sessions, and `market_rule_audit` lines.
- Paper focused gates prove configured market rules and trading sessions are enforced locally.
- `ops-smoke.ps1` summary includes `market_rules_governance_smoke = passed`.

## Results

- `cargo fmt`:
- Storage focused gate:
- Paper market-rules gate:
- Paper trading-session gate:
- API effective readback gate:
- CLI readback gate:
- Ops smoke:

## Remaining Gaps

- No live-money orders were submitted.
- No real-broker market-rule validation was performed.
- No production RBAC/SSO/IdP or hosted approval system was validated.
