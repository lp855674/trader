use std::collections::HashMap;
use domain::NormalizedBar;
use crate::core::DataSourceError;

// ── FileFormat ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FileFormat {
    Csv,
    Parquet,
    Json,
}

// ── CsvConfig ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CsvConfig {
    pub delimiter: char,
    pub has_header: bool,
    pub ts_column: String,
    pub price_columns: Vec<String>,
}

impl Default for CsvConfig {
    fn default() -> Self {
        Self {
            delimiter: ',',
            has_header: true,
            ts_column: "ts_ms".to_string(),
            price_columns: vec![
                "open".to_string(),
                "high".to_string(),
                "low".to_string(),
                "close".to_string(),
                "volume".to_string(),
            ],
        }
    }
}

// ── ParsedRow ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ParsedRow {
    pub ts_ms: i64,
    pub fields: HashMap<String, f64>,
}

// ── FileParser ────────────────────────────────────────────────────────────────

pub struct FileParser;

impl FileParser {
    /// Parse timestamp string — tries i64 first, then ISO 8601 prefix
    fn parse_timestamp(s: &str) -> Result<i64, DataSourceError> {
        let trimmed = s.trim();
        // Try direct integer (already ms)
        if let Ok(v) = trimmed.parse::<i64>() {
            return Ok(v);
        }
        // Try ISO 8601: "YYYY-MM-DDTHH:MM:SS" or "YYYY-MM-DD"
        // Simple parse — split on T and parse date/time parts
        let date_part = trimmed.split('T').next().unwrap_or(trimmed);
        let parts: Vec<&str> = date_part.split('-').collect();
        if parts.len() == 3 {
            if let (Ok(y), Ok(m), Ok(d)) = (
                parts[0].parse::<i64>(),
                parts[1].parse::<i64>(),
                parts[2].trim_end_matches(|c: char| !c.is_numeric()).parse::<i64>(),
            ) {
                // Days since epoch (simplified, ignores leap years precisely but fine for tests)
                let days = days_since_epoch(y, m, d);
                return Ok(days * 86_400_000);
            }
        }
        Err(DataSourceError::Parse(format!("cannot parse timestamp: {}", s)))
    }

    pub fn parse_csv(content: &str, config: &CsvConfig) -> Result<Vec<ParsedRow>, DataSourceError> {
        let mut lines = content.lines();
        let header_line = if config.has_header {
            lines.next().ok_or_else(|| DataSourceError::Parse("empty CSV".to_string()))?
        } else {
            return Err(DataSourceError::Parse("headerless CSV not supported".to_string()));
        };

        let headers: Vec<&str> = header_line.split(config.delimiter).map(|s| s.trim()).collect();

        let ts_idx = headers
            .iter()
            .position(|h| *h == config.ts_column.as_str())
            .ok_or_else(|| DataSourceError::Parse(format!("ts column '{}' not found", config.ts_column)))?;

        let mut rows = Vec::new();
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let cells: Vec<&str> = line.split(config.delimiter).map(|s| s.trim()).collect();
            if cells.len() <= ts_idx {
                continue;
            }
            let ts_ms = Self::parse_timestamp(cells[ts_idx])?;
            let mut fields = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                if i == ts_idx || i >= cells.len() {
                    continue;
                }
                match cells[i].parse::<f64>() {
                    Ok(v) => {
                        fields.insert(header.to_string(), v);
                    }
                    Err(_) => {
                        // skip non-numeric with warning (eprintln in production code is fine here)
                        eprintln!("warning: skipping non-numeric field '{}' in column '{}'", cells[i], header);
                    }
                }
            }
            rows.push(ParsedRow { ts_ms, fields });
        }
        Ok(rows)
    }

    pub fn csv_to_bars(content: &str, config: &CsvConfig) -> Result<Vec<NormalizedBar>, DataSourceError> {
        let rows = Self::parse_csv(content, config)?;
        let required = ["open", "high", "low", "close", "volume"];
        let mut bars = Vec::with_capacity(rows.len());
        for row in rows {
            for &col in &required {
                if !row.fields.contains_key(col) {
                    return Err(DataSourceError::Parse(format!("missing column: {}", col)));
                }
            }
            bars.push(NormalizedBar {
                ts_ms: row.ts_ms,
                open: row.fields["open"],
                high: row.fields["high"],
                low: row.fields["low"],
                close: row.fields["close"],
                volume: row.fields["volume"],
            });
        }
        Ok(bars)
    }

    pub fn detect_format(filename: &str) -> FileFormat {
        let lower = filename.to_lowercase();
        if lower.ends_with(".parquet") || lower.ends_with(".pq") {
            FileFormat::Parquet
        } else if lower.ends_with(".json") {
            FileFormat::Json
        } else {
            FileFormat::Csv
        }
    }
}

fn days_since_epoch(year: i64, month: i64, day: i64) -> i64 {
    // Simplified Julian Day Number approach relative to 1970-01-01
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 12 } else { month };
    let a = y / 100;
    let b = 2 - a + a / 4;
    let jdn = ((365.25 * (y + 4716) as f64) as i64)
        + ((30.6001 * (m + 1) as f64) as i64)
        + day
        + b
        - 1524;
    jdn - 2_440_588 // subtract JDN of 1970-01-01
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CSV: &str = "ts_ms,open,high,low,close,volume
1000,1.0,2.0,0.5,1.5,100.0
2000,2.0,3.0,1.5,2.5,200.0
3000,3.0,4.0,2.5,3.5,300.0";

    #[test]
    fn parse_csv_basic() {
        let config = CsvConfig::default();
        let rows = FileParser::parse_csv(SAMPLE_CSV, &config).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].ts_ms, 1000);
        assert!((rows[0].fields["open"] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn csv_to_bars_basic() {
        let config = CsvConfig::default();
        let bars = FileParser::csv_to_bars(SAMPLE_CSV, &config).unwrap();
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[1].ts_ms, 2000);
        assert!((bars[1].close - 2.5).abs() < 1e-9);
    }

    #[test]
    fn csv_missing_column_returns_error() {
        let csv = "ts_ms,open,high,low,close\n1000,1.0,2.0,0.5,1.5";
        let config = CsvConfig::default();
        let result = FileParser::csv_to_bars(csv, &config);
        assert!(result.is_err());
    }

    #[test]
    fn detect_format_by_extension() {
        assert_eq!(FileParser::detect_format("data.csv"), FileFormat::Csv);
        assert_eq!(FileParser::detect_format("data.parquet"), FileFormat::Parquet);
        assert_eq!(FileParser::detect_format("data.pq"), FileFormat::Parquet);
        assert_eq!(FileParser::detect_format("data.json"), FileFormat::Json);
        assert_eq!(FileParser::detect_format("data.unknown"), FileFormat::Csv);
    }

    #[test]
    fn parse_csv_missing_ts_column_returns_error() {
        let csv = "time,open,high,low,close,volume\n1000,1.0,2.0,0.5,1.5,100.0";
        let config = CsvConfig::default(); // expects "ts_ms"
        let result = FileParser::parse_csv(csv, &config);
        assert!(result.is_err());
    }
}
