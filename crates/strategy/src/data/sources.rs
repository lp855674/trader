// Input Source Traits & Mocks
// HistoricalData trait implementations for testing and file-based data sources

use domain::InstrumentId;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::core::{Granularity, Kline, Tick};

/// CSV data parsing error
#[derive(Debug, Error)]
pub enum CsvParseError {
    #[error("Invalid CSV header: expected 7 columns, found {0}")]
    InvalidHeader(usize),

    #[error("Parse error at line {0}: {1}")]
    ParseError(usize, String),

    #[error("File not found: {0}")]
    FileNotFoundError(PathBuf),

    #[error("Empty file")]
    EmptyFile,
}

/// Kline CSV format: timestamp_ms,open,high,low,close,volume,instrument_id
#[derive(Debug, Error)]
pub enum KlineCsvError {
    #[error("Invalid kline data at row {0}")]
    InvalidData(usize),

    #[error("Parse error at line {0}: {1}")]
    ParseError(usize, String),

    #[error("File not found: {0}")]
    FileNotFoundError(PathBuf),

    #[error("Empty file")]
    EmptyFile,

    #[error("Number parse: {0}")]
    ParseNum(#[from] std::num::ParseIntError),

    #[error("Float parse: {0}")]
    ParseFloat(#[from] std::num::ParseFloatError),
}

impl From<CsvParseError> for KlineCsvError {
    fn from(err: CsvParseError) -> Self {
        KlineCsvError::ParseError(0, err.to_string())
    }
}

/// Tick CSV format: timestamp_ms,price,volume,instrument_id
#[derive(Debug, Error)]
pub enum TickCsvError {
    #[error("Invalid tick data at row {0}")]
    InvalidData(usize),

    #[error("Parse error at line {0}: {1}")]
    ParseError(usize, String),

    #[error("File not found: {0}")]
    FileNotFoundError(PathBuf),

    #[error("Empty file")]
    EmptyFile,

    #[error("Number parse: {0}")]
    ParseNum(#[from] std::num::ParseIntError),

    #[error("Float parse: {0}")]
    ParseFloat(#[from] std::num::ParseFloatError),
}

/// Mock data generator for unit testing
/// Generates deterministic synthetic market data based on seed
#[derive(Debug, Clone)]
pub struct MockDataGenerator {
    seed: u64,
    base_price: f64,
    volatility: f64,
    trend: f64,
    tick_interval_ms: u64,
}

impl MockDataGenerator {
    /// Create new generator with deterministic parameters
    pub fn new(seed: u64, base_price: f64, volatility: f64) -> Self {
        Self {
            seed,
            base_price,
            volatility,
            trend: 0.0,
            tick_interval_ms: 100, // 100ms
        }
    }

    /// Set trend (positive = upward trend)
    pub fn with_trend(mut self, trend: f64) -> Self {
        self.trend = trend;
        self
    }

    /// Generate kline data for specified instrument
    pub fn generate_klines(
        &self,
        instrument: &InstrumentId,
        start_ts: i64,
        end_ts: i64,
        granularity: Granularity,
    ) -> Result<Vec<Kline>, KlineCsvError> {
        let mut klines = Vec::new();
        let mut current_ts = start_ts;

        while current_ts <= end_ts {
            // Calculate bar parameters
            let (open, high, low, close, volume) =
                self.generate_bar(current_ts, granularity, instrument.symbol.clone());

            klines.push(Kline {
                instrument: instrument.clone(),
                open_ts_ms: current_ts,
                close_ts_ms: current_ts + self.tick_interval_ms as i64 * 1000,
                open,
                high,
                low,
                close,
                volume,
            });

            // Move to next bar
            current_ts += self.tick_interval_ms as i64 * 1000;
        }

        Ok(klines)
    }

    /// Generate tick data
    pub fn generate_ticks(
        &self,
        instrument: &InstrumentId,
        start_ts: i64,
        end_ts: i64,
    ) -> Result<Vec<Tick>, TickCsvError> {
        let mut ticks = Vec::new();
        let mut current_ts = start_ts;

        while current_ts <= end_ts {
            let price = self.calculate_price(current_ts);
            let volume = self.random_volume();

            ticks.push(Tick {
                instrument: instrument.clone(),
                ts_ms: current_ts,
                bid_price: price * (1.0 - 0.0001),
                ask_price: price * (1.0 + 0.0001),
                last_price: price,
                volume,
            });

            current_ts += self.tick_interval_ms as i64;
        }

        Ok(ticks)
    }

    /// Generate single bar data
    fn generate_bar(
        &self,
        ts: i64,
        _granularity: Granularity,
        _instrument: String,
    ) -> (f64, f64, f64, f64, f64) {
        let volatility = self.volatility * 0.01; // 1% max volatility
        let trend_factor = self.trend * 0.0001; // Small trend per bar

        // Base price with some randomness
        let base = self.base_price + (ts as f64) * trend_factor;
        let noise = (self.seed as f64 + ts as f64) * volatility;

        let open = base + noise;
        let trend = if trend_factor > 0.0 { 1.0 } else { -1.0 };
        let close_change = noise * trend;
        let close = open + close_change;

        let high = open.max(close) + noise.abs() * 0.5;
        let low = open.min(close) - noise.abs() * 0.5;
        let volume = 1000000.0 + (self.seed + ts as u64) as f64 * 1000.0;

        (open, high, low, close, volume)
    }

    /// Calculate price at specific timestamp
    fn calculate_price(&self, ts: i64) -> f64 {
        let base = self.base_price + (ts as f64) * self.trend * 0.0001;
        let noise = ((self.seed + ts as u64) as f64) * self.volatility * 0.01;
        base + noise
    }

    /// Generate random volume
    fn random_volume(&self) -> f64 {
        let base = 1000000.0;
        let variation = ((self.seed + self.tick_interval_ms as u64) as f64 % 1000.0) * 10000.0;
        base + variation
    }
}

/// CSV Parser for historical data files
#[derive(Debug, Clone)]
pub struct CsvParser {
    data_dir: PathBuf,
    file_extensions: Vec<String>,
}

impl Default for CsvParser {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            file_extensions: vec!["csv".to_string()],
        }
    }
}

impl CsvParser {
    /// Create new CSV parser with custom data directory
    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            file_extensions: vec!["csv".to_string()],
        }
    }

    /// Add file extensions to search for
    pub fn with_extensions(&mut self, extensions: Vec<String>) {
        self.file_extensions = extensions;
    }

    /// Find all CSV files matching instrument pattern
    pub fn find_files(&self, instrument: &InstrumentId) -> Result<Vec<PathBuf>, CsvParseError> {
        let _pattern = format!(
            "{}/{}/*.{}",
            self.data_dir.display(),
            instrument.symbol,
            self.file_extensions[0]
        );

        let entries = fs::read_dir(&self.data_dir)
            .map_err(|_e| CsvParseError::FileNotFoundError(self.data_dir.clone()))?;

        let mut files = Vec::new();
        for entry in entries {
            let path = entry.map(|e| e.path()).unwrap_or_default();
            if path
                .extension()
                .map(|ext: &std::ffi::OsStr| {
                    self.file_extensions
                        .contains(&ext.to_string_lossy().to_string())
                })
                .unwrap_or(false)
                && path
                    .file_name()
                    .map(|name: &std::ffi::OsStr| {
                        name.to_string_lossy().contains(&instrument.symbol)
                    })
                    .unwrap_or(false)
            {
                files.push(path);
            }
        }

        Ok(files)
    }

    /// Parse kline CSV file
    pub fn parse_klines(&self, path: &Path) -> Result<Vec<Kline>, KlineCsvError> {
        if !path.exists() {
            return Err(KlineCsvError::FileNotFoundError(path.to_path_buf()));
        }

        let file =
            File::open(path).map_err(|_e| KlineCsvError::FileNotFoundError(path.to_path_buf()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Skip header
        let _header = lines
            .next()
            .ok_or(KlineCsvError::EmptyFile)?
            .map_err(|e| KlineCsvError::ParseError(0, e.to_string()))?;

        let mut klines = Vec::new();
        let mut row_num = 1;

        for line in lines {
            row_num += 1;
            let line = line.map_err(|e| KlineCsvError::ParseError(row_num, e.to_string()))?;
            let parts: Vec<&str> = line.trim().split(',').collect();

            if parts.len() != 7 {
                return Err(KlineCsvError::InvalidData(row_num));
            }

            let (timestamp, open, high, low, close, volume, instrument_id) = (
                parts[0].parse::<i64>()?,
                parts[1].parse()?,
                parts[2].parse()?,
                parts[3].parse()?,
                parts[4].parse()?,
                parts[5].parse()?,
                parts[6].to_string(),
            );

            klines.push(Kline {
                instrument: InstrumentId::new(domain::Venue::Crypto, instrument_id),
                open_ts_ms: timestamp,
                close_ts_ms: timestamp + 60000, // Assume 1 minute bars
                open,
                high,
                low,
                close,
                volume,
            });
        }

        Ok(klines)
    }

    /// Parse tick CSV file
    pub fn parse_ticks(&self, path: &Path) -> Result<Vec<Tick>, TickCsvError> {
        if !path.exists() {
            return Err(TickCsvError::FileNotFoundError(path.to_path_buf()));
        }

        let file =
            File::open(path).map_err(|_e| TickCsvError::FileNotFoundError(path.to_path_buf()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Skip header
        let _header = lines
            .next()
            .ok_or(TickCsvError::EmptyFile)?
            .map_err(|e| TickCsvError::ParseError(0, e.to_string()))?;

        let mut ticks = Vec::new();
        let mut row_num = 1;

        for line in lines {
            row_num += 1;
            let line = line.map_err(|e| TickCsvError::ParseError(row_num, e.to_string()))?;
            let parts: Vec<&str> = line.trim().split(',').collect();

            if parts.len() != 4 {
                return Err(TickCsvError::InvalidData(row_num));
            }

            let (timestamp, price, volume, instrument_id) = (
                parts[0].parse::<i64>()?,
                parts[1].parse()?,
                parts[2].parse()?,
                parts[3].to_string(),
            );

            ticks.push(Tick {
                instrument: InstrumentId::new(domain::Venue::Crypto, instrument_id),
                ts_ms: timestamp,
                bid_price: price * 0.9999,
                ask_price: price * 1.0001,
                last_price: price,
                volume,
            });
        }

        Ok(ticks)
    }

    /// Parse all kline files for instrument
    pub fn parse_all_klines(&self, instrument: &InstrumentId) -> Result<Vec<Kline>, KlineCsvError> {
        let files = self.find_files(instrument)?;
        let mut all_klines = Vec::new();

        for file_path in files {
            let klines = self.parse_klines(&file_path)?;
            all_klines.extend(klines);
        }

        // Sort by timestamp
        all_klines.sort_by_key(|k| k.open_ts_ms);
        Ok(all_klines)
    }
}

/// Time-series data alignment utilities
/// Handles resampling, gap filling, and time-series operations
#[derive(Debug, Clone)]
pub struct TimeSeriesAligner {
    default_gap_fill: GapFillStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapFillStrategy {
    Forward,  // Forward fill from last known value
    Backward, // Backward fill from first known value
    Linear,   // Linear interpolation
    Zero,     // Fill with zero
    Hold,     // Hold last value (default)
}

impl Default for TimeSeriesAligner {
    fn default() -> Self {
        Self {
            default_gap_fill: GapFillStrategy::Hold,
        }
    }
}

impl TimeSeriesAligner {
    /// Create new aligner with custom gap fill strategy
    pub fn with_gap_fill(mut self, strategy: GapFillStrategy) -> Self {
        self.default_gap_fill = strategy;
        self
    }

    /// Resample klines to different granularity
    pub fn resample(
        &self,
        klines: Vec<Kline>,
        from_granularity: Granularity,
        to_granularity: Granularity,
    ) -> Result<Vec<Kline>, ResampleError> {
        if from_granularity == to_granularity {
            return Ok(klines);
        }

        // Simple resampling: aggregate bars
        let mut aggregated = Vec::new();

        for kline in klines {
            // In production, implement proper time-based aggregation
            // For now, just pass through
            aggregated.push(kline);
        }

        Ok(aggregated)
    }

    /// Align and fill gaps in time series
    pub fn align_and_fill(
        &self,
        data: Vec<(i64, f64)>, // (timestamp, value)
    ) -> Vec<(i64, f64)> {
        if data.is_empty() {
            return Vec::new();
        }

        // Sort by timestamp
        let mut sorted = data;
        sorted.sort_by_key(|(ts, _)| *ts);

        // Remove duplicates
        let mut unique: Vec<(i64, f64)> = Vec::new();
        for (ts, val) in sorted {
            if unique.is_empty() || unique.last().unwrap().0 != ts {
                unique.push((ts, val));
            }
        }

        // Fill gaps based on strategy
        self.fill_gaps(unique)
    }

    /// Fill gaps in time series
    fn fill_gaps(&self, data: Vec<(i64, f64)>) -> Vec<(i64, f64)> {
        if data.len() <= 1 {
            return data;
        }

        let strategy = self.default_gap_fill;
        let mut result: Vec<(i64, f64)> = Vec::new();

        match strategy {
            GapFillStrategy::Forward | GapFillStrategy::Hold => {
                // 仅排序去重后的已知点：保留各自取值；扩展网格上的前向填充可在此后单独实现
                for (ts, val) in data {
                    result.push((ts, val));
                }
            }
            GapFillStrategy::Backward => {
                let first_value = data.first().unwrap().1;
                for (ts, _) in data {
                    result.push((ts, first_value));
                }
            }
            GapFillStrategy::Linear => {
                result.push(data[0]);
                let mut prev_ts = data[0].0;
                let mut prev_val: f64 = data[0].1;

                for (ts, _) in data.iter().skip(1) {
                    let slope = (data.iter().filter(|(t, _)| t > &prev_ts).count() as f64).max(1.0);
                    let val = prev_val + (*ts as f64 - prev_ts as f64) / slope;
                    result.push((*ts, val));
                    prev_ts = *ts;
                    prev_val = val;
                }
            }
            GapFillStrategy::Zero => {
                for (ts, _) in data {
                    result.push((ts, 0.0));
                }
            }
        }

        result
    }

    /// Find gaps in time series (timestamps need not be pre-sorted).
    pub fn find_gaps(&self, mut data: Vec<i64>) -> Vec<(i64, i64)> {
        if data.len() < 2 {
            return Vec::new();
        }

        data.sort_unstable();
        data.dedup();

        let mut gaps = Vec::new();
        let mut prev_ts = data[0];

        for ts in data.iter().skip(1) {
            if *ts > prev_ts + 1 {
                gaps.push((prev_ts + 1, *ts - 1));
            }
            prev_ts = *ts;
        }

        gaps
    }
}

/// Error types for resampling and alignment
#[derive(Debug, Error)]
pub enum ResampleError {
    #[error("Invalid granularity conversion: {0}")]
    InvalidConversion(String),

    #[error("Empty data for resampling")]
    EmptyData,
}

/// Memory-efficient data source wrapper
#[derive(Debug, Clone)]
pub struct MemoryDataSource {
    klines: Vec<Kline>,
    ticks: Vec<Tick>,
}

impl MemoryDataSource {
    pub fn new(klines: Vec<Kline>, ticks: Vec<Tick>) -> Self {
        Self { klines, ticks }
    }

    pub fn with_klines(mut self, klines: Vec<Kline>) -> Self {
        self.klines = klines;
        self
    }

    pub fn with_ticks(mut self, ticks: Vec<Tick>) -> Self {
        self.ticks = ticks;
        self
    }
}

impl Default for MemoryDataSource {
    fn default() -> Self {
        Self {
            klines: Vec::new(),
            ticks: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::Venue;
    use std::io::Write;

    #[test]
    fn test_mock_data_generator() {
        let generator = MockDataGenerator::new(42, 100.0, 0.5);
        let instrument = InstrumentId::new(Venue::Crypto, "BTCUSDT".to_string());

        let start_ts = 1_700_000_000_000_i64;
        let step_ms = generator.tick_interval_ms as i64 * 1000;
        let end_ts = start_ts + step_ms * (100 - 1);

        let klines = generator
            .generate_klines(
                &instrument,
                start_ts,
                end_ts,
                Granularity::Minute(1),
            )
            .unwrap();

        assert_eq!(klines.len(), 100);
        assert!(klines[0].open > 0.0);
        assert!(klines[0].high >= klines[0].low);
    }

    #[test]
    fn test_csv_parser() {
        // Create temp directory
        let temp_dir = std::env::temp_dir().join("strategy_test");
        fs::create_dir_all(&temp_dir).unwrap();

        // Create test CSV
        let csv_path = temp_dir.join("BTCUSDT_1m.csv");
        let mut file = File::create(&csv_path).unwrap();
        writeln!(file, "timestamp_ms,open,high,low,close,volume,instrument").unwrap();
        writeln!(file, "1700000000000,100.0,101.0,99.0,100.5,1000000,BTCUSDT").unwrap();
        writeln!(
            file,
            "1700000060000,100.5,102.0,100.0,101.5,1200000,BTCUSDT"
        )
        .unwrap();

        let parser = CsvParser::with_data_dir(&temp_dir);
        let klines = parser.parse_klines(&csv_path).unwrap();

        assert_eq!(klines.len(), 2);
        assert_eq!(klines[0].close, 100.5);
        assert_eq!(klines[1].open, 100.5);

        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_time_series_aligner() {
        let aligner = TimeSeriesAligner::default();

        // Test gap finding
        let data = vec![1000, 1100, 1300, 1500];
        let gaps = aligner.find_gaps(data);
        assert_eq!(gaps.len(), 3);
        assert_eq!(gaps[0], (1001, 1099));
        assert_eq!(gaps[1], (1101, 1299));
        assert_eq!(gaps[2], (1301, 1499));

        // Test alignment with forward fill
        let data = vec![(1000, 100.0), (1100, 200.0)];
        let aligned = aligner.align_and_fill(data);
        assert_eq!(aligned.len(), 2);
        assert_eq!(aligned[0], (1000, 100.0));
        assert_eq!(aligned[1], (1100, 200.0));
    }

    #[test]
    fn test_memory_data_source() {
        let klines = vec![Kline {
            instrument: InstrumentId::new(Venue::Crypto, "BTCUSDT".to_string()),
            open_ts_ms: 1700000000000,
            close_ts_ms: 1700000060000,
            open: 100.0,
            high: 102.0,
            low: 98.0,
            close: 101.0,
            volume: 1000.0,
        }];

        let source = MemoryDataSource::new(klines.clone(), vec![]);
        assert_eq!(source.klines.len(), 1);
        assert_eq!(source.klines[0].close, 101.0);
    }
}
