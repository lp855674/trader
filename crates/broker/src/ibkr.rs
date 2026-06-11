use async_trait::async_trait;
use ibapi::{
    Client,
    contracts::Contract,
    orders::{
        Action, CancelOrder, ExecutionFilter, Executions, Order, Orders, PlaceOrder, TimeInForce,
    },
    prelude::StreamExt,
    subscriptions::SubscriptionItem,
};
use rust_decimal::{Decimal, prelude::FromPrimitive};
use serde::Serialize;
use std::{fmt, sync::Arc, time::Duration};
use tokio::time::timeout;
use trader_core::OrderRequest;

use crate::{
    Broker, BrokerAccountSnapshot, BrokerCapabilities, BrokerError, BrokerKind, BrokerOrder,
    BrokerStatus, PlaceOrderResponse,
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
    async fn open_orders(&self) -> Result<Vec<IbkrOpenOrder>, BrokerError>;
    async fn executions(
        &self,
        request_id: i64,
        account_id: &str,
        symbol: &str,
    ) -> Result<Vec<IbkrExecution>, BrokerError>;
    async fn next_order_id(&self) -> Result<i64, BrokerError>;
    async fn cancel_order(&self, order_id: i64) -> Result<IbkrOrderStatus, BrokerError>;
    async fn place_limit_order(
        &self,
        account_id: &str,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError>;
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

    async fn next_order_id(&self) -> Result<i64, BrokerError> {
        let client = self.connect_client().await?;
        let order_id = timeout(self.settings.connect_timeout, client.next_valid_order_id())
            .await
            .map_err(|_| self.timeout_error("next order id"))?
            .map_err(map_ibapi_error)?;
        client.disconnect().await;
        Ok(i64::from(order_id))
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
        let contract = ibkr_stock_contract(&order.symbol);
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
}

#[async_trait]
impl Broker for IbkrPaperGatewayAdapter {
    async fn place_order(&self, _request: OrderRequest) -> Result<PlaceOrderResponse, BrokerError> {
        Err(BrokerError::Rejected(
            "IBKR paper order submit is not implemented".to_string(),
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
        Err(BrokerError::Rejected(format!(
            "IBKR paper account snapshot is not implemented for {account_id}"
        )))
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

fn ibkr_stock_contract(symbol: &str) -> Contract {
    Contract::stock(symbol).build()
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

fn map_ibapi_error(error: ibapi::Error) -> BrokerError {
    BrokerError::Connection(format!("IBKR API error: {error}"))
}

fn map_ibapi_connect_error(address: &str, error: ibapi::Error) -> BrokerError {
    BrokerError::Connection(format!(
        "unable to connect to IBKR paper gateway at {address}: IBKR API error: {error}"
    ))
}
