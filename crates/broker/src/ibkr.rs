use async_trait::async_trait;
use chrono::Utc;
use ibapi::{
    Client, Notice, NoticeCategory,
    accounts::{AccountSummaryResult, AccountSummaryTags, PositionUpdate, types::AccountGroup},
    contracts::{Contract, SecurityType, tick_types::TickType},
    market_data::{MarketDataType, realtime::TickTypes},
    orders::{
        Action, CancelOrder, ExecutionFilter, Executions, Order, Orders, PlaceOrder, TimeInForce,
    },
    prelude::StreamExt,
    subscriptions::SubscriptionItem,
};
use rust_decimal::{Decimal, prelude::FromPrimitive};
use serde::Serialize;
use std::{collections::HashMap, fmt, sync::Arc, time::Duration};
use tokio::time::{Instant, sleep_until, timeout};
use trader_core::OrderRequest;

use crate::{
    Broker, BrokerAccountSnapshot, BrokerCapabilities, BrokerCashBalance, BrokerContractMetadata,
    BrokerError, BrokerExecution, BrokerKind, BrokerOpenOrder, BrokerOrder, BrokerOrderStatus,
    BrokerPositionSide, BrokerPositionSnapshot, BrokerStatus, PlaceOrderResponse,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IbkrOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IbkrLimitOrderRequest {
    pub symbol: String,
    pub side: IbkrOrderSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub outside_rth: bool,
    pub route_exchange: Option<String>,
    pub override_percentage_constraints: bool,
    pub client_order_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrOrderAck {
    pub order_id: i64,
    pub client_order_id: String,
    pub status: String,
    pub filled_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrTrade {
    pub trade_id: String,
    pub order_id: i64,
    pub symbol: String,
    pub price: Decimal,
    pub qty: Decimal,
    pub fee: Decimal,
    pub ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrMarketDataSnapshot {
    pub symbol: String,
    pub bid: Option<Decimal>,
    pub ask: Option<Decimal>,
    pub last: Option<Decimal>,
    pub ts_ms: i64,
    pub market_data_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrOpenOrder {
    pub order_id: i64,
    pub account_id: String,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub quantity: Decimal,
    pub limit_price: Option<Decimal>,
    pub status: String,
    pub client_order_id: String,
    pub filled_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrExecution {
    pub request_id: i64,
    pub order_id: i64,
    pub client_order_id: String,
    pub trade_id: String,
    pub symbol: String,
    pub side: String,
    pub qty: Decimal,
    pub price: Decimal,
    pub fee: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrOrderStatus {
    pub order_id: i64,
    pub status: String,
    pub filled_qty: Decimal,
    pub remaining_qty: Decimal,
    pub avg_fill_price: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrOrderDiagnosticReport {
    pub order_id: i64,
    pub client_order_id: String,
    pub latest_status: Option<String>,
    pub terminal_status: Option<String>,
    pub filled_qty: Decimal,
    pub completion_reason: String,
    pub observed_for_ms: u64,
    pub events: Vec<IbkrOrderDiagnosticEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IbkrOrderDiagnosticEvent {
    pub sequence: u64,
    pub elapsed_ms: u64,
    pub source: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filled_qty: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_qty: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_fill_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_qty: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_price: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commission: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commission_currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advanced_order_reject_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IbkrPaperGatewaySettings {
    pub host: String,
    pub port: u16,
    pub client_id: u32,
    pub connect_timeout: Duration,
}

#[derive(Clone)]
pub struct IbkrPaperGatewayAdapter {
    settings: IbkrPaperGatewaySettings,
    client: Arc<dyn IbkrGatewayClient>,
}

impl fmt::Debug for IbkrPaperGatewayAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IbkrPaperGatewayAdapter")
            .field("host", &self.settings.host)
            .field("port", &self.settings.port)
            .field("client_id", &self.settings.client_id)
            .field("connect_timeout", &self.settings.connect_timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IbkrServerVersion {
    pub server_version: i64,
    pub connection_time: String,
}

#[async_trait]
pub trait IbkrGatewayClient: Send + Sync {
    async fn connect_probe(&self) -> Result<(), BrokerError>;
    async fn connect_and_handshake(&self) -> Result<IbkrServerVersion, BrokerError>;
    async fn managed_accounts(&self) -> Result<Vec<String>, BrokerError>;
    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError>;
    async fn open_orders(&self) -> Result<Vec<IbkrOpenOrder>, BrokerError>;
    async fn executions(
        &self,
        request_id: i64,
        account_id: &str,
        symbol: &str,
    ) -> Result<Vec<IbkrExecution>, BrokerError>;
    async fn market_data_snapshot(
        &self,
        symbol: &str,
        route_exchange: Option<&str>,
    ) -> Result<IbkrMarketDataSnapshot, BrokerError>;
    async fn next_order_id(&self) -> Result<i64, BrokerError>;
    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError>;
    async fn cancel_order(&self, order_id: i64) -> Result<IbkrOrderStatus, BrokerError>;
    async fn place_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError>;
    async fn diagnose_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
        observation_timeout: Duration,
    ) -> Result<IbkrOrderDiagnosticReport, BrokerError>;
}

#[derive(Debug, Clone)]
pub struct IbapiIbkrGatewayClient {
    settings: IbkrPaperGatewaySettings,
}

impl IbapiIbkrGatewayClient {
    pub fn new(settings: IbkrPaperGatewaySettings) -> Self {
        Self { settings }
    }
}

impl IbkrPaperGatewayAdapter {
    pub fn try_new(settings: IbkrPaperGatewaySettings) -> Result<Self, BrokerError> {
        if settings.port == 7496 {
            return Err(BrokerError::Config(
                "IBKR paper adapter requires a paper port; got common live port 7496".to_string(),
            ));
        }
        Ok(Self::new_with_gateway_client(
            settings.clone(),
            Arc::new(IbapiIbkrGatewayClient::new(settings)),
        ))
    }

    pub fn new_with_gateway_client(
        settings: IbkrPaperGatewaySettings,
        client: Arc<dyn IbkrGatewayClient>,
    ) -> Self {
        Self { settings, client }
    }

    pub fn settings(&self) -> &IbkrPaperGatewaySettings {
        &self.settings
    }

    pub async fn connect_probe(&self) -> Result<(), BrokerError> {
        self.client.connect_probe().await
    }

    pub async fn connect_and_handshake(&self) -> Result<IbkrServerVersion, BrokerError> {
        self.client.connect_and_handshake().await
    }

    pub async fn managed_accounts(&self) -> Result<Vec<String>, BrokerError> {
        self.client.managed_accounts().await
    }

    pub async fn open_orders(&self) -> Result<Vec<IbkrOpenOrder>, BrokerError> {
        self.client.open_orders().await
    }

    pub async fn executions(
        &self,
        request_id: i64,
        account_id: &str,
        symbol: &str,
    ) -> Result<Vec<IbkrExecution>, BrokerError> {
        self.client.executions(request_id, account_id, symbol).await
    }

    pub async fn next_order_id(&self) -> Result<i64, BrokerError> {
        self.client.next_order_id().await
    }

    pub async fn market_data_snapshot(
        &self,
        symbol: &str,
        route_exchange: Option<&str>,
    ) -> Result<IbkrMarketDataSnapshot, BrokerError> {
        self.client
            .market_data_snapshot(symbol, route_exchange)
            .await
    }

    pub async fn cancel_ibkr_order(&self, order_id: i64) -> Result<IbkrOrderStatus, BrokerError> {
        self.client.cancel_order(order_id).await
    }

    pub async fn place_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        validate_limit_order(account_id, order)?;
        self.client.place_limit_order(account_id, order).await
    }

    pub async fn diagnose_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
        observation_timeout: Duration,
    ) -> Result<IbkrOrderDiagnosticReport, BrokerError> {
        validate_limit_order(account_id, order)?;
        if observation_timeout.is_zero() {
            return Err(BrokerError::Config(
                "IBKR order diagnostic observation timeout must be positive".to_string(),
            ));
        }
        self.client
            .diagnose_limit_order(account_id, order, observation_timeout)
            .await
    }

    pub async fn validate_paper_account(
        &self,
        account_id: &str,
    ) -> Result<Vec<String>, BrokerError> {
        let trimmed = account_id.trim();
        if trimmed.is_empty() || trimmed == "ibkr-paper" {
            return Err(BrokerError::Config(
                "configured IBKR paper account id must be a real TWS / Gateway paper account id, usually DU...".to_string(),
            ));
        }
        let accounts = self.managed_accounts().await?;
        if accounts.iter().any(|account| account == trimmed) {
            return Ok(accounts);
        }
        Err(BrokerError::Config(format!(
            "configured IBKR paper account id {trimmed} was not returned by TWS / Gateway; returned accounts: {}",
            accounts.join(",")
        )))
    }
}

impl IbapiIbkrGatewayClient {
    async fn connect_client(&self) -> Result<Client, BrokerError> {
        let address = self.address();
        timeout(
            self.settings.connect_timeout,
            Client::connect(&address, client_id_i32(self.settings.client_id)?),
        )
        .await
        .map_err(|_| {
            BrokerError::Connection(format!(
                "unable to connect to IBKR paper gateway at {address}: timeout"
            ))
        })?
        .map_err(|error| map_ibapi_connect_error(&address, error))
    }

    fn address(&self) -> String {
        format!("{}:{}", self.settings.host, self.settings.port)
    }

    fn timeout_error(&self, operation: &str) -> BrokerError {
        BrokerError::Connection(format!(
            "IBKR paper gateway {operation} timed out at {}",
            self.address()
        ))
    }
}

#[async_trait]
impl IbkrGatewayClient for IbapiIbkrGatewayClient {
    async fn connect_probe(&self) -> Result<(), BrokerError> {
        let client = self.connect_client().await?;
        client.disconnect().await;
        Ok(())
    }

    async fn connect_and_handshake(&self) -> Result<IbkrServerVersion, BrokerError> {
        let client = self.connect_client().await?;
        let version = IbkrServerVersion {
            server_version: i64::from(client.server_version()),
            connection_time: client
                .connection_time()
                .map(|value| value.to_string())
                .unwrap_or_default(),
        };
        client.disconnect().await;
        Ok(version)
    }

    async fn managed_accounts(&self) -> Result<Vec<String>, BrokerError> {
        let client = self.connect_client().await?;
        let accounts = timeout(self.settings.connect_timeout, client.managed_accounts())
            .await
            .map_err(|_| self.timeout_error("managed accounts"))?
            .map_err(map_ibapi_error)?;
        client.disconnect().await;
        Ok(accounts)
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        let client = self.connect_client().await?;
        let group = AccountGroup::from("All");
        let tags = [
            AccountSummaryTags::TOTAL_CASH_VALUE,
            AccountSummaryTags::NET_LIQUIDATION,
            AccountSummaryTags::BUYING_POWER,
            AccountSummaryTags::MAINT_MARGIN_REQ,
        ];
        let mut subscription = timeout(
            self.settings.connect_timeout,
            client.account_summary(&group, &tags),
        )
        .await
        .map_err(|_| self.timeout_error("account summary"))?
        .map_err(map_ibapi_error)?;
        let mut values = HashMap::new();
        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("account summary response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(AccountSummaryResult::Summary(summary))
                    if summary.account == account_id =>
                {
                    values.insert(summary.tag, summary.value);
                }
                SubscriptionItem::Data(AccountSummaryResult::Summary(_)) => {}
                SubscriptionItem::Data(AccountSummaryResult::End) => {
                    break;
                }
                SubscriptionItem::Notice(_) => {}
            }
        }
        client.disconnect().await;
        account_snapshot_from_summary(account_id, &values)
    }

    async fn open_orders(&self) -> Result<Vec<IbkrOpenOrder>, BrokerError> {
        let client = self.connect_client().await?;
        let mut subscription = timeout(self.settings.connect_timeout, client.all_open_orders())
            .await
            .map_err(|_| self.timeout_error("open orders"))?
            .map_err(map_ibapi_error)?;
        let mut orders = Vec::new();
        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("open orders response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(Orders::OrderData(order)) => {
                    orders.push(map_open_order(order)?);
                }
                SubscriptionItem::Data(Orders::OrderStatus(_)) | SubscriptionItem::Notice(_) => {}
            }
        }
        client.disconnect().await;
        Ok(orders)
    }

    async fn executions(
        &self,
        request_id: i64,
        account_id: &str,
        symbol: &str,
    ) -> Result<Vec<IbkrExecution>, BrokerError> {
        let client = self.connect_client().await?;
        let filter = ExecutionFilter {
            client_id: Some(client_id_i32(self.settings.client_id)?),
            account_code: account_id.to_string(),
            symbol: symbol.to_string(),
            security_type: "STK".to_string(),
            ..Default::default()
        };
        let mut subscription = timeout(self.settings.connect_timeout, client.executions(filter))
            .await
            .map_err(|_| self.timeout_error("executions"))?
            .map_err(map_ibapi_error)?;
        let mut executions = Vec::new();
        let mut commissions = Vec::new();
        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("executions response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(Executions::ExecutionData(execution)) => {
                    executions.push(map_execution(request_id, execution)?);
                }
                SubscriptionItem::Data(Executions::CommissionReport(report)) => {
                    commissions.push(report);
                }
                SubscriptionItem::Notice(_) => {}
            }
        }
        for execution in &mut executions {
            if let Some(report) = commissions
                .iter()
                .find(|report| report.execution_id == execution.trade_id)
            {
                execution.fee = decimal_from_f64(report.commission, "IBKR commission")?;
            }
        }
        client.disconnect().await;
        Ok(executions)
    }

    async fn market_data_snapshot(
        &self,
        symbol: &str,
        route_exchange: Option<&str>,
    ) -> Result<IbkrMarketDataSnapshot, BrokerError> {
        let client = self.connect_client().await?;
        let contract = ibkr_stock_contract(symbol, route_exchange);
        let mut subscription = timeout(
            self.settings.connect_timeout,
            client.market_data(&contract).snapshot().subscribe(),
        )
        .await
        .map_err(|_| self.timeout_error("market data snapshot request"))?
        .map_err(map_ibapi_error)?;
        let mut bid = None;
        let mut ask = None;
        let mut last = None;
        let mut market_data_type = MarketDataType::Unknown;

        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("market data snapshot response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(TickTypes::Price(tick)) if tick.price > 0.0 => {
                    let price = decimal_from_f64(tick.price, "IBKR market data price")?;
                    match tick.tick_type {
                        TickType::Bid => bid = Some(price),
                        TickType::Ask => ask = Some(price),
                        TickType::Last => last = Some(price),
                        _ => {}
                    }
                }
                SubscriptionItem::Data(TickTypes::MarketDataType(value)) => {
                    market_data_type = value;
                }
                SubscriptionItem::Data(TickTypes::SnapshotEnd) => break,
                SubscriptionItem::Data(_) | SubscriptionItem::Notice(_) => {}
            }
        }
        client.disconnect().await;

        Ok(IbkrMarketDataSnapshot {
            symbol: symbol.to_string(),
            bid,
            ask,
            last,
            ts_ms: Utc::now().timestamp_millis(),
            market_data_type: ibkr_market_data_type_name(market_data_type).to_string(),
        })
    }

    async fn next_order_id(&self) -> Result<i64, BrokerError> {
        let client = self.connect_client().await?;
        let order_id = timeout(self.settings.connect_timeout, client.next_valid_order_id())
            .await
            .map_err(|_| self.timeout_error("next order id"))?
            .map_err(map_ibapi_error)?;
        client.disconnect().await;
        Ok(i64::from(order_id))
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        let client = self.connect_client().await?;
        let mut subscription = timeout(self.settings.connect_timeout, client.positions())
            .await
            .map_err(|_| self.timeout_error("positions"))?
            .map_err(map_ibapi_error)?;
        let mut positions = Vec::new();
        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("positions response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(PositionUpdate::Position(position))
                    if position.account == account_id =>
                {
                    if let Some(snapshot) = map_position_snapshot(position)? {
                        positions.push(snapshot);
                    }
                }
                SubscriptionItem::Data(PositionUpdate::Position(_)) => {}
                SubscriptionItem::Data(PositionUpdate::PositionEnd) => {
                    break;
                }
                SubscriptionItem::Notice(_) => {}
            }
        }
        client.disconnect().await;
        Ok(positions)
    }

    async fn cancel_order(&self, order_id: i64) -> Result<IbkrOrderStatus, BrokerError> {
        let client = self.connect_client().await?;
        let mut subscription = timeout(
            self.settings.connect_timeout,
            client.cancel_order(order_id_i32(order_id)?, ""),
        )
        .await
        .map_err(|_| self.timeout_error("cancel order"))?
        .map_err(map_ibapi_error)?;
        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("cancel order response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(CancelOrder::OrderStatus(status))
                    if i64::from(status.order_id) == order_id =>
                {
                    client.disconnect().await;
                    return map_order_status(status);
                }
                SubscriptionItem::Data(CancelOrder::OrderStatus(_))
                | SubscriptionItem::Notice(_) => {}
            }
        }
        client.disconnect().await;
        Err(BrokerError::Rejected(format!(
            "IBKR cancel order {order_id} returned no order status"
        )))
    }

    async fn place_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        validate_limit_order(account_id, order)?;
        let client = self.connect_client().await?;
        let order_id = timeout(self.settings.connect_timeout, client.next_valid_order_id())
            .await
            .map_err(|_| self.timeout_error("next order id"))?
            .map_err(map_ibapi_error)?;
        let contract = ibkr_stock_contract(&order.symbol, order.route_exchange.as_deref());
        let ib_order = ibkr_limit_order(account_id, order)?;
        let mut subscription = timeout(
            self.settings.connect_timeout,
            client.place_order(order_id, &contract, &ib_order),
        )
        .await
        .map_err(|_| self.timeout_error("place limit order"))?
        .map_err(map_ibapi_error)?;

        while let Some(update) = timeout(self.settings.connect_timeout, subscription.next())
            .await
            .map_err(|_| self.timeout_error("place limit order response"))?
        {
            match update.map_err(map_ibapi_error)? {
                SubscriptionItem::Data(PlaceOrder::OrderStatus(status))
                    if status.order_id == order_id =>
                {
                    client.disconnect().await;
                    return Ok(IbkrOrderAck {
                        order_id: i64::from(order_id),
                        client_order_id: order.client_order_id.clone(),
                        status: status.status.to_string(),
                        filled_qty: decimal_from_f64(status.filled, "IBKR filled quantity")?,
                    });
                }
                SubscriptionItem::Data(PlaceOrder::OpenOrder(order_data))
                    if order_data.order_id == order_id =>
                {
                    client.disconnect().await;
                    return Ok(IbkrOrderAck {
                        order_id: i64::from(order_id),
                        client_order_id: order.client_order_id.clone(),
                        status: order_data.order_state.status.to_string(),
                        filled_qty: Decimal::ZERO,
                    });
                }
                SubscriptionItem::Data(
                    PlaceOrder::OrderStatus(_)
                    | PlaceOrder::OpenOrder(_)
                    | PlaceOrder::ExecutionData(_)
                    | PlaceOrder::CommissionReport(_),
                )
                | SubscriptionItem::Notice(_) => {}
            }
        }
        client.disconnect().await;
        Err(BrokerError::Rejected(format!(
            "IBKR place order {order_id} returned no order status"
        )))
    }

    async fn diagnose_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
        observation_timeout: Duration,
    ) -> Result<IbkrOrderDiagnosticReport, BrokerError> {
        validate_limit_order(account_id, order)?;
        let client = self.connect_client().await?;
        let mut notice_stream = client.notice_stream().map_err(map_ibapi_error)?;
        let order_id = timeout(self.settings.connect_timeout, client.next_valid_order_id())
            .await
            .map_err(|_| self.timeout_error("next order id"))?
            .map_err(map_ibapi_error)?;
        let contract = ibkr_stock_contract(&order.symbol, order.route_exchange.as_deref());
        let ib_order = ibkr_limit_order(account_id, order)?;
        let mut subscription = timeout(
            self.settings.connect_timeout,
            client.place_order(order_id, &contract, &ib_order),
        )
        .await
        .map_err(|_| self.timeout_error("place limit order diagnostic"))?
        .map_err(map_ibapi_error)?;

        let started = Instant::now();
        let hard_deadline = started + observation_timeout;
        let terminal_drain = Duration::from_secs(2).min(observation_timeout);
        let mut terminal_deadline = None;
        let mut notice_stream_open = true;
        let mut report = IbkrOrderDiagnosticReport {
            order_id: i64::from(order_id),
            client_order_id: order.client_order_id.clone(),
            latest_status: None,
            terminal_status: None,
            filled_qty: Decimal::ZERO,
            completion_reason: "observation_timeout".to_string(),
            observed_for_ms: 0,
            events: Vec::new(),
        };

        loop {
            let deadline = terminal_deadline
                .map(|deadline: Instant| deadline.min(hard_deadline))
                .unwrap_or(hard_deadline);
            tokio::select! {
                update = subscription.next() => {
                    match update {
                        Some(Ok(SubscriptionItem::Data(update))) => {
                            if record_diagnostic_order_update(
                                &mut report,
                                started,
                                order_id,
                                update,
                            )? && terminal_deadline.is_none() {
                                terminal_deadline = Some(Instant::now() + terminal_drain);
                            }
                        }
                        Some(Ok(SubscriptionItem::Notice(notice))) => {
                            push_diagnostic_notice(
                                &mut report,
                                started,
                                "order_subscription",
                                notice,
                            );
                        }
                        Some(Err(ibapi::Error::Notice(notice))) => {
                            push_diagnostic_notice(
                                &mut report,
                                started,
                                "order_subscription_error",
                                notice,
                            );
                            report.completion_reason = "subscription_error".to_string();
                            break;
                        }
                        Some(Err(error)) => {
                            push_diagnostic_stream_error(&mut report, started, error.to_string());
                            report.completion_reason = "subscription_error".to_string();
                            break;
                        }
                        None => {
                            report.completion_reason = "subscription_ended".to_string();
                            break;
                        }
                    }
                }
                notice = notice_stream.next(), if notice_stream_open => {
                    match notice {
                        Some(notice) => {
                            push_diagnostic_notice(
                                &mut report,
                                started,
                                "global_notice_stream",
                                notice,
                            );
                        }
                        None => {
                            notice_stream_open = false;
                        }
                    }
                }
                _ = sleep_until(deadline) => {
                    report.completion_reason = if report.terminal_status.is_some() {
                        "terminal_status".to_string()
                    } else {
                        "observation_timeout".to_string()
                    };
                    break;
                }
            }
        }

        report.observed_for_ms = duration_ms(started.elapsed());
        client.disconnect().await;
        Ok(report)
    }
}

#[async_trait]
impl Broker for IbkrPaperGatewayAdapter {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "IBKR paper order submit is not implemented".to_string(),
        ))
    }

    async fn cancel_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        let order_id = broker_order_id
            .parse::<i64>()
            .map_err(|_| BrokerError::OrderNotFound(broker_order_id.to_string()))?;
        let open_order = self
            .client
            .open_orders()
            .await?
            .into_iter()
            .find(|order| order.order_id == order_id)
            .ok_or_else(|| BrokerError::OrderNotFound(broker_order_id.to_string()))?;
        let cancelled = self.client.cancel_order(order_id).await?;
        Ok(ibkr_open_order_into_broker_order(
            open_order,
            broker_order_status_from_ibkr_status(&cancelled.status),
        ))
    }

    async fn query_order(&self, broker_order_id: &str) -> Result<BrokerOrder, BrokerError> {
        Err(BrokerError::OrderNotFound(broker_order_id.to_string()))
    }

    async fn account_snapshot(
        &self,
        account_id: &str,
    ) -> Result<BrokerAccountSnapshot, BrokerError> {
        self.client.account_snapshot(account_id).await
    }

    async fn position_snapshots(
        &self,
        account_id: &str,
    ) -> Result<Vec<BrokerPositionSnapshot>, BrokerError> {
        self.client.position_snapshots(account_id).await
    }

    async fn open_orders(&self, account_id: &str) -> Result<Vec<BrokerOpenOrder>, BrokerError> {
        let orders = self.client.open_orders().await?;
        Ok(orders
            .into_iter()
            .filter(|order| order.account_id == account_id)
            .map(ibkr_open_order_into_broker_open_order)
            .collect())
    }

    async fn executions(
        &self,
        account_id: &str,
        symbol: Option<&str>,
    ) -> Result<Vec<BrokerExecution>, BrokerError> {
        let request_id = Utc::now().timestamp_millis();
        let symbol = symbol.unwrap_or("");
        let executions = self
            .client
            .executions(request_id, account_id, symbol)
            .await?;
        Ok(executions
            .into_iter()
            .map(|execution| ibkr_execution_into_broker_execution(account_id, execution))
            .collect())
    }

    async fn status(&self) -> Result<BrokerStatus, BrokerError> {
        self.connect_and_handshake().await?;
        Ok(BrokerStatus {
            kind: BrokerKind::InteractiveBrokers,
            connected: true,
            trading_enabled: false,
            capabilities: BrokerCapabilities {
                market_data: true,
                order_submit: false,
                order_cancel: true,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

fn ibkr_open_order_into_broker_open_order(order: IbkrOpenOrder) -> BrokerOpenOrder {
    BrokerOpenOrder {
        broker_order_id: order.order_id.to_string(),
        client_order_id: order.client_order_id,
        account_id: order.account_id,
        symbol: order.symbol,
        side: parse_broker_order_side(&order.side),
        order_type: parse_broker_order_type(&order.order_type),
        price: order.limit_price,
        qty: order.quantity,
        filled_qty: order.filled_qty,
        status: order.status,
    }
}

fn ibkr_open_order_into_broker_order(
    order: IbkrOpenOrder,
    status: BrokerOrderStatus,
) -> BrokerOrder {
    BrokerOrder {
        broker_order_id: order.order_id.to_string(),
        account_id: order.account_id,
        symbol: order.symbol,
        side: parse_broker_order_side(&order.side),
        order_type: parse_broker_order_type(&order.order_type),
        price: order.limit_price,
        qty: order.quantity,
        status,
    }
}

fn broker_order_status_from_ibkr_status(status: &str) -> BrokerOrderStatus {
    if status.eq_ignore_ascii_case("cancelled") || status.eq_ignore_ascii_case("apicancelled") {
        BrokerOrderStatus::Cancelled
    } else {
        BrokerOrderStatus::Accepted
    }
}

fn ibkr_execution_into_broker_execution(
    account_id: &str,
    execution: IbkrExecution,
) -> BrokerExecution {
    BrokerExecution {
        trade_id: execution.trade_id,
        broker_order_id: execution.order_id.to_string(),
        client_order_id: non_empty_string(execution.client_order_id),
        account_id: account_id.to_string(),
        symbol: execution.symbol,
        side: parse_broker_order_side(&execution.side),
        price: execution.price,
        qty: execution.qty,
        fee: execution.fee,
        ts_ms: Utc::now().timestamp_millis(),
    }
}

fn parse_broker_order_side(side: &str) -> trader_core::OrderSide {
    if side.eq_ignore_ascii_case("SELL") || side.eq_ignore_ascii_case("SLD") {
        trader_core::OrderSide::Sell
    } else {
        trader_core::OrderSide::Buy
    }
}

fn parse_broker_order_type(order_type: &str) -> trader_core::OrderType {
    match order_type.to_ascii_uppercase().as_str() {
        "MKT" | "MARKET" => trader_core::OrderType::Market,
        "STP" | "STOP" => trader_core::OrderType::Stop,
        "STP LMT" | "STOP_LIMIT" | "STOPLIMIT" => trader_core::OrderType::StopLimit,
        _ => trader_core::OrderType::Limit,
    }
}

fn record_diagnostic_order_update(
    report: &mut IbkrOrderDiagnosticReport,
    started: Instant,
    expected_order_id: i32,
    update: PlaceOrder,
) -> Result<bool, BrokerError> {
    let mut event = diagnostic_event(report, started, "order_subscription", "");
    let terminal = match update {
        PlaceOrder::OpenOrder(order_data) => {
            let status = order_data.order_state.status;
            let matches_order = order_data.order_id == expected_order_id;
            event.kind = "open_order".to_string();
            event.order_id = Some(i64::from(order_data.order_id));
            event.status = Some(status.to_string());
            event.warning_text = non_empty_string(order_data.order_state.warning_text);
            event.reject_reason = non_empty_string(order_data.order_state.reject_reason);
            event.completed_status = non_empty_string(order_data.order_state.completed_status);
            if matches_order {
                report.latest_status = Some(status.to_string());
                if status.is_terminal() {
                    report.terminal_status = Some(status.to_string());
                }
            }
            matches_order && status.is_terminal()
        }
        PlaceOrder::OrderStatus(status) => {
            let status_kind = status.status;
            let matches_order = status.order_id == expected_order_id;
            let filled_qty = decimal_from_f64(status.filled, "IBKR filled quantity")?;
            event.kind = "order_status".to_string();
            event.order_id = Some(i64::from(status.order_id));
            event.status = Some(status_kind.to_string());
            event.filled_qty = Some(filled_qty);
            event.remaining_qty = Some(decimal_from_f64(
                status.remaining,
                "IBKR remaining quantity",
            )?);
            event.avg_fill_price = status
                .average_fill_price
                .map(|price| decimal_from_f64(price, "IBKR average fill price"))
                .transpose()?;
            if matches_order {
                report.latest_status = Some(status_kind.to_string());
                if filled_qty > report.filled_qty {
                    report.filled_qty = filled_qty;
                }
                if status_kind.is_terminal() {
                    report.terminal_status = Some(status_kind.to_string());
                }
            }
            matches_order && status_kind.is_terminal()
        }
        PlaceOrder::ExecutionData(execution_data) => {
            let execution = execution_data.execution;
            let matches_order = execution.order_id == expected_order_id;
            let cumulative_qty = decimal_from_f64(
                execution.cumulative_quantity,
                "IBKR cumulative execution quantity",
            )?;
            event.kind = "execution".to_string();
            event.order_id = Some(i64::from(execution.order_id));
            event.execution_id = Some(execution.execution_id);
            event.execution_qty = Some(decimal_from_f64(
                execution.shares,
                "IBKR execution quantity",
            )?);
            event.execution_price =
                Some(decimal_from_f64(execution.price, "IBKR execution price")?);
            if matches_order && cumulative_qty > report.filled_qty {
                report.filled_qty = cumulative_qty;
            }
            false
        }
        PlaceOrder::CommissionReport(commission_report) => {
            event.kind = "commission".to_string();
            event.execution_id = Some(commission_report.execution_id);
            event.commission = Some(decimal_from_f64(
                commission_report.commission,
                "IBKR commission",
            )?);
            event.commission_currency = non_empty_string(commission_report.currency);
            false
        }
    };
    report.events.push(event);
    Ok(terminal)
}

fn push_diagnostic_notice(
    report: &mut IbkrOrderDiagnosticReport,
    started: Instant,
    source: &str,
    notice: Notice,
) {
    let mut event = diagnostic_event(report, started, source, "notice");
    event.notice_code = Some(notice.code);
    event.notice_category = Some(notice_category_slug(notice.category()).to_string());
    event.message = Some(notice.message);
    event.error_time = notice.error_time.map(|value| value.to_string());
    event.advanced_order_reject_json = non_empty_string(notice.advanced_order_reject_json);
    report.events.push(event);
}

fn push_diagnostic_stream_error(
    report: &mut IbkrOrderDiagnosticReport,
    started: Instant,
    message: String,
) {
    let mut event = diagnostic_event(report, started, "order_subscription_error", "stream_error");
    event.message = Some(message);
    report.events.push(event);
}

fn diagnostic_event(
    report: &IbkrOrderDiagnosticReport,
    started: Instant,
    source: &str,
    kind: &str,
) -> IbkrOrderDiagnosticEvent {
    IbkrOrderDiagnosticEvent {
        sequence: report.events.len() as u64 + 1,
        elapsed_ms: duration_ms(started.elapsed()),
        source: source.to_string(),
        kind: kind.to_string(),
        order_id: None,
        status: None,
        filled_qty: None,
        remaining_qty: None,
        avg_fill_price: None,
        execution_id: None,
        execution_qty: None,
        execution_price: None,
        commission: None,
        commission_currency: None,
        notice_code: None,
        notice_category: None,
        message: None,
        error_time: None,
        advanced_order_reject_json: None,
        warning_text: None,
        reject_reason: None,
        completed_status: None,
    }
}

fn notice_category_slug(category: NoticeCategory) -> &'static str {
    match category {
        NoticeCategory::Cancellation => "cancellation",
        NoticeCategory::Warning => "warning",
        NoticeCategory::SystemMessage => "system_message",
        NoticeCategory::OrderRejection => "order_rejection",
        NoticeCategory::Error => "error",
        _ => "unknown",
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn map_open_order(order_data: ibapi::orders::OrderData) -> Result<IbkrOpenOrder, BrokerError> {
    Ok(IbkrOpenOrder {
        order_id: i64::from(order_data.order_id),
        account_id: order_data.order.account,
        symbol: order_data.contract.symbol.to_string(),
        side: order_data.order.action.to_string(),
        order_type: order_data.order.order_type,
        quantity: decimal_from_f64(order_data.order.total_quantity, "IBKR order quantity")?,
        limit_price: order_data
            .order
            .limit_price
            .map(|price| decimal_from_f64(price, "IBKR limit price"))
            .transpose()?,
        status: order_data.order_state.status.to_string(),
        client_order_id: order_data.order.order_ref,
        filled_qty: Decimal::ZERO,
    })
}

fn map_execution(
    request_id: i64,
    execution_data: ibapi::orders::ExecutionData,
) -> Result<IbkrExecution, BrokerError> {
    Ok(IbkrExecution {
        request_id,
        order_id: i64::from(execution_data.execution.order_id),
        client_order_id: execution_data.execution.order_reference,
        trade_id: execution_data.execution.execution_id,
        symbol: execution_data.contract.symbol.to_string(),
        side: execution_data.execution.side.to_string(),
        qty: decimal_from_f64(execution_data.execution.shares, "IBKR execution quantity")?,
        price: decimal_from_f64(execution_data.execution.price, "IBKR execution price")?,
        fee: Decimal::ZERO,
    })
}

fn map_order_status(status: ibapi::orders::OrderStatus) -> Result<IbkrOrderStatus, BrokerError> {
    Ok(IbkrOrderStatus {
        order_id: i64::from(status.order_id),
        status: status.status.to_string(),
        filled_qty: decimal_from_f64(status.filled, "IBKR filled quantity")?,
        remaining_qty: decimal_from_f64(status.remaining, "IBKR remaining quantity")?,
        avg_fill_price: status
            .average_fill_price
            .map(|price| decimal_from_f64(price, "IBKR average fill price"))
            .transpose()?
            .unwrap_or(Decimal::ZERO),
    })
}

fn map_position_snapshot(
    position: ibapi::accounts::Position,
) -> Result<Option<BrokerPositionSnapshot>, BrokerError> {
    let qty = decimal_from_f64(position.position, "IBKR position quantity")?;
    if qty == Decimal::ZERO {
        return Ok(None);
    }
    let avg_price = decimal_from_f64(position.average_cost, "IBKR average cost")?;
    let position_side = BrokerPositionSide::from_signed_qty(qty).ok_or_else(|| {
        BrokerError::Config(format!(
            "IBKR position {} has zero quantity and no side",
            position.contract.symbol
        ))
    })?;
    Ok(Some(BrokerPositionSnapshot {
        account_id: position.account,
        exchange: "IBKR".to_string(),
        symbol: ibkr_position_symbol(&position.contract),
        position_side,
        qty,
        avg_price,
        mark_price: None,
        margin_used: Decimal::ZERO,
        unrealized_pnl: Decimal::ZERO,
        ts_ms: Utc::now().timestamp_millis(),
        contract: Some(broker_contract_metadata_from_ibkr_contract(
            &position.contract,
        )?),
        liquidation_price: None,
        open_interest: None,
    }))
}

fn account_snapshot_from_summary(
    account_id: &str,
    values: &HashMap<String, String>,
) -> Result<BrokerAccountSnapshot, BrokerError> {
    let cash = summary_decimal(values, AccountSummaryTags::TOTAL_CASH_VALUE)?;
    let equity = summary_decimal(values, AccountSummaryTags::NET_LIQUIDATION)?;
    let buying_power = summary_decimal(values, AccountSummaryTags::BUYING_POWER)?;
    let margin_used = summary_decimal(values, AccountSummaryTags::MAINT_MARGIN_REQ)?;
    Ok(BrokerAccountSnapshot {
        account_id: account_id.to_string(),
        cash,
        equity,
        buying_power,
        margin_used,
        cash_balances: vec![BrokerCashBalance {
            account_id: account_id.to_string(),
            currency: "USD".to_string(),
            cash,
            available_cash: cash,
            frozen_cash: Decimal::ZERO,
            equity: Some(equity),
            buying_power: Some(buying_power),
            margin_used: Some(margin_used),
            source_ts_ms: Utc::now().timestamp_millis(),
        }],
    })
}

fn summary_decimal(
    values: &HashMap<String, String>,
    tag: &'static str,
) -> Result<Decimal, BrokerError> {
    values
        .get(tag)
        .ok_or_else(|| BrokerError::Config(format!("IBKR account summary missing {tag}")))?
        .parse::<Decimal>()
        .map_err(|error| BrokerError::Config(format!("invalid IBKR {tag}: {error}")))
}

fn ibkr_position_symbol(contract: &Contract) -> String {
    let exchange = if contract.primary_exchange.to_string().trim().is_empty() {
        contract.exchange.to_string()
    } else {
        contract.primary_exchange.to_string()
    };
    match contract.security_type {
        SecurityType::Stock => {
            let market = ibkr_stock_market(contract, &exchange);
            format!("{market}:{exchange}:{}:EQUITY", contract.symbol)
        }
        SecurityType::Crypto => format!("CRYPTO:{exchange}:{}:CRYPTO_SPOT", contract.symbol),
        _ => format!(
            "IBKR:{exchange}:{}:{}",
            contract.symbol, contract.security_type
        ),
    }
}

fn ibkr_stock_market(contract: &Contract, exchange: &str) -> &'static str {
    let currency = contract.currency.to_string();
    let currency = currency.trim();
    let exchange = exchange.trim();
    if exchange.eq_ignore_ascii_case("SEHK")
        || exchange.eq_ignore_ascii_case("HKEX")
        || currency.eq_ignore_ascii_case("HKD")
    {
        return "HK";
    }
    if exchange.eq_ignore_ascii_case("SSE")
        || exchange.eq_ignore_ascii_case("SZSE")
        || currency.eq_ignore_ascii_case("CNY")
        || currency.eq_ignore_ascii_case("CNH")
    {
        return "CN";
    }
    if currency.eq_ignore_ascii_case("USD") {
        return "US";
    }
    "IBKR"
}

fn broker_contract_metadata_from_ibkr_contract(
    contract: &Contract,
) -> Result<BrokerContractMetadata, BrokerError> {
    Ok(BrokerContractMetadata {
        conid: if contract.contract_id == 0 {
            None
        } else {
            Some(i64::from(contract.contract_id))
        },
        sec_type: Some(contract.security_type.to_string()),
        currency: non_empty_string(contract.currency.to_string()),
        exchange: non_empty_string(contract.exchange.to_string()),
        primary_exchange: non_empty_string(contract.primary_exchange.to_string()),
        multiplier: non_empty_decimal(contract.multiplier.to_string(), "IBKR contract multiplier")?,
        expiry: non_empty_string(contract.last_trade_date_or_contract_month.to_string()),
        right: contract
            .right
            .as_ref()
            .and_then(|right| non_empty_string(right.to_string())),
        strike: if contract.strike == 0.0 {
            None
        } else {
            Some(decimal_from_f64(contract.strike, "IBKR option strike")?)
        },
        local_symbol: non_empty_string(contract.local_symbol.to_string()),
        trading_class: non_empty_string(contract.trading_class.to_string()),
    })
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_empty_decimal(value: String, name: &str) -> Result<Option<Decimal>, BrokerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<Decimal>()
        .map(Some)
        .map_err(|error| BrokerError::Config(format!("invalid {name}: {error}")))
}

fn ibkr_stock_contract(symbol: &str, route_exchange: Option<&str>) -> Contract {
    match route_exchange {
        Some(exchange) if !exchange.trim().is_empty() => {
            Contract::stock(symbol).on_exchange(exchange.trim()).build()
        }
        _ => Contract::stock(symbol).build(),
    }
}

fn ibkr_limit_order(
    account_id: &str,
    request: &IbkrLimitOrderRequest,
) -> Result<Order, BrokerError> {
    Ok(Order {
        account: account_id.to_string(),
        action: match request.side {
            IbkrOrderSide::Buy => Action::Buy,
            IbkrOrderSide::Sell => Action::Sell,
        },
        total_quantity: decimal_to_f64(request.quantity, "IBKR order quantity")?,
        order_type: "LMT".to_string(),
        limit_price: Some(decimal_to_f64(request.price, "IBKR limit price")?),
        tif: TimeInForce::Day,
        outside_rth: request.outside_rth,
        override_percentage_constraints: request.override_percentage_constraints,
        order_ref: request.client_order_id.clone(),
        transmit: true,
        ..Default::default()
    })
}

fn validate_limit_order(
    account_id: &str,
    order: &IbkrLimitOrderRequest,
) -> Result<(), BrokerError> {
    if account_id.trim().is_empty() {
        return Err(BrokerError::Config(
            "IBKR account id must not be empty".to_string(),
        ));
    }
    if order.symbol.trim().is_empty() {
        return Err(BrokerError::Config(
            "IBKR order symbol must not be empty".to_string(),
        ));
    }
    if order.quantity <= Decimal::ZERO {
        return Err(BrokerError::Config(
            "IBKR order quantity must be positive".to_string(),
        ));
    }
    if order.price <= Decimal::ZERO {
        return Err(BrokerError::Config(
            "IBKR limit price must be positive".to_string(),
        ));
    }
    Ok(())
}

fn decimal_to_f64(value: Decimal, name: &str) -> Result<f64, BrokerError> {
    value
        .to_string()
        .parse::<f64>()
        .map_err(|error| BrokerError::Config(format!("invalid {name}: {error}")))
}

fn decimal_from_f64(value: f64, name: &str) -> Result<Decimal, BrokerError> {
    Decimal::from_f64(value).ok_or_else(|| BrokerError::Config(format!("invalid {name}: {value}")))
}

fn client_id_i32(client_id: u32) -> Result<i32, BrokerError> {
    i32::try_from(client_id)
        .map_err(|_| BrokerError::Config(format!("IBKR client_id {client_id} exceeds i32 range")))
}

fn order_id_i32(order_id: i64) -> Result<i32, BrokerError> {
    i32::try_from(order_id)
        .map_err(|_| BrokerError::Config(format!("IBKR order id {order_id} exceeds i32 range")))
}

fn ibkr_market_data_type_name(market_data_type: MarketDataType) -> &'static str {
    match market_data_type {
        MarketDataType::Realtime => "realtime",
        MarketDataType::Frozen => "frozen",
        MarketDataType::Delayed => "delayed",
        MarketDataType::DelayedFrozen => "delayed_frozen",
        MarketDataType::Unknown => "unknown",
    }
}

fn map_ibapi_error(error: ibapi::Error) -> BrokerError {
    BrokerError::Connection(format!("IBKR API error: {error}"))
}

fn map_ibapi_connect_error(address: &str, error: ibapi::Error) -> BrokerError {
    BrokerError::Connection(format!(
        "unable to connect to IBKR paper gateway at {address}: IBKR API error: {error}"
    ))
}

#[cfg(test)]
mod ibkr_contract_metadata_tests {
    use super::*;
    use ibapi::orders::{OrderStatus, OrderStatusKind};
    use rust_decimal_macros::dec;

    fn diagnostic_report() -> IbkrOrderDiagnosticReport {
        IbkrOrderDiagnosticReport {
            order_id: 42,
            client_order_id: "client-42".to_string(),
            latest_status: None,
            terminal_status: None,
            filled_qty: Decimal::ZERO,
            completion_reason: "observation_timeout".to_string(),
            observed_for_ms: 0,
            events: vec![],
        }
    }

    #[test]
    fn diagnostic_keeps_pre_submitted_open_and_marks_cancelled_terminal() {
        let started = Instant::now();
        let mut report = diagnostic_report();

        let terminal = record_diagnostic_order_update(
            &mut report,
            started,
            42,
            PlaceOrder::OrderStatus(OrderStatus {
                order_id: 42,
                status: OrderStatusKind::PreSubmitted,
                filled: 0.0,
                remaining: 1.0,
                ..Default::default()
            }),
        )
        .unwrap();

        assert!(!terminal);
        assert_eq!(report.latest_status.as_deref(), Some("PreSubmitted"));
        assert_eq!(report.terminal_status, None);

        let terminal = record_diagnostic_order_update(
            &mut report,
            started,
            42,
            PlaceOrder::OrderStatus(OrderStatus {
                order_id: 42,
                status: OrderStatusKind::Cancelled,
                filled: 0.0,
                remaining: 1.0,
                ..Default::default()
            }),
        )
        .unwrap();

        assert!(terminal);
        assert_eq!(report.terminal_status.as_deref(), Some("Cancelled"));
        assert_eq!(report.events.len(), 2);
        assert_eq!(report.events[0].kind, "order_status");
        assert_eq!(report.events[1].sequence, 2);
    }

    #[test]
    fn diagnostic_notice_preserves_rejection_payload() {
        let mut report = diagnostic_report();

        push_diagnostic_notice(
            &mut report,
            Instant::now(),
            "order_subscription_error",
            Notice {
                code: 201,
                message: "Order rejected".to_string(),
                error_time: None,
                advanced_order_reject_json: r#"{"errorCode":"XYZ"}"#.to_string(),
            },
        );

        let event = &report.events[0];
        assert_eq!(event.kind, "notice");
        assert_eq!(event.notice_code, Some(201));
        assert_eq!(event.notice_category.as_deref(), Some("order_rejection"));
        assert_eq!(event.message.as_deref(), Some("Order rejected"));
        assert_eq!(
            event.advanced_order_reject_json.as_deref(),
            Some(r#"{"errorCode":"XYZ"}"#)
        );
    }

    #[test]
    fn ibkr_contract_metadata_maps_stock_contract_fields() {
        let mut contract = Contract::stock("AAPL").build();
        contract.contract_id = 265598;
        contract.exchange = "SMART".into();
        contract.primary_exchange = "NASDAQ".into();
        contract.currency = "USD".into();
        contract.local_symbol = "AAPL".into();
        contract.trading_class = "NMS".into();

        let metadata = broker_contract_metadata_from_ibkr_contract(&contract).unwrap();

        assert_eq!(metadata.conid, Some(265598));
        assert_eq!(metadata.currency.as_deref(), Some("USD"));
        assert_eq!(metadata.exchange.as_deref(), Some("SMART"));
        assert_eq!(metadata.primary_exchange.as_deref(), Some("NASDAQ"));
        assert_eq!(metadata.local_symbol.as_deref(), Some("AAPL"));
        assert_eq!(metadata.trading_class.as_deref(), Some("NMS"));
    }

    #[test]
    fn ibkr_position_snapshot_keeps_contract_metadata() {
        let mut contract = Contract::stock("AAPL").build();
        contract.contract_id = 265598;
        contract.exchange = "SMART".into();
        contract.primary_exchange = "NASDAQ".into();
        contract.currency = "USD".into();

        let position = ibapi::accounts::Position {
            account: "DU123".to_string(),
            contract,
            position: 2.0,
            average_cost: 180.0,
        };

        let snapshot = map_position_snapshot(position).unwrap().unwrap();
        assert_eq!(snapshot.symbol, "US:NASDAQ:AAPL:EQUITY");
        assert_eq!(snapshot.contract.unwrap().conid, Some(265598));
        assert_eq!(snapshot.qty, dec!(2));
    }

    #[test]
    fn ibkr_position_symbol_maps_hong_kong_stock_market_from_contract_metadata() {
        let mut contract = Contract::stock("0700").build();
        contract.contract_id = 8068578;
        contract.exchange = "SMART".into();
        contract.primary_exchange = "SEHK".into();
        contract.currency = "HKD".into();
        contract.local_symbol = "0700".into();

        let position = ibapi::accounts::Position {
            account: "DU123".to_string(),
            contract,
            position: 100.0,
            average_cost: 320.0,
        };

        let snapshot = map_position_snapshot(position).unwrap().unwrap();
        assert_eq!(snapshot.symbol, "HK:SEHK:0700:EQUITY");
        let contract = snapshot.contract.unwrap();
        assert_eq!(contract.currency.as_deref(), Some("HKD"));
        assert_eq!(contract.primary_exchange.as_deref(), Some("SEHK"));
    }

    #[test]
    fn ibkr_position_symbol_does_not_default_unknown_stock_market_to_us() {
        let mut contract = Contract::stock("SAP").build();
        contract.exchange = "SMART".into();
        contract.primary_exchange = "IBIS".into();
        contract.currency = "EUR".into();

        let position = ibapi::accounts::Position {
            account: "DU123".to_string(),
            contract,
            position: 10.0,
            average_cost: 120.0,
        };

        let snapshot = map_position_snapshot(position).unwrap().unwrap();
        assert_eq!(snapshot.symbol, "IBKR:IBIS:SAP:EQUITY");
    }
}
