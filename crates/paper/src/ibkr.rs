use async_trait::async_trait;
use broker::{
    BrokerError, IbkrLimitOrderRequest, IbkrOrderAck, IbkrOrderSide, IbkrPaperGatewayAdapter,
    IbkrTrade,
};
use rust_decimal::Decimal;
use std::time::Duration;
use trader_core::{OrderRequest, OrderSide, OrderType};

use crate::{ExecutedPaperOrder, PaperOrderExecutor};

const DEFAULT_IBKR_SETTLEMENT_POLL_ATTEMPTS: usize = 6;
const DEFAULT_IBKR_SETTLEMENT_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[async_trait]
pub trait IbkrPaperOrderClient: Send + Sync {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<IbkrOrderAck>, BrokerError>;

    async fn place_limit_order(
        &self,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError>;

    async fn query_order(&self, symbol: &str, order_id: i64) -> Result<IbkrOrderAck, BrokerError>;

    async fn cancel_order(&self, symbol: &str, order_id: i64) -> Result<IbkrOrderAck, BrokerError>;

    async fn executions(&self, symbol: &str, order_id: i64) -> Result<Vec<IbkrTrade>, BrokerError>;
}

pub struct IbkrPaperGatewayOrderClient {
    adapter: IbkrPaperGatewayAdapter,
    account_id: String,
}

impl IbkrPaperGatewayOrderClient {
    pub fn new(adapter: IbkrPaperGatewayAdapter, account_id: impl Into<String>) -> Self {
        Self {
            adapter,
            account_id: account_id.into(),
        }
    }
}

#[async_trait]
impl IbkrPaperOrderClient for IbkrPaperGatewayOrderClient {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<IbkrOrderAck>, BrokerError> {
        Ok(self
            .adapter
            .open_orders()
            .await?
            .into_iter()
            .find(|order| order.symbol == symbol && order.client_order_id == client_order_id)
            .map(open_order_ack))
    }

    async fn place_limit_order(
        &self,
        order: &IbkrLimitOrderRequest,
    ) -> Result<IbkrOrderAck, BrokerError> {
        self.adapter
            .place_limit_order(&self.account_id, order)
            .await
    }

    async fn query_order(&self, _symbol: &str, order_id: i64) -> Result<IbkrOrderAck, BrokerError> {
        self.adapter
            .open_orders()
            .await?
            .into_iter()
            .find(|order| order.order_id == order_id)
            .map(open_order_ack)
            .ok_or_else(|| BrokerError::OrderNotFound(order_id.to_string()))
    }

    async fn cancel_order(
        &self,
        _symbol: &str,
        order_id: i64,
    ) -> Result<IbkrOrderAck, BrokerError> {
        let status = self.adapter.cancel_ibkr_order(order_id).await?;
        Ok(IbkrOrderAck {
            order_id: status.order_id,
            client_order_id: String::new(),
            status: status.status,
            filled_qty: status.filled_qty,
        })
    }

    async fn executions(&self, symbol: &str, order_id: i64) -> Result<Vec<IbkrTrade>, BrokerError> {
        Ok(self
            .adapter
            .executions(1, &self.account_id, symbol)
            .await?
            .into_iter()
            .filter(|execution| execution.order_id == order_id)
            .map(|execution| IbkrTrade {
                trade_id: execution.trade_id,
                order_id: execution.order_id,
                symbol: execution.symbol,
                price: execution.price,
                qty: execution.qty,
                fee: execution.fee,
                ts_ms: 0,
            })
            .collect())
    }
}

pub struct IbkrPaperOrderExecutor<Client> {
    client: Client,
    client_order_prefix: String,
    route_exchange: Option<String>,
    override_percentage_constraints: bool,
    settlement_poll_attempts: usize,
    settlement_poll_interval: Duration,
}

impl<Client> IbkrPaperOrderExecutor<Client> {
    pub fn new(client: Client) -> Self {
        Self::new_with_client_order_prefix(client, "default")
    }

    pub fn new_with_client_order_prefix(
        client: Client,
        client_order_prefix: impl Into<String>,
    ) -> Self {
        Self::new_with_settlement_polling(
            client,
            client_order_prefix,
            DEFAULT_IBKR_SETTLEMENT_POLL_ATTEMPTS,
            DEFAULT_IBKR_SETTLEMENT_POLL_INTERVAL,
        )
    }

    pub fn new_with_settlement_polling(
        client: Client,
        client_order_prefix: impl Into<String>,
        settlement_poll_attempts: usize,
        settlement_poll_interval: Duration,
    ) -> Self {
        Self {
            client,
            client_order_prefix: client_order_prefix.into(),
            route_exchange: None,
            override_percentage_constraints: false,
            settlement_poll_attempts: settlement_poll_attempts.max(1),
            settlement_poll_interval,
        }
    }

    pub fn with_route_exchange(mut self, route_exchange: Option<String>) -> Self {
        self.route_exchange = route_exchange.and_then(|exchange| {
            let trimmed = exchange.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        self
    }

    pub fn with_override_percentage_constraints(mut self, enabled: bool) -> Self {
        self.override_percentage_constraints = enabled;
        self
    }
}

#[async_trait]
impl<Client> PaperOrderExecutor for IbkrPaperOrderExecutor<Client>
where
    Client: IbkrPaperOrderClient,
{
    fn client_order_id(&self, _run_id: &str, order_number: usize) -> String {
        ibkr_client_order_id(&self.client_order_prefix, order_number)
    }

    async fn execute_order(
        &self,
        order: OrderRequest,
        mark_price: Decimal,
        order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        if order.order_type != OrderType::Market {
            anyhow::bail!("IBKR paper executor only accepts market intents");
        }
        if mark_price <= Decimal::ZERO {
            anyhow::bail!("IBKR paper executor requires positive mark price");
        }
        let symbol = ibkr_stock_symbol(&order.symbol)?;
        let client_order_id = self.client_order_id("", order_number);
        let placed = match self
            .client
            .query_order_by_client_order_id(&symbol, &client_order_id)
            .await?
        {
            Some(existing) => existing,
            None => {
                let request = IbkrLimitOrderRequest {
                    symbol: symbol.clone(),
                    side: ibkr_order_side(order.side),
                    quantity: order.qty,
                    price: mark_price,
                    outside_rth: true,
                    route_exchange: self.route_exchange.clone(),
                    override_percentage_constraints: self.override_percentage_constraints,
                    client_order_id: client_order_id.clone(),
                };
                self.client.place_limit_order(&request).await?
            }
        };
        let queried = if ibkr_order_is_terminal(&placed.status) {
            placed.clone()
        } else {
            match self.client.query_order(&symbol, placed.order_id).await {
                Ok(order) => order,
                Err(BrokerError::OrderNotFound(_)) => placed.clone(),
                Err(error) => return Err(error.into()),
            }
        };
        let (mut status, mut trades) = self
            .wait_for_ibkr_settlement(&symbol, placed.order_id, queried.status.clone(), order.qty)
            .await?;
        if ibkr_order_is_open(&status) && ibkr_trade_qty(&trades) < order.qty {
            status = match self.client.cancel_order(&symbol, placed.order_id).await {
                Ok(cancelled) => cancelled.status,
                Err(BrokerError::Connection(message)) => ibkr_cancel_terminal_status(&message)
                    .ok_or_else(|| BrokerError::Connection(message))?,
                Err(error) => return Err(error.into()),
            };
            if ibkr_order_is_open(&status) {
                status = match self.client.query_order(&symbol, placed.order_id).await {
                    Ok(order) => order.status,
                    Err(BrokerError::OrderNotFound(_)) => "Cancelled".to_string(),
                    Err(error) => return Err(error.into()),
                };
            }
            // Capture executions that can arrive while the remaining quantity is cancelled.
            trades = self.client.executions(&symbol, placed.order_id).await?;
        }
        if trades.is_empty() {
            return Ok(ExecutedPaperOrder {
                client_order_id,
                broker_order_id: placed.order_id.to_string(),
                status,
                price: mark_price,
                qty: Decimal::ZERO,
                fee: Decimal::ZERO,
            });
        }
        let (qty, price, fee) = aggregate_ibkr_trades(&trades)?;
        if qty >= order.qty {
            status = "Filled".to_string();
        }

        Ok(ExecutedPaperOrder {
            client_order_id,
            broker_order_id: placed.order_id.to_string(),
            status,
            price,
            qty,
            fee,
        })
    }
}

impl<Client> IbkrPaperOrderExecutor<Client>
where
    Client: IbkrPaperOrderClient,
{
    async fn wait_for_ibkr_settlement(
        &self,
        symbol: &str,
        order_id: i64,
        initial_status: String,
        target_qty: Decimal,
    ) -> Result<(String, Vec<IbkrTrade>), BrokerError> {
        let mut status = initial_status;
        let mut trades = self.client.executions(symbol, order_id).await?;
        if ibkr_trade_qty(&trades) >= target_qty || !ibkr_order_is_open(&status) {
            return Ok((status, trades));
        }

        for _ in 1..self.settlement_poll_attempts {
            if !self.settlement_poll_interval.is_zero() {
                tokio::time::sleep(self.settlement_poll_interval).await;
            }
            match self.client.query_order(symbol, order_id).await {
                Ok(order) => {
                    status = order.status;
                }
                Err(BrokerError::OrderNotFound(_)) => {}
                Err(error) => return Err(error),
            }
            trades = self.client.executions(symbol, order_id).await?;
            if ibkr_trade_qty(&trades) >= target_qty || !ibkr_order_is_open(&status) {
                break;
            }
        }

        Ok((status, trades))
    }
}

pub fn ibkr_stock_symbol(symbol: &str) -> anyhow::Result<String> {
    if !symbol.contains(':') {
        return Ok(symbol.to_string());
    }
    let parts = symbol.split(':').collect::<Vec<_>>();
    if parts.len() == 4 && parts[3] == "EQUITY" {
        return Ok(parts[2].to_string());
    }
    anyhow::bail!("unsupported IBKR paper symbol {symbol}");
}

fn ibkr_order_side(side: OrderSide) -> IbkrOrderSide {
    match side {
        OrderSide::Buy => IbkrOrderSide::Buy,
        OrderSide::Sell => IbkrOrderSide::Sell,
    }
}

fn ibkr_client_order_id(client_order_prefix: &str, order_number: usize) -> String {
    let sanitized = client_order_prefix
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .collect::<String>();
    let prefix = if sanitized.is_empty() {
        "run".to_string()
    } else if sanitized.len() <= 16 {
        sanitized
    } else {
        format!("{:016x}", fnv1a_64(client_order_prefix.as_bytes()))
    };
    format!("trader-paper-{prefix}-{order_number}")
}

fn fnv1a_64(value: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    value.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

fn ibkr_order_is_open(status: &str) -> bool {
    matches!(
        status,
        "PendingSubmit" | "PreSubmitted" | "Submitted" | "ApiPending" | "PendingCancel"
    )
}

fn ibkr_order_is_terminal(status: &str) -> bool {
    matches!(
        status,
        "Filled" | "Cancelled" | "Canceled" | "ApiCancelled" | "Inactive"
    )
}

fn ibkr_cancel_terminal_status(message: &str) -> Option<String> {
    if message.contains("[10147]") {
        return Some("Cancelled".to_string());
    }
    if !message.contains("[10148]") {
        return None;
    }
    if message.contains("Filled") || message.contains("状态：Filled") {
        return Some("Filled".to_string());
    }
    if message.contains("Cancelled")
        || message.contains("Canceled")
        || message.contains("状态：Cancelled")
        || message.contains("状态：Canceled")
    {
        return Some("Cancelled".to_string());
    }
    None
}

fn ibkr_trade_qty(trades: &[IbkrTrade]) -> Decimal {
    trades
        .iter()
        .fold(Decimal::ZERO, |total, trade| total + trade.qty)
}

fn aggregate_ibkr_trades(trades: &[IbkrTrade]) -> anyhow::Result<(Decimal, Decimal, Decimal)> {
    let mut qty = Decimal::ZERO;
    let mut notional = Decimal::ZERO;
    let mut fee = Decimal::ZERO;
    for trade in trades {
        qty += trade.qty;
        notional += trade.qty * trade.price;
        fee += trade.fee;
    }
    if qty <= Decimal::ZERO {
        anyhow::bail!("IBKR paper order has no executions");
    }
    Ok((qty, notional / qty, fee))
}

fn open_order_ack(order: broker::IbkrOpenOrder) -> IbkrOrderAck {
    IbkrOrderAck {
        order_id: order.order_id,
        client_order_id: order.client_order_id,
        status: order.status,
        filled_qty: order.filled_qty,
    }
}
