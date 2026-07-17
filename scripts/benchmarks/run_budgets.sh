#!/usr/bin/env bash
set -euo pipefail

# [CI] Budget regressions are test contracts, not advisory benchmark output.
# FAILURE MODE: -e and pipefail stop the job at the first violated performance invariant.
cargo test --locked performance::tests
cargo test --locked pi_tokens::tests::steady_state_suffix_only_reads_zero_then_exact_append_bytes
