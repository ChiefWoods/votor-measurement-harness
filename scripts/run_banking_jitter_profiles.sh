#!/usr/bin/env bash
set -euo pipefail

profiles=(baseline cpu_saturation io_stall allocator_pressure)

for profile in "${profiles[@]}"; do
  PROFILE="$profile" bash scripts/run_banking_jitter_harness.sh
done
