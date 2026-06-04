use std::sync::atomic::Ordering;
use votor_measurement_harness::banking_stage_metrics::BankingStageMetrics;

#[test]
fn banking_stage_metrics_record_max_and_sum() {
    let metrics = BankingStageMetrics::new();

    metrics.record_queue_delay(10);
    metrics.record_queue_delay(25);
    metrics.record_execution(7);
    metrics.record_execution(12);
    metrics.record_batch();

    assert_eq!(metrics.queue_delay_us_sum.load(Ordering::Relaxed), 35);
    assert_eq!(metrics.queue_delay_us_max.load(Ordering::Relaxed), 25);
    assert_eq!(metrics.execution_us_sum.load(Ordering::Relaxed), 19);
    assert_eq!(metrics.execution_us_max.load(Ordering::Relaxed), 12);
    assert_eq!(metrics.processed_batches.load(Ordering::Relaxed), 1);
}
