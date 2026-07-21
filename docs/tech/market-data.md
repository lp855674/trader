# Market Data

## Responsibility

`crates/market_data` owns runtime market-data provider interfaces and vendor adapters.
It converts provider-specific payloads into the normalized models from `crates/data`.

The crate does not submit, cancel, query, or reconcile orders.

## Dependency Direction

```text
provider SDK or broker market-data API
                  |
                  v
          crates/market_data
                  |
                  v
             crates/data
```

`crates/data` must not depend on `crates/market_data`.

## Current Providers

- `IbkrMarketDataProvider` preserves the existing IBKR snapshot behavior while keeping it
  separate from the IBKR order client.
- `LongbridgeMarketDataProvider` uses the official Longbridge Rust SDK. It combines
  `QuoteContext::quote` for last price and exchange timestamp with `QuoteContext::depth`
  for the best positive bid and ask at the lowest depth position.

The IBKR paper executor receives a `MarketDataProvider` independently from its
`IbkrPaperOrderClient`. Selecting Longbridge changes quote acquisition and the pre-order
market-data gate; account validation, order submission, cancellation, and reconciliation
remain on IBKR.

## Configuration

Omitting `[market_data]` keeps the backwards-compatible IBKR provider. To use Longbridge:

```toml
[market_data]
provider = "longbridge"
longbridge_app_key_env = "LONGBRIDGE_APP_KEY"
longbridge_app_secret_env = "LONGBRIDGE_APP_SECRET"
longbridge_access_token_env = "LONGBRIDGE_ACCESS_TOKEN"
```

The TOML stores environment-variable names only. Set the corresponding app key, app
secret, and access token in the process environment. Do not put credential values in the
configuration file.

See `configs/paper/ibkr_aapl_1d_longbridge.toml` for a disabled-by-default example.

Use the read-only probe without enabling order submission:

```powershell
$env:LONGBRIDGE_APP_KEY = "..."
$env:LONGBRIDGE_APP_SECRET = "..."
$env:LONGBRIDGE_ACCESS_TOKEN = "..."
cargo run -p trader-cli -- market-data-probe `
  --config configs/paper/ibkr_aapl_1d_longbridge.toml `
  --symbol AAPL --symbol MSFT --symbol BSET
```

The command prints one normalized JSON quote per symbol and fails unless every quote is
realtime, fresh, uncrossed, and has positive bid and ask values. It does not initialize
an IBKR order client or enter the paper runtime when Longbridge is selected.

## Symbol Mapping

The Longbridge adapter currently supports US equities:

- `US:NASDAQ:AAPL:EQUITY` becomes `AAPL.US`
- `AAPL` becomes `AAPL.US`
- `AAPL.US` remains `AAPL.US`

Unsupported market-qualified symbols fail closed instead of being sent to the wrong
market.

## Quote Contract

Providers return `data::Quote`, including:

- normalized symbol
- bid, ask, and optional last price
- optional exchange timestamp
- local receive timestamp
- source identity
- realtime, frozen, delayed, or unknown data kind

Before the IBKR paper runtime can submit orders, the configured provider must return a
realtime quote for every configured strategy symbol. The shared gate rejects missing or
non-positive bid/ask values, crossed markets, wrong data kinds, and stale timestamps.
Order execution repeats the same validation immediately before calculating its
marketable limit price.
