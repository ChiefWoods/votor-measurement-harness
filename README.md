# Votor Measurement Harness

Laptop-safe jitter harness for comparing Agave banking-stage-shaped delays against Alpenglow/Votor timing budgets.

## Source Boundary

The real system under observation is Agave's banking stage source at https://github.com/anza-xyz/agave/blob/master/core/src/banking_stage.rs.

This harness does not run a validator. It mirrors the timing boundaries visible in that source: receive/buffer, batch construction, scheduling/account-lock-shaped work, transaction execution-shaped work, and result recording.

## Measurement Budgets

This harness uses these timing anchors from Alpenglow v1.1:

| Paper source | Requirement | Harness budget |
| --- | --- | --- |
| Abstract / Section 2.4 | Fast finalization: one voting round if `>= 80%` stake participates with notarization votes | Banking-stage jitter should remain well below a one-round finalization budget |
| Abstract / Section 2.4 | Slow finalization: two voting rounds if `>= 60%` stake is responsive | Banking-stage jitter should not repeatedly consume a large share of the two-round budget |
| Abstract | Concurrent modes finalize in `min(delta_80%, 2 * delta_60%)` after block distribution | Summaries compare p95/p99 against one-round and two-round references |
| Definition 17 / Algorithm 2 | `Timeout(i) = clock() + Delta_timeout + (i - s + 1) * Delta_block` | Sustained p99 near a timeout boundary is critical |
| Table 10 | `Delta_block = 400 ms` | Sustained p99 or max near `400 ms` is critical |

Additional context: the paper's latency simulations discuss common-case finality around `115-150 ms`.

## Commands

Available commands are defined in `justfile`. Run `just` to list them.

Raw traces are written to `artifacts/banking-jitter/<profile>/trace.json`; the Rust summarizer writes `artifacts/banking-jitter/summary.md`.

## Profiles

- `baseline`: steady moderate synthetic queue/execution workload
- `cpu_saturation`: controlled CPU-bound pressure to expose scheduling jitter
- `io_stall`: bounded file writes during record-result work
- `allocator_pressure`: bursty temporary allocations during batch construction

## Interpretation Policy

A local banking-stage latency spike is important if it consumes a substantial fraction of the `115-150 ms` common-case budget, or if p95/p99 threatens the `400 ms` block-time boundary.

Use this decomposition:

- queue delay: ingress to first processing attempt
- build delay: batch construction-shaped work
- lock delay: scheduling/account-lock-shaped work
- execution delay: bank execution-shaped work
- record delay: result/bookkeeping-shaped work

## Post-Harness Observation

The smoke-test run used 20 batches per profile. All simulated profiles stayed far below the Alpenglow/Votor timing references:

- `baseline`: total p99 `5.894 ms`
- `cpu_saturation`: total p99 `5.849 ms`
- `io_stall`: total p99 `4.051 ms`
- `allocator_pressure`: total p99 `4.459 ms`

None of these simulated runs approached the `115-150 ms` common-case finalization reference, and none came close to the `400 ms` block-time boundary. Under this lightweight local harness, banking-stage-shaped jitter does not threaten Alpenglow-compatible timing.

The profiles still show how to diagnose the source of jitter:

- `io_stall` moved the delay into `record_us`, which is the expected signature of blocking I/O or result-recording stalls.
- `allocator_pressure` raised `build_batch_us`, which is the expected signature of allocation-heavy batch construction.
- `cpu_saturation` increased total tail latency, which is consistent with CPU scheduling pressure.
- Async task interference would require running with `tokio-console` and looking for long wakeup-to-poll or long poll durations.
