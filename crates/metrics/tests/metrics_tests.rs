use metrics::{MetricsSummary, paper_summary, total_return};
use rust_decimal_macros::dec;

#[test]
fn total_return_uses_start_and_end_equity() {
    assert_eq!(total_return(dec!(100), dec!(125)), dec!(0.25));
}

#[test]
fn paper_summary_formats_total_return_and_counts() {
    assert_eq!(
        paper_summary(2, 1, dec!(100), dec!(125)),
        MetricsSummary {
            total_return: "0.25".to_string(),
            order_count: 2,
            fill_count: 1,
        }
    );
}
