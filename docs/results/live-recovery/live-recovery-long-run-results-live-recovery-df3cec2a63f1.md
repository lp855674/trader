# Live Recovery Long Run Results: live-recovery-df3cec2a63f1

## Scope

- Local fake/injected broker iterations: 20
- Local runtime test invocations: 320
- Binance read-only recovery: skipped
- Binance network recovery: skipped
- IBKR read-only recovery: skipped

## Command

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check\verify-live-recovery.ps1 -Iterations 20 -DelaySeconds 1
```

## Evidence

- Summary: `data/verification/live-recovery/live-recovery-df3cec2a63f1/summary.json`
- `iterations_requested`: 20
- `iterations_completed`: 20
- `status`: `completed`
- Non-zero test exit codes: 0

## Result

- Overall status: completed
- Startup recovery: pass
- Unmatched open order fail/warn-only: pass
- Recovered execution de-dup: pass
- Broker snapshot drift: pass
- Alert retry/cooldown: pass

## Failures

None observed.

## Adapter Coverage

- Binance read-only recovery: skipped by default to avoid touching network credentials or live broker state.
- Binance network recovery: skipped by default.
- IBKR read-only recovery: skipped because no paper account and running Gateway were provided for this pass.

## Decision

Live recovery is stable enough to start a focused Live process isolation design plan. Adapter read-only recovery remains explicitly deferred until the operator opts in with testnet credentials or an IBKR paper account.
