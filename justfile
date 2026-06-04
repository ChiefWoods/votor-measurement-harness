set shell := ["bash", "-cu"]

default:
    @just --list

build:
    cargo build

test:
    cargo test

fmt:
    cargo fmt

run profile="baseline" iterations="":
    @if [[ -n "{{iterations}}" ]]; then \
        PROFILE="{{profile}}" ITERATIONS="{{iterations}}" bash scripts/run_banking_jitter_harness.sh; \
    else \
        PROFILE="{{profile}}" bash scripts/run_banking_jitter_harness.sh; \
    fi

baseline iterations="":
    just run baseline "{{iterations}}"

cpu iterations="":
    just run cpu_saturation "{{iterations}}"

io iterations="":
    just run io_stall "{{iterations}}"

allocator iterations="":
    just run allocator_pressure "{{iterations}}"

profiles iterations="":
    @if [[ -n "{{iterations}}" ]]; then \
        ITERATIONS="{{iterations}}" bash scripts/run_banking_jitter_profiles.sh; \
    else \
        bash scripts/run_banking_jitter_profiles.sh; \
    fi

summarize:
    cargo run --bin summarize_banking_jitter -- artifacts/banking-jitter/*/trace.json

smoke iterations="20":
    just test
    just profiles "{{iterations}}"
    just summarize
