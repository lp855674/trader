# Config Governance RBAC Local Verification Template

Use this template for local, credential-free config governance evidence. This does not prove production RBAC, SSO, IdP integration, or live-money deployment authorization.

## Run Metadata

- Date:
- Operator:
- Git commit:
- Database URL:
- Config path:
- Smoke command:

## Policy Readback

- API endpoint: `GET /api/v1/config-governance/policy`
- CLI command: `trader configs governance-policy --config <config>`
- Expected production publish rule:
  - `target_env=production`
  - `transition_to=published`
  - `required_role=release_manager`
  - `required_approvals=2`
  - `requires_independent_actor=true`

## Queue Evidence

- Staging config name:
- Production config name:
- API pending queue endpoint:
- CLI pending queue command:
- Required fields observed:
  - `required_role`
  - `required_approvals`
  - `approval_count`
  - `remaining_approvals`

## Quorum Evidence

- First production approver:
- Publish attempt after one approval:
- Expected block: `production config publish requires 2 approvals`
- Second production approver:
- Publish after quorum:
- Release readback:
- Audit readback:

## Remaining Production Limits

- External authentication / SSO / IdP:
- Real user directory and group sync:
- Fine-grained production RBAC assignment:
- Hosted approval workflow:
- Live-money deployment authorization:
