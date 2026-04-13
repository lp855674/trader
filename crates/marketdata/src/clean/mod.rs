use crate::core::DataItem;
use domain::NormalizedBar;

// ── CleaningRule ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CleaningRule {
    RemoveDuplicates,
    RemoveOutliers { z_threshold: f64 },
    FillGaps { max_gap_ms: u64 },
    ClampPrices { min: f64, max: f64 },
    RequirePositiveVolume,
}

// ── CleaningReport ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct CleaningReport {
    pub duplicates_removed: u32,
    pub outliers_removed: u32,
    pub gaps_filled: u32,
    pub prices_clamped: u32,
    pub zero_volume_removed: u32,
}

// ── DataCleaner ───────────────────────────────────────────────────────────────

pub struct DataCleaner {
    pub rules: Vec<CleaningRule>,
}

impl DataCleaner {
    pub fn new(rules: Vec<CleaningRule>) -> Self {
        Self { rules }
    }

    pub fn clean(&self, bars: Vec<NormalizedBar>) -> (Vec<NormalizedBar>, CleaningReport) {
        let mut current = bars;
        let mut report = CleaningReport::default();

        for rule in &self.rules {
            match rule {
                CleaningRule::RemoveDuplicates => {
                    let before = current.len();
                    let mut seen = std::collections::HashSet::new();
                    current.retain(|b| seen.insert(b.ts_ms));
                    report.duplicates_removed += (before - current.len()) as u32;
                }

                CleaningRule::RemoveOutliers { z_threshold } => {
                    if current.len() < 2 {
                        continue;
                    }
                    let closes: Vec<f64> = current.iter().map(|b| b.close).collect();
                    let mean = closes.iter().sum::<f64>() / closes.len() as f64;
                    let variance = closes.iter().map(|c| (c - mean).powi(2)).sum::<f64>()
                        / closes.len() as f64;
                    let std_dev = variance.sqrt();
                    if std_dev < 1e-12 {
                        continue;
                    }
                    let before = current.len();
                    current.retain(|b| ((b.close - mean) / std_dev).abs() <= *z_threshold);
                    report.outliers_removed += (before - current.len()) as u32;
                }

                CleaningRule::FillGaps { max_gap_ms } => {
                    if current.len() < 2 {
                        continue;
                    }
                    let mut filled: Vec<NormalizedBar> = Vec::new();
                    for i in 0..current.len() {
                        filled.push(current[i].clone());
                        if i + 1 < current.len() {
                            let gap = (current[i + 1].ts_ms - current[i].ts_ms) as u64;
                            if gap > 0 && gap <= *max_gap_ms {
                                // forward fill
                                let prev = current[i].clone();
                                let next_ts = current[i + 1].ts_ms;
                                // Use 1-step fill (fill the gap with a copy)
                                // We don't know the interval, so just fill mid-point
                                let mid = current[i].ts_ms + (gap / 2) as i64;
                                if mid != current[i].ts_ms && mid != next_ts {
                                    filled.push(NormalizedBar {
                                        ts_ms: mid,
                                        volume: 0.0,
                                        ..prev
                                    });
                                    report.gaps_filled += 1;
                                }
                            }
                        }
                    }
                    current = filled;
                }

                CleaningRule::ClampPrices { min, max } => {
                    let before_clamped = report.prices_clamped;
                    for bar in &mut current {
                        let mut clamped = false;
                        if bar.open < *min || bar.open > *max {
                            bar.open = bar.open.clamp(*min, *max);
                            clamped = true;
                        }
                        if bar.high < *min || bar.high > *max {
                            bar.high = bar.high.clamp(*min, *max);
                            clamped = true;
                        }
                        if bar.low < *min || bar.low > *max {
                            bar.low = bar.low.clamp(*min, *max);
                            clamped = true;
                        }
                        if bar.close < *min || bar.close > *max {
                            bar.close = bar.close.clamp(*min, *max);
                            clamped = true;
                        }
                        if clamped {
                            report.prices_clamped += 1;
                        }
                    }
                    let _ = before_clamped;
                }

                CleaningRule::RequirePositiveVolume => {
                    let before = current.len();
                    current.retain(|b| b.volume > 0.0);
                    report.zero_volume_removed += (before - current.len()) as u32;
                }
            }
        }

        (current, report)
    }

    pub fn clean_items(&self, items: Vec<DataItem>) -> (Vec<DataItem>, CleaningReport) {
        let mut report = CleaningReport::default();

        // Extract bars from items for cleaning
        let mut bars_with_idx: Vec<(usize, NormalizedBar)> = Vec::new();
        let mut non_bars: Vec<(usize, DataItem)> = Vec::new();

        for (i, item) in items.into_iter().enumerate() {
            match item {
                DataItem::Bar(b) => bars_with_idx.push((i, b)),
                other => non_bars.push((i, other)),
            }
        }

        let bars: Vec<NormalizedBar> = bars_with_idx.iter().map(|(_, b)| b.clone()).collect();
        let (cleaned_bars, r) = self.clean(bars);
        report.duplicates_removed += r.duplicates_removed;
        report.outliers_removed += r.outliers_removed;
        report.gaps_filled += r.gaps_filled;
        report.prices_clamped += r.prices_clamped;
        report.zero_volume_removed += r.zero_volume_removed;

        let mut result: Vec<DataItem> = cleaned_bars.into_iter().map(DataItem::Bar).collect();
        for (_, item) in non_bars {
            result.push(item);
        }
        result.sort_by_key(|i| i.ts_ms());

        (result, report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts_ms: i64, close: f64, volume: f64) -> NormalizedBar {
        NormalizedBar {
            ts_ms,
            open: close,
            high: close,
            low: close,
            close,
            volume,
        }
    }

    #[test]
    fn remove_duplicates() {
        let bars = vec![bar(1, 10.0, 1.0), bar(1, 10.0, 1.0), bar(2, 11.0, 1.0)];
        let cleaner = DataCleaner::new(vec![CleaningRule::RemoveDuplicates]);
        let (result, report) = cleaner.clean(bars);
        assert_eq!(result.len(), 2);
        assert_eq!(report.duplicates_removed, 1);
    }

    #[test]
    fn remove_outliers() {
        // Mean=10, values far from mean will be removed
        let mut bars = (0..10).map(|i| bar(i, 10.0, 1.0)).collect::<Vec<_>>();
        bars.push(bar(100, 1000.0, 1.0)); // extreme outlier
        let cleaner = DataCleaner::new(vec![CleaningRule::RemoveOutliers { z_threshold: 2.0 }]);
        let (result, report) = cleaner.clean(bars);
        assert_eq!(report.outliers_removed, 1);
        assert!(result.iter().all(|b| b.close < 100.0));
    }

    #[test]
    fn clamp_prices() {
        let bars = vec![bar(1, 5.0, 1.0), bar(2, 150.0, 1.0), bar(3, 50.0, 1.0)];
        let cleaner = DataCleaner::new(vec![CleaningRule::ClampPrices {
            min: 10.0,
            max: 100.0,
        }]);
        let (result, report) = cleaner.clean(bars);
        assert_eq!(report.prices_clamped, 2);
        assert!((result[0].close - 10.0).abs() < 1e-9);
        assert!((result[1].close - 100.0).abs() < 1e-9);
        assert!((result[2].close - 50.0).abs() < 1e-9);
    }

    #[test]
    fn require_positive_volume() {
        let bars = vec![bar(1, 10.0, 0.0), bar(2, 10.0, -1.0), bar(3, 10.0, 100.0)];
        let cleaner = DataCleaner::new(vec![CleaningRule::RequirePositiveVolume]);
        let (result, report) = cleaner.clean(bars);
        assert_eq!(result.len(), 1);
        assert_eq!(report.zero_volume_removed, 2);
    }

    #[test]
    fn fill_gaps_works() {
        let bars = vec![
            bar(0, 10.0, 1.0),
            bar(1000, 11.0, 1.0), // gap of 1000ms
        ];
        let cleaner = DataCleaner::new(vec![CleaningRule::FillGaps { max_gap_ms: 2000 }]);
        let (result, report) = cleaner.clean(bars);
        assert!(result.len() >= 2);
        assert!(report.gaps_filled >= 1);
    }
}
