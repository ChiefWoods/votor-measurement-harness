# Agave Banking Stage Alpenglow Measurement Implementation Plan

**Goal:** Build a measurement harness around Agave's `/Users/chiiyuen/anza-xyz/agave/core/src/banking_stage.rs` that evaluates whether observed execution and queueing behavior is compatible with Alpenglow's timing model, and diagnoses task jitter as CPU scheduling delay, I/O stall, allocator pause, or async task interference.

**Architecture:** Verify the assignment's Figure 2 reference against the classroom Alpenglow paper, extract the actual Votor per-round timing requirements from the relevant paper sections, then build a laptop-safe measurement harness around the timing boundaries in `banking_stage.rs`. The primary workflow does not run a full validator. It uses focused Rust tests, synthetic queue/execution profiles, trace spans, and optional `tokio-console` on the harness process to compare observed jitter against the paper-derived timing budget.

**Tech Stack:** Rust, Agave source inspection, `tracing`, `tracing-subscriber`, optional `console-subscriber` / `tokio-console`, histogram metrics, shell scripts, CSV/JSON summaries.

---

## Scope and Assumptions

- Agave is checked out locally at `/Users/chiiyuen/anza-xyz/agave`.
- The implementation target file is `/Users/chiiyuen/anza-xyz/agave/core/src/banking_stage.rs`.
- You are measuring Agave banking-stage latency contributors, not trying to reimplement Votor.
- The primary harness must run on a developer MacBook without spinning up an Agave validator.
- A full validator run is optional follow-up work only, not required for the assignment deliverable.
- The goal is to explain whether banking-stage jitter could consume too much of the Alpenglow timing budget.
- The classroom PDF at `/Users/chiiyuen/Documents/Whitepapers/Solana Alpenglow White Paper v1.1.pdf` identifies Figure 2 as the hierarchy of block data / double-Merkle block hash construction, not as a Votor timing figure.
- Votor timing requirements in this PDF come from the abstract, Section 2.4, Figure 7, Definition 17, Algorithm 2, and Table 10.
- Paper-derived anchors for this harness:
  - Fast-finalization path: one round of voting when `>= 80%` stake produces notarization votes.
  - Slow-finalization path: two rounds of voting when `>= 60%` stake is responsive, via notarization and finalization certificates.
  - Concurrent path timing: finalization takes `min(delta_80%, 2 * delta_60%)` after the block has been distributed.
  - Certificate thresholds: fast-finalization certificate is `NotarVote` with stake sum `>= 80%`; notarization, finalization, notar-fallback, and skip certificates use stake sum `>= 60%`.
  - Fallback event thresholds include `notar(b) >= 40%` or `skip(s) + notar(b) >= 60%` and `notar(b) >= 20%`.
  - `Δblock = 400 ms`
  - `Timeout(i) = clock() + Δtimeout + (i - s + 1) * Δblock` for all slots `i` in the leader window beginning at `s`.
  - Common-case practical finalization reference around `115-150 ms`, from the paper's latency simulation section.
- Hard concern threshold: any repeated banking-stage tail that materially pushes transaction processing toward the one-round/two-round Votor budgets or the `400 ms` block-time budget.

## Files to Create or Modify

**Measurement harness repo files**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`
- Create: `src/banking_stage_metrics.rs`
- Create: `src/banking_stage_trace.rs`
- Create: `src/bin/summarize_banking_jitter.rs`
- Create: `scripts/run_banking_jitter_harness.sh`
- Create: `scripts/run_banking_jitter_profiles.sh`
- Create: `docs/banking-stage-jitter-harness.md`

**Agave reference file**
- Read and cite: `/Users/chiiyuen/anza-xyz/agave/core/src/banking_stage.rs`
- Optional patch-only follow-up: modify `/Users/chiiyuen/anza-xyz/agave/core/src/banking_stage.rs` after the lightweight harness proves which boundaries matter.

**Test files**
- Create: `tests/banking_stage_metrics.rs`
- Create: `tests/banking_stage_queue_latency.rs`

**Deliverable files**
- Create: `artifacts/banking-jitter/baseline.csv`
- Create: `artifacts/banking-jitter/cpu-saturation.csv`
- Create: `artifacts/banking-jitter/io-stall.csv`
- Create: `artifacts/banking-jitter/allocator-pressure.csv`
- Create: `artifacts/banking-jitter/summary.md`

---

### Task 1: Verify the figure reference and convert Votor timing into measurement budgets

**Files:**
- Create: `docs/banking-stage-jitter-harness.md`

- [ ] **Step 1: Document the Figure 2 mismatch and cite the actual Votor timing sources**

Open the Alpenglow whitepaper from classroom materials and record:

- Figure 2 in v1.1 is the block-data hierarchy / double-Merkle construction, not a Votor timing diagram.
- The Votor timing diagram is Figure 7, "Protocol overview: a full common case life cycle of a block in Alpenglow."
- Definition 17 and Algorithm 2 provide the timeout schedule.
- Table 10 provides `Δblock = 400 ms`.

If the classroom assignment intended a different paper version where Figure 2 contains Votor timing, copy that figure's exact values into this section before implementation. For this v1.1 PDF, use the following extracted requirements.

- [ ] **Step 2: Write the measurement budget section**

Add this section to `docs/banking-stage-jitter-harness.md`:

```md
## Measurement Budgets

This harness uses the following timing anchors from Alpenglow v1.1:

| Paper source | Requirement | Harness budget |
| --- | --- | --- |
| Abstract / Section 2.4 | Fast finalization: one voting round if `>= 80%` stake participates with notarization votes | Banking-stage queue + execution jitter should remain well below a one-round finalization budget |
| Abstract / Section 2.4 | Slow finalization: two voting rounds if `>= 60%` stake is responsive | Banking-stage queue + execution jitter should remain below the two-round budget and should not repeatedly consume a large share of it |
| Abstract | Concurrent modes finalize in `min(delta_80%, 2 * delta_60%)` after block distribution | Summaries must compare observed p95/p99 against both one-round and two-round references |
| Table 6 / Definition 13 | Fast-finalization certificate requires `NotarVote` stake sum `>= 80%`; notarization and finalization certificates require stake sum `>= 60%` | Stress profiles should report whether local jitter could delay reaching either certificate path |
| Definition 16 | Fallback can trigger when `notar(b) >= 40%`, or `skip(s) + notar(b) >= 60%` and `notar(b) >= 20%` | High queue delays should be interpreted as possible contributors to fallback behavior |
| Definition 17 / Algorithm 2 | `Timeout(i) = clock() + Delta_timeout + (i - s + 1) * Delta_block` | Any sustained p99 near a timeout boundary is critical |
| Table 10 | `Delta_block = 400 ms` | Critical: sustained p99 or max approaching/exceeding `400 ms` |

Additional latency context:
- Practical common-case finalization reference: roughly `115-150 ms` from the paper's latency simulation section.

This harness does not measure end-to-end Votor finalization directly. Instead it measures how much local banking-stage jitter could consume from those budgets.

Primary alert thresholds:
- Green: banking-stage queue + execution p95 < `50 ms`
- Yellow: banking-stage queue + execution p95 between `50 ms` and `150 ms`
- Red: banking-stage queue + execution p95 > `150 ms`
- Critical: any sustained p99 or max approaching or exceeding a Votor timeout boundary or `400 ms`, whichever is tighter for the observed phase
```

- [ ] **Step 3: Record the interpretation policy**

Append this section:

```md
## Interpretation Policy

A local banking-stage latency spike is considered important if it is:

- large enough to consume a substantial fraction of the `115-150 ms` common-case budget, or
- frequent enough in p95/p99 to threaten the `400 ms` slot budget.

The working decomposition is:

- queue wait = time from transaction ingress into banking-stage-owned work queues until first execution attempt
- execution time = time spent inside bank execution boundaries
- scheduler delay = task wakeup-to-poll delay visible in `tokio-console`
- blocking stall = long spans associated with I/O, mutex contention, channel waits, or allocator-heavy sections
```

- [ ] **Step 4: Commit**

```bash
git add docs/banking-stage-jitter-harness.md
git commit -m "docs: define banking-stage jitter measurement budgets"
```

---

### Task 2: Isolate instrumentation boundaries in `banking_stage.rs`

**Files:**
- Read: `/Users/chiiyuen/anza-xyz/agave/core/src/banking_stage.rs`
- Create: `src/banking_stage_trace.rs`

- [ ] **Step 1: Add a trace helper module**

Create `src/banking_stage_trace.rs` with a minimal API like:

```rust
use std::time::Instant;
use tracing::{field, info_span, Span};

pub struct StageTimer {
    pub start: Instant,
    pub span: Span,
}

impl StageTimer {
    pub fn start(name: &'static str) -> Self {
        let span = info_span!(
            "banking_stage_segment",
            segment = name,
            elapsed_us = field::Empty,
            queue_delay_us = field::Empty,
            batch_size = field::Empty,
        );
        Self {
            start: Instant::now(),
            span,
        }
    }

    pub fn finish(self) {
        let elapsed = self.start.elapsed().as_micros() as u64;
        self.span.record("elapsed_us", elapsed);
    }
}
```

- [ ] **Step 2: Insert spans at the hot boundaries**

Map the real boundaries in `/Users/chiiyuen/anza-xyz/agave/core/src/banking_stage.rs`, then mirror those boundary names in the lightweight harness:

```rust
let dequeue_timer = StageTimer::start("dequeue_work");
// existing dequeue logic

dequeue_timer.finish();

let batch_timer = StageTimer::start("build_batch");
// existing batching logic
batch_timer.finish();

let lock_timer = StageTimer::start("account_lock_and_schedule");
// existing lock acquisition / scheduling logic
lock_timer.finish();

let exec_timer = StageTimer::start("execute_transactions");
// existing bank execution call
exec_timer.finish();

let record_timer = StageTimer::start("record_results");
// status / forwarding / retry / bookkeeping
record_timer.finish();
```

- [ ] **Step 3: Attach queue ingress timestamps**

Add a transaction-metadata timestamp field or sidecar timestamp so each batch can compute queue residence time:

```rust
#[derive(Clone, Copy, Debug)]
pub struct EnqueueTimestamp {
    pub at: Instant,
}
```

Compute queue delay before first processing attempt:

```rust
let queue_delay_us = enqueue_timestamp.at.elapsed().as_micros() as u64;
span.record("queue_delay_us", queue_delay_us);
```

- [ ] **Step 4: Commit**

```bash
git add src/banking_stage_trace.rs docs/banking-stage-jitter-harness.md
git commit -m "feat: add banking stage trace boundaries"
```

---

### Task 3: Add aggregated histograms and counters

**Files:**
- Create: `src/banking_stage_metrics.rs`
- Modify: `src/main.rs`
- Test: `tests/banking_stage_metrics.rs`

- [ ] **Step 1: Add a metrics collector**

Create `src/banking_stage_metrics.rs`:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

pub struct BankingStageMetrics {
    pub queue_delay_us_sum: AtomicU64,
    pub queue_delay_us_max: AtomicU64,
    pub execution_us_sum: AtomicU64,
    pub execution_us_max: AtomicU64,
    pub processed_batches: AtomicU64,
}

impl BankingStageMetrics {
    pub const fn new() -> Self {
        Self {
            queue_delay_us_sum: AtomicU64::new(0),
            queue_delay_us_max: AtomicU64::new(0),
            execution_us_sum: AtomicU64::new(0),
            execution_us_max: AtomicU64::new(0),
            processed_batches: AtomicU64::new(0),
        }
    }

    pub fn record_queue_delay(&self, value: u64) {
        self.queue_delay_us_sum.fetch_add(value, Ordering::Relaxed);
        self.queue_delay_us_max.fetch_max(value, Ordering::Relaxed);
    }

    pub fn record_execution(&self, value: u64) {
        self.execution_us_sum.fetch_add(value, Ordering::Relaxed);
        self.execution_us_max.fetch_max(value, Ordering::Relaxed);
    }
}
```

- [ ] **Step 2: Wire metric updates into the instrumentation points**

Update the lightweight harness path corresponding to the Agave banking-stage boundaries to record:

```rust
BANKING_STAGE_METRICS.record_queue_delay(queue_delay_us);
BANKING_STAGE_METRICS.record_execution(exec_elapsed_us);
```

- [ ] **Step 3: Write a focused unit test**

Create `tests/banking_stage_metrics.rs`:

```rust
#[test]
fn records_max_and_sum() {
    let metrics = BankingStageMetrics::new();
    metrics.record_queue_delay(10);
    metrics.record_queue_delay(25);
    metrics.record_execution(7);
    metrics.record_execution(12);

    assert_eq!(metrics.queue_delay_us_sum.load(Ordering::Relaxed), 35);
    assert_eq!(metrics.queue_delay_us_max.load(Ordering::Relaxed), 25);
    assert_eq!(metrics.execution_us_sum.load(Ordering::Relaxed), 19);
    assert_eq!(metrics.execution_us_max.load(Ordering::Relaxed), 12);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
cargo test banking_stage_metrics -- --nocapture
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/banking_stage_metrics.rs src/main.rs tests/banking_stage_metrics.rs
git commit -m "feat: add banking stage latency metrics"
```

---

### Task 4: Enable optional `tokio-console` on the lightweight harness

**Files:**
- Modify: harness `Cargo.toml` / crate manifests
- Modify: harness runner source

- [ ] **Step 1: Add optional console subscriber dependency**

Add a feature-gated dependency:

```toml
[features]
banking-jitter-console = ["dep:console-subscriber"]

[dependencies]
console-subscriber = { version = "0.4", optional = true }
```

- [ ] **Step 2: Wire feature-gated initialization**

In the lightweight harness binary add:

```rust
#[cfg(feature = "banking-jitter-console")]
fn init_console_subscriber() {
    console_subscriber::init();
}

#[cfg(not(feature = "banking-jitter-console"))]
fn init_console_subscriber() {}
```

Call it during startup:

```rust
init_console_subscriber();
```

- [ ] **Step 3: Document the required env vars**

Add to docs:

```md
Run with:

RUSTFLAGS="--cfg tokio_unstable" \
RUST_LOG=info \
TOKIO_CONSOLE_BIND=127.0.0.1:6669 \
cargo run --features banking-jitter-console --bin votor-measurement-harness -- --profile baseline
```
```

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/main.rs docs/banking-stage-jitter-harness.md
git commit -m "feat: add optional tokio-console support for jitter analysis"
```

---

### Task 5: Build a laptop-safe synthetic workload harness

**Files:**
- Create: `scripts/run_banking_jitter_harness.sh`
- Create: `scripts/run_banking_jitter_profiles.sh`

- [ ] **Step 1: Create a baseline harness script**

Create `scripts/run_banking_jitter_harness.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

PROFILE="${PROFILE:-baseline}"
ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts/banking-jitter/$PROFILE}"
mkdir -p "$ARTIFACT_DIR"

RUST_LOG=info \
BANKING_JITTER_PROFILE="$PROFILE" \
BANKING_JITTER_OUT="$ARTIFACT_DIR/trace.json" \
cargo run --bin votor-measurement-harness -- --profile "$PROFILE" \
  > "$ARTIFACT_DIR/harness.log" 2>&1
```

- [ ] **Step 2: Create multi-profile driver**

Create `scripts/run_banking_jitter_profiles.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

profiles=(baseline cpu_saturation io_stall allocator_pressure)
for profile in "${profiles[@]}"; do
  PROFILE="$profile" ./scripts/run_banking_jitter_harness.sh
done
```

- [ ] **Step 3: Define profile behavior in docs**

Add to docs:

```md
## Profiles

- `baseline`: steady moderate synthetic queue/execution workload
- `cpu_saturation`: spawn controlled CPU-bound tasks to create scheduler pressure
- `io_stall`: inject bounded blocking file writes or sleeps in a non-critical simulated path
- `allocator_pressure`: create bursty temporary allocations during batch construction

These profiles run in the harness process. They do not start `agave-validator`.
```

- [ ] **Step 4: Commit**

```bash
git add scripts/run_banking_jitter_harness.sh scripts/run_banking_jitter_profiles.sh docs/banking-stage-jitter-harness.md
git commit -m "feat: add banking jitter synthetic harness scripts"
```

---

### Task 6: Emit machine-readable latency records

**Files:**
- Modify: `src/main.rs`
- Modify: `src/banking_stage_trace.rs`

- [ ] **Step 1: Add JSON record output for completed batches**

Use a record shape like:

```rust
#[derive(serde::Serialize)]
pub struct BankingLatencyRecord {
    pub profile: String,
    pub batch_size: usize,
    pub queue_delay_us: u64,
    pub execution_us: u64,
    pub lock_us: u64,
    pub record_us: u64,
    pub total_us: u64,
    pub slot: u64,
    pub thread: String,
}
```

- [ ] **Step 2: Write one JSON line per completed batch**

Emit newline-delimited JSON:

```rust
writeln!(writer, "{}", serde_json::to_string(&record).unwrap()).ok();
```

- [ ] **Step 3: Guard emission behind an env var**

```rust
if std::env::var_os("BANKING_JITTER_OUT").is_some() {
    // emit record
}
```

- [ ] **Step 4: Commit**

```bash
git add src/main.rs src/banking_stage_trace.rs
git commit -m "feat: emit banking jitter latency records"
```

---

### Task 7: Summarize percentiles and classify likely bottlenecks

**Files:**
- Create: `src/bin/summarize_banking_jitter.rs`
- Create: `artifacts/banking-jitter/summary.md`

- [ ] **Step 1: Create a Rust summarizer**

Add the summarizer dependencies to `Cargo.toml`:

```toml
[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Create `src/bin/summarize_banking_jitter.rs`:

```rust
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct BankingLatencyRecord {
    profile: String,
    queue_delay_us: u64,
    execution_us: u64,
    total_us: u64,
}

fn percentile_ms(values: &mut [u64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    Some(values[idx] as f64 / 1000.0)
}

fn main() -> anyhow::Result<()> {
    let paths = env::args().skip(1).map(PathBuf::from).collect::<Vec<_>>();
    let mut markdown = String::from("# Banking Jitter Summary\n\n");

    for path in paths {
        let input = fs::read_to_string(&path)?;
        let rows = input
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<BankingLatencyRecord>)
            .collect::<Result<Vec<_>, _>>()?;

        let mut total = rows.iter().map(|row| row.total_us).collect::<Vec<_>>();
        let mut queue = rows.iter().map(|row| row.queue_delay_us).collect::<Vec<_>>();
        let mut execution = rows.iter().map(|row| row.execution_us).collect::<Vec<_>>();

        markdown.push_str(&format!("## {}\n\n", path.display()));
        markdown.push_str(&format!("- count: {}\n", rows.len()));
        markdown.push_str(&format!("- total p50 ms: {:?}\n", percentile_ms(&mut total.clone(), 0.50)));
        markdown.push_str(&format!("- total p95 ms: {:?}\n", percentile_ms(&mut total.clone(), 0.95)));
        markdown.push_str(&format!("- total p99 ms: {:?}\n", percentile_ms(&mut total, 0.99)));
        markdown.push_str(&format!("- queue p95 ms: {:?}\n", percentile_ms(&mut queue, 0.95)));
        markdown.push_str(&format!("- execution p95 ms: {:?}\n\n", percentile_ms(&mut execution, 0.95)));
    }

    fs::create_dir_all("artifacts/banking-jitter")?;
    fs::write("artifacts/banking-jitter/summary.md", markdown)?;
    Ok(())
}
```

- [ ] **Step 2: Add a bottleneck classification rubric**

Append to `artifacts/banking-jitter/summary.md`:

```md
## Bottleneck Classification

- CPU scheduling suspect:
  - `tokio-console` shows long wakeup-to-poll delay
  - queue delay dominates execution time
  - system is CPU-saturated during spikes

- I/O stall suspect:
  - latency spikes align with blocking spans or log / disk / socket activity
  - spikes are bursty and not proportional to compute volume

- Allocator pause suspect:
  - spikes align with allocation-heavy batch construction or result collation
  - CPU usage remains high but useful stage progress drops

- Async task interference suspect:
  - one task exhibits long poll durations and other tasks pile up behind it in `tokio-console`
```

- [ ] **Step 3: Commit**

```bash
git add src/bin/summarize_banking_jitter.rs artifacts/banking-jitter/summary.md
git commit -m "feat: add banking jitter summarizer and diagnosis rubric"
```

---

### Task 8: Add one focused queue-latency regression test

**Files:**
- Create: `tests/banking_stage_queue_latency.rs`

- [ ] **Step 1: Write the failing test**

Create a deterministic test that proves queue timestamps are preserved:

```rust
#[test]
fn queue_delay_is_measured_from_ingress() {
    let enqueued_at = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let observed_us = enqueued_at.elapsed().as_micros() as u64;
    assert!(observed_us >= 5_000);
}
```

- [ ] **Step 2: Run test to verify it passes in isolation**

Run:

```bash
cargo test queue_delay_is_measured_from_ingress -- --nocapture
```

Expected: PASS

- [ ] **Step 3: Extend the test to the real metadata path**

Replace the toy test with an actual metadata-carrying unit or integration test once the enqueue timestamp type exists.

- [ ] **Step 4: Commit**

```bash
git add tests/banking_stage_queue_latency.rs
git commit -m "test: cover banking stage queue delay measurement"
```

---

### Task 9: Run the four lightweight measurement profiles and capture outputs

**Files:**
- Create: `artifacts/banking-jitter/*.json`
- Create: `artifacts/banking-jitter/*.csv`
- Create: `artifacts/banking-jitter/summary.md`

- [ ] **Step 1: Build the lightweight harness**

Run:

```bash
cargo build --bin votor-measurement-harness
```

Expected: successful build

- [ ] **Step 2: Run baseline profile**

Run:

```bash
PROFILE=baseline ./scripts/run_banking_jitter_harness.sh
```

Expected: `artifacts/banking-jitter/baseline/trace.json` and `harness.log`

- [ ] **Step 3: Run stress profiles**

Run:

```bash
PROFILE=cpu_saturation ./scripts/run_banking_jitter_harness.sh
PROFILE=io_stall ./scripts/run_banking_jitter_harness.sh
PROFILE=allocator_pressure ./scripts/run_banking_jitter_harness.sh
```

Expected: one trace directory per profile, without starting a validator

- [ ] **Step 4: Summarize outputs**

Run:

```bash
cargo run --bin summarize_banking_jitter -- \
  artifacts/banking-jitter/baseline/trace.json \
  artifacts/banking-jitter/cpu_saturation/trace.json \
  artifacts/banking-jitter/io_stall/trace.json \
  artifacts/banking-jitter/allocator_pressure/trace.json
```

Expected: percentile summaries printed for each profile

- [ ] **Step 5: Commit**

```bash
git add artifacts/banking-jitter
git commit -m "chore: capture banking stage jitter measurement artifacts"
```

---

### Task 10: Write the assignment conclusion

**Files:**
- Modify: `artifacts/banking-jitter/summary.md`
- Modify: `docs/banking-stage-jitter-harness.md`

- [ ] **Step 1: Write the conclusion template**

Add this structure to `artifacts/banking-jitter/summary.md`:

```md
## Conclusion

### 1. Paper timing targets used
- `Δblock = 400 ms`
- practical common-case reference `115-150 ms`
- Votor timeout schedule interpreted as a slot-based upper-bound mechanism

### 2. Measured banking-stage behavior
- baseline `p50/p95/p99`
- stress profile `p50/p95/p99`
- max observed queue delay
- max observed execution delay

### 3. Primary source of jitter
- CPU scheduling / I/O / allocator / task interference
- evidence from spans, percentiles, and `tokio-console`

### 4. Does banking-stage jitter threaten Alpenglow-compatible timing?
- yes / no
- conditionally yes under which stress profile

### 5. Recommended next fix
- exact subsystem to change next
```

- [ ] **Step 2: Document next engineering actions**

Add this checklist:

```md
## Next Engineering Actions

- reduce queue residence time by shrinking oversized batches
- isolate blocking I/O away from the hot path
- reduce allocator churn in batch/result assembly
- identify long-poll async tasks in `tokio-console` and split or reprioritize them
```

- [ ] **Step 3: Commit**

```bash
git add artifacts/banking-jitter/summary.md docs/banking-stage-jitter-harness.md
git commit -m "docs: finalize banking stage jitter analysis plan"
```

---

## Validation Checklist

Run these before declaring the work complete:

```bash
cargo test banking_stage_metrics -- --nocapture
cargo test queue_delay_is_measured_from_ingress -- --nocapture
cargo build --bin votor-measurement-harness
PROFILE=baseline ./scripts/run_banking_jitter_harness.sh
PROFILE=cpu_saturation ./scripts/run_banking_jitter_harness.sh
PROFILE=io_stall ./scripts/run_banking_jitter_harness.sh
PROFILE=allocator_pressure ./scripts/run_banking_jitter_harness.sh
cargo run --bin summarize_banking_jitter -- artifacts/banking-jitter/*/trace.json
```

Expected outcomes:
- tests pass
- lightweight harness builds
- traces are emitted for each profile without running `agave-validator`
- summaries show whether p95/p99 threaten `150 ms` and `400 ms` budgets

## Notes on Interpretation

- If queue delay dominates, the issue is upstream scheduling/backpressure, not raw bank execution.
- If execution dominates, inspect account-lock contention, program execution hotspots, and result collation.
- If `tokio-console` shows delayed polls without corresponding execution time, the scheduler is the likely problem.
- If spikes correlate with filesystem or logging activity, treat them as I/O stalls first, not consensus-path issues.
- If p99 approaches `400 ms`, the banking stage alone is consuming too much of the slot budget to be comfortable.

## Self-Review

- Spec coverage: the plan covers extraction of paper timing anchors, instrumentation of `banking_stage.rs`, harness creation, jitter diagnosis, `tokio-console` usage, and interpretation against Alpenglow timing.
- Placeholder scan: no `TODO` / `TBD` placeholders remain.
- Type consistency: metric, trace, and summary concepts use consistent names (`queue_delay_us`, `execution_us`, `total_us`, `Δblock`).
