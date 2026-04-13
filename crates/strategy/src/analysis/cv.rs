use thiserror::Error;

/// Cross-validation method.
#[derive(Debug, Clone, PartialEq)]
pub enum CvMethod {
    KFold,
    PurgedKFold,
    WalkForwardKFold,
}

/// Configuration for cross-validation.
#[derive(Debug, Clone)]
pub struct CvConfig {
    pub n_folds: usize,
    pub method: CvMethod,
    /// Bars to skip between train and test (prevent leakage).
    pub purge_gap: usize,
}

/// Result for a single fold.
#[derive(Debug, Clone)]
pub struct FoldResult {
    pub fold_id: usize,
    pub train_start: usize,
    pub train_end: usize,
    pub test_start: usize,
    pub test_end: usize,
    pub train_score: f64,
    pub test_score: f64,
    /// train_score / test_score; infinity if test_score == 0.
    pub overfit_ratio: f64,
}

/// Aggregated cross-validation result.
#[derive(Debug, Clone)]
pub struct CvResult {
    pub folds: Vec<FoldResult>,
    pub mean_test_score: f64,
    pub std_test_score: f64,
    pub mean_train_score: f64,
    /// mean(train) / mean(test); values >> 1 indicate overfit.
    pub overfit_score: f64,
    /// True if overfit_score > 2.0.
    pub is_overfit: bool,
}

/// Cross-validation errors.
#[derive(Debug, Error)]
pub enum CvError {
    #[error("Insufficient data for cross-validation")]
    InsufficientData,
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
}

/// Time-series-safe cross-validator.
pub struct CrossValidator {
    pub config: CvConfig,
}

impl CrossValidator {
    pub fn new(config: CvConfig) -> Self {
        Self { config }
    }

    /// Returns (train_start, train_end, test_start, test_end) for each fold.
    pub fn split(&self, total_bars: usize) -> Result<Vec<(usize, usize, usize, usize)>, CvError> {
        let cfg = &self.config;
        if cfg.n_folds == 0 {
            return Err(CvError::InvalidConfig("n_folds must be > 0".to_string()));
        }

        match cfg.method {
            CvMethod::KFold => self.kfold_split(total_bars),
            CvMethod::PurgedKFold => self.purged_kfold_split(total_bars),
            CvMethod::WalkForwardKFold => self.walk_forward_kfold_split(total_bars),
        }
    }

    fn kfold_split(&self, total_bars: usize) -> Result<Vec<(usize, usize, usize, usize)>, CvError> {
        let n = self.config.n_folds;
        let chunk = total_bars / n;
        if chunk == 0 {
            return Err(CvError::InsufficientData);
        }

        let mut splits = Vec::new();
        for fold in 0..n {
            let test_start = fold * chunk;
            let test_end = if fold == n - 1 {
                total_bars
            } else {
                (fold + 1) * chunk
            };

            // Training is everything except the test fold (concatenated)
            // For time series: use all bars before test fold as training
            let train_start = 0;
            let train_end = test_start;

            if train_end == 0 {
                // First fold has no training data before it — skip
                continue;
            }

            splits.push((train_start, train_end, test_start, test_end));
        }

        if splits.is_empty() {
            return Err(CvError::InsufficientData);
        }
        Ok(splits)
    }

    fn purged_kfold_split(
        &self,
        total_bars: usize,
    ) -> Result<Vec<(usize, usize, usize, usize)>, CvError> {
        let n = self.config.n_folds;
        let gap = self.config.purge_gap;
        let chunk = total_bars / n;
        if chunk == 0 {
            return Err(CvError::InsufficientData);
        }

        let mut splits = Vec::new();
        for fold in 0..n {
            let test_start = fold * chunk;
            let test_end = if fold == n - 1 {
                total_bars
            } else {
                (fold + 1) * chunk
            };

            // Training: bars before (test_start - gap)
            let train_end = test_start.saturating_sub(gap);
            let train_start = 0;

            if train_end == 0 {
                continue;
            }

            splits.push((train_start, train_end, test_start, test_end));
        }

        if splits.is_empty() {
            return Err(CvError::InsufficientData);
        }
        Ok(splits)
    }

    fn walk_forward_kfold_split(
        &self,
        total_bars: usize,
    ) -> Result<Vec<(usize, usize, usize, usize)>, CvError> {
        let n = self.config.n_folds;
        let chunk = total_bars / (n + 1);
        if chunk == 0 {
            return Err(CvError::InsufficientData);
        }

        let mut splits = Vec::new();
        for fold in 0..n {
            // Training: expanding window [0..(fold+1)*chunk]
            let train_start = 0;
            let train_end = (fold + 1) * chunk;
            // Test: next chunk
            let test_start = train_end;
            let test_end = (fold + 2) * chunk;
            if test_end > total_bars {
                break;
            }
            splits.push((train_start, train_end, test_start, test_end));
        }

        if splits.is_empty() {
            return Err(CvError::InsufficientData);
        }
        Ok(splits)
    }

    /// Run cross-validation. evaluator: (train_start, train_end, test_start, test_end) → (train_score, test_score).
    pub fn run<F>(&self, total_bars: usize, mut evaluator: F) -> Result<CvResult, CvError>
    where
        F: FnMut(usize, usize, usize, usize) -> (f64, f64),
    {
        let splits = self.split(total_bars)?;
        let folds: Vec<FoldResult> = splits
            .into_iter()
            .enumerate()
            .map(|(id, (train_start, train_end, test_start, test_end))| {
                let (train_score, test_score) =
                    evaluator(train_start, train_end, test_start, test_end);
                let overfit_ratio = if test_score.abs() < 1e-10 {
                    f64::INFINITY
                } else {
                    train_score / test_score
                };
                FoldResult {
                    fold_id: id,
                    train_start,
                    train_end,
                    test_start,
                    test_end,
                    train_score,
                    test_score,
                    overfit_ratio,
                }
            })
            .collect();

        let n = folds.len() as f64;
        let mean_test_score = folds.iter().map(|f| f.test_score).sum::<f64>() / n;
        let std_test_score = {
            let variance = folds
                .iter()
                .map(|f| (f.test_score - mean_test_score).powi(2))
                .sum::<f64>()
                / n;
            variance.sqrt()
        };
        let mean_train_score = folds.iter().map(|f| f.train_score).sum::<f64>() / n;

        let overfit_score = if mean_test_score.abs() < 1e-10 {
            f64::INFINITY
        } else {
            mean_train_score / mean_test_score
        };

        let is_overfit = overfit_score > 2.0;

        Ok(CvResult {
            folds,
            mean_test_score,
            std_test_score,
            mean_train_score,
            overfit_score,
            is_overfit,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(n: usize, method: CvMethod) -> CvConfig {
        CvConfig {
            n_folds: n,
            method,
            purge_gap: 0,
        }
    }

    #[test]
    fn kfold_generates_n_folds() {
        let cfg = make_config(5, CvMethod::KFold);
        let cv = CrossValidator::new(cfg);
        let splits = cv.split(100).unwrap();
        assert_eq!(splits.len(), 4); // first fold has no training data, skipped
    }

    #[test]
    fn kfold_index_ranges_correct() {
        let cfg = make_config(4, CvMethod::KFold);
        let cv = CrossValidator::new(cfg);
        let splits = cv.split(100).unwrap();
        // Fold 1: train=[0,25), test=[25,50)
        assert_eq!(splits[0], (0, 25, 25, 50));
        // Fold 2: train=[0,50), test=[50,75)
        assert_eq!(splits[1], (0, 50, 50, 75));
    }

    #[test]
    fn purged_kfold_has_gap() {
        let cfg = CvConfig {
            n_folds: 4,
            method: CvMethod::PurgedKFold,
            purge_gap: 5,
        };
        let cv = CrossValidator::new(cfg);
        let splits = cv.split(100).unwrap();
        for (train_start, train_end, test_start, _test_end) in &splits {
            // Gap must be respected: train_end <= test_start - gap
            assert!(*train_end + 5 <= *test_start || *train_end == 0);
            let _ = train_start; // use it
        }
    }

    #[test]
    fn walk_forward_kfold_expands() {
        let cfg = make_config(4, CvMethod::WalkForwardKFold);
        let cv = CrossValidator::new(cfg);
        let splits = cv.split(100).unwrap();
        assert!(!splits.is_empty());
        // Training window should expand each fold
        for i in 1..splits.len() {
            assert!(splits[i].1 > splits[i - 1].1);
        }
    }

    #[test]
    fn overfit_detection() {
        let cfg = make_config(4, CvMethod::KFold);
        let cv = CrossValidator::new(cfg);
        // train=10.0, test=1.0 → overfit_score = 10 > 2 → overfit
        let result = cv.run(100, |_, _, _, _| (10.0, 1.0)).unwrap();
        assert!(result.is_overfit);
        assert!(result.overfit_score > 2.0);
    }

    #[test]
    fn no_overfit_when_balanced() {
        let cfg = make_config(4, CvMethod::KFold);
        let cv = CrossValidator::new(cfg);
        // train=1.0, test=1.0 → overfit_score = 1.0 ≤ 2
        let result = cv.run(100, |_, _, _, _| (1.0, 1.0)).unwrap();
        assert!(!result.is_overfit);
    }

    #[test]
    fn insufficient_data_error() {
        let cfg = make_config(10, CvMethod::KFold);
        let cv = CrossValidator::new(cfg);
        let result = cv.split(5);
        assert!(matches!(result, Err(CvError::InsufficientData)));
    }

    #[test]
    fn cv_run_produces_correct_stats() {
        let cfg = make_config(4, CvMethod::KFold);
        let cv = CrossValidator::new(cfg);
        let result = cv.run(100, |_, _, _, _| (2.0, 1.0)).unwrap();
        assert!((result.mean_train_score - 2.0).abs() < 1e-10);
        assert!((result.mean_test_score - 1.0).abs() < 1e-10);
        assert!((result.std_test_score).abs() < 1e-10); // all same → std=0
    }
}
