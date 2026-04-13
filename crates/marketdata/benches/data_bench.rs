// Market data performance benchmarks.
// Run with: cargo bench -p marketdata

use std::hint::black_box;
use std::time::Instant;

fn bench_correlation_matrix(n: usize) {
    use domain::NormalizedBar;
    use marketdata::analysis::correlation::CorrelationMatrix;

    fn bar(i: usize, price: f64) -> NormalizedBar {
        NormalizedBar {
            ts_ms: i as i64,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 1.0,
        }
    }

    let mut cm = CorrelationMatrix::new();
    let bars_a: Vec<NormalizedBar> = (0..n).map(|i| bar(i, 100.0 + i as f64 * 0.1)).collect();
    let bars_b: Vec<NormalizedBar> = (0..n).map(|i| bar(i, 200.0 - i as f64 * 0.05)).collect();

    let start = Instant::now();
    cm.add_bars("A", &bars_a);
    cm.add_bars("B", &bars_b);
    let _ = black_box(cm.pearson("A", "B"));
    let _ = black_box(cm.spearman("A", "B"));
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "correlation_matrix ({n} bars): {:.2}ms",
        elapsed / 1_000_000.0
    );
}

fn bench_bar_ingestion(n: usize) {
    use domain::NormalizedBar;
    use marketdata::analysis::correlation::CorrelationMatrix;

    let bars: Vec<NormalizedBar> = (0..n)
        .map(|i| NormalizedBar {
            ts_ms: i as i64 * 60_000,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5 + (i % 10) as f64 * 0.1,
            volume: 1000.0 + (i % 50) as f64,
        })
        .collect();

    let start = Instant::now();
    let mut cm = CorrelationMatrix::new();
    cm.add_bars(black_box("AAPL"), &bars);
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "bar_ingestion ({n} bars): {:.2}ms, {:.1}ns/bar",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
}

fn bench_diversification_score(n_instruments: usize, n_bars: usize) {
    use domain::NormalizedBar;
    use marketdata::analysis::correlation::CorrelationMatrix;

    let mut cm = CorrelationMatrix::new();
    for inst in 0..n_instruments {
        let bars: Vec<NormalizedBar> = (0..n_bars)
            .map(|i| NormalizedBar {
                ts_ms: i as i64,
                open: 100.0,
                high: 101.0,
                low: 99.0,
                close: 100.0 + (i * inst) as f64 * 0.001,
                volume: 1.0,
            })
            .collect();
        cm.add_bars(&format!("inst_{}", inst), &bars);
    }

    let start = Instant::now();
    let score = black_box(cm.diversification_score());
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "diversification_score ({n_instruments} instruments, {n_bars} bars): {:.2}ms, score={:.4}",
        elapsed / 1_000_000.0,
        score
    );
}

fn main() {
    println!("=== Market Data Performance Benchmarks ===\n");

    bench_bar_ingestion(10_000);
    bench_bar_ingestion(100_000);

    bench_correlation_matrix(1_000);
    bench_correlation_matrix(10_000);

    bench_diversification_score(5, 500);
    bench_diversification_score(10, 500);

    println!("\n=== Done ===");
}
