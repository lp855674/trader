# IBKR Paper Market Data Gate Results - 2026-07-17

## Scope

This record captures read-only market data checks against the local IB Gateway
paper endpoint at `127.0.0.1:4002`. No order command, `paper-run`, or soak was
executed.

## Real-Time Probe

Command:

```powershell
cargo run -p trader-cli -- ibkr-paper-market-data `
  --config configs/paper/ibkr_aapl_1d_parquet.toml `
  --symbol BSET --symbol MSFT --symbol AAPL
```

Results:

- BSET: IBKR error `10089`; API access requires the
  `NASDAQ.NMS/TOP/ALL` market data subscription.
- MSFT: IBKR error `10168`; market data is not subscribed and delayed market
  data is not enabled.
- AAPL: IBKR error `10089`; API access requires the
  `NASDAQ.NMS/TOP/ALL` market data subscription.

The command failed all three real-time snapshots and returned non-zero.

## Delayed Diagnostic

The same symbols were requested with `--delayed`. BSET returned no usable ticks
before the bounded snapshot deadline, MSFT returned IBKR error `10197` for a
competing real-account trading session, and AAPL returned an empty snapshot with
market data type `unknown`.

Delayed data is diagnostic only. It does not satisfy the real-time
order-submission gate.

## Decision

IBKR paper order submission remains blocked. Do not run TinyOrder, AutoRun, or
soak until every configured strategy symbol returns a fresh real-time bid and
ask and the required API market data subscriptions and account-session
conditions have been corrected.

## Code Follow-Up - 2026-07-18

The real-time market data gate was extended to both CLI and API `paper-run`
startup paths. With IBKR order submission enabled, the runtime now validates the
paper account and requests a fresh real-time snapshot for every configured
strategy symbol before constructing the order executor. The CLI and API
preflight paths use the same snapshot requirements.

Snapshot validation is shared with the per-order guard and requires real-time
data, positive bid and ask, a non-crossed quote, and a timestamp no more than
five seconds old. Regression coverage includes delayed data, missing bid,
crossed quotes, stale timestamps, and future timestamps.

The market-data request deadline now starts before Gateway connection and is
shared by connection, market-data-type switching, subscription, and tick
collection. API startup constructs the broker runtime before recording
`paper.started`, so a failed external gate cannot leave a false `running`
strategy record.

Known market-data notices `10089`, `10168`, and `10197` now retain the original
IBKR message and add an `action:` suffix describing the required subscription,
the diagnostic-only delayed-data limitation, or the competing-session cleanup.
Unknown notices remain unchanged.

No live gateway probe, order command, `paper-run`, or soak was executed during
this follow-up. TinyOrder, AutoRun, and soak remain blocked by the external IBKR
subscription and competing-session conditions recorded above.
