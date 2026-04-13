use domain::NormalizedBar;
use polars::prelude::*;
use thiserror::Error;

pub mod align;
pub mod analysis;
pub mod api;
pub mod cache;
pub mod clean;
pub mod core;
pub mod data_api;
pub mod data_config;
pub mod data_sources;
pub mod lifecycle;
pub mod metadata;
pub mod monitor;
pub mod parser;
pub mod quality;
pub mod replay;
pub mod storage;

pub const BAR_COLUMNS: [&str; 6] = ["ts_ms", "open", "high", "low", "close", "volume"];

#[derive(Debug, Error)]
pub enum MarketDataError {
    #[error("missing column: {0}")]
    MissingColumn(&'static str),

    #[error("polars error: {0}")]
    Polars(#[from] PolarsError),
}

/// 研究/离线侧的 OHLCV DataFrame 封装（与 `domain::NormalizedBar` 对齐）。
#[derive(Debug, Clone)]
pub struct BarsFrame {
    df: DataFrame,
}

impl BarsFrame {
    pub fn df(&self) -> &DataFrame {
        &self.df
    }

    pub fn into_df(self) -> DataFrame {
        self.df
    }

    pub fn empty() -> Self {
        let df = DataFrame::new_infer_height(vec![
            Column::new("ts_ms".into(), Vec::<i64>::new()),
            Column::new("open".into(), Vec::<f64>::new()),
            Column::new("high".into(), Vec::<f64>::new()),
            Column::new("low".into(), Vec::<f64>::new()),
            Column::new("close".into(), Vec::<f64>::new()),
            Column::new("volume".into(), Vec::<f64>::new()),
        ])
        .expect("empty dataframe creation should not fail");

        Self { df }
    }

    pub fn from_bars(bars: &[NormalizedBar]) -> Result<Self, MarketDataError> {
        let mut ts = Vec::with_capacity(bars.len());
        let mut open = Vec::with_capacity(bars.len());
        let mut high = Vec::with_capacity(bars.len());
        let mut low = Vec::with_capacity(bars.len());
        let mut close = Vec::with_capacity(bars.len());
        let mut volume = Vec::with_capacity(bars.len());

        for b in bars {
            ts.push(b.ts_ms);
            open.push(b.open);
            high.push(b.high);
            low.push(b.low);
            close.push(b.close);
            volume.push(b.volume);
        }

        let df = DataFrame::new_infer_height(vec![
            Column::new("ts_ms".into(), ts),
            Column::new("open".into(), open),
            Column::new("high".into(), high),
            Column::new("low".into(), low),
            Column::new("close".into(), close),
            Column::new("volume".into(), volume),
        ])?;

        Ok(Self { df })
    }

    pub fn sort_by_time(mut self, descending: bool) -> Result<Self, MarketDataError> {
        self.df = self.df.sort(
            ["ts_ms"],
            SortMultipleOptions::default().with_order_descending(descending),
        )?;
        Ok(self)
    }

    /// 给 BarsFrame 增加 `symbol`（Utf8）列，便于做多标的面板。
    pub fn with_symbol(mut self, symbol: &str) -> Result<Self, MarketDataError> {
        let height = self.df.height();
        let values: Vec<&str> = std::iter::repeat(symbol).take(height).collect();
        let col = Column::new("symbol".into(), values);
        self.df.with_column(col)?;
        Ok(self)
    }

    /// 按时间范围过滤（闭区间）。`start_ts_ms` / `end_ts_ms` 可选。
    pub fn filter_time_range(
        self,
        start_ts_ms: Option<i64>,
        end_ts_ms: Option<i64>,
    ) -> Result<Self, MarketDataError> {
        let mut lf = self.df.lazy();

        if let Some(start) = start_ts_ms {
            lf = lf.filter(col("ts_ms").gt_eq(lit(start)));
        }
        if let Some(end) = end_ts_ms {
            lf = lf.filter(col("ts_ms").lt_eq(lit(end)));
        }

        Ok(Self { df: lf.collect()? })
    }

    pub fn to_bars(&self) -> Result<Vec<NormalizedBar>, MarketDataError> {
        for &c in &BAR_COLUMNS {
            if !self
                .df
                .get_column_names()
                .iter()
                .any(|name| name.as_str() == c)
            {
                return Err(MarketDataError::MissingColumn(c));
            }
        }

        let ts = self.df.column("ts_ms")?.i64()?;
        let open = self.df.column("open")?.f64()?;
        let high = self.df.column("high")?.f64()?;
        let low = self.df.column("low")?.f64()?;
        let close = self.df.column("close")?.f64()?;
        let volume = self.df.column("volume")?.f64()?;

        let len = self.df.height();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            out.push(NormalizedBar {
                ts_ms: ts.get(i).unwrap_or_default(),
                open: open.get(i).unwrap_or_default(),
                high: high.get(i).unwrap_or_default(),
                low: low.get(i).unwrap_or_default(),
                close: close.get(i).unwrap_or_default(),
                volume: volume.get(i).unwrap_or_default(),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bars() {
        let bars = vec![
            NormalizedBar {
                ts_ms: 1,
                open: 1.0,
                high: 2.0,
                low: 0.5,
                close: 1.5,
                volume: 100.0,
            },
            NormalizedBar {
                ts_ms: 2,
                open: 2.0,
                high: 3.0,
                low: 1.5,
                close: 2.5,
                volume: 200.0,
            },
        ];

        let df = BarsFrame::from_bars(&bars).unwrap();
        let back = df.to_bars().unwrap();
        assert_eq!(bars, back);
    }

    #[test]
    fn filter_time_range_works() {
        let bars = vec![
            NormalizedBar {
                ts_ms: 1,
                open: 1.0,
                high: 1.0,
                low: 1.0,
                close: 1.0,
                volume: 1.0,
            },
            NormalizedBar {
                ts_ms: 2,
                open: 2.0,
                high: 2.0,
                low: 2.0,
                close: 2.0,
                volume: 2.0,
            },
            NormalizedBar {
                ts_ms: 3,
                open: 3.0,
                high: 3.0,
                low: 3.0,
                close: 3.0,
                volume: 3.0,
            },
        ];

        let frame = BarsFrame::from_bars(&bars).unwrap();
        let filtered = frame.filter_time_range(Some(2), Some(3)).unwrap();
        let out = filtered.to_bars().unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].ts_ms, 2);
        assert_eq!(out[1].ts_ms, 3);
    }
}
