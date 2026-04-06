use std::collections::HashMap;
use domain::NormalizedBar;

// ── FillStrategy ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum FillStrategy {
    ForwardFill,
    BackwardFill,
    LinearInterpolation,
    ZeroFill,
    CustomValue(f64),
}

// ── GapSpec ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct GapSpec {
    pub instrument: String,
    pub start_ts_ms: i64,
    pub end_ts_ms: i64,
    pub gap_ms: u64,
}

// ── TimeAligner ───────────────────────────────────────────────────────────────

pub struct TimeAligner;

impl TimeAligner {
    pub fn align(
        bars: &[NormalizedBar],
        interval_ms: u64,
        strategy: FillStrategy,
    ) -> Vec<NormalizedBar> {
        if bars.is_empty() {
            return Vec::new();
        }

        let start = bars[0].ts_ms;
        let end = bars[bars.len() - 1].ts_ms;

        // Build index of existing bars by ts_ms
        let existing: HashMap<i64, &NormalizedBar> =
            bars.iter().map(|b| (b.ts_ms, b)).collect();

        let mut result = Vec::new();
        let mut ts = start;
        while ts <= end {
            if let Some(&bar) = existing.get(&ts) {
                result.push(bar.clone());
            } else {
                // Need to fill
                let filled = match &strategy {
                    FillStrategy::ForwardFill => {
                        // Find previous bar
                        let prev = result.last().cloned();
                        if let Some(prev_bar) = prev {
                            NormalizedBar { ts_ms: ts, volume: 0.0, ..prev_bar }
                        } else {
                            make_zero_bar(ts)
                        }
                    }
                    FillStrategy::BackwardFill => {
                        // Find next bar
                        let next = bars.iter().find(|b| b.ts_ms > ts).cloned();
                        if let Some(next_bar) = next {
                            NormalizedBar { ts_ms: ts, volume: 0.0, ..next_bar }
                        } else {
                            make_zero_bar(ts)
                        }
                    }
                    FillStrategy::LinearInterpolation => {
                        // Find prev and next
                        let prev = result.last().cloned();
                        let next = bars.iter().find(|b| b.ts_ms > ts).cloned();
                        if let (Some(p), Some(n)) = (prev, next) {
                            let span = (n.ts_ms - p.ts_ms) as f64;
                            let t = (ts - p.ts_ms) as f64 / span;
                            let price = p.close + t * (n.close - p.close);
                            NormalizedBar {
                                ts_ms: ts,
                                open: price,
                                high: price,
                                low: price,
                                close: price,
                                volume: 0.0,
                            }
                        } else {
                            make_zero_bar(ts)
                        }
                    }
                    FillStrategy::ZeroFill => make_zero_bar(ts),
                    FillStrategy::CustomValue(v) => NormalizedBar {
                        ts_ms: ts,
                        open: *v,
                        high: *v,
                        low: *v,
                        close: *v,
                        volume: 0.0,
                    },
                };
                result.push(filled);
            }
            ts += interval_ms as i64;
        }
        result
    }

    pub fn detect_gaps(bars: &[NormalizedBar], expected_interval_ms: u64) -> Vec<GapSpec> {
        let mut gaps = Vec::new();
        for i in 1..bars.len() {
            let actual = (bars[i].ts_ms - bars[i - 1].ts_ms) as u64;
            if actual > expected_interval_ms {
                gaps.push(GapSpec {
                    instrument: String::new(),
                    start_ts_ms: bars[i - 1].ts_ms,
                    end_ts_ms: bars[i].ts_ms,
                    gap_ms: actual,
                });
            }
        }
        gaps
    }

    pub fn multi_align(
        series: &HashMap<String, Vec<NormalizedBar>>,
        interval_ms: u64,
        strategy: FillStrategy,
    ) -> HashMap<String, Vec<NormalizedBar>> {
        series
            .iter()
            .map(|(k, v)| (k.clone(), Self::align(v, interval_ms, strategy.clone())))
            .collect()
    }
}

fn make_zero_bar(ts_ms: i64) -> NormalizedBar {
    NormalizedBar {
        ts_ms,
        open: 0.0,
        high: 0.0,
        low: 0.0,
        close: 0.0,
        volume: 0.0,
    }
}

// ── ResampleAggregator ────────────────────────────────────────────────────────

pub struct ResampleAggregator;

impl ResampleAggregator {
    /// Downsample bars to a larger interval by aggregating OHLCV.
    pub fn downsample(bars: &[NormalizedBar], target_ms: u64) -> Vec<NormalizedBar> {
        if bars.is_empty() || target_ms == 0 {
            return Vec::new();
        }

        let start = bars[0].ts_ms;
        let mut result: Vec<NormalizedBar> = Vec::new();
        let mut bucket_start = (start as u64 / target_ms * target_ms) as i64;
        let mut agg: Option<NormalizedBar> = None;

        for bar in bars {
            let bar_bucket = (bar.ts_ms as u64 / target_ms * target_ms) as i64;
            if bar_bucket != bucket_start {
                if let Some(a) = agg.take() {
                    result.push(a);
                }
                bucket_start = bar_bucket;
            }
            agg = Some(match agg.take() {
                None => NormalizedBar {
                    ts_ms: bar_bucket,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                },
                Some(mut a) => {
                    a.high = a.high.max(bar.high);
                    a.low = a.low.min(bar.low);
                    a.close = bar.close;
                    a.volume += bar.volume;
                    a
                }
            });
        }
        if let Some(a) = agg {
            result.push(a);
        }
        result
    }

    /// Upsample bars to a smaller interval using forward fill.
    pub fn upsample_forward_fill(bars: &[NormalizedBar], target_ms: u64) -> Vec<NormalizedBar> {
        if bars.is_empty() || target_ms == 0 {
            return Vec::new();
        }
        let mut result = Vec::new();
        for i in 0..bars.len() {
            result.push(bars[i].clone());
            let next_ts = if i + 1 < bars.len() {
                bars[i + 1].ts_ms
            } else {
                bars[i].ts_ms + target_ms as i64
            };
            let mut ts = bars[i].ts_ms + target_ms as i64;
            while ts < next_ts {
                result.push(NormalizedBar {
                    ts_ms: ts,
                    volume: 0.0,
                    ..bars[i].clone()
                });
                ts += target_ms as i64;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts_ms: i64, close: f64) -> NormalizedBar {
        NormalizedBar {
            ts_ms,
            open: close,
            high: close,
            low: close,
            close,
            volume: 10.0,
        }
    }

    #[test]
    fn forward_fill_creates_missing() {
        let bars = vec![bar(0, 1.0), bar(3000, 2.0)];
        let aligned = TimeAligner::align(&bars, 1000, FillStrategy::ForwardFill);
        assert_eq!(aligned.len(), 4);
        assert_eq!(aligned[0].ts_ms, 0);
        assert_eq!(aligned[1].ts_ms, 1000);
        assert!((aligned[1].close - 1.0).abs() < 1e-9);
        assert_eq!(aligned[1].volume, 0.0);
    }

    #[test]
    fn gap_detection() {
        let bars = vec![bar(0, 1.0), bar(1000, 2.0), bar(5000, 3.0)];
        let gaps = TimeAligner::detect_gaps(&bars, 1000);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].start_ts_ms, 1000);
        assert_eq!(gaps[0].end_ts_ms, 5000);
        assert_eq!(gaps[0].gap_ms, 4000);
    }

    #[test]
    fn linear_interpolation() {
        let bars = vec![bar(0, 0.0), bar(2000, 2.0)];
        let aligned = TimeAligner::align(&bars, 1000, FillStrategy::LinearInterpolation);
        assert_eq!(aligned.len(), 3);
        // middle bar at ts=1000 should have close ~1.0
        assert!((aligned[1].close - 1.0).abs() < 0.01);
    }

    #[test]
    fn downsample_5_to_1() {
        // 5 one-second bars → 1 five-second bar
        let bars: Vec<NormalizedBar> = (0..5)
            .map(|i| NormalizedBar {
                ts_ms: i * 1000,
                open: 1.0,
                high: (i + 1) as f64,
                low: 1.0,
                close: (i + 1) as f64,
                volume: 10.0,
            })
            .collect();
        let downsampled = ResampleAggregator::downsample(&bars, 5000);
        assert_eq!(downsampled.len(), 1);
        assert!((downsampled[0].volume - 50.0).abs() < 1e-9);
        assert!((downsampled[0].high - 5.0).abs() < 1e-9);
    }
}
