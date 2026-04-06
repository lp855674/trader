use domain::InstrumentId;
use crate::core::r#trait::Kline;

#[derive(Debug, Clone, PartialEq)]
pub enum TimeGranularity {
    Tick,
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

impl TimeGranularity {
    pub fn to_ms(&self) -> Option<u64> {
        match self {
            TimeGranularity::Tick => None,
            TimeGranularity::Seconds(s) => Some(*s as u64 * 1_000),
            TimeGranularity::Minutes(m) => Some(*m as u64 * 60_000),
            TimeGranularity::Hours(h) => Some(*h as u64 * 3_600_000),
            TimeGranularity::Days(d) => Some(*d as u64 * 86_400_000),
        }
    }
}

// ─── KlineResampler ───────────────────────────────────────────────────────────

pub struct KlineResampler {
    target: TimeGranularity,
    buffer: Option<Kline>,
    period_end_ms: i64,
}

impl KlineResampler {
    pub fn new(target: TimeGranularity) -> Self {
        Self { target, buffer: None, period_end_ms: 0 }
    }

    pub fn push(&mut self, kline: Kline) -> Option<Kline> {
        let period_ms = match self.target.to_ms() {
            Some(ms) => ms as i64,
            None => {
                // Tick granularity — pass through each kline
                return Some(kline);
            }
        };

        match &mut self.buffer {
            None => {
                // Start new period aligned to period boundary
                let period_start = (kline.open_ts_ms / period_ms) * period_ms;
                self.period_end_ms = period_start + period_ms;
                self.buffer = Some(kline);
                None
            }
            Some(buf) => {
                if kline.close_ts_ms <= self.period_end_ms {
                    // Aggregate into buffer
                    buf.high = buf.high.max(kline.high);
                    buf.low = buf.low.min(kline.low);
                    buf.close = kline.close;
                    buf.volume += kline.volume;
                    buf.close_ts_ms = kline.close_ts_ms;

                    if kline.close_ts_ms >= self.period_end_ms {
                        // Period complete
                        let completed = buf.clone();
                        self.buffer = None;
                        Some(completed)
                    } else {
                        None
                    }
                } else {
                    // New period — emit current buffer and start fresh
                    let completed = buf.clone();
                    let period_start = (kline.open_ts_ms / period_ms) * period_ms;
                    self.period_end_ms = period_start + period_ms;
                    self.buffer = Some(kline);
                    Some(completed)
                }
            }
        }
    }
}

// ─── TickToKline ─────────────────────────────────────────────────────────────

pub struct TickToKline {
    granularity_ms: u64,
    current_bar: Option<Kline>,
}

impl TickToKline {
    pub fn new(granularity_ms: u64) -> Self {
        Self { granularity_ms, current_bar: None }
    }

    pub fn push_tick(
        &mut self,
        instrument: InstrumentId,
        ts_ms: i64,
        price: f64,
        volume: f64,
    ) -> Option<Kline> {
        let period = self.granularity_ms as i64;
        let bar_start = (ts_ms / period) * period;
        let bar_end = bar_start + period;

        match &mut self.current_bar {
            None => {
                self.current_bar = Some(Kline {
                    instrument,
                    open_ts_ms: bar_start,
                    close_ts_ms: bar_end,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume,
                });
                None
            }
            Some(bar) => {
                if ts_ms < bar.close_ts_ms {
                    // Same bar
                    bar.high = bar.high.max(price);
                    bar.low = bar.low.min(price);
                    bar.close = price;
                    bar.volume += volume;
                    None
                } else {
                    // New bar — emit old
                    let completed = bar.clone();
                    self.current_bar = Some(Kline {
                        instrument,
                        open_ts_ms: bar_start,
                        close_ts_ms: bar_end,
                        open: price,
                        high: price,
                        low: price,
                        close: price,
                        volume,
                    });
                    Some(completed)
                }
            }
        }
    }
}

// ─── GranularityConverter ─────────────────────────────────────────────────────

pub struct GranularityConverter;

impl GranularityConverter {
    /// Downsample a slice of bars to coarser resolution.
    pub fn downsample(bars: &[Kline], target_ms: u64) -> Vec<Kline> {
        if bars.is_empty() {
            return Vec::new();
        }

        let period = target_ms as i64;
        let mut result: Vec<Kline> = Vec::new();
        let mut current: Option<Kline> = None;

        for bar in bars {
            let bucket = (bar.open_ts_ms / period) * period;

            match &mut current {
                None => {
                    let mut agg = bar.clone();
                    agg.open_ts_ms = bucket;
                    agg.close_ts_ms = bucket + period;
                    current = Some(agg);
                }
                Some(agg) => {
                    let agg_bucket = (agg.open_ts_ms / period) * period;
                    if bucket == agg_bucket {
                        agg.high = agg.high.max(bar.high);
                        agg.low = agg.low.min(bar.low);
                        agg.close = bar.close;
                        agg.volume += bar.volume;
                        agg.close_ts_ms = bucket + period;
                    } else {
                        result.push(agg.clone());
                        let mut new_agg = bar.clone();
                        new_agg.open_ts_ms = bucket;
                        new_agg.close_ts_ms = bucket + period;
                        *agg = new_agg;
                    }
                }
            }
        }

        if let Some(agg) = current {
            result.push(agg);
        }

        result
    }

    /// Fill gaps in a bar series by forward-filling with zero-volume bars.
    pub fn upsample_fill_forward(bars: &[Kline], target_ms: u64) -> Vec<Kline> {
        if bars.is_empty() {
            return Vec::new();
        }

        let period = target_ms as i64;
        let mut result: Vec<Kline> = Vec::new();

        for (i, bar) in bars.iter().enumerate() {
            if i == 0 {
                result.push(bar.clone());
                continue;
            }
            let prev = &bars[i - 1];
            let mut fill_ts = prev.close_ts_ms;
            while fill_ts < bar.open_ts_ms {
                let prev_close = prev.close;
                result.push(Kline {
                    instrument: bar.instrument.clone(),
                    open_ts_ms: fill_ts,
                    close_ts_ms: fill_ts + period,
                    open: prev_close,
                    high: prev_close,
                    low: prev_close,
                    close: prev_close,
                    volume: 0.0,
                });
                fill_ts += period;
            }
            result.push(bar.clone());
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{InstrumentId, Venue};

    fn instrument() -> InstrumentId {
        InstrumentId::new(Venue::Crypto, "BTC")
    }

    fn make_kline(open_ts: i64, close_ts: i64, close: f64, volume: f64) -> Kline {
        Kline {
            instrument: instrument(),
            open_ts_ms: open_ts,
            close_ts_ms: close_ts,
            open: close,
            high: close + 1.0,
            low: close - 1.0,
            close,
            volume,
        }
    }

    #[test]
    fn to_ms_values() {
        assert_eq!(TimeGranularity::Seconds(30).to_ms(), Some(30_000));
        assert_eq!(TimeGranularity::Minutes(5).to_ms(), Some(300_000));
        assert_eq!(TimeGranularity::Hours(1).to_ms(), Some(3_600_000));
        assert_eq!(TimeGranularity::Days(1).to_ms(), Some(86_400_000));
        assert_eq!(TimeGranularity::Tick.to_ms(), None);
    }

    #[test]
    fn resampler_5_minute_bars() {
        let mut resampler = KlineResampler::new(TimeGranularity::Minutes(5));
        let mut completed = Vec::new();

        // 5x1min bars aligned to 5min boundary: 0..60000, 60000..120000, ... 240000..300000
        let bars: Vec<Kline> = (0..5)
            .map(|i| {
                let open_ts = i * 60_000i64;
                let close_ts = open_ts + 60_000;
                make_kline(open_ts, close_ts, 100.0 + i as f64, 10.0)
            })
            .collect();

        for bar in bars {
            if let Some(agg) = resampler.push(bar) {
                completed.push(agg);
            }
        }

        // The 5th bar (240000..300000) has close_ts=300000 == period_end=300000, so it emits
        assert_eq!(completed.len(), 1);
        let agg = &completed[0];
        assert_eq!(agg.volume, 50.0);
        assert!((agg.close - 104.0).abs() < 1e-9);
        assert!((agg.open - 100.0).abs() < 1e-9);
    }

    #[test]
    fn tick_to_kline_assembles() {
        let mut builder = TickToKline::new(60_000);
        let instr = instrument();

        assert!(builder.push_tick(instr.clone(), 0, 100.0, 10.0).is_none());
        assert!(builder.push_tick(instr.clone(), 10_000, 105.0, 5.0).is_none());
        assert!(builder.push_tick(instr.clone(), 59_999, 102.0, 8.0).is_none());

        // Tick at ts=60000 starts new bar — emits bar 0
        let bar = builder.push_tick(instr.clone(), 60_000, 103.0, 2.0);
        assert!(bar.is_some());
        let b = bar.unwrap();
        assert!((b.open - 100.0).abs() < 1e-9);
        assert!((b.high - 105.0).abs() < 1e-9);
        assert!((b.low - 100.0).abs() < 1e-9);
        assert!((b.close - 102.0).abs() < 1e-9);
        assert!((b.volume - 23.0).abs() < 1e-9);
    }

    #[test]
    fn downsample_5_1min_to_5min() {
        let bars: Vec<Kline> = (0..5)
            .map(|i| {
                let open_ts = i * 60_000i64;
                let close_ts = open_ts + 60_000;
                make_kline(open_ts, close_ts, 100.0 + i as f64, 10.0)
            })
            .collect();

        let resampled = GranularityConverter::downsample(&bars, 300_000);
        assert_eq!(resampled.len(), 1);
        assert_eq!(resampled[0].volume, 50.0);
    }

    #[test]
    fn upsample_fill_forward_fills_gaps() {
        let bars = vec![
            make_kline(0, 60_000, 100.0, 10.0),
            make_kline(180_000, 240_000, 105.0, 10.0), // Gap: 60000..180000
        ];
        let filled = GranularityConverter::upsample_fill_forward(&bars, 60_000);
        // Expect: bar at 0, fill at 60000, fill at 120000, bar at 180000
        assert_eq!(filled.len(), 4);
        assert_eq!(filled[1].volume, 0.0);
        assert!((filled[1].close - 100.0).abs() < 1e-9);
    }
}
