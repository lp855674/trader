#![forbid(unsafe_code)]

use async_trait::async_trait;
use broker::{BrokerError, IbkrPaperGatewayAdapter};
use data::{MarketDataKind, MarketDataSource, Quote};
use longbridge::{Config, quote::QuoteContext};
use rust_decimal::Decimal;
use std::{
    env,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarketDataError {
    #[error("market data provider error: {0}")]
    Provider(String),
    #[error("market data credential environment variable {0} is not set")]
    MissingCredential(String),
    #[error("unsupported market data symbol {0}")]
    UnsupportedSymbol(String),
}

impl From<BrokerError> for MarketDataError {
    fn from(error: BrokerError) -> Self {
        Self::Provider(error.to_string())
    }
}

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    async fn snapshot(&self, symbol: &str) -> Result<Quote, MarketDataError>;
}

#[async_trait]
impl<Provider> MarketDataProvider for Box<Provider>
where
    Provider: MarketDataProvider + ?Sized,
{
    async fn snapshot(&self, symbol: &str) -> Result<Quote, MarketDataError> {
        self.as_ref().snapshot(symbol).await
    }
}

#[derive(Clone)]
pub struct IbkrMarketDataProvider {
    adapter: IbkrPaperGatewayAdapter,
    route_exchange: Option<String>,
}

impl IbkrMarketDataProvider {
    pub fn new(adapter: IbkrPaperGatewayAdapter, route_exchange: Option<String>) -> Self {
        Self {
            adapter,
            route_exchange: normalized_exchange(route_exchange),
        }
    }
}

#[async_trait]
impl MarketDataProvider for IbkrMarketDataProvider {
    async fn snapshot(&self, symbol: &str) -> Result<Quote, MarketDataError> {
        let snapshot = self
            .adapter
            .market_data_snapshot(symbol, self.route_exchange.as_deref())
            .await?;
        Ok(Quote::new(
            snapshot.symbol,
            snapshot.bid,
            snapshot.ask,
            snapshot.last,
            None,
            snapshot.ts_ms,
            MarketDataSource::Ibkr,
            MarketDataKind::from_provider_name(&snapshot.market_data_type),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongbridgeMarketDataSettings {
    pub app_key_env: String,
    pub app_secret_env: String,
    pub access_token_env: String,
}

impl LongbridgeMarketDataSettings {
    pub fn new(
        app_key_env: impl Into<String>,
        app_secret_env: impl Into<String>,
        access_token_env: impl Into<String>,
    ) -> Self {
        Self {
            app_key_env: app_key_env.into(),
            app_secret_env: app_secret_env.into(),
            access_token_env: access_token_env.into(),
        }
    }
}

#[derive(Clone)]
pub struct LongbridgeMarketDataProvider {
    context: QuoteContext,
}

impl LongbridgeMarketDataProvider {
    pub fn from_env(settings: &LongbridgeMarketDataSettings) -> Result<Self, MarketDataError> {
        let app_key = required_env(&settings.app_key_env)?;
        let app_secret = required_env(&settings.app_secret_env)?;
        let access_token = required_env(&settings.access_token_env)?;
        let config = Arc::new(Config::from_apikey(app_key, app_secret, access_token));
        let (context, _push_events) = QuoteContext::new(config);
        Ok(Self { context })
    }
}

#[async_trait]
impl MarketDataProvider for LongbridgeMarketDataProvider {
    async fn snapshot(&self, symbol: &str) -> Result<Quote, MarketDataError> {
        let provider_symbol = longbridge_symbol(symbol)?;
        let quotes = self
            .context
            .quote([provider_symbol.as_str()])
            .await
            .map_err(provider_error)?;
        let quote = quotes
            .into_iter()
            .find(|quote| quote.symbol.eq_ignore_ascii_case(&provider_symbol))
            .ok_or_else(|| {
                MarketDataError::Provider(format!(
                    "Longbridge returned no quote for {provider_symbol}"
                ))
            })?;
        let depth = self
            .context
            .depth(provider_symbol.clone())
            .await
            .map_err(provider_error)?;
        let received_ts_ms = unix_timestamp_ms()?;

        Ok(Quote::new(
            symbol,
            best_depth_price(&depth.bids),
            best_depth_price(&depth.asks),
            positive_price(quote.last_done),
            Some(
                i64::try_from(quote.timestamp.unix_timestamp_nanos() / 1_000_000).map_err(
                    |_| {
                        MarketDataError::Provider(
                            "Longbridge quote timestamp does not fit in i64 milliseconds"
                                .to_string(),
                        )
                    },
                )?,
            ),
            received_ts_ms,
            MarketDataSource::Longbridge,
            MarketDataKind::Realtime,
        ))
    }
}

pub fn longbridge_symbol(symbol: &str) -> Result<String, MarketDataError> {
    let trimmed = symbol.trim();
    if trimmed.is_empty() {
        return Err(MarketDataError::UnsupportedSymbol(symbol.to_string()));
    }
    if let Some((ticker, market)) = trimmed.rsplit_once('.') {
        if !ticker.is_empty() && market.eq_ignore_ascii_case("US") {
            return Ok(format!("{}.US", ticker.to_ascii_uppercase()));
        }
    }

    let parts = trimmed.split(':').collect::<Vec<_>>();
    if parts.len() == 4
        && parts[0].eq_ignore_ascii_case("US")
        && parts[3].eq_ignore_ascii_case("EQUITY")
        && !parts[2].is_empty()
    {
        return Ok(format!("{}.US", parts[2].to_ascii_uppercase()));
    }

    if trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '.')
        && !trimmed.contains(':')
        && !trimmed.contains('.')
    {
        return Ok(format!("{}.US", trimmed.to_ascii_uppercase()));
    }

    Err(MarketDataError::UnsupportedSymbol(symbol.to_string()))
}

fn best_depth_price(depth: &[longbridge::quote::Depth]) -> Option<Decimal> {
    depth
        .iter()
        .filter_map(|level| {
            level
                .price
                .filter(|price| *price > Decimal::ZERO)
                .map(|price| (level.position, price))
        })
        .min_by_key(|(position, _)| *position)
        .map(|(_, price)| price)
}

fn positive_price(price: Decimal) -> Option<Decimal> {
    (price > Decimal::ZERO).then_some(price)
}

fn required_env(name: &str) -> Result<String, MarketDataError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| MarketDataError::MissingCredential(name.to_string()))
}

fn provider_error(error: longbridge::Error) -> MarketDataError {
    MarketDataError::Provider(format!("Longbridge API error: {error}"))
}

fn unix_timestamp_ms() -> Result<i64, MarketDataError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MarketDataError::Provider(format!("system clock error: {error}")))?;
    i64::try_from(duration.as_millis()).map_err(|_| {
        MarketDataError::Provider("system timestamp does not fit in i64 milliseconds".to_string())
    })
}

fn normalized_exchange(exchange: Option<String>) -> Option<String> {
    exchange.and_then(|exchange| {
        let trimmed = exchange.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use longbridge::quote::Depth;
    use rust_decimal_macros::dec;

    #[test]
    fn maps_supported_symbols_to_longbridge_us_symbols() {
        assert_eq!(
            longbridge_symbol("US:NASDAQ:AAPL:EQUITY").unwrap(),
            "AAPL.US"
        );
        assert_eq!(longbridge_symbol("msft").unwrap(), "MSFT.US");
        assert_eq!(longbridge_symbol("bset.us").unwrap(), "BSET.US");
    }

    #[test]
    fn rejects_unsupported_longbridge_symbols() {
        assert!(matches!(
            longbridge_symbol("HK:SEHK:700:EQUITY"),
            Err(MarketDataError::UnsupportedSymbol(_))
        ));
        assert!(matches!(
            longbridge_symbol(""),
            Err(MarketDataError::UnsupportedSymbol(_))
        ));
    }

    #[test]
    fn selects_positive_depth_price_with_lowest_position() {
        let depth = vec![
            depth(3, Some(dec!(103))),
            depth(1, Some(dec!(101))),
            depth(0, None),
            depth(2, Some(dec!(102))),
            depth(-1, Some(Decimal::ZERO)),
        ];

        assert_eq!(best_depth_price(&depth), Some(dec!(101)));
    }

    #[test]
    fn missing_credential_error_names_only_the_environment_variable() {
        let env_name = "TRADER_TEST_LONGBRIDGE_CREDENTIAL_THAT_MUST_NOT_EXIST";

        let error = required_env(env_name).unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("market data credential environment variable {env_name} is not set")
        );
    }

    fn depth(position: i32, price: Option<Decimal>) -> Depth {
        Depth {
            position,
            price,
            volume: 1,
            order_num: 1,
        }
    }
}
