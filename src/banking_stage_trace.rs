use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{Span, field, info_span};

#[derive(Clone, Copy, Debug)]
pub struct EnqueueTimestamp {
    pub at: Instant,
}

impl EnqueueTimestamp {
    pub fn now() -> Self {
        Self { at: Instant::now() }
    }

    pub fn elapsed_us(self) -> u64 {
        self.at.elapsed().as_micros() as u64
    }
}

pub struct StageTimer {
    start: Instant,
    span: Span,
}

impl StageTimer {
    pub fn start(name: &'static str, profile: &str, batch_size: usize) -> Self {
        let span = info_span!(
            "banking_stage_segment",
            segment = name,
            profile = profile,
            elapsed_us = field::Empty,
            queue_delay_us = field::Empty,
            batch_size = batch_size as u64,
        );

        Self {
            start: Instant::now(),
            span,
        }
    }

    pub fn finish(self) -> u64 {
        let elapsed = self.start.elapsed().as_micros() as u64;
        self.span.record("elapsed_us", elapsed);
        elapsed
    }

    pub fn record_queue_delay(&self, value: u64) {
        self.span.record("queue_delay_us", value);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankingLatencyRecord {
    /// Harness profile that produced this record.
    pub profile: String,
    /// Number of simulated transactions/items in this batch.
    pub batch_size: usize,
    /// Time from simulated enqueue/ingress until first processing attempt.
    pub queue_delay_us: u64,
    /// Time spent constructing the simulated batch.
    pub build_batch_us: u64,
    /// Time spent in account-locking and scheduling-shaped work.
    pub lock_us: u64,
    /// Time spent in transaction-execution-shaped work.
    pub execution_us: u64,
    /// Time spent recording results and bookkeeping.
    pub record_us: u64,
    /// End-to-end time for the simulated batch pipeline.
    pub total_us: u64,
    /// Monotonic batch index used as a slot-like identifier.
    pub slot: u64,
    /// Thread that emitted the record.
    pub thread: String,
}
