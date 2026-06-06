use async_trait::async_trait;
use broker::{BrokerError, IbkrLimitOrderRequest, IbkrOrderAck, IbkrOrderSide, IbkrTrade};
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

use crate::{ExecutedPaperOrder, PaperOrderExecutor};

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

pub struct IbkrPaperOrderExecutor<Client> {
    client: Client,
    client_order_prefix: String,
}

impl<Client> IbkrPaperOrderExecutor<Client> {
    pub fn new(client: Client) -> Self {
        Self::new_with_client_order_prefix(client, "default")
    }

    pub fn new_with_client_order_prefix(
        client: Client,
        client_order_prefix: impl Into<String>,
    ) -> Self {
        Self {
            client,
            client_order_prefix: client_order_prefix.into(),
        }
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
                    client_order_id: client_order_id.clone(),
                };
                self.client.place_limit_order(&request).await?
            }
        };
        let queried = if ibkr_order_is_terminal(&placed.status) {
            placed.clone()
        } else {
            self.client.query_order(&symbol, placed.order_id).await?
        };
        let mut status = queried.status.clone();
        let trades = self.client.executions(&symbol, placed.order_id).await?;
        if trades.is_empty() && ibkr_order_is_open(&queried.status) {
            status = self
                .client
                .cancel_order(&symbol, placed.order_id)
                .await?
                .status;
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
        .take(16)
        .collect::<String>();
    let prefix = if sanitized.is_empty() {
        "run".to_string()
    } else {
        sanitized
    };
    format!("trader-paper-{prefix}-{order_number}")
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
