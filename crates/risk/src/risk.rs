#![forbid(unsafe_code)]

mod live_guards;

use portfolio::TargetPosition;
use rust_decimal::Decimal;
use thiserror::Error;
use trader_core::{OrderRequest, OrderSide};

pub use live_guards::{
    DailyLossGuard, LiveRiskRejection, MarketDataFreshnessGuard, OrderThrottleGuard,
    PriceDeviationGuard, StrategyCircuitBreaker, TradingSessionGuard,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RiskError {
    #[error("target quantity exceeds max position")]
    MaxPosition,
    #[error("order quantity exceeds max order quantity")]
    MaxOrderQuantity,
    #[error("order notional exceeds max order notional")]
    MaxOrderNotional,
    #[error("buy order requires more cash than available")]
    InsufficientCash,
    #[error("trading is halted")]
    TradingHalted,
    #[error("portfolio exposure exceeds max exposure")]
    MaxExposure,
    #[error("portfolio drawdown exceeds max drawdown")]
    MaxDrawdown,
    #[error("portfolio leverage exceeds max leverage")]
    MaxLeverage,
    #[error("portfolio margin exceeds max margin")]
    MaxMargin,
    #[error("short selling is disabled")]
    ShortSellingDisabled,
}

pub fn check_max_position(target: &TargetPosition, max_abs_qty: Decimal) -> Result<(), RiskError> {
    if target.target_qty.abs() > max_abs_qty {
        return Err(RiskError::MaxPosition);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskPolicy {
    pub max_order_qty: Decimal,
    pub max_order_notional: Decimal,
    pub min_cash_after_order: Decimal,
}

impl RiskPolicy {
    pub fn new(
        max_order_qty: Decimal,
        max_order_notional: Decimal,
        min_cash_after_order: Decimal,
    ) -> Self {
        Self {
            max_order_qty,
            max_order_notional,
            min_cash_after_order,
        }
    }

    pub fn check_order(
        &self,
        order: &OrderRequest,
        reference_price: Decimal,
        available_cash: Decimal,
        trading_halted: bool,
    ) -> Result<(), RiskError> {
        if trading_halted {
            return Err(RiskError::TradingHalted);
        }
        if order.qty > self.max_order_qty {
            return Err(RiskError::MaxOrderQuantity);
        }
        let notional = order.qty * order.price.unwrap_or(reference_price);
        if notional > self.max_order_notional {
            return Err(RiskError::MaxOrderNotional);
        }
        if order.side == OrderSide::Buy && available_cash - notional < self.min_cash_after_order {
            return Err(RiskError::InsufficientCash);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioRiskState {
    pub equity: Decimal,
    pub peak_equity: Decimal,
    pub gross_exposure: Decimal,
    pub margin_used: Decimal,
    pub trading_halted: bool,
}

impl PortfolioRiskState {
    pub fn new(
        equity: Decimal,
        peak_equity: Decimal,
        gross_exposure: Decimal,
        margin_used: Decimal,
        trading_halted: bool,
    ) -> Self {
        Self {
            equity,
            peak_equity,
            gross_exposure,
            margin_used,
            trading_halted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortfolioRiskPolicy {
    pub max_exposure: Decimal,
    pub max_drawdown: Decimal,
    pub max_leverage: Decimal,
    pub max_margin_used: Decimal,
    pub allow_short: bool,
}

impl PortfolioRiskPolicy {
    pub fn new(
        max_exposure: Decimal,
        max_drawdown: Decimal,
        max_leverage: Decimal,
        max_margin_used: Decimal,
    ) -> Self {
        Self {
            max_exposure,
            max_drawdown,
            max_leverage,
            max_margin_used,
            allow_short: false,
        }
    }

    pub fn with_shorting(mut self, allow_short: bool) -> Self {
        self.allow_short = allow_short;
        self
    }

    pub fn check_portfolio(&self, state: &PortfolioRiskState) -> Result<(), RiskError> {
        if state.trading_halted {
            return Err(RiskError::TradingHalted);
        }
        if state.gross_exposure > self.max_exposure {
            return Err(RiskError::MaxExposure);
        }
        if state.peak_equity > Decimal::ZERO {
            let drawdown = (state.peak_equity - state.equity) / state.peak_equity;
            if drawdown > self.max_drawdown {
                return Err(RiskError::MaxDrawdown);
            }
        }
        if state.equity > Decimal::ZERO {
            let leverage = state.gross_exposure / state.equity;
            if leverage > self.max_leverage {
                return Err(RiskError::MaxLeverage);
            }
        }
        if self.max_margin_used > Decimal::ZERO && state.margin_used > self.max_margin_used {
            return Err(RiskError::MaxMargin);
        }
        Ok(())
    }

    pub fn check_projected_order(
        &self,
        order: &OrderRequest,
        reference_price: Decimal,
        state: &PortfolioRiskState,
    ) -> Result<(), RiskError> {
        let order_notional = order.qty * order.price.unwrap_or(reference_price);
        let projected_exposure = state.gross_exposure + order_notional;
        self.check_portfolio(&PortfolioRiskState {
            gross_exposure: projected_exposure,
            ..state.clone()
        })
    }

    pub fn check_projected_target(
        &self,
        target: &TargetPosition,
        current_qty: Decimal,
        reference_price: Decimal,
        state: &PortfolioRiskState,
    ) -> Result<(), RiskError> {
        if target.target_qty < Decimal::ZERO && !self.allow_short {
            return Err(RiskError::ShortSellingDisabled);
        }
        let current_symbol_exposure = current_qty.abs() * reference_price;
        let target_symbol_exposure = target.target_qty.abs() * reference_price;
        let projected_exposure = (state.gross_exposure - current_symbol_exposure)
            .max(Decimal::ZERO)
            + target_symbol_exposure;
        self.check_portfolio(&PortfolioRiskState {
            gross_exposure: projected_exposure,
            ..state.clone()
        })
    }
}
