#!/usr/bin/env bash
set -euo pipefail

cargo test --locked performance::tests
cargo test --locked pi_tokens::tests::steady_state_suffix_only_reads_zero_then_exact_append_bytes
