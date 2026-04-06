use polars::prelude::*;

pub struct PolarsDataFrameExt;

impl PolarsDataFrameExt {
    /// Adds `return` column: (close - close.shift(1)) / close.shift(1)
    pub fn add_returns(df: &DataFrame) -> Result<DataFrame, PolarsError> {
        let lf = df.clone().lazy();
        let prev_close = col("close").shift(lit(1));
        let returns = (col("close") - prev_close.clone()) / prev_close;
        lf.with_column(returns.alias("return")).collect()
    }

    /// Adds `rolling_mean` column on close with given window size, computed via map_batches
    pub fn add_rolling_mean(df: &DataFrame, window: usize) -> Result<DataFrame, PolarsError> {
        let close = df.column("close")?.f64()?;
        let values: Vec<f64> = close.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let rolling: Vec<Option<f64>> = compute_rolling_mean(&values, window);
        let series = Series::new("rolling_mean".into(), rolling);
        let mut out = df.clone();
        out.with_column(series.into())?;
        Ok(out)
    }

    /// Adds `rolling_std` column on close with given window size
    pub fn add_rolling_std(df: &DataFrame, window: usize) -> Result<DataFrame, PolarsError> {
        let close = df.column("close")?.f64()?;
        let values: Vec<f64> = close.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let rolling: Vec<Option<f64>> = compute_rolling_std(&values, window);
        let series = Series::new("rolling_std".into(), rolling);
        let mut out = df.clone();
        out.with_column(series.into())?;
        Ok(out)
    }

    /// Adds `bb_upper`, `bb_lower`, `bb_mid` Bollinger Bands columns
    pub fn add_bollinger_bands(
        df: &DataFrame,
        window: usize,
        n_std: f64,
    ) -> Result<DataFrame, PolarsError> {
        let close = df.column("close")?.f64()?;
        let values: Vec<f64> = close.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let means = compute_rolling_mean(&values, window);
        let stds = compute_rolling_std(&values, window);

        let bb_mid: Vec<Option<f64>> = means.clone();
        let bb_upper: Vec<Option<f64>> = means.iter().zip(stds.iter()).map(|(m, s)| {
            match (m, s) {
                (Some(m), Some(s)) => Some(m + n_std * s),
                _ => None,
            }
        }).collect();
        let bb_lower: Vec<Option<f64>> = means.iter().zip(stds.iter()).map(|(m, s)| {
            match (m, s) {
                (Some(m), Some(s)) => Some(m - n_std * s),
                _ => None,
            }
        }).collect();

        let mut out = df.clone();
        out.with_column(Series::new("bb_mid".into(), bb_mid).into())?;
        out.with_column(Series::new("bb_upper".into(), bb_upper).into())?;
        out.with_column(Series::new("bb_lower".into(), bb_lower).into())?;
        Ok(out)
    }

    /// Adds cumulative VWAP column: cumsum(close*volume)/cumsum(volume)
    pub fn add_vwap(df: &DataFrame) -> Result<DataFrame, PolarsError> {
        let close = df.column("close")?.f64()?;
        let volume = df.column("volume")?.f64()?;

        let mut cum_cv = 0.0f64;
        let mut cum_v = 0.0f64;
        let vwap: Vec<f64> = close
            .into_iter()
            .zip(volume.into_iter())
            .map(|(c, v)| {
                let c = c.unwrap_or(0.0);
                let v = v.unwrap_or(0.0);
                cum_cv += c * v;
                cum_v += v;
                if cum_v > 1e-12 { cum_cv / cum_v } else { 0.0 }
            })
            .collect();

        let series = Series::new("vwap".into(), vwap);
        let mut out = df.clone();
        out.with_column(series.into())?;
        Ok(out)
    }

    /// Resample bars to given interval_ms using OHLCV aggregation
    pub fn resample_to_bars(df: &DataFrame, interval_ms: u64) -> Result<DataFrame, PolarsError> {
        let interval_ms_i64 = interval_ms as i64;
        let lf = df.clone().lazy();
        let bucket = (col("ts_ms") / lit(interval_ms_i64) * lit(interval_ms_i64)).alias("bucket");
        lf.with_column(bucket)
            .group_by([col("bucket")])
            .agg([
                col("ts_ms").min().alias("ts_ms"),
                col("open").first().alias("open"),
                col("high").max().alias("high"),
                col("low").min().alias("low"),
                col("close").last().alias("close"),
                col("volume").sum().alias("volume"),
            ])
            .sort(["ts_ms"], SortMultipleOptions::default())
            .select([
                col("ts_ms"),
                col("open"),
                col("high"),
                col("low"),
                col("close"),
                col("volume"),
            ])
            .collect()
    }
}

fn compute_rolling_mean(values: &[f64], window: usize) -> Vec<Option<f64>> {
    values
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let start = if i + 1 >= window { i + 1 - window } else { 0 };
            let slice = &values[start..=i];
            let mean = slice.iter().sum::<f64>() / slice.len() as f64;
            Some(mean)
        })
        .collect()
}

fn compute_rolling_std(values: &[f64], window: usize) -> Vec<Option<f64>> {
    values
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let start = if i + 1 >= window { i + 1 - window } else { 0 };
            let slice = &values[start..=i];
            if slice.len() < 2 {
                return Some(0.0);
            }
            let mean = slice.iter().sum::<f64>() / slice.len() as f64;
            let variance = slice.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / slice.len() as f64;
            Some(variance.sqrt())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::NormalizedBar;
    use crate::BarsFrame;

    fn make_bars(n: usize) -> Vec<NormalizedBar> {
        (0..n)
            .map(|i| NormalizedBar {
                ts_ms: i as i64 * 1000,
                open: 100.0 + i as f64,
                high: 101.0 + i as f64,
                low: 99.0 + i as f64,
                close: 100.0 + i as f64 * 0.5,
                volume: 1000.0 + i as f64 * 10.0,
            })
            .collect()
    }

    #[test]
    fn rolling_mean_has_correct_column() {
        let bars = make_bars(10);
        let frame = BarsFrame::from_bars(&bars).unwrap();
        let df = PolarsDataFrameExt::add_rolling_mean(frame.df(), 3).unwrap();
        assert!(df.column("rolling_mean").is_ok());
        assert_eq!(df.height(), 10);
    }

    #[test]
    fn returns_column_added() {
        let bars = make_bars(5);
        let frame = BarsFrame::from_bars(&bars).unwrap();
        let df = PolarsDataFrameExt::add_returns(frame.df()).unwrap();
        assert!(df.column("return").is_ok());
    }

    #[test]
    fn bollinger_bands_columns() {
        let bars = make_bars(20);
        let frame = BarsFrame::from_bars(&bars).unwrap();
        let df = PolarsDataFrameExt::add_bollinger_bands(frame.df(), 5, 2.0).unwrap();
        assert!(df.column("bb_upper").is_ok());
        assert!(df.column("bb_lower").is_ok());
        assert!(df.column("bb_mid").is_ok());
    }

    #[test]
    fn vwap_column_added() {
        let bars = make_bars(10);
        let frame = BarsFrame::from_bars(&bars).unwrap();
        let df = PolarsDataFrameExt::add_vwap(frame.df()).unwrap();
        assert!(df.column("vwap").is_ok());
    }
}
