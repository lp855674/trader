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
        timeout(
            self.settings.connect_timeout,
            stream.write_all(&ibkr_managed_accounts_request()),
        )
        .await
        .map_err(|_| {
            BrokerError::Connection(format!(
                "IBKR paper gateway managed accounts request timed out at {address}"
            ))
        })?
        .map_err(|error| {
            BrokerError::Connection(format!(
                "failed to write IBKR paper gateway managed accounts request at {address}: {error}"
            ))
        })?;

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
    let fields = payload
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .map(|field| {
            std::str::from_utf8(field)
                .map(str::to_string)
                .map_err(|error| BrokerError::Config(format!("invalid IBKR UTF-8 field: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
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
