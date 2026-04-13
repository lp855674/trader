// Slippage model accuracy and performance benchmarks.
// Run with: cargo bench -p exec

use std::hint::black_box;
use std::time::Instant;

/// Minimal fixed-slippage model: slippage = bps/10000 * price
fn fixed_slippage(price: f64, bps: f64) -> f64 {
    price * bps / 10_000.0
}

/// Volume-based slippage: scales with sqrt of quantity fraction
fn volume_slippage(price: f64, qty: f64, adv: f64, impact_factor: f64) -> f64 {
    let participation = qty / adv;
    price * impact_factor * participation.sqrt()
}

/// Market depth slippage: linear with order-book consumption
fn depth_slippage(price: f64, qty: f64, depth_at_touch: f64, spread: f64) -> f64 {
    if depth_at_touch <= 0.0 {
        return spread;
    }
    let consumption = (qty / depth_at_touch).min(1.0);
    spread * consumption
}

fn bench_fixed_slippage(n: usize) -> f64 {
    let start = Instant::now();
    let mut total = 0.0f64;
    for i in 0..n {
        total += black_box(fixed_slippage(100.0 + i as f64 * 0.01, 5.0));
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "fixed_slippage: {n} iters in {:.2}ms, avg {:.1}ns/iter, total={total:.4}",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
    total
}

fn bench_volume_slippage(n: usize) -> f64 {
    let start = Instant::now();
    let mut total = 0.0f64;
    for i in 0..n {
        let qty = 100.0 + (i % 500) as f64;
        total += black_box(volume_slippage(150.0, qty, 50_000.0, 0.1));
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "volume_slippage: {n} iters in {:.2}ms, avg {:.1}ns/iter, total={total:.4}",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
    total
}

fn bench_depth_slippage(n: usize) -> f64 {
    let start = Instant::now();
    let mut total = 0.0f64;
    for i in 0..n {
        let qty = 50.0 + (i % 200) as f64;
        total += black_box(depth_slippage(200.0, qty, 10_000.0, 0.02));
    }
    let elapsed = start.elapsed().as_nanos() as f64;
    println!(
        "depth_slippage: {n} iters in {:.2}ms, avg {:.1}ns/iter, total={total:.4}",
        elapsed / 1_000_000.0,
        elapsed / n as f64
    );
    total
}

fn bench_model_accuracy() {
    // Accuracy test: expected cost for 1000-share order in $150 stock with 5bps fixed slippage
    let expected = 150.0 * 5.0 / 10_000.0; // $0.075 per share
    let actual = fixed_slippage(150.0, 5.0);
    let error = (actual - expected).abs();
    assert!(
        error < 1e-10,
        "model accuracy failed: expected {expected}, got {actual}"
    );
    println!("model_accuracy: fixed slippage error={error:.2e} ✓");

    // Volume slippage: 200 shares vs 10k ADV, impact 10bps per sqrt(participation)
    let vol_slip = volume_slippage(100.0, 200.0, 10_000.0, 0.01);
    let participation = 200.0f64 / 10_000.0;
    let expected_vol = 100.0 * 0.01 * participation.sqrt();
    let vol_error = (vol_slip - expected_vol).abs();
    assert!(vol_error < 1e-10, "volume slippage accuracy failed");
    println!("model_accuracy: volume slippage error={vol_error:.2e} ✓");
}

fn main() {
    println!("=== Slippage Model Benchmarks ===\n");

    bench_model_accuracy();
    println!();

    let n = 100_000;
    bench_fixed_slippage(n);
    bench_volume_slippage(n);
    bench_depth_slippage(n);

    println!("\n=== Done ===");
}
