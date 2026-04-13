use domain::NormalizedBar;

#[derive(Debug, Clone)]
pub enum NormalizationMethod {
    MinMax,
    ZScore,
    RobustScaler,
    LogTransform,
}

#[derive(Debug, Clone)]
pub struct NormalizationStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub std: f64,
    pub median: f64,
    pub iqr: f64,
}

pub struct DataNormalizer;

impl DataNormalizer {
    pub fn stats(values: &[f64]) -> NormalizationStats {
        if values.is_empty() {
            return NormalizationStats {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                std: 0.0,
                median: 0.0,
                iqr: 0.0,
            };
        }
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std = variance.sqrt();
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let median = if n % 2 == 0 {
            (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
        } else {
            sorted[n / 2]
        };
        let q1 = sorted[n / 4];
        let q3 = sorted[3 * n / 4];
        let iqr = q3 - q1;
        NormalizationStats {
            min,
            max,
            mean,
            std,
            median,
            iqr,
        }
    }

    pub fn normalize(values: &[f64], method: NormalizationMethod) -> Vec<f64> {
        let s = Self::stats(values);
        match method {
            NormalizationMethod::MinMax => {
                let range = s.max - s.min;
                if range < 1e-12 {
                    return vec![0.0; values.len()];
                }
                values.iter().map(|v| (v - s.min) / range).collect()
            }
            NormalizationMethod::ZScore => {
                if s.std < 1e-12 {
                    return vec![0.0; values.len()];
                }
                values.iter().map(|v| (v - s.mean) / s.std).collect()
            }
            NormalizationMethod::RobustScaler => {
                if s.iqr < 1e-12 {
                    return vec![0.0; values.len()];
                }
                values.iter().map(|v| (v - s.median) / s.iqr).collect()
            }
            NormalizationMethod::LogTransform => values.iter().map(|v| (v + 1e-8).ln()).collect(),
        }
    }

    pub fn denormalize(
        normalized: &[f64],
        stats: &NormalizationStats,
        method: NormalizationMethod,
    ) -> Vec<f64> {
        match method {
            NormalizationMethod::MinMax => {
                let range = stats.max - stats.min;
                normalized.iter().map(|v| v * range + stats.min).collect()
            }
            NormalizationMethod::ZScore => normalized
                .iter()
                .map(|v| v * stats.std + stats.mean)
                .collect(),
            NormalizationMethod::RobustScaler => normalized
                .iter()
                .map(|v| v * stats.iqr + stats.median)
                .collect(),
            NormalizationMethod::LogTransform => {
                normalized.iter().map(|v| v.exp() - 1e-8).collect()
            }
        }
    }

    pub fn normalize_bars(
        bars: &[NormalizedBar],
        method: NormalizationMethod,
    ) -> Vec<NormalizedBar> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let normalized = Self::normalize(&closes, method);
        bars.iter()
            .zip(normalized.iter())
            .map(|(b, &nc)| NormalizedBar {
                ts_ms: b.ts_ms,
                open: b.open,
                high: b.high,
                low: b.low,
                close: nc,
                volume: b.volume,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minmax_roundtrip() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = DataNormalizer::stats(&values);
        let norm = DataNormalizer::normalize(&values, NormalizationMethod::MinMax);
        assert!((norm[0] - 0.0).abs() < 1e-9);
        assert!((norm[4] - 1.0).abs() < 1e-9);
        let back = DataNormalizer::denormalize(&norm, &stats, NormalizationMethod::MinMax);
        for (o, r) in values.iter().zip(back.iter()) {
            assert!((o - r).abs() < 1e-9, "roundtrip failed: {} != {}", o, r);
        }
    }

    #[test]
    fn zscore_roundtrip() {
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let stats = DataNormalizer::stats(&values);
        let norm = DataNormalizer::normalize(&values, NormalizationMethod::ZScore);
        let back = DataNormalizer::denormalize(&norm, &stats, NormalizationMethod::ZScore);
        for (o, r) in values.iter().zip(back.iter()) {
            assert!((o - r).abs() < 1e-6, "roundtrip failed: {} != {}", o, r);
        }
    }

    #[test]
    fn log_transform_roundtrip() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = DataNormalizer::stats(&values);
        let norm = DataNormalizer::normalize(&values, NormalizationMethod::LogTransform);
        let back = DataNormalizer::denormalize(&norm, &stats, NormalizationMethod::LogTransform);
        for (o, r) in values.iter().zip(back.iter()) {
            assert!((o - r).abs() < 1e-6, "roundtrip failed: {} != {}", o, r);
        }
    }
}
