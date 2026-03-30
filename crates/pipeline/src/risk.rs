//! 下单前风控：数量与名义金额上限。

use domain::Signal;

/// 从环境变量加载，见 `RiskLimits::from_env`；[`Default`] 与 `from_env` 未设置变量时的默认一致。
#[derive(Debug, Clone, Copy)]
pub struct RiskLimits {
    pub max_order_qty: f64,
    pub max_order_notional: f64,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_order_qty: 100.0,
            max_order_notional: 1_000_000.0,
        }
    }
}

impl RiskLimits {
    /// `QUANTD_MAX_ORDER_QTY`（默认 `100`）、`QUANTD_MAX_ORDER_NOTIONAL`（默认 `1_000_000`）。
    pub fn from_env() -> Self {
        let max_order_qty = std::env::var("QUANTD_MAX_ORDER_QTY")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|v: &f64| v.is_finite() && *v > 0.0)
            .unwrap_or(RiskLimits::default().max_order_qty);
        let max_order_notional = std::env::var("QUANTD_MAX_ORDER_NOTIONAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|v: &f64| v.is_finite() && *v > 0.0)
            .unwrap_or(RiskLimits::default().max_order_notional);
        Self {
            max_order_qty,
            max_order_notional,
        }
    }

    /// 校验策略信号是否通过限额；不通过时返回人类可读原因（写入 `risk_decisions`）。
    pub fn check(&self, signal: &Signal) -> Result<(), String> {
        if !signal.limit_price.is_finite() || signal.limit_price <= 0.0 {
            return Err("limit_price must be finite and positive".to_string());
        }
        if !signal.qty.is_finite() || signal.qty <= 0.0 {
            return Err("qty must be finite and positive".to_string());
        }
        if signal.qty > self.max_order_qty {
            return Err(format!(
                "qty {} exceeds max_order_qty {}",
                signal.qty, self.max_order_qty
            ));
        }
        let notional = signal.qty * signal.limit_price;
        if !notional.is_finite() {
            return Err("notional is not finite".to_string());
        }
        if notional > self.max_order_notional {
            return Err(format!(
                "notional {:.2} exceeds max_order_notional {:.2}",
                notional, self.max_order_notional
            ));
        }
        Ok(())
    }
}
