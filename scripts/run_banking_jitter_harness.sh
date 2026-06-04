#!/usr/bin/env bash
set -euo pipefail

PROFILE="${PROFILE:-baseline}"
ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts/banking-jitter/$PROFILE}"
ITERATIONS="${ITERATIONS:-}"
mkdir -p "$ARTIFACT_DIR"

args=(--profile "$PROFILE" --out "$ARTIFACT_DIR/trace.json")
if [[ -n "$ITERATIONS" ]]; then
  args+=(--iterations "$ITERATIONS")
fi

RUST_LOG="${RUST_LOG:-info}" \
BANKING_JITTER_PROFILE="$PROFILE" \
BANKING_JITTER_OUT="$ARTIFACT_DIR/trace.json" \
cargo run --bin votor-measurement-harness -- "${args[@]}" \
  > "$ARTIFACT_DIR/harness.log" 2>&1
