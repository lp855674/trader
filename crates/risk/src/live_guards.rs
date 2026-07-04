use rust_decimal::Decimal;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{risk_type}: {reason}")]
pub struct LiveRiskRejection {
    pub risk_type: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyLossGuard {
    daily_loss_limit: Decimal,
}

impl DailyLossGuard {
    pub fn new(daily_loss_limit: Decimal) -> Self {
        Self { daily_loss_limit }
    }

    pub fn check(
        &self,
        day_start_equity: Decimal,
        current_equity: Decimal,
    ) -> Result<(), LiveRiskRejection> {
        let day_loss = day_start_equity - current_equity;
        if day_loss > self.daily_loss_limit {
            return Err(LiveRiskRejection {
                risk_type: "daily_loss_limit",
                reason: format!(
                    "day loss {day_loss} exceeds limit {}",
                    self.daily_loss_limit
                ),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderThrottleGuard {
    max_attempts_per_day: Option<u32>,
    max_failures_per_day: Option<u32>,
}

impl OrderThrottleGuard {
    pub fn new(max_attempts_per_day: Option<u32>, max_failures_per_day: Option<u32>) -> Self {
        Self {
            max_attempts_per_day,
            max_failures_per_day,
        }
    }

    pub fn check_attempts(&self, attempts_today: u32) -> Result<(), LiveRiskRejection> {
        if self
            .max_attempts_per_day
            .is_some_and(|limit| attempts_today >= limit)
        {
            return Err(LiveRiskRejection {
                risk_type: "max_order_attempts",
                reason: format!(
                    "order attempts {attempts_today} reached limit {}",
                    self.max_attempts_per_day.unwrap_or_default()
                ),
            });
        }
        Ok(())
    }

    pub fn check_failures(&self, failures_today: u32) -> Result<(), LiveRiskRejection> {
        if self
            .max_failures_per_day
            .is_some_and(|limit| failures_today >= limit)
        {
            return Err(LiveRiskRejection {
                risk_type: "max_order_failures",
                reason: format!(
                    "order failures {failures_today} reached limit {}",
                    self.max_failures_per_day.unwrap_or_default()
                ),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketDataFreshnessGuard {
    max_market_data_age_ms: u64,
}

impl MarketDataFreshnessGuard {
    pub fn new(max_market_data_age_ms: u64) -> Self {
        Self {
            max_market_data_age_ms,
        }
    }

    pub fn check(&self, latest_ts_ms: i64, now_ts_ms: i64) -> Result<(), LiveRiskRejection> {
        let age_ms = now_ts_ms.saturating_sub(latest_ts_ms);
        if age_ms > self.max_market_data_age_ms as i64 {
            return Err(LiveRiskRejection {
                risk_type: "stale_market_data",
                reason: format!(
                    "market data age {age_ms}ms exceeds limit {}ms",
                    self.max_market_data_age_ms
                ),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceDeviationGuard {
    max_price_deviation_bps: Decimal,
}

impl PriceDeviationGuard {
    pub fn new(max_price_deviation_bps: Decimal) -> Self {
        Self {
            max_price_deviation_bps,
        }
    }

    pub fn check(
        &self,
        order_price: Decimal,
        reference_price: Decimal,
    ) -> Result<(), LiveRiskRejection> {
        if reference_price <= Decimal::ZERO {
            return Err(LiveRiskRejection {
                risk_type: "price_deviation",
                reason: "reference price must be positive".to_string(),
            });
        }
        let deviation_bps =
            ((order_price - reference_price).abs() / reference_price) * Decimal::from(10_000);
        if deviation_bps > self.max_price_deviation_bps {
            return Err(LiveRiskRejection {
                risk_type: "price_deviation",
                reason: format!(
                    "price deviation {deviation_bps}bps exceeds limit {}bps",
                    self.max_price_deviation_bps
                ),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyCircuitBreaker {
    max_consecutive_losses: Option<u32>,
    max_consecutive_errors: Option<u32>,
}

impl StrategyCircuitBreaker {
    pub fn new(max_consecutive_losses: Option<u32>, max_consecutive_errors: Option<u32>) -> Self {
        Self {
            max_consecutive_losses,
            max_consecutive_errors,
        }
    }

    pub fn check(
        &self,
        consecutive_losses: u32,
        consecutive_errors: u32,
    ) -> Result<(), LiveRiskRejection> {
        if self
            .max_consecutive_losses
            .is_some_and(|limit| consecutive_losses >= limit)
        {
            return Err(LiveRiskRejection {
                risk_type: "strategy_loss_circuit_breaker",
                reason: format!(
                    "consecutive losses {consecutive_losses} reached limit {}",
                    self.max_consecutive_losses.unwrap_or_default()
                ),
            });
        }
        if self
            .max_consecutive_errors
            .is_some_and(|limit| consecutive_errors >= limit)
        {
            return Err(LiveRiskRejection {
                risk_type: "strategy_error_circuit_breaker",
                reason: format!(
                    "consecutive errors {consecutive_errors} reached limit {}",
                    self.max_consecutive_errors.unwrap_or_default()
                ),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradingSessionGuard {
    start_minute: u32,
    end_minute: u32,
}

impl TradingSessionGuard {
    pub fn new(start_minute: u32, end_minute: u32) -> Self {
        Self {
            start_minute,
            end_minute,
        }
    }

    pub fn check(&self, is_weekday: bool, minute_of_day: u32) -> Result<(), LiveRiskRejection> {
        let in_session =
            is_weekday && minute_of_day >= self.start_minute && minute_of_day <= self.end_minute;
        if !in_session {
            return Err(LiveRiskRejection {
                risk_type: "trading_session_closed",
                reason: format!(
                    "minute {minute_of_day} is outside configured session {}-{}",
                    self.start_minute, self.end_minute
                ),
            });
        }
        Ok(())
    }
}
