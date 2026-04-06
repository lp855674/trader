//! K-line (candlestick) data source
//!
//! Provides kline aggregation, gap detection, and resampling functionality.

use std::collections::HashMap;

use tokio::sync::mpsc;
use tracing::{debug, warn};

/// K-line (candlestick) data structure
#[derive(Debug, Clone)]
pub struct Kline {
    /// Instrument ID
    pub instrument_id: String,
    /// Time in milliseconds
    pub timestamp_ms: i64,
    /// Open price
    pub open: f64,
    /// High price
    pub high: f64,
    /// Low price
    pub low: f64,
    /// Close price
    pub close: f64,
    /// Volume
    pub volume: f64,
    /// Optional turnover
    pub turnover: Option<f64>,
}

impl Kline {
    pub fn new(
        instrument_id: String,
        timestamp_ms: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Self {
        Self {
            instrument_id,
            timestamp_ms,
            open,
            high,
            low,
            close,
            volume,
            turnover: None,
        }
    }

    pub fn with_turnover(mut self, turnover: f64) -> Self {
        self.turnover = Some(turnover);
        self
    }

    pub fn is_bullish(&self) -> bool {
        self.close >= self.open
    }

    pub fn is_bearish(&self) -> bool {
        self.close < self.open
    }

    pub fn body_size(&self) -> f64 {
        let body = if self.is_bullish() {
            self.close - self.open
        } else {
            self.open - self.close
        };
        body.abs()
    }

    pub fn upper_shadow(&self) -> f64 {
        let high = if self.high > self.close && self.high > self.open {
            self.high
        } else {
            self.close.max(self.open)
        };
        high - high.min(self.close.max(self.open))
    }

    pub fn lower_shadow(&self) -> f64 {
        let low = if self.low < self.close && self.low < self.open {
            self.low
        } else {
            self.close.min(self.open)
        };
        low.abs() - low
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Granularity {
    Tick,
    Minute(i64),
    Hour(i64),
    Day,
    Custom(i64), // milliseconds
}

impl Granularity {
    pub fn to_interval_ms(&self) -> i64 {
        match self {
            Granularity::Tick => 1,
            Granularity::Minute(n) => *n as i64 * 60_000,
            Granularity::Hour(n) => *n as i64 * 3_600_000,
            Granularity::Day => 86_400_000,
            Granularity::Custom(ms) => *ms,
        }
    }

    pub fn from_interval_ms(interval_ms: i64) -> Self {
        if interval_ms == 1 {
            Granularity::Tick
        } else if interval_ms % 60_000 == 0 {
            Granularity::Minute(interval_ms / 60_000)
        } else if interval_ms % 3_600_000 == 0 {
            Granularity::Hour(interval_ms / 3_600_000)
        } else {
            Granularity::Custom(interval_ms)
        }
    }

    pub fn start_of_interval(&self, timestamp_ms: i64) -> i64 {
        let interval = self.to_interval_ms();
        (timestamp_ms / interval) * interval
    }
}

/// K-line source trait
pub trait KlineSource: Send + Sync {
    /// Get kline data for a specific instrument and time range
    fn get_klines(
        &self,
        instrument_id: &str,
        granularity: Granularity,
        start_ts: i64,
        end_ts: i64,
    ) -> Vec<Kline>;

    /// Subscribe to kline updates
    fn subscribe(
        &self,
        instrument_id: String,
        granularity: Granularity,
    ) -> Vec<mpsc::Receiver<Kline>>;

    /// Check if data exists for a given instrument and granularity
    fn has_data(&self, instrument_id: &str, granularity: Granularity) -> bool;

    /// Get the latest timestamp for a given instrument and granularity
    fn latest_timestamp(&self, instrument_id: &str, granularity: Granularity) -> Option<i64>;
}

/// K-line aggregator for real-time aggregation
pub struct KlineAggregator {
    current_kline: Option<Kline>,
    last_tick_ts: i64,
    interval: Granularity,
}

impl KlineAggregator {
    pub fn new(_instrument_id: String, interval: Granularity) -> Self {
        Self {
            current_kline: None,
            last_tick_ts: 0,
            interval,
        }
    }

    pub fn update(&mut self, tick: &Kline) {
        let current_ts = tick.timestamp_ms;
        let bucket_start = self.interval.start_of_interval(current_ts);

        let roll = match &self.current_kline {
            None => true,
            Some(k) => self.interval.start_of_interval(k.timestamp_ms) != bucket_start,
        };

        if roll {
            if let Some(mut closed) = self.current_kline.take() {
                closed.close = tick.open;
                closed.high = closed.high.max(tick.high);
                closed.low = closed.low.min(tick.low);
                closed.volume += tick.volume;
                if let Some(turnover) = tick.turnover {
                    closed.turnover = Some(closed.turnover.unwrap_or(0.0) + turnover);
                }
                debug!("Kline closed: {:?}", closed);
            }

            self.current_kline = Some(Kline::new(
                tick.instrument_id.clone(),
                bucket_start,
                tick.open,
                tick.high,
                tick.low,
                tick.close,
                tick.volume,
            ));
            self.last_tick_ts = current_ts;
            return;
        }

        if let Some(ref mut kline) = self.current_kline {
            kline.high = kline.high.max(tick.high);
            kline.low = kline.low.min(tick.low);
            kline.close = tick.close;
            kline.volume += tick.volume;
            if let Some(turnover) = tick.turnover {
                kline.turnover = Some(kline.turnover.unwrap_or(0.0) + turnover);
            }
            self.last_tick_ts = current_ts;
        }
    }

    pub fn get_current(&self) -> Option<Kline> {
        self.current_kline.clone()
    }

    pub fn reset(&mut self) {
        self.current_kline = None;
        self.last_tick_ts = 0;
    }
}

/// Gap detector for kline data
pub struct GapDetector {
    last_timestamp: Option<i64>,
    tolerance_ms: i64,
}

impl GapDetector {
    pub fn new(tolerance_ms: i64) -> Self {
        Self {
            last_timestamp: None,
            tolerance_ms,
        }
    }

    pub fn check_gap(&mut self, timestamp_ms: i64) -> bool {
        if let Some(last) = self.last_timestamp {
            let gap = timestamp_ms - last;
            if gap > self.tolerance_ms {
                warn!(
                    "Gap detected: {}ms > tolerance {}ms",
                    gap, self.tolerance_ms
                );
                return true;
            }
        }
        self.last_timestamp = Some(timestamp_ms);
        false
    }
}

/// K-line source implementation with memory storage
pub struct InMemoryKlineSource {
    storage: HashMap<(String, Granularity), Vec<Kline>>,
    aggregators: HashMap<(String, Granularity), KlineAggregator>,
    gap_detectors: HashMap<(String, Granularity), GapDetector>,
}

impl InMemoryKlineSource {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
            aggregators: HashMap::new(),
            gap_detectors: HashMap::new(),
        }
    }

    pub fn add_kline(&mut self, kline: Kline) {
        let key = (kline.instrument_id.clone(), Granularity::Minute(1));

        // Get or create aggregator
        if !self.aggregators.contains_key(&key) {
            self.aggregators.insert(
                key.clone(),
                KlineAggregator::new(kline.instrument_id.clone(), Granularity::Minute(1)),
            );
        }

        let aggregator = self.aggregators.get_mut(&key).unwrap();
        aggregator.update(&kline);

        // Store in memory
        self.storage.entry(key).or_insert_with(Vec::new).push(kline);
    }

    fn get_key(&self, instrument_id: &str, granularity: Granularity) -> (String, Granularity) {
        (instrument_id.to_string(), granularity)
    }

    pub fn get_klines(
        &self,
        instrument_id: &str,
        granularity: Granularity,
        start_ts: i64,
        end_ts: i64,
    ) -> Vec<Kline> {
        let key = self.get_key(instrument_id, granularity);

        if let Some(klines) = self.storage.get(&key) {
            klines
                .iter()
                .filter(|k| k.timestamp_ms >= start_ts && k.timestamp_ms <= end_ts)
                .cloned()
                .collect()
        } else {
            vec![]
        }
    }

    pub fn has_data(&self, instrument_id: &str, granularity: Granularity) -> bool {
        let key = self.get_key(instrument_id, granularity);
        self.storage.contains_key(&key)
    }

    pub fn latest_timestamp(&self, instrument_id: &str, granularity: Granularity) -> Option<i64> {
        let key = self.get_key(instrument_id, granularity);
        self.storage
            .get(&key)
            .and_then(|klines| klines.iter().map(|k| k.timestamp_ms).max())
    }

    pub fn get_latest_kline(&self, instrument_id: &str, granularity: Granularity) -> Option<Kline> {
        let key = self.get_key(instrument_id, granularity);
        self.storage.get(&key).and_then(|klines| {
            klines
                .iter()
                .rfind(|k| {
                    let interval = granularity.to_interval_ms();
                    k.timestamp_ms % interval == 0
                })
                .cloned()
        })
    }

    pub fn check_gaps(
        &self,
        instrument_id: &str,
        granularity: Granularity,
        tolerance_ms: i64,
    ) -> Vec<i64> {
        let key = self.get_key(instrument_id, granularity);

        if let Some(klines) = self.storage.get(&key) {
            let mut gaps = Vec::new();
            let mut last_ts = None;

            for kline in klines {
                if let Some(prev_ts) = last_ts {
                    let gap = kline.timestamp_ms - prev_ts;
                    if gap > tolerance_ms {
                        gaps.push(prev_ts);
                    }
                }
                last_ts = Some(kline.timestamp_ms);
            }

            gaps
        } else {
            vec![]
        }
    }
}

impl KlineSource for InMemoryKlineSource {
    fn get_klines(
        &self,
        instrument_id: &str,
        granularity: Granularity,
        start_ts: i64,
        end_ts: i64,
    ) -> Vec<Kline> {
        self.get_klines(instrument_id, granularity, start_ts, end_ts)
    }

    fn subscribe(
        &self,
        _instrument_id: String,
        _granularity: Granularity,
    ) -> Vec<mpsc::Receiver<Kline>> {
        vec![] // Not implemented for memory source
    }

    fn has_data(&self, instrument_id: &str, granularity: Granularity) -> bool {
        self.has_data(instrument_id, granularity)
    }

    fn latest_timestamp(&self, instrument_id: &str, granularity: Granularity) -> Option<i64> {
        self.latest_timestamp(instrument_id, granularity)
    }
}

/// Resampler for converting tick data to klines
pub struct Resampler {
    aggregator: KlineAggregator,
    gap_detector: GapDetector,
    tolerance_ms: i64,
}

impl Resampler {
    pub fn new(instrument_id: String, interval: Granularity) -> Self {
        Self {
            aggregator: KlineAggregator::new(instrument_id, interval),
            gap_detector: GapDetector::new(5000), // 5 second tolerance
            tolerance_ms: 5000,
        }
    }

    pub fn update(&mut self, tick: &Kline) {
        // Check for gaps
        self.gap_detector.check_gap(tick.timestamp_ms);

        // Aggregate into kline
        self.aggregator.update(tick);

        // Get current kline if available
        if let Some(current) = self.aggregator.get_current() {
            debug!("Aggregated kline: {:?}", current);
        }
    }

    pub fn get_current_kline(&self) -> Option<Kline> {
        self.aggregator.get_current()
    }

    pub fn reset(&mut self) {
        self.aggregator.reset();
        self.gap_detector.last_timestamp = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kline_aggregation() {
        let mut aggregator = KlineAggregator::new("BTC/USDT".to_string(), Granularity::Minute(1));

        // Add ticks
        aggregator.update(&Kline::new(
            "BTC/USDT".to_string(),
            1712345600000, // 10:00:00
            100.0,
            105.0,
            99.0,
            102.0,
            100.0,
        ));

        aggregator.update(&Kline::new(
            "BTC/USDT".to_string(),
            1712345610000, // 10:00:10
            102.5,
            103.0,
            101.0,
            103.0,
            50.0,
        ));

        let current = aggregator.get_current().unwrap();
        assert_eq!(current.open, 100.0);
        assert_eq!(current.high, 105.0);
        assert_eq!(current.low, 99.0);
        assert_eq!(current.close, 103.0);
        assert_eq!(current.volume, 150.0);
    }

    #[test]
    fn test_gap_detector() {
        let mut detector = GapDetector::new(5000); // 5s tolerance

        assert!(!detector.check_gap(1712345600000));
        assert!(!detector.check_gap(1712345605000)); // 与上次间隔 5s，等于容差，不算缺口
        assert!(detector.check_gap(1712345611000)); // 与上次间隔 6s，超过容差
    }

    #[test]
    fn test_resampler() {
        let mut resampler = Resampler::new("BTC/USDT".to_string(), Granularity::Minute(1));

        resampler.update(&Kline::new(
            "BTC/USDT".to_string(),
            1712345600000,
            100.0,
            105.0,
            99.0,
            102.0,
            100.0,
        ));

        let current = resampler.get_current_kline().unwrap();
        assert_eq!(current.close, 102.0);
    }
}
