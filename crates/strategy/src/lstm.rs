use async_trait::async_trait;
use domain::{Side, Signal};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;

use crate::strategy::{ScoredCandidate, Strategy, StrategyContext};

pub struct LstmStrategy {
    pub client: Client,
    pub service_url: String,
    pub model_type: String,
    pub lookback: i64,
    pub buy_threshold: f64,
    pub sell_threshold: f64,
    pub db: Option<db::Db>,
    pub data_source_id: String,
}

impl LstmStrategy {
    pub fn new(
        service_url: String,
        model_type: String,
        lookback: i64,
        buy_threshold: f64,
        sell_threshold: f64,
        db: db::Db,
        data_source_id: String,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default(),
            service_url,
            model_type,
            lookback,
            buy_threshold,
            sell_threshold,
            db: Some(db),
            data_source_id,
        }
    }
}

#[derive(Serialize)]
struct BarPayload {
    ts_ms: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Serialize)]
struct PredictRequest<'a> {
    symbol: &'a str,
    model_type: &'a str,
    bars: Vec<BarPayload>,
}

#[derive(Deserialize)]
struct PredictResponse {
    score: f64,
    #[allow(dead_code)]
    side: String,
    confidence: f64,
}

#[derive(Deserialize)]
struct ServiceError {
    detail: ServiceErrorDetail,
}

#[derive(Deserialize)]
struct ServiceErrorDetail {
    error_code: String,
    message: String,
}

fn normalize_model_error_code(error_code: Option<&str>, fallback: &str) -> String {
    match error_code {
        Some("model_not_found") => "model_not_found".to_string(),
        Some("insufficient_bars") => "insufficient_bars".to_string(),
        Some("response_parse_failed") => "response_parse_failed".to_string(),
        Some("model_service_error") => "model_service_error".to_string(),
        Some(other) if !other.is_empty() => other.to_string(),
        _ => fallback.to_string(),
    }
}

impl LstmStrategy {
    async fn load_bars(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<(Vec<BarPayload>, f64)>, String> {
        let bars = if let Some(ref db) = self.db {
            match db::get_recent_bars(
                db.pool(),
                context.instrument_db_id,
                &self.data_source_id,
                self.lookback,
            )
            .await
            {
                Ok(rows) if rows.len() >= self.lookback as usize => rows
                    .into_iter()
                    .map(|r| BarPayload {
                        ts_ms: r.ts_ms,
                        open: r.open,
                        high: r.high,
                        low: r.low,
                        close: r.close,
                        volume: r.volume,
                    })
                    .collect::<Vec<_>>(),
                Ok(_) => {
                    tracing::warn!(
                        channel = "model_strategy",
                        instrument = %context.instrument,
                        "insufficient bars for model lookback; skipping"
                    );
                    return Ok(None);
                }
                Err(e) => {
                    tracing::error!(channel = "model_strategy", err = %e, "db error reading bars");
                    return Ok(None);
                }
            }
        } else {
            let close = match context.last_bar_close {
                Some(value) => value,
                None => return Ok(None),
            };
            let bars = (0..self.lookback)
                .map(|i| BarPayload {
                    ts_ms: context.ts_ms - (self.lookback - i) * 86_400_000,
                    open: close,
                    high: close,
                    low: close,
                    close,
                    volume: 0.0,
                })
                .collect::<Vec<_>>();
            return Ok(Some((bars, close)));
        };

        let limit_price = match context.last_bar_close {
            Some(value) => value,
            None => return Ok(None),
        };
        Ok(Some((bars, limit_price)))
    }

    async fn request_prediction(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<(PredictResponse, f64)>, String> {
        let Some((bars, limit_price)) = self.load_bars(context).await? else {
            return Ok(None);
        };

        let symbol = context.instrument.symbol.as_str();
        let req = PredictRequest {
            symbol,
            model_type: &self.model_type,
            bars,
        };

        let url = format!("{}/predict", self.service_url);
        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .map_err(|_| "model_unreachable".to_string())?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            if let Ok(parsed) = serde_json::from_str::<ServiceError>(&body) {
                return Err(normalize_model_error_code(
                    Some(parsed.detail.error_code.as_str()),
                    "model_service_error",
                ));
            }
            return Err(normalize_model_error_code(None, "model_service_error"));
        }

        let pred: PredictResponse = resp
            .json()
            .await
            .map_err(|_| "response_parse_failed".to_string())?;
        Ok(Some((pred, limit_price)))
    }

    async fn evaluate_with_signal(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<Signal>, String> {
        let Some((pred, limit_price)) = self.request_prediction(context).await? else {
            return Ok(None);
        };

        if pred.score > self.buy_threshold {
            Ok(Some(Signal {
                strategy_id: format!("model_{}", self.model_type),
                instrument: context.instrument.clone(),
                instrument_db_id: context.instrument_db_id,
                side: Side::Buy,
                qty: 1.0,
                limit_price,
                ts_ms: context.ts_ms,
            }))
        } else if pred.score < self.sell_threshold {
            Ok(Some(Signal {
                strategy_id: format!("model_{}", self.model_type),
                instrument: context.instrument.clone(),
                instrument_db_id: context.instrument_db_id,
                side: Side::Sell,
                qty: 1.0,
                limit_price,
                ts_ms: context.ts_ms,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn evaluate_candidate(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        let Some((prediction, _limit_price)) = self.request_prediction(context).await? else {
            return Ok(None);
        };
        Ok(Some(ScoredCandidate {
            symbol: context.instrument.symbol.clone(),
            score: prediction.score,
            confidence: prediction.confidence,
        }))
    }
}

#[async_trait]
impl Strategy for LstmStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        match self.evaluate_with_signal(context).await {
            Ok(signal) => signal,
            Err(error) => {
                tracing::warn!(channel = "model_strategy", error = %error, "model-service failure");
                None
            }
        }
    }

    async fn evaluate_candidate(
        &self,
        context: &StrategyContext,
    ) -> Result<Option<ScoredCandidate>, String> {
        LstmStrategy::evaluate_candidate(self, context).await
    }
}

pub type ModelStrategy = LstmStrategy;

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Venue};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_strategy(url: &str) -> LstmStrategy {
        LstmStrategy {
            client: reqwest::Client::new(),
            service_url: url.to_string(),
            model_type: "alstm".to_string(),
            lookback: 3,
            buy_threshold: 0.6,
            sell_threshold: -0.6,
            db: None,
            data_source_id: "test".to_string(),
        }
    }

    fn make_context() -> crate::strategy::StrategyContext {
        crate::strategy::StrategyContext {
            instrument: InstrumentId::new(Venue::UsEquity, "AAPL"),
            instrument_db_id: 1,
            last_bar_close: Some(180.0),
            ts_ms: 1_700_000_000_000,
        }
    }

    #[tokio::test]
    async fn buy_signal_when_score_above_threshold() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "score": 0.75, "side": "buy", "confidence": 0.75
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri());
        let signal = strategy.evaluate(&make_context()).await;
        assert!(signal.is_some());
        assert_eq!(signal.unwrap().side, Side::Buy);
    }

    #[tokio::test]
    async fn sell_signal_when_score_below_threshold() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "score": -0.75, "side": "sell", "confidence": 0.75
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri());
        let signal = strategy.evaluate(&make_context()).await;
        assert!(signal.is_some());
        assert_eq!(signal.unwrap().side, Side::Sell);
    }

    #[tokio::test]
    async fn no_signal_when_hold() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "score": 0.1, "side": "hold", "confidence": 0.1
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri());
        assert!(strategy.evaluate(&make_context()).await.is_none());
    }

    #[tokio::test]
    async fn no_signal_when_service_unreachable() {
        let strategy = make_strategy("http://127.0.0.1:19999");
        // Service unreachable → None (no crash, no panic)
        assert!(strategy.evaluate(&make_context()).await.is_none());
    }

    #[tokio::test]
    async fn evaluate_candidate_maps_structured_model_failure_to_reason_code() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
                "detail": {
                    "error_code": "model_not_found",
                    "message": "please train first"
                }
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri());
        let result = strategy.evaluate_candidate(&make_context()).await;
        assert_eq!(result.unwrap_err(), "model_not_found");
    }

    #[tokio::test]
    async fn evaluate_candidate_maps_transport_failure_to_model_unreachable() {
        let strategy = make_strategy("http://127.0.0.1:19999");
        let result = strategy.evaluate_candidate(&make_context()).await;
        assert_eq!(result.unwrap_err(), "model_unreachable");
    }

    #[tokio::test]
    async fn structured_service_failure_reports_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/predict"))
            .respond_with(ResponseTemplate::new(503).set_body_json(serde_json::json!({
                "detail": {
                    "error_code": "model_not_found",
                    "message": "please train first"
                }
            })))
            .mount(&server)
            .await;

        let strategy = make_strategy(&server.uri());
        let result = strategy.evaluate_with_signal(&make_context()).await;
        assert!(
            result.is_err(),
            "expected structured failure to be reported"
        );
        let err = result.unwrap_err();
        assert_eq!(err, "model_not_found");
    }
}
