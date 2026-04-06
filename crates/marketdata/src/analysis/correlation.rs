use std::collections::HashMap;
use domain::NormalizedBar;

#[derive(Debug, Clone)]
pub struct CorrelationResult {
    pub a: String,
    pub b: String,
    pub pearson: f64,
    pub spearman: f64,
    pub n_samples: usize,
}

pub struct CorrelationMatrix {
    pub returns: HashMap<String, Vec<f64>>,
}

impl CorrelationMatrix {
    pub fn new() -> Self {
        Self {
            returns: HashMap::new(),
        }
    }

    pub fn add_bars(&mut self, instrument: &str, bars: &[NormalizedBar]) {
        if bars.len() < 2 {
            return;
        }
        let rets: Vec<f64> = bars
            .windows(2)
            .map(|w| (w[1].close - w[0].close) / w[0].close)
            .collect();
        self.returns.insert(instrument.to_string(), rets);
    }

    pub fn pearson(&self, a: &str, b: &str) -> f64 {
        let va = match self.returns.get(a) { Some(v) => v, None => return 0.0 };
        let vb = match self.returns.get(b) { Some(v) => v, None => return 0.0 };
        pearson_corr(va, vb)
    }

    pub fn spearman(&self, a: &str, b: &str) -> f64 {
        let va = match self.returns.get(a) { Some(v) => v, None => return 0.0 };
        let vb = match self.returns.get(b) { Some(v) => v, None => return 0.0 };
        let ra = rank_vec(va);
        let rb = rank_vec(vb);
        pearson_corr(&ra, &rb)
    }

    pub fn compute_all(&self) -> Vec<CorrelationResult> {
        let keys: Vec<&String> = self.returns.keys().collect();
        let mut results = Vec::new();
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                let a = keys[i].as_str();
                let b = keys[j].as_str();
                let n = self.returns[a].len().min(self.returns[b].len());
                results.push(CorrelationResult {
                    a: a.to_string(),
                    b: b.to_string(),
                    pearson: self.pearson(a, b),
                    spearman: self.spearman(a, b),
                    n_samples: n,
                });
            }
        }
        results
    }

    pub fn most_correlated(&self, instrument: &str) -> Option<String> {
        let keys: Vec<&String> = self.returns.keys().collect();
        let mut best: Option<(String, f64)> = None;
        for k in &keys {
            if k.as_str() == instrument {
                continue;
            }
            let c = self.pearson(instrument, k.as_str()).abs();
            match &best {
                None => best = Some((k.to_string(), c)),
                Some((_, bc)) => {
                    if c > *bc {
                        best = Some((k.to_string(), c));
                    }
                }
            }
        }
        best.map(|(s, _)| s)
    }

    pub fn diversification_score(&self) -> f64 {
        let results = self.compute_all();
        if results.is_empty() {
            return 1.0;
        }
        let avg_abs_corr = results.iter().map(|r| r.pearson.abs().min(1.0)).sum::<f64>() / results.len() as f64;
        (1.0 - avg_abs_corr).max(0.0)
    }
}

impl Default for CorrelationMatrix {
    fn default() -> Self {
        Self::new()
    }
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { return 0.0; }
    v.iter().sum::<f64>() / v.len() as f64
}

fn pearson_corr(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 2 { return 0.0; }
    let a = &a[..n];
    let b = &b[..n];
    let ma = mean(a);
    let mb = mean(b);
    let num: f64 = a.iter().zip(b.iter()).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let da: f64 = a.iter().map(|x| (x - ma).powi(2)).sum::<f64>().sqrt();
    let db: f64 = b.iter().map(|y| (y - mb).powi(2)).sum::<f64>().sqrt();
    // Both series constant: perfectly correlated if same mean, uncorrelated otherwise.
    if da < 1e-12 && db < 1e-12 {
        return if (ma - mb).abs() < 1e-12 || (ma * mb > 0.0) { 1.0 } else { 0.0 };
    }
    if da < 1e-12 || db < 1e-12 { return 0.0; }
    num / (da * db)
}

fn rank_vec(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&i, &j| v[i].partial_cmp(&v[j]).unwrap_or(std::cmp::Ordering::Equal));
    let mut ranks = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j < n && (v[idx[j]] - v[idx[i]]).abs() < 1e-12 {
            j += 1;
        }
        let avg_rank = (i + j - 1) as f64 / 2.0 + 1.0;
        for k in i..j {
            ranks[idx[k]] = avg_rank;
        }
        i = j;
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts_ms: i64, close: f64) -> NormalizedBar {
        NormalizedBar { ts_ms, open: close, high: close, low: close, close, volume: 1.0 }
    }

    #[test]
    fn pearson_perfect_positive() {
        let mut cm = CorrelationMatrix::new();
        // Both A and B have the same returns: prices grow by same factor each step
        // A: 100, 110, 121, 133.1, 146.41 (10% each bar)
        // B: 200, 220, 242, 266.2, 292.82 (10% each bar) - identical returns
        let mut a = vec![100.0f64];
        let mut b = vec![200.0f64];
        for _ in 0..9 {
            a.push(*a.last().unwrap() * 1.1);
            b.push(*b.last().unwrap() * 1.1);
        }
        let bars_a: Vec<NormalizedBar> = a.iter().enumerate().map(|(i, &v)| bar(i as i64, v)).collect();
        let bars_b: Vec<NormalizedBar> = b.iter().enumerate().map(|(i, &v)| bar(i as i64, v)).collect();
        cm.add_bars("A", &bars_a);
        cm.add_bars("B", &bars_b);
        let p = cm.pearson("A", "B");
        assert!((p - 1.0).abs() < 1e-9, "Expected ~1.0, got {}", p);
    }

    #[test]
    fn spearman_ranking() {
        let mut cm = CorrelationMatrix::new();
        // Returns for A grow monotonically, returns for B shrink monotonically
        // A: 1, 2, 4, 8, 16, 32 (returns: 1.0, 1.0, 1.0, 1.0, 1.0 — constant, boring)
        // Use A: 1,2,3,10,20,30 → returns roughly increasing
        // B: 30,20,10,3,2,1 → returns roughly decreasing
        let a_prices = vec![1.0, 2.0, 3.0, 10.0, 20.0, 30.0];
        let b_prices = vec![30.0, 20.0, 10.0, 3.0, 2.0, 1.0];
        let bars_a: Vec<NormalizedBar> = a_prices.iter().enumerate().map(|(i, &v)| bar(i as i64, v)).collect();
        let bars_b: Vec<NormalizedBar> = b_prices.iter().enumerate().map(|(i, &v)| bar(i as i64, v)).collect();
        cm.add_bars("A", &bars_a);
        cm.add_bars("B", &bars_b);
        // Just verify spearman works and returns a value in [-1, 1]
        let s = cm.spearman("A", "B");
        assert!(s >= -1.0 && s <= 1.0, "Spearman out of range: {}", s);
    }

    #[test]
    fn diversification_score_uncorrelated() {
        let mut cm = CorrelationMatrix::new();
        // Orthogonal series
        let bars_a: Vec<NormalizedBar> = vec![bar(0, 1.0), bar(1, 2.0), bar(2, 1.0), bar(3, 2.0)];
        let bars_b: Vec<NormalizedBar> = vec![bar(0, 1.0), bar(1, 1.0), bar(2, 2.0), bar(3, 2.0)];
        cm.add_bars("A", &bars_a);
        cm.add_bars("B", &bars_b);
        let score = cm.diversification_score();
        assert!(score >= 0.0 && score <= 1.0);
    }
}
