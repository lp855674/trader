use async_trait::async_trait;
use broker::{
    BinanceLimitOrderRequest, BinanceOrderAck, BinanceOrderSide, BinanceSpotTestnetAdapter,
    BinanceTrade, BrokerError,
};
use rust_decimal::Decimal;
use trader_core::{OrderRequest, OrderSide, OrderType};

use crate::{ExecutedPaperOrder, PaperOrderExecutor};

#[async_trait]
pub trait BinancePaperOrderClient: Send + Sync {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError>;

    async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError>;

    async fn query_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError>;

    async fn cancel_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError>;

    async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError>;
}

#[async_trait]
impl BinancePaperOrderClient for BinanceSpotTestnetAdapter {
    async fn query_order_by_client_order_id(
        &self,
        symbol: &str,
        client_order_id: &str,
    ) -> Result<Option<BinanceOrderAck>, BrokerError> {
        match self
            .query_binance_order_by_client_order_id(symbol, client_order_id)
            .await
        {
            Ok(order) => Ok(Some(order)),
            Err(BrokerError::Rejected(message)) if message.contains("code=-2013") => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn place_limit_order(
        &self,
        order: &BinanceLimitOrderRequest,
    ) -> Result<BinanceOrderAck, BrokerError> {
        self.place_limit_order(order).await
    }

    async fn query_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        self.query_binance_order(symbol, order_id).await
    }

    async fn cancel_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceOrderAck, BrokerError> {
        self.cancel_binance_order(symbol, order_id).await
    }

    async fn my_trades(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<Vec<BinanceTrade>, BrokerError> {
        self.my_trades(symbol, order_id).await
    }
}

pub struct BinancePaperOrderExecutor<Client> {
    client: Client,
    client_order_prefix: String,
}

impl<Client> BinancePaperOrderExecutor<Client> {
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
impl<Client> PaperOrderExecutor for BinancePaperOrderExecutor<Client>
where
    Client: BinancePaperOrderClient,
{
    fn client_order_id(&self, _run_id: &str, order_number: usize) -> String {
        binance_client_order_id(&self.client_order_prefix, order_number)
    }

    async fn execute_order(
        &self,
        order: OrderRequest,
        mark_price: Decimal,
        order_number: usize,
    ) -> anyhow::Result<ExecutedPaperOrder> {
        if order.order_type != OrderType::Market {
            anyhow::bail!("Binance paper executor only accepts market intents");
        }
        if mark_price <= Decimal::ZERO {
            anyhow::bail!("Binance paper executor requires positive mark price");
        }
        let symbol = binance_spot_symbol(&order.symbol)?;
        let client_order_id = self.client_order_id("", order_number);
        let placed = match self
            .client
            .query_order_by_client_order_id(&symbol, &client_order_id)
            .await?
        {
            Some(existing) => existing,
            None => {
                let request = BinanceLimitOrderRequest {
                    symbol: symbol.clone(),
                    side: binance_order_side(order.side),
                    quantity: order.qty,
                    price: mark_price,
                    client_order_id: client_order_id.clone(),
                };
                self.client.place_limit_order(&request).await?
            }
        };
        let queried = if placed.status == "FILLED" || placed.status == "PARTIALLY_FILLED" {
            placed.clone()
        } else {
            self.client.query_order(&symbol, placed.order_id).await?
        };
        let mut status = queried.status.clone();
        let mut trades = self.client.my_trades(&symbol, placed.order_id).await?;
        if trades.is_empty() && binance_order_is_open(&queried.status) {
            match self.client.cancel_order(&symbol, placed.order_id).await {
                Ok(cancelled) => {
                    status = cancelled.status;
                }
                Err(BrokerError::Rejected(message))
                    if binance_cancel_unknown_order_message(&message) =>
                {
                    let refreshed = self.client.query_order(&symbol, placed.order_id).await?;
                    status = refreshed.status;
                    trades = self.client.my_trades(&symbol, placed.order_id).await?;
                }
                Err(error) => return Err(error.into()),
            }
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
        let (qty, price, fee) = aggregate_binance_trades(&trades)?;

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

fn binance_order_is_open(status: &str) -> bool {
    matches!(status, "NEW" | "PARTIALLY_FILLED" | "PENDING_NEW")
}

fn binance_cancel_unknown_order_message(message: &str) -> bool {
    message.contains("code=-2011") || message.contains("Unknown order sent")
}

pub fn binance_spot_symbol(symbol: &str) -> anyhow::Result<String> {
    if !symbol.contains(':') {
        return Ok(symbol.to_string());
    }
    let parts = symbol.split(':').collect::<Vec<_>>();
    if parts.len() == 4 && parts[0] == "CRYPTO" && parts[1] == "BINANCE" {
        return Ok(parts[2].to_string());
    }
    anyhow::bail!("unsupported Binance paper symbol {symbol}");
}

fn binance_order_side(side: OrderSide) -> BinanceOrderSide {
    match side {
        OrderSide::Buy => BinanceOrderSide::Buy,
        OrderSide::Sell => BinanceOrderSide::Sell,
    }
}

fn binance_client_order_id(client_order_prefix: &str, order_number: usize) -> String {
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

fn aggregate_binance_trades(
    trades: &[BinanceTrade],
) -> anyhow::Result<(Decimal, Decimal, Decimal)> {
    let mut qty = Decimal::ZERO;
    let mut notional = Decimal::ZERO;
    let mut fee = Decimal::ZERO;
    for trade in trades {
        qty += trade.qty;
        notional += trade.qty * trade.price;
        fee += trade.fee;
    }
    if qty <= Decimal::ZERO {
        anyhow::bail!("Binance testnet order has no fills");
    }
    Ok((qty, notional / qty, fee))
}
