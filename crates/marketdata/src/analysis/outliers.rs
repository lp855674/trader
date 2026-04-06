use domain::NormalizedBar;

#[derive(Debug, Clone)]
pub enum OutlierMethod {
    ZScore { threshold: f64 },
    IQR { multiplier: f64 },
    Isolation { contamination: f64 },
}

#[derive(Debug, Clone)]
pub struct OutlierResult {
    pub index: usize,
    pub ts_ms: i64,
    pub value: f64,
    pub score: f64,
    pub method: String,
}

pub struct OutlierDetector;

impl OutlierDetector {
    pub fn detect_z_score(values: &[f64], threshold: f64) -> Vec<OutlierResult> {
        if values.len() < 2 {
            return Vec::new();
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std = variance.sqrt();
        if std < 1e-12 {
            return Vec::new();
        }
        values
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                let z = (v - mean) / std;
                if z.abs() > threshold {
                    Some(OutlierResult {
                        index: i,
                        ts_ms: i as i64,
                        value: v,
                        score: z.abs(),
                        method: "ZScore".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn detect_iqr(values: &[f64], multiplier: f64) -> Vec<OutlierResult> {
        if values.len() < 4 {
            return Vec::new();
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let n = sorted.len();
        let q1 = sorted[n / 4];
        let q3 = sorted[3 * n / 4];
        let iqr = q3 - q1;
        let lower = q1 - multiplier * iqr;
        let upper = q3 + multiplier * iqr;
        values
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| {
                if v < lower || v > upper {
                    let distance = if v < lower { lower - v } else { v - upper };
                    Some(OutlierResult {
                        index: i,
                        ts_ms: i as i64,
                        value: v,
                        score: distance / (iqr.max(1e-12)),
                        method: "IQR".to_string(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn detect(values: &[f64], method: OutlierMethod) -> Vec<OutlierResult> {
        match method {
            OutlierMethod::ZScore { threshold } => Self::detect_z_score(values, threshold),
            OutlierMethod::IQR { multiplier } => Self::detect_iqr(values, multiplier),
            OutlierMethod::Isolation { contamination } => {
                // Simple stub: flag top contamination% as outliers by distance from mean
                if values.is_empty() { return Vec::new(); }
                let mean = values.iter().sum::<f64>() / values.len() as f64;
                let mut indexed: Vec<(usize, f64, f64)> = values
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| (i, v, (v - mean).abs()))
                    .collect();
                indexed.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
                let k = ((contamination * values.len() as f64).ceil() as usize).min(values.len());
                indexed[..k]
                    .iter()
                    .map(|&(i, v, score)| OutlierResult {
                        index: i,
                        ts_ms: i as i64,
                        value: v,
                        score,
                        method: "Isolation".to_string(),
                    })
                    .collect()
            }
        }
    }

    pub fn detect_bar_outliers(bars: &[NormalizedBar], method: OutlierMethod) -> Vec<OutlierResult> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let mut results = Self::detect(&closes, method);
        // Patch ts_ms from actual bars
        for r in &mut results {
            if r.index < bars.len() {
                r.ts_ms = bars[r.index].ts_ms;
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_score_detects_extreme() {
        let mut values: Vec<f64> = (0..20).map(|i| i as f64).collect();
        values.push(1000.0);
        let results = OutlierDetector::detect_z_score(&values, 2.0);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.value > 100.0));
    }

    #[test]
    fn iqr_detects_extreme() {
        let mut values: Vec<f64> = (0..20).map(|i| i as f64).collect();
        values.push(1000.0);
        let results = OutlierDetector::detect_iqr(&values, 1.5);
        assert!(!results.is_empty());
    }

    #[test]
    fn no_outliers_in_uniform() {
        let values: Vec<f64> = vec![1.0; 20];
        let results = OutlierDetector::detect_z_score(&values, 2.0);
        assert!(results.is_empty());
    }
}
