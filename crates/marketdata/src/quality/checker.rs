use domain::NormalizedBar;
use crate::core::DataItem;

// ── QualityRule ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum QualityRule {
    PriceInRange { min: f64, max: f64 },
    VolumePosNonZero,
    TimestampAscending,
    NoBidAskCrossing,
    SpreadReasonable { max_bps: f64 },
}

// ── QualityViolation ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityViolation {
    pub rule: String,
    pub ts_ms: i64,
    pub detail: String,
}

// ── QualityChecker ────────────────────────────────────────────────────────────

pub struct QualityChecker {
    pub rules: Vec<QualityRule>,
}

impl QualityChecker {
    pub fn new(rules: Vec<QualityRule>) -> Self {
        Self { rules }
    }

    pub fn check_bars(&self, bars: &[NormalizedBar]) -> Vec<QualityViolation> {
        let mut violations = Vec::new();
        let mut prev_ts: Option<i64> = None;

        for bar in bars {
            for rule in &self.rules {
                match rule {
                    QualityRule::PriceInRange { min, max } => {
                        for (name, price) in &[
                            ("open", bar.open),
                            ("high", bar.high),
                            ("low", bar.low),
                            ("close", bar.close),
                        ] {
                            if *price < *min || *price > *max {
                                violations.push(QualityViolation {
                                    rule: "PriceInRange".to_string(),
                                    ts_ms: bar.ts_ms,
                                    detail: format!(
                                        "{} price {} out of range [{}, {}]",
                                        name, price, min, max
                                    ),
                                });
                            }
                        }
                    }
                    QualityRule::VolumePosNonZero => {
                        if bar.volume <= 0.0 {
                            violations.push(QualityViolation {
                                rule: "VolumePosNonZero".to_string(),
                                ts_ms: bar.ts_ms,
                                detail: format!("volume {} is not positive", bar.volume),
                            });
                        }
                    }
                    QualityRule::TimestampAscending => {
                        if let Some(prev) = prev_ts {
                            if bar.ts_ms <= prev {
                                violations.push(QualityViolation {
                                    rule: "TimestampAscending".to_string(),
                                    ts_ms: bar.ts_ms,
                                    detail: format!(
                                        "ts_ms {} <= previous {}",
                                        bar.ts_ms, prev
                                    ),
                                });
                            }
                        }
                    }
                    // Bar-level rules below don't apply to NoBidAskCrossing / SpreadReasonable
                    QualityRule::NoBidAskCrossing => {}
                    QualityRule::SpreadReasonable { .. } => {}
                }
            }
            prev_ts = Some(bar.ts_ms);
        }
        violations
    }

    pub fn check_items(&self, items: &[DataItem]) -> Vec<QualityViolation> {
        let mut violations = Vec::new();
        let mut prev_ts: Option<i64> = None;

        for item in items {
            match item {
                DataItem::Bar(bar) => {
                    let bar_violations = self.check_bars(std::slice::from_ref(bar));
                    violations.extend(bar_violations);
                }
                DataItem::Tick { ts_ms, bid, ask, .. } => {
                    for rule in &self.rules {
                        match rule {
                            QualityRule::NoBidAskCrossing => {
                                if bid >= ask {
                                    violations.push(QualityViolation {
                                        rule: "NoBidAskCrossing".to_string(),
                                        ts_ms: *ts_ms,
                                        detail: format!(
                                            "bid {} >= ask {}",
                                            bid, ask
                                        ),
                                    });
                                }
                            }
                            QualityRule::SpreadReasonable { max_bps } => {
                                if *ask > 0.0 {
                                    let spread_bps = (ask - bid) / ask * 10_000.0;
                                    if spread_bps > *max_bps {
                                        violations.push(QualityViolation {
                                            rule: "SpreadReasonable".to_string(),
                                            ts_ms: *ts_ms,
                                            detail: format!(
                                                "spread {:.2} bps > max {:.2} bps",
                                                spread_bps, max_bps
                                            ),
                                        });
                                    }
                                }
                            }
                            QualityRule::TimestampAscending => {
                                if let Some(prev) = prev_ts {
                                    if *ts_ms <= prev {
                                        violations.push(QualityViolation {
                                            rule: "TimestampAscending".to_string(),
                                            ts_ms: *ts_ms,
                                            detail: format!(
                                                "ts_ms {} <= previous {}",
                                                ts_ms, prev
                                            ),
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                DataItem::OrderBook { .. } => {}
            }
            prev_ts = Some(item.ts_ms());
        }
        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts_ms: i64, price: f64, volume: f64) -> NormalizedBar {
        NormalizedBar {
            ts_ms,
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
        }
    }

    #[test]
    fn price_out_of_range_detected() {
        let checker = QualityChecker::new(vec![QualityRule::PriceInRange { min: 10.0, max: 100.0 }]);
        let bars = vec![bar(1000, 5.0, 1.0), bar(2000, 50.0, 1.0)];
        let violations = checker.check_bars(&bars);
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.ts_ms == 1000));
    }

    #[test]
    fn volume_zero_detected() {
        let checker = QualityChecker::new(vec![QualityRule::VolumePosNonZero]);
        let bars = vec![bar(1000, 50.0, 0.0)];
        let violations = checker.check_bars(&bars);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn timestamp_descending_detected() {
        let checker = QualityChecker::new(vec![QualityRule::TimestampAscending]);
        let bars = vec![bar(2000, 50.0, 1.0), bar(1000, 50.0, 1.0)];
        let violations = checker.check_bars(&bars);
        assert!(!violations.is_empty());
    }
}
