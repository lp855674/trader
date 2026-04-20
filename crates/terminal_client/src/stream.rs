use futures::{SinkExt, StreamExt};
use terminal_core::errors::TerminalError;
use terminal_core::models::StreamMessage;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

pub type QuantdWsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Clone)]
pub struct QuantdStreamClient {
    stream_url: String,
    api_key: Option<String>,
}

impl QuantdStreamClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let base_url = base_url.into().trim_end_matches('/').to_string();
        let stream_url = if let Some(rest) = base_url.strip_prefix("http://") {
            format!("ws://{rest}/v1/stream")
        } else if let Some(rest) = base_url.strip_prefix("https://") {
            format!("wss://{rest}/v1/stream")
        } else {
            format!("{base_url}/v1/stream")
        };
        Self {
            stream_url,
            api_key,
        }
    }

    pub async fn connect(&self) -> Result<QuantdWsStream, TerminalError> {
        let mut request = self
            .stream_url
            .as_str()
            .into_client_request()
            .map_err(|error| TerminalError::new("stream_request_invalid", error.to_string()))?;
        if let Some(api_key) = &self.api_key {
            request.headers_mut().insert(
                tokio_tungstenite::tungstenite::http::header::AUTHORIZATION,
                format!("Bearer {api_key}")
                    .parse::<tokio_tungstenite::tungstenite::http::HeaderValue>()
                    .map_err(|error| {
                        TerminalError::new("stream_request_invalid", error.to_string())
                    })?,
            );
        }
        let (stream, _response) = connect_async(request)
            .await
            .map_err(|error| TerminalError::new("stream_connect_failed", error.to_string()))?;
        Ok(stream)
    }

    pub async fn next_message(
        stream: &mut QuantdWsStream,
    ) -> Result<Option<StreamMessage>, TerminalError> {
        while let Some(message) = stream.next().await {
            let message =
                message.map_err(|error| TerminalError::new("stream_read_failed", error.to_string()))?;
            match message {
                Message::Text(text) => {
                    let parsed = serde_json::from_str::<StreamMessage>(&text)
                        .map_err(|error| TerminalError::new("stream_decode_failed", error.to_string()))?;
                    return Ok(Some(parsed));
                }
                Message::Ping(payload) => {
                    stream
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| TerminalError::new("stream_write_failed", error.to_string()))?;
                }
                Message::Close(_) => return Ok(None),
                _ => {}
            }
        }
        Ok(None)
    }
}
