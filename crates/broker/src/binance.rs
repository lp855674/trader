use async_trait::async_trait;
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{fmt, sync::Arc};
use trader_core::OrderRequest;

use crate::{
    Broker, BrokerAccountSnapshot, BrokerError, BrokerExecution, BrokerKind, BrokerOpenOrder,
    BrokerOrder, BrokerPositionSide, BrokerPositionSnapshot, BrokerStatus, PlaceOrderResponse,
    fake_status,
};

#[derive(Debug, Clone)]
pub struct BinanceSpotTestnetSettings {
    pub base_url: String,
    pub api_key: String,
    pub secret_key: String,
    pub recv_window_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceSignedRequest {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinanceOrderSide {
    Buy,
    Sell,
}

impl BinanceOrderSide {
    fn as_query_value(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceLimitOrderRequest {
    pub symbol: String,
    pub side: BinanceOrderSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub client_order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceOrderAck {
    pub order_id: u64,
    pub client_order_id: String,
    pub status: String,
    pub executed_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceTrade {
    pub trade_id: u64,
    pub order_id: u64,
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub fee_asset: String,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceOpenOrder {
    pub order_id: u64,
    pub client_order_id: String,
    pub symbol: String,
    pub status: String,
    pub side: String,
    pub price: Decimal,
    pub orig_qty: Decimal,
    pub executed_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceAssetBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

impl BinanceAssetBalance {
    pub fn total(&self) -> Decimal {
        self.free + self.locked
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinanceKlineBar {
    pub ts_ms: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

#[async_trait]
pub trait BinanceHttpClient: Send + Sync {
    async fn get(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError>;
    async fn post(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError>;
    async fn delete(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError>;
}

#[derive(Debug, Clone)]
pub struct ReqwestBinanceHttpClient {
    client: reqwest::Client,
}

impl ReqwestBinanceHttpClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl BinanceHttpClient for ReqwestBinanceHttpClient {
    async fn get(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError> {
        let mut request = self.client.get(url);
        if let Some(api_key) = api_key {
            request = request.header("X-MBX-APIKEY", api_key);
        }
        binance_response_body(request.send().await?).await
    }

    async fn post(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError> {
        let mut request = self.client.post(url);
        if let Some(api_key) = api_key {
            request = request.header("X-MBX-APIKEY", api_key);
        }
        binance_response_body(request.send().await?).await
    }

    async fn delete(&self, url: &str, api_key: Option<&str>) -> Result<String, BrokerError> {
        let mut request = self.client.delete(url);
        if let Some(api_key) = api_key {
            request = request.header("X-MBX-APIKEY", api_key);
        }
        binance_response_body(request.send().await?).await
    }
}

#[derive(Clone)]
pub struct BinanceSpotTestnetAdapter {
    settings: BinanceSpotTestnetSettings,
    client: Arc<dyn BinanceHttpClient>,
}

impl fmt::Debug for BinanceSpotTestnetAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BinanceSpotTestnetAdapter")
            .field("base_url", &self.settings.base_url)
            .field("recv_window_ms", &self.settings.recv_window_ms)
            .finish_non_exhaustive()
    }
}

impl BinanceSpotTestnetAdapter {
    pub fn try_new(settings: BinanceSpotTestnetSettings) -> Result<Self, BrokerError> {
        if !settings.base_url.contains("testnet.binance.vision") {
            return Err(BrokerError::Config(
                "Binance paper adapter requires Spot testnet base_url".to_string(),
            ));
        }
        Ok(Self::new(settings))
    }

    pub fn new(settings: BinanceSpotTestnetSettings) -> Self {
        Self::new_with_client(settings, reqwest::Client::new())
    }

    pub fn new_with_client(settings: BinanceSpotTestnetSettings, client: reqwest::Client) -> Self {
        Self::new_with_http_client(settings, Arc::new(ReqwestBinanceHttpClient::new(client)))
    }

    pub fn new_with_http_client(
        settings: BinanceSpotTestnetSettings,
        client: Arc<dyn BinanceHttpClient>,
    ) -> Self {
        Self { settings, client }
    }

    pub fn signed_account_request(&self, timestamp_ms: i64) -> BinanceSignedRequest {
        let query = format!(
            "timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/account", &query)
    }

    pub fn signed_limit_order_request(
        &self,
        order: &BinanceLimitOrderRequest,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={}&side={}&type=LIMIT&timeInForce=GTC&quantity={}&price={}&newClientOrderId={}&timestamp={timestamp_ms}&recvWindow={}",
            order.symbol,
            order.side.as_query_value(),
            order.quantity,
            order.price,
            order.client_order_id,
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_query_order_request(
        &self,
        symbol: &str,
        order_id: u64,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&orderId={order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_query_order_by_client_order_id_request(
        &self,
        symbol: &str,
        client_order_id: &str,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&origClientOrderId={client_order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_cancel_order_request(
        &self,
        symbol: &str,
        order_id: u64,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&orderId={order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/order", &query)
    }

    pub fn signed_my_trades_request(
        &self,
        symbol: &str,
        order_id: u64,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&orderId={order_id}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/myTrades", &query)
    }

    pub fn signed_my_trades_for_symbol_request(
        &self,
        symbol: &str,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/myTrades", &query)
    }

    pub fn signed_open_orders_request(
        &self,
        symbol: &str,
        timestamp_ms: i64,
    ) -> BinanceSignedRequest {
        let query = format!(
            "symbol={symbol}&timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/openOrders", &query)
    }

    pub fn signed_all_open_orders_request(&self, timestamp_ms: i64) -> BinanceSignedRequest {
        let query = format!(
            "timestamp={timestamp_ms}&recvWindow={}",
            self.settings.recv_window_ms
        );
        self.signed_request("/v3/openOrders", &query)
    }

    pub fn klines_url(&self, symbol: &str, interval: &str, limit: u16) -> String {
        format!(
            "{}/v3/klines?symbol={symbol}&interval={interval}&limit={limit}",
            self.settings.base_url.trim_end_matches('/')
        )
    }

    pub async fn server_time_ms(&self) -> Result<i64, BrokerError> {
        let body = self
            .client
            .get(&format!("{}/v3/time", self.settings.base_url), None)
            .await?;
        Self::parse_server_time_json(&body)
    }

    pub async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request = self.signed_limit_order_request(order, self.server_time_ms().await?);
        let body = self
            .client
            .post(&request.url, Some(&request.api_key))
            .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn query_binance_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request =
            self.signed_query_order_request(symbol, order_id, self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn query_binance_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request = self.signed_query_order_by_client_order_id_request(
            symbol,
            client_order_id,
            self.server_time_ms().await?,
        );
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn cancel_binance_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        let request =
            self.signed_cancel_order_request(symbol, order_id, self.server_time_ms().await?);
        let body = self
            .client
            .delete(&request.url, Some(&request.api_key))
            .await?;
        let response = serde_json::from_str::<BinanceOrderResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.into_ack())
    }

    pub async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        let request = self.signed_my_trades_request(symbol, order_id, self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        Self::parse_trades_json(&body)
    }

    pub async fn my_trades_for_symbol(
        &self,
        symbol: &str,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        let request =
            self.signed_my_trades_for_symbol_request(symbol, self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        Self::parse_trades_json(&body)
    }

    pub async fn open_orders(&self, symbol: &str) -> Result<Vec<BinanceOpenOrder>, BrokerError> {
        let request = self.signed_open_orders_request(symbol, self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        Self::parse_open_orders_json(&body)
    }

    pub async fn all_open_orders(&self) -> Result<Vec<BinanceOpenOrder>, BrokerError> {
        let request = self.signed_all_open_orders_request(self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        Self::parse_open_orders_json(&body)
    }

    pub async fn account_balances(&self) -> Result<Vec<BinanceAssetBalance>, BrokerError> {
        let request = self.signed_account_request(self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        Self::parse_account_balances_json(&body)
    }

    pub async fn klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: u16,
    ) -> Result<Vec<BinanceKlineBar>, BrokerError> {
        let body = self
            .client
            .get(&self.klines_url(symbol, interval, limit), None)
            .await?;
        Self::parse_klines_json(&body)
    }

    pub fn parse_server_time_json(input: &str) -> Result<i64, BrokerError> {
        let response = serde_json::from_str::<BinanceServerTimeResponse>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(response.server_time)
    }

    pub fn parse_open_orders_json(input: &str) -> Result<Vec<BinanceOpenOrder>, BrokerError> {
        let response = serde_json::from_str::<Vec<BinanceOpenOrderResponse>>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        response
            .into_iter()
            .map(BinanceOpenOrderResponse::try_into_open_order)
            .collect()
    }

    pub fn parse_account_balances_json(
        input: &str,
    ) -> Result<Vec<BinanceAssetBalance>, BrokerError> {
        let response = serde_json::from_str::<BinanceAccountResponse>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        response
            .balances
            .into_iter()
            .map(BinanceBalance::try_into_asset_balance)
            .collect()
    }

    pub fn parse_klines_json(input: &str) -> Result<Vec<BinanceKlineBar>, BrokerError> {
        let rows = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        rows.into_iter()
            .map(Self::parse_kline_row)
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn parse_trades_json(input: &str) -> Result<Vec<BinanceTrade>, BrokerError> {
        let response = serde_json::from_str::<Vec<BinanceTradeResponse>>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        response
            .into_iter()
            .map(BinanceTradeResponse::try_into_trade)
            .collect()
    }

    pub fn parse_position_risk_json(
        account_id: &str,
        input: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        let response = serde_json::from_str::<Vec<BinancePositionRiskResponse>>(input)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        response
            .into_iter()
            .filter_map(|position| position.try_into_snapshot(account_id).transpose())
            .collect()
    }

    pub fn format_error_body(status: u16, body: &str) -> String {
        match serde_json::from_str::<BinanceErrorResponse>(body) {
            Ok(error) => format!(
                "Binance API error {status} code={} msg={}",
                error.code, error.msg
            ),
            Err(_) => format!("Binance API error {status}: {body}"),
        }
    }

    fn signed_request(&self, path: &str, query: &str) -> BinanceSignedRequest {
        let signature = hmac_sha256_hex(&self.settings.secret_key, query);
        BinanceSignedRequest {
            url: format!(
                "{}{path}?{query}&signature={signature}",
                self.settings.base_url.trim_end_matches('/')
            ),
            api_key: self.settings.api_key.clone(),
        }
    }

    fn parse_kline_row(row: Vec<serde_json::Value>) -> Result<BinanceKlineBar, BrokerError> {
        if row.len() < 6 {
            return Err(BrokerError::Config(
                "Binance kline row has fewer than 6 columns".to_string(),
            ));
        }
        Ok(BinanceKlineBar {
            ts_ms: json_i64(&row[0], "open_time")?,
            open: json_decimal(&row[1], "open")?,
            high: json_decimal(&row[2], "high")?,
            low: json_decimal(&row[3], "low")?,
            close: json_decimal(&row[4], "close")?,
            volume: json_decimal(&row[5], "volume")?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOrderResponse {
    order_id: u64,
    client_order_id: Option<String>,
    orig_client_order_id: Option<String>,
    status: String,
    executed_qty: Option<String>,
}

impl BinanceOrderResponse {
    fn into_ack(self) -> BinanceOrderAck {
        BinanceOrderAck {
            order_id: self.order_id,
            client_order_id: self
                .client_order_id
                .or(self.orig_client_order_id)
                .unwrap_or_default(),
            status: self.status,
            executed_qty: self
                .executed_qty
                .and_then(|qty| qty.parse::<Decimal>().ok())
                .unwrap_or(Decimal::ZERO),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceServerTimeResponse {
    server_time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceOpenOrderResponse {
    order_id: u64,
    client_order_id: String,
    symbol: String,
    status: String,
    side: String,
    price: String,
    orig_qty: String,
    executed_qty: String,
}

impl BinanceOpenOrderResponse {
    fn try_into_open_order(self) -> Result<BinanceOpenOrder, BrokerError> {
        Ok(BinanceOpenOrder {
            order_id: self.order_id,
            client_order_id: self.client_order_id,
            symbol: self.symbol,
            status: self.status,
            side: self.side,
            price: self
                .price
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            orig_qty: self
                .orig_qty
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            executed_qty: self
                .executed_qty
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceTradeResponse {
    id: u64,
    order_id: u64,
    symbol: String,
    price: String,
    qty: String,
    commission: String,
    commission_asset: String,
    time: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinancePositionRiskResponse {
    symbol: String,
    position_amt: String,
    entry_price: String,
    leverage: String,
    isolated_margin: String,
    un_realized_profit: String,
    position_side: String,
    update_time: i64,
}

impl BinancePositionRiskResponse {
    fn try_into_snapshot(
        self,
        account_id: &str,
    ) -> Result<Option<BrokerPositionSnapshot>, BrokerError> {
        let qty = self
            .position_amt
            .parse::<Decimal>()
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        if qty == Decimal::ZERO {
            return Ok(None);
        }
        let avg_price = self
            .entry_price
            .parse::<Decimal>()
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        let leverage = self
            .leverage
            .parse::<Decimal>()
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        let isolated_margin = self
            .isolated_margin
            .parse::<Decimal>()
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        let unrealized_pnl = self
            .un_realized_profit
            .parse::<Decimal>()
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        let position_side = match self.position_side.as_str() {
            "LONG" => BrokerPositionSide::Long,
            "SHORT" => BrokerPositionSide::Short,
            _ => BrokerPositionSide::from_signed_qty(qty).ok_or_else(|| {
                BrokerError::Config(format!(
                    "Binance position {} has zero quantity and no side",
                    self.symbol
                ))
            })?,
        };
        let margin_used = if isolated_margin == Decimal::ZERO && leverage != Decimal::ZERO {
            qty.abs() * avg_price / leverage
        } else {
            isolated_margin
        };

        Ok(Some(BrokerPositionSnapshot {
            account_id: account_id.to_string(),
            exchange: "BINANCE".to_string(),
            symbol: format!("CRYPTO:BINANCE:{}_PERP:CRYPTO_PERP", self.symbol),
            position_side,
            qty,
            avg_price,
            margin_used,
            unrealized_pnl,
            ts_ms: self.update_time,
        }))
    }
}

impl BinanceTradeResponse {
    fn try_into_trade(self) -> Result<BinanceTrade, BrokerError> {
        Ok(BinanceTrade {
            trade_id: self.id,
            order_id: self.order_id,
            symbol: self.symbol,
            price: self
                .price
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            qty: self
                .qty
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            fee: self
                .commission
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            fee_asset: self.commission_asset,
            ts_ms: self.time,
        })
    }
}

#[async_trait]
impl Broker for BinanceSpotTestnetAdapter {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "Binance testnet order submit is not enabled yet".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        let request = self.signed_account_request(self.server_time_ms().await?);
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        let response = serde_json::from_str::<BinanceAccountResponse>(&body)
            .map_err(|error| BrokerError::Config(error.to_string()))?;
        Ok(BrokerAccountSnapshot {
            account_id: account_id.to_string(),
            cash: response
                .balances
                .iter()
                .find(|balance| balance.asset == "USDT")
                .and_then(|balance| balance.free.parse::<Decimal>().ok())
                .unwrap_or(Decimal::ZERO),
            equity: Decimal::ZERO,
            buying_power: Decimal::ZERO,
            margin_used: Decimal::ZERO,
        })
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        let request = self.signed_request(
            "/fapi/v2/positionRisk",
            &format!(
                "timestamp={}&recvWindow={}",
                self.server_time_ms().await?,
                self.settings.recv_window_ms
            ),
        );
        let body = self
            .client
            .get(&request.url, Some(&request.api_key))
            .await?;
        Self::parse_position_risk_json(account_id, &body)
    }

    async fn open_orders(&self, account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        Ok(self
            .all_open_orders()
            .await?
            .into_iter()
            .map(|order| BrokerOpenOrder {
                broker_order_id: order.order_id.to_string(),
                client_order_id: order.client_order_id,
                account_id: account_id.to_string(),
                symbol: order.symbol,
                side: parse_broker_order_side(&order.side),
                order_type: trader_core::OrderType::Limit,
                price: Some(order.price),
                qty: order.orig_qty,
                filled_qty: order.executed_qty,
                status: order.status,
            })
            .collect())
    }

    async fn executions(
        &self,
        account_id: &str,
        symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        let Some(symbol) = symbol else {
            return Ok(Vec::new());
        };
        Ok(self
            .my_trades_for_symbol(symbol)
            .await?
            .into_iter()
            .map(|trade| BrokerExecution {
                trade_id: trade.trade_id.to_string(),
                broker_order_id: trade.order_id.to_string(),
                client_order_id: None,
                account_id: account_id.to_string(),
                symbol: trade.symbol,
                side: trader_core::OrderSide::Buy,
                price: trade.price,
                qty: trade.qty,
                fee: trade.fee,
                ts_ms: trade.ts_ms,
            })
            .collect())
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        self.client
            .get(
                &format!("{}/v3/ping", self.settings.base_url.trim_end_matches('/')),
                None,
            )
            .await?;
        Ok(fake_status(BrokerKind::Binance))
    }
}

fn parse_broker_order_side(side: &str) -> trader_core::OrderSide {
    if side.eq_ignore_ascii_case("SELL") {
        trader_core::OrderSide::Sell
    } else {
        trader_core::OrderSide::Buy
    }
}

#[derive(Debug, Deserialize)]
struct BinanceAccountResponse {
    balances: Vec<BinanceBalance>,
}

#[derive(Debug, Deserialize)]
struct BinanceBalance {
    asset: String,
    free: String,
    locked: String,
}

impl BinanceBalance {
    fn try_into_asset_balance(self) -> Result<BinanceAssetBalance, BrokerError> {
        Ok(BinanceAssetBalance {
            asset: self.asset,
            free: self
                .free
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
            locked: self
                .locked
                .parse::<Decimal>()
                .map_err(|error| BrokerError::Config(error.to_string()))?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct BinanceErrorResponse {
    code: i64,
    msg: String,
}

async fn binance_response_body(response: reqwest::Response) -> Result<String, BrokerError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(BrokerError::Rejected(
            BinanceSpotTestnetAdapter::format_error_body(status.as_u16(), &body),
        ));
    }
    Ok(body)
}

fn json_i64(value: &serde_json::Value, field: &str) -> Result<i64, BrokerError> {
    value
        .as_i64()
        .ok_or_else(|| BrokerError::Config(format!("Binance kline {field} is not an integer")))
}

fn json_decimal(value: &serde_json::Value, field: &str) -> Result<Decimal, BrokerError> {
    let raw = value
        .as_str()
        .ok_or_else(|| BrokerError::Config(format!("Binance kline {field} is not a string")))?;
    raw.parse::<Decimal>()
        .map_err(|error| BrokerError::Config(error.to_string()))
}

fn hmac_sha256_hex(secret_key: &str, payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
