use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct BankingStageMetrics {
    pub queue_delay_us_sum: AtomicU64,
    pub queue_delay_us_max: AtomicU64,
    pub build_batch_us_sum: AtomicU64,
    pub build_batch_us_max: AtomicU64,
    pub lock_us_sum: AtomicU64,
    pub lock_us_max: AtomicU64,
    pub execution_us_sum: AtomicU64,
    pub execution_us_max: AtomicU64,
    pub record_us_sum: AtomicU64,
    pub record_us_max: AtomicU64,
    pub processed_batches: AtomicU64,
}

impl BankingStageMetrics {
    pub const fn new() -> Self {
        Self {
            queue_delay_us_sum: AtomicU64::new(0),
            queue_delay_us_max: AtomicU64::new(0),
            build_batch_us_sum: AtomicU64::new(0),
            build_batch_us_max: AtomicU64::new(0),
            lock_us_sum: AtomicU64::new(0),
            lock_us_max: AtomicU64::new(0),
            execution_us_sum: AtomicU64::new(0),
            execution_us_max: AtomicU64::new(0),
            record_us_sum: AtomicU64::new(0),
            record_us_max: AtomicU64::new(0),
            processed_batches: AtomicU64::new(0),
        }
    }

    pub fn record_queue_delay(&self, value: u64) {
        self.queue_delay_us_sum.fetch_add(value, Ordering::Relaxed);
        self.queue_delay_us_max.fetch_max(value, Ordering::Relaxed);
    }

    pub fn record_build_batch(&self, value: u64) {
        self.build_batch_us_sum.fetch_add(value, Ordering::Relaxed);
        self.build_batch_us_max.fetch_max(value, Ordering::Relaxed);
    }

    pub fn record_lock(&self, value: u64) {
        self.lock_us_sum.fetch_add(value, Ordering::Relaxed);
        self.lock_us_max.fetch_max(value, Ordering::Relaxed);
    }

    pub fn record_execution(&self, value: u64) {
        self.execution_us_sum.fetch_add(value, Ordering::Relaxed);
        self.execution_us_max.fetch_max(value, Ordering::Relaxed);
    }

    pub fn record_results(&self, value: u64) {
        self.record_us_sum.fetch_add(value, Ordering::Relaxed);
        self.record_us_max.fetch_max(value, Ordering::Relaxed);
    }

    pub fn record_batch(&self) {
        self.processed_batches.fetch_add(1, Ordering::Relaxed);
    }
}
