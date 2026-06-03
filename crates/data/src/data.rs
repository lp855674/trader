#![forbid(unsafe_code)]

mod bar;
mod csv;
mod parquet;

pub use bar::*;
pub use csv::*;
pub use parquet::*;

pub fn load_bars(source: &str, path: impl AsRef<std::path::Path>) -> Result<Vec<Bar>, DataError> {
    match source {
        "csv" => load_bars_from_csv(path),
        "parquet" => load_bars_from_parquet(path),
        other => Err(DataError::UnsupportedSource(other.to_string())),
    }
}
