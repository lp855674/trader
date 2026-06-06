use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Serialize;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
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

#[derive(Debug, Clone)]
pub struct IbkrPaperGatewaySettings {
    pub host: String,
    pub port: u16,
    pub client_id: u32,
    pub connect_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct IbkrPaperGatewayAdapter {
    settings: IbkrPaperGatewaySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IbkrServerVersion {
    pub server_version: i64,
    pub connection_time: String,
}

impl IbkrPaperGatewayAdapter {
    pub fn try_new(settings: IbkrPaperGatewaySettings) -> Result<Self, BrokerError> {
        if settings.port == 7496 {
            return Err(BrokerError::Config(
                "IBKR paper adapter requires a paper port; got common live port 7496".to_string(),
            ));
        }
        Ok(Self { settings })
    }

    pub fn settings(&self) -> &IbkrPaperGatewaySettings {
        &self.settings
    }

    pub async fn connect_probe(&self) -> Result<(), BrokerError> {
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        self.connect_socket(&address).await?;
        Ok(())
    }

    pub async fn connect_and_handshake(&self) -> Result<IbkrServerVersion, BrokerError> {
        let (_stream, version) = self.open_session().await?;
        Ok(version)
    }

    pub async fn managed_accounts(&self) -> Result<Vec<String>, BrokerError> {
        let (mut stream, _version) = self.open_session().await?;
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        self.write_request(
            &mut stream,
            &ibkr_managed_accounts_request(),
            "managed accounts",
        )
        .await?;

        loop {
            let frame = timeout(self.settings.connect_timeout, read_ibkr_frame(&mut stream))
                .await
                .map_err(|_| {
                    BrokerError::Connection(format!(
                        "IBKR paper gateway managed accounts response timed out at {address}"
                    ))
                })??;
            if let Some(accounts) = ibkr_parse_managed_accounts_frame_if_present(&frame)? {
                return Ok(accounts);
            }
        }
    }

    pub async fn open_orders(&self) -> Result<Vec<IbkrOpenOrder>, BrokerError> {
        let (mut stream, _version) = self.open_session().await?;
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        self.write_request(&mut stream, &ibkr_open_orders_request(), "open orders")
            .await?;
        let mut orders = Vec::new();

        loop {
            let frame = timeout(self.settings.connect_timeout, read_ibkr_frame(&mut stream))
                .await
                .map_err(|_| {
                    BrokerError::Connection(format!(
                        "IBKR paper gateway open orders response timed out at {address}"
                    ))
                })??;
            if ibkr_frame_message_id_is(&frame, "53")? {
                return Ok(orders);
            }
            if ibkr_frame_message_id_is(&frame, "5")? {
                orders.push(ibkr_parse_open_order_frame(&frame)?);
            }
        }
    }

    pub async fn executions(
        &self,
        request_id: i64,
        account_id: &str,
        symbol: &str,
    ) -> Result<Vec<IbkrExecution>, BrokerError> {
        let (mut stream, _version) = self.open_session().await?;
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        self.write_request(
            &mut stream,
            &ibkr_executions_request(request_id, account_id, symbol),
            "executions",
        )
        .await?;
        let mut executions = Vec::new();

        loop {
            let frame = timeout(self.settings.connect_timeout, read_ibkr_frame(&mut stream))
                .await
                .map_err(|_| {
                    BrokerError::Connection(format!(
                        "IBKR paper gateway executions response timed out at {address}"
                    ))
                })??;
            if ibkr_frame_message_id_is(&frame, "55")? {
                return Ok(executions);
            }
            if ibkr_frame_message_id_is(&frame, "11")? {
                executions.push(ibkr_parse_execution_frame(&frame)?);
            }
        }
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

    async fn open_session(&self) -> Result<(TcpStream, IbkrServerVersion), BrokerError> {
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        let mut stream = self.connect_socket(&address).await?;
        timeout(
            self.settings.connect_timeout,
            stream.write_all(&ibkr_client_version_handshake(100, 178)),
        )
        .await
        .map_err(|_| {
            BrokerError::Connection(format!(
                "IBKR paper gateway handshake timed out at {address}"
            ))
        })?
        .map_err(|error| {
            BrokerError::Connection(format!(
                "failed to write IBKR paper gateway handshake at {address}: {error}"
            ))
        })?;
        let frame = timeout(self.settings.connect_timeout, read_ibkr_frame(&mut stream))
            .await
            .map_err(|_| {
                BrokerError::Connection(format!(
                    "IBKR paper gateway server version timed out at {address}"
                ))
            })??;
        let version = ibkr_parse_server_version(&frame)?;
        Ok((stream, version))
    }

    async fn connect_socket(&self, address: &str) -> Result<TcpStream, BrokerError> {
        timeout(self.settings.connect_timeout, TcpStream::connect(address))
            .await
            .map_err(|_| {
                BrokerError::Connection(format!(
                    "unable to connect to IBKR paper gateway at {address}: timeout"
                ))
            })?
            .map_err(|error| {
                BrokerError::Connection(format!(
                    "unable to connect to IBKR paper gateway at {address}: {error}"
                ))
            })
    }

    async fn write_request(
        &self,
        stream: &mut TcpStream,
        request: &[u8],
        request_name: &str,
    ) -> Result<(), BrokerError> {
        let address = format!("{}:{}", self.settings.host, self.settings.port);
        timeout(self.settings.connect_timeout, stream.write_all(request))
            .await
            .map_err(|_| {
                BrokerError::Connection(format!(
                    "IBKR paper gateway {request_name} request timed out at {address}"
                ))
            })?
            .map_err(|error| {
                BrokerError::Connection(format!(
                    "failed to write IBKR paper gateway {request_name} request at {address}: {error}"
                ))
            })
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
                order_cancel: false,
                paper_trading: true,
                live_trading: false,
            },
        })
    }
}

async fn read_ibkr_frame(stream: &mut TcpStream) -> Result<Vec<u8>, BrokerError> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).await.map_err(|error| {
        BrokerError::Connection(format!("failed to read IBKR frame length: {error}"))
    })?;
    let payload_len = u32::from_be_bytes(length) as usize;
    let mut frame = Vec::with_capacity(4 + payload_len);
    frame.extend_from_slice(&length);
    let mut payload = vec![0; payload_len];
    stream.read_exact(&mut payload).await.map_err(|error| {
        BrokerError::Connection(format!("failed to read IBKR frame payload: {error}"))
    })?;
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn ibkr_encode_frame(fields: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<u8> {
    let mut payload = Vec::new();
    for field in fields {
        payload.extend_from_slice(field.as_ref().as_bytes());
        payload.push(0);
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    frame
}

pub fn ibkr_decode_frame(input: &[u8]) -> Result<Option<(Vec<String>, usize)>, BrokerError> {
    if input.len() < 4 {
        return Ok(None);
    }
    let payload_len = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    let frame_len = 4 + payload_len;
    if input.len() < frame_len {
        return Ok(None);
    }
    let payload = &input[4..frame_len];
    let mut fields = payload
        .split(|byte| *byte == 0)
        .map(|field| {
            std::str::from_utf8(field)
                .map(str::to_string)
                .map_err(|error| BrokerError::Config(format!("invalid IBKR UTF-8 field: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if fields.last().is_some_and(String::is_empty) {
        fields.pop();
    }
    Ok(Some((fields, frame_len)))
}

pub fn ibkr_client_version_handshake(min_version: u16, max_version: u16) -> Vec<u8> {
    let payload = format!("v{min_version}..{max_version}");
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(b"API\0");
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload.as_bytes());
    frame
}

pub fn ibkr_managed_accounts_request() -> Vec<u8> {
    ibkr_encode_frame(["17", "1"])
}

pub fn ibkr_open_orders_request() -> Vec<u8> {
    ibkr_encode_frame(["5", "1"])
}

pub fn ibkr_executions_request(request_id: i64, account_id: &str, symbol: &str) -> Vec<u8> {
    ibkr_encode_frame([
        "7",
        "3",
        &request_id.to_string(),
        account_id,
        "",
        symbol,
        "",
        "",
        "",
        "",
    ])
}

pub fn ibkr_parse_server_version(frame: &[u8]) -> Result<IbkrServerVersion, BrokerError> {
    let Some((fields, _consumed)) = ibkr_decode_frame(frame)? else {
        return Err(BrokerError::Config(
            "incomplete IBKR server version frame".to_string(),
        ));
    };
    let server_version = fields
        .first()
        .ok_or_else(|| BrokerError::Config("missing IBKR server version".to_string()))?
        .parse::<i64>()
        .map_err(|error| BrokerError::Config(format!("invalid IBKR server version: {error}")))?;
    let connection_time = fields
        .get(1)
        .ok_or_else(|| BrokerError::Config("missing IBKR connection time".to_string()))?
        .clone();
    Ok(IbkrServerVersion {
        server_version,
        connection_time,
    })
}

pub fn ibkr_parse_managed_accounts_frame(frame: &[u8]) -> Result<Vec<String>, BrokerError> {
    ibkr_parse_managed_accounts_frame_if_present(frame)?.ok_or_else(|| {
        BrokerError::Config("IBKR frame is not a managed accounts response".to_string())
    })
}

fn ibkr_parse_managed_accounts_frame_if_present(
    frame: &[u8],
) -> Result<Option<Vec<String>>, BrokerError> {
    let Some((fields, _consumed)) = ibkr_decode_frame(frame)? else {
        return Err(BrokerError::Config(
            "incomplete IBKR managed accounts frame".to_string(),
        ));
    };
    if fields.first().map(String::as_str) != Some("15") {
        return Ok(None);
    }
    let account_list = fields
        .last()
        .ok_or_else(|| BrokerError::Config("missing IBKR managed accounts list".to_string()))?;
    let accounts = account_list
        .split(',')
        .map(str::trim)
        .filter(|account| !account.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if accounts.is_empty() {
        return Err(BrokerError::Config(
            "IBKR managed accounts response contained no accounts".to_string(),
        ));
    }
    Ok(Some(accounts))
}

fn ibkr_frame_message_id_is(frame: &[u8], message_id: &str) -> Result<bool, BrokerError> {
    let Some((fields, _consumed)) = ibkr_decode_frame(frame)? else {
        return Err(BrokerError::Config("incomplete IBKR frame".to_string()));
    };
    Ok(fields.first().map(String::as_str) == Some(message_id))
}

pub fn ibkr_parse_open_order_frame(frame: &[u8]) -> Result<IbkrOpenOrder, BrokerError> {
    let Some((fields, _consumed)) = ibkr_decode_frame(frame)? else {
        return Err(BrokerError::Config(
            "incomplete IBKR open order frame".to_string(),
        ));
    };
    if fields.first().map(String::as_str) != Some("5") {
        return Err(BrokerError::Config(
            "IBKR frame is not an open order response".to_string(),
        ));
    }
    Ok(IbkrOpenOrder {
        order_id: parse_i64_field(&fields, 1, "IBKR open order id")?,
        account_id: string_field(&fields, 2, "IBKR open order account")?,
        symbol: string_field(&fields, 3, "IBKR open order symbol")?,
        side: string_field(&fields, 4, "IBKR open order side")?,
        order_type: string_field(&fields, 5, "IBKR open order type")?,
        quantity: parse_decimal_field(&fields, 6, "IBKR open order quantity")?,
        limit_price: optional_decimal_field(&fields, 7, "IBKR open order limit price")?,
        status: string_field(&fields, 8, "IBKR open order status")?,
        client_order_id: string_field(&fields, 9, "IBKR open order client order id")?,
        filled_qty: parse_decimal_field(&fields, 10, "IBKR open order filled quantity")?,
    })
}

pub fn ibkr_parse_execution_frame(frame: &[u8]) -> Result<IbkrExecution, BrokerError> {
    let Some((fields, _consumed)) = ibkr_decode_frame(frame)? else {
        return Err(BrokerError::Config(
            "incomplete IBKR execution frame".to_string(),
        ));
    };
    if fields.first().map(String::as_str) != Some("11") {
        return Err(BrokerError::Config(
            "IBKR frame is not an execution response".to_string(),
        ));
    }
    Ok(IbkrExecution {
        request_id: parse_i64_field(&fields, 1, "IBKR execution request id")?,
        symbol: string_field(&fields, 2, "IBKR execution symbol")?,
        order_id: parse_i64_field(&fields, 6, "IBKR execution order id")?,
        trade_id: string_field(&fields, 7, "IBKR execution id")?,
        side: string_field(&fields, 10, "IBKR execution side")?,
        qty: parse_decimal_field(&fields, 11, "IBKR execution quantity")?,
        price: parse_decimal_field(&fields, 12, "IBKR execution price")?,
        fee: optional_decimal_field(&fields, 13, "IBKR execution fee")?.unwrap_or(Decimal::ZERO),
    })
}

fn string_field(fields: &[String], index: usize, name: &str) -> Result<String, BrokerError> {
    fields
        .get(index)
        .cloned()
        .ok_or_else(|| BrokerError::Config(format!("missing {name}")))
}

fn parse_i64_field(fields: &[String], index: usize, name: &str) -> Result<i64, BrokerError> {
    string_field(fields, index, name)?
        .parse::<i64>()
        .map_err(|error| BrokerError::Config(format!("invalid {name}: {error}")))
}

fn parse_decimal_field(
    fields: &[String],
    index: usize,
    name: &str,
) -> Result<Decimal, BrokerError> {
    string_field(fields, index, name)?
        .parse::<Decimal>()
        .map_err(|error| BrokerError::Config(format!("invalid {name}: {error}")))
}

fn optional_decimal_field(
    fields: &[String],
    index: usize,
    name: &str,
) -> Result<Option<Decimal>, BrokerError> {
    let Some(value) = fields.get(index) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    value
        .parse::<Decimal>()
        .map(Some)
        .map_err(|error| BrokerError::Config(format!("invalid {name}: {error}")))
}
