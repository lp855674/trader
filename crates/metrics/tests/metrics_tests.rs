use metrics::{
    MetricsSummary, max_drawdown, paper_summary, sharpe_ratio, sortino_ratio, total_return,
    win_rate,
};
use rust_decimal_macros::dec;

#[test]
fn total_return_uses_start_and_end_equity() {
    assert_eq!(total_return(dec!(100), dec!(125)), dec!(0.25));
}

#[test]
fn paper_summary_formats_total_return_and_counts() {
    let equity = [dec!(100), dec!(125)];
    let returns = [dec!(0.25)];

    assert_eq!(
        paper_summary(2, 1, &equity, &returns),
        MetricsSummary {
            total_return: "0.25".to_string(),
            sharpe: "0".to_string(),
            sortino: "0".to_string(),
            max_drawdown: "0".to_string(),
            win_rate: "1".to_string(),
            order_count: 2,
            fill_count: 1,
        }
    );
}

#[test]
fn paper_summary_defaults_empty_series_to_zero_metrics() {
    assert_eq!(
        paper_summary(2, 1, &[], &[]),
        MetricsSummary {
            total_return: "0".to_string(),
            sharpe: "0".to_string(),
            sortino: "0".to_string(),
            max_drawdown: "0".to_string(),
            win_rate: "0".to_string(),
            order_count: 2,
            fill_count: 1,
        }
    );
}

#[test]
fn max_drawdown_uses_peak_to_trough_decline() {
    let equity = [dec!(100), dec!(120), dec!(90), dec!(110)];

    assert_eq!(max_drawdown(&equity), dec!(0.25));
}

#[test]
fn win_rate_counts_positive_returns() {
    let returns = [dec!(0.10), dec!(-0.05), dec!(0), dec!(0.20)];

    assert_eq!(win_rate(&returns), dec!(0.5));
}

#[test]
fn sharpe_ratio_uses_mean_over_population_deviation() {
    let returns = [dec!(0.02), dec!(0.00)];

    assert_eq!(sharpe_ratio(&returns), dec!(1));
}

#[test]
fn sortino_ratio_uses_downside_deviation_only() {
    let returns = [dec!(0.03), dec!(-0.01)];

    assert_eq!(sortino_ratio(&returns), dec!(1));
}

#[test]
fn paper_summary_includes_v1_performance_metrics() {
    let equity = [dec!(100), dec!(120), dec!(90), dec!(110)];
    let returns = [dec!(0.20), dec!(-0.25), dec!(0.111111111)];

    let summary = paper_summary(3, 2, &equity, &returns);

    assert_eq!(summary.total_return, "0.1");
    assert_eq!(summary.max_drawdown, "0.25");
    assert_eq!(summary.win_rate, "0.6666666666666666666666666667");
    assert_eq!(summary.order_count, 3);
    assert_eq!(summary.fill_count, 2);
}
