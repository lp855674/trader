use async_trait::async_trait;
use domain::{Side, Signal};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::strategy::{Strategy, StrategyContext};

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
    #[allow(dead_code)]
    confidence: f64,
}

#[async_trait]
impl Strategy for LstmStrategy {
    async fn evaluate(&self, context: &StrategyContext) -> Option<Signal> {
        let bars: Vec<BarPayload> = if let Some(ref db) = self.db {
            match db::get_recent_bars(db.pool(), context.instrument_db_id, &self.data_source_id, self.lookback).await {
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
                    .collect(),
                Ok(_) => {
                    tracing::warn!(
                        channel = "lstm_strategy",
                        instrument = %context.instrument,
                        "insufficient bars for LSTM lookback; skipping"
                    );
                    return None;
                }
                Err(e) => {
                    tracing::error!(channel = "lstm_strategy", err = %e, "db error reading bars");
                    return None;
                }
            }
        } else {
            // Fallback: fill lookback synthetic bars from last_bar_close (for tests without DB)
            let close = context.last_bar_close?;
            (0..self.lookback)
                .map(|i| BarPayload {
                    ts_ms: context.ts_ms - (self.lookback - i) * 86_400_000,
                    open: close,
                    high: close,
                    low: close,
                    close,
                    volume: 0.0,
                })
                .collect()
        };

        let symbol = context.instrument.symbol.as_str();
        let req = PredictRequest {
            symbol,
            model_type: &self.model_type,
            bars,
        };

        let url = format!("{}/predict", self.service_url);
        let resp = match self.client.post(&url).json(&req).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(channel = "lstm_strategy", err = %e, "lstm-service unreachable");
                return None;
            }
        };

        if !resp.status().is_success() {
            tracing::warn!(
                channel = "lstm_strategy",
                status = %resp.status(),
                "lstm-service returned error"
            );
            return None;
        }

        let pred: PredictResponse = match resp.json().await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(channel = "lstm_strategy", err = %e, "failed to parse predict response");
                return None;
            }
        };

        let limit_price = context.last_bar_close?;

        if pred.score > self.buy_threshold {
            Some(Signal {
                strategy_id: format!("lstm_{}", self.model_type),
                instrument: context.instrument.clone(),
                instrument_db_id: context.instrument_db_id,
                side: Side::Buy,
                qty: 1.0,
                limit_price,
                ts_ms: context.ts_ms,
            })
        } else if pred.score < self.sell_threshold {
            Some(Signal {
                strategy_id: format!("lstm_{}", self.model_type),
                instrument: context.instrument.clone(),
                instrument_db_id: context.instrument_db_id,
                side: Side::Sell,
                qty: 1.0,
                limit_price,
                ts_ms: context.ts_ms,
            })
        } else {
            None
        }
    }
}

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
}
