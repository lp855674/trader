// Strategy evaluation benchmarks.
//
// Run with: cargo bench -p strategy

use std::{collections::HashMap, sync::Arc};

use criterion::{criterion_group, criterion_main, Criterion};

use domain::{InstrumentId, Side, Venue};

use strategy::core::{
    combinator::{Pipeline, QuantityScaler, SideFilter, WeightedAverage},
    combinators::DynamicWeightedAverage,
    metrics::{MeteredStrategy, MetricsRegistry},
    r#trait::{Signal, StrategyContext, StrategyError, Strategy},
};

// ─── Helper strategies ────────────────────────────────────────────────────────

struct AlwaysBuy;

impl Strategy for AlwaysBuy {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        Ok(Some(Signal::new(
            ctx.instrument.clone(),
            Side::Buy,
            1.0,
            Some(100.0),
            ctx.ts_ms,
            "always_buy".into(),
            HashMap::new(),
        )))
    }

    fn name(&self) -> &str {
        "always_buy"
    }
}

struct AlwaysSell;

impl Strategy for AlwaysSell {
    fn evaluate(&self, ctx: &StrategyContext) -> Result<Option<Signal>, StrategyError> {
        Ok(Some(Signal::new(
            ctx.instrument.clone(),
            Side::Sell,
            0.5,
            Some(99.0),
            ctx.ts_ms,
            "always_sell".into(),
            HashMap::new(),
        )))
    }

    fn name(&self) -> &str {
        "always_sell"
    }
}

fn make_ctx() -> StrategyContext {
    let mut ctx = StrategyContext::new(InstrumentId::new(Venue::Crypto, "BTC"), 1_000_000);
    ctx.last_bar_close = Some(100.0);
    ctx
}

// ─── Benchmarks ──────────────────────────────────────────────────────────────

fn bench_always_buy(c: &mut Criterion) {
    let ctx = make_ctx();
    let strategy = AlwaysBuy;
    c.bench_function("AlwaysBuy::evaluate", |b| {
        b.iter(|| strategy.evaluate(&ctx).unwrap())
    });
}

fn bench_weighted_average_3(c: &mut Criterion) {
    let ctx = make_ctx();
    let wa = WeightedAverage::new(
        vec![
            (Box::new(AlwaysBuy) as Box<dyn Strategy>, 1.0),
            (Box::new(AlwaysBuy), 2.0),
            (Box::new(AlwaysSell), 0.5),
        ],
        "wa",
    );
    c.bench_function("WeightedAverage_3::evaluate", |b| {
        b.iter(|| wa.evaluate(&ctx).unwrap())
    });
}

fn bench_dynamic_weighted_average(c: &mut Criterion) {
    let ctx = make_ctx();
    let (dwa, _tracker) = DynamicWeightedAverage::with_new_tracker(
        vec![
            Box::new(AlwaysBuy) as Box<dyn Strategy>,
            Box::new(AlwaysSell),
            Box::new(AlwaysBuy),
        ],
        20,
        "dwa",
    );
    c.bench_function("DynamicWeightedAverage::evaluate", |b| {
        b.iter(|| dwa.evaluate(&ctx).unwrap())
    });
}

fn bench_pipeline_3_filters(c: &mut Criterion) {
    let ctx = make_ctx();
    let pipeline = Pipeline::new(
        Box::new(AlwaysBuy),
        vec![
            Box::new(QuantityScaler::new(1.0, "scale1")) as Box<dyn strategy::core::combinator::SignalFilter>,
            Box::new(QuantityScaler::new(1.0, "scale2")),
            Box::new(SideFilter::new(Side::Buy, "buy_only")),
        ],
        "pipeline",
    );
    c.bench_function("Pipeline_3_filters::evaluate", |b| {
        b.iter(|| pipeline.evaluate(&ctx).unwrap())
    });
}

fn bench_metered_strategy_overhead(c: &mut Criterion) {
    let ctx = make_ctx();
    let registry = Arc::new(MetricsRegistry::new());

    let bare = AlwaysBuy;
    let metered = MeteredStrategy::new(Box::new(AlwaysBuy), Arc::clone(&registry));

    let mut group = c.benchmark_group("MeteredStrategy overhead");
    group.bench_function("bare AlwaysBuy", |b| {
        b.iter(|| bare.evaluate(&ctx).unwrap())
    });
    group.bench_function("MeteredStrategy wrapping AlwaysBuy", |b| {
        b.iter(|| metered.evaluate(&ctx).unwrap())
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_always_buy,
    bench_weighted_average_3,
    bench_dynamic_weighted_average,
    bench_pipeline_3_filters,
    bench_metered_strategy_overhead,
);
criterion_main!(benches);
