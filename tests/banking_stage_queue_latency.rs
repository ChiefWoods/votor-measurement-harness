use std::{thread, time::Duration};
use votor_measurement_harness::banking_stage_trace::EnqueueTimestamp;

#[test]
fn queue_delay_is_measured_from_ingress() {
    let enqueued_at = EnqueueTimestamp::now();

    thread::sleep(Duration::from_millis(5));

    assert!(enqueued_at.elapsed_us() >= 5_000);
}
