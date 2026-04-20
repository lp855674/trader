use terminal_core::errors::TerminalError;
use terminal_core::models::{
    AmendOrderRequest, CancelOrderRequest, OrderActionResult, OrderRow, QuoteView, SubmitOrderRequest,
    TerminalOverview,
};

use crate::stream::QuantdStreamClient;

#[derive(Debug, Clone)]
pub struct QuantdHttpClient {
    base_url: String,
    http: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiErrorBody {
    error_code: String,
    message: String,
}

impl QuantdHttpClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
            api_key,
        }
    }

    pub async fn submit_order(
        &self,
        request: &SubmitOrderRequest,
    ) -> Result<OrderActionResult, TerminalError> {
        let response = self
            .request(reqwest::Method::POST, "/v1/orders")
            .json(request)
            .send()
            .await
            .map_err(map_transport_error)?;
        decode_json(response).await
    }

    pub async fn cancel_order(
        &self,
        request: &CancelOrderRequest,
        order_id: &str,
    ) -> Result<OrderActionResult, TerminalError> {
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/v1/orders/{order_id}/cancel"),
            )
            .json(request)
            .send()
            .await
            .map_err(map_transport_error)?;
        decode_json(response).await
    }

    pub async fn amend_order(
        &self,
        request: &AmendOrderRequest,
    ) -> Result<OrderActionResult, TerminalError> {
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/v1/orders/{}/amend", request.order_id),
            )
            .json(request)
            .send()
            .await
            .map_err(map_transport_error)?;
        decode_json(response).await
    }

    pub async fn get_overview(&self, account_id: &str) -> Result<TerminalOverview, TerminalError> {
        let response = self
            .request(
                reqwest::Method::GET,
                &format!("/v1/terminal/overview?account_id={account_id}"),
            )
            .send()
            .await
            .map_err(map_transport_error)?;
        decode_json(response).await
    }

    pub async fn get_quote(&self, symbol: &str) -> Result<QuoteView, TerminalError> {
        let response = self
            .request(reqwest::Method::GET, &format!("/v1/quotes/{symbol}"))
            .send()
            .await
            .map_err(map_transport_error)?;
        decode_json(response).await
    }

    pub async fn get_orders(&self, account_id: &str) -> Result<Vec<OrderRow>, TerminalError> {
        let response = self
            .request(reqwest::Method::GET, &format!("/v1/orders?account_id={account_id}"))
            .send()
            .await
            .map_err(map_transport_error)?;
        decode_json(response).await
    }

    pub fn stream_client(&self) -> QuantdStreamClient {
        QuantdStreamClient::new(self.base_url.clone(), self.api_key.clone())
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base_url, path));
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        request
    }
}

async fn decode_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, TerminalError> {
    if response.status().is_success() {
        response
            .json::<T>()
            .await
            .map_err(|error| TerminalError::new("decode_failed", error.to_string()))
    } else {
        let body = response
            .json::<ApiErrorBody>()
            .await
            .map_err(|error| TerminalError::new("decode_failed", error.to_string()))?;
        Err(TerminalError::new(body.error_code, body.message))
    }
}

fn map_transport_error(error: reqwest::Error) -> TerminalError {
    TerminalError::new("transport_error", error.to_string())
}
