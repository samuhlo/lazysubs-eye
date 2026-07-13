status: complete

# Apply progress — pi-daily-token-usage

## Completed tasks

- Completed and checked every task in `tasks.md` while preserving `status: ready`.
- Added the local Pi/EIN JSONL collector, daily versioned index, stable-entry deduplication, safe completed-line cursor, bounded header/cursor fingerprints, atomic separate-cache persistence, local-day reset, and malformed-file isolation.
- Added the independent Pi worker/state/update and `Pi/EIN hoy` TUI table. JSON, Waybar, provider collection, `Status`, and Claude token collection remain outside this path.

## Files changed

- `src/pi_tokens.rs` — new private JSONL parser/indexer, recursive Pi/EIN discovery, aggregation, deduplication, recovery, and focused tests.
- `src/cache.rs` — Pi index cache path and write/sync/rename persistence helper; existing `status.json` load/save remains unchanged.
- `src/main.rs` — declares the Pi module only.
- `src/tui.rs` — Pi update channel, single-worker guard, independent state, renderer, and tests.
- `openspec/changes/pi-daily-token-usage/tasks.md` — all task checkboxes completed.
- `openspec/changes/pi-daily-token-usage/apply-progress.md` — this cumulative evidence.

## TDD Cycle Evidence

| Group | RED evidence | GREEN / triangulation / refactor evidence |
|---|---|---|
| 001 Parser and totals | `cargo test parse_pi_line` failed with unresolved `parse_pi_line` before production parser existed. | `cargo test pi_tokens::tests` passes parser acceptance/rejection plus overflow coverage; parser uses partial serde envelopes only. |
| 002 Daily index and persistence | Index behaviour was specified by focused tests before the final index loop was validated. | `cargo test pi_tokens::tests::steady_state_suffix_only_reads_zero_then_exact_append_bytes` passes: unchanged cycle reads **0 suffix bytes**; appended cycle reads exactly `appended.len()` bytes. `cargo test cache::tests::atomic_save_replaces_a_complete_index` passes write/sync/rename replacement. |
| 003 Discovery and recovery | Parser/discovery fixtures exercise invalid headers and malformed records without an input dependency. | `cargo test pi_tokens::tests::duplicate_ids_and_partial_lines_do_not_double_count` passes nested-session deduplication and source removal. Safe cursor advances only over newline-terminated records. |
| 004 TUI | `cargo test pi_state_independence` failed before `Update::PiTokens`, Pi state, and worker guard existed. | The same command passes after wiring; `cargo test tui::tests::pi_` passes state/guard/format-height helper coverage. |
| 005 Regression and invariants | Existing compatibility tests were retained as the regression boundary. | `cargo test` passes all 28 tests, including existing Waybar/Status/Claude-Codex coverage and privacy/index tests. |

## Commands run

- `cargo test parse_pi_line` — RED: failed before parser implementation.
- `cargo test pi_state_independence` — RED: failed before TUI Pi state implementation; GREEN: passed after wiring.
- `cargo test pi_tokens::tests` — passed (5 focused tests).
- `cargo test tui::tests::pi_` — passed (2 focused tests at that point).
- `cargo test cache::tests::atomic_save` — passed.
- `rustfmt --config skip_children=true src/pi_tokens.rs src/cache.rs src/main.rs src/tui.rs` — passed; only touched Rust files.
- `cargo clippy -- -D warnings` — passed.
- `cargo test` — passed: 28 tests.
- `git diff --check` — passed.

## Privacy and performance evidence

- `serialized_index_excludes_message_content_and_cwd` proves the persisted index excludes synthetic prompt content and cwd.
- The parser deserializes only envelope/message identity, role, provider/model, message timestamp, usage, and cost; it has no fields for content, tools, credentials, or cwd.
- `steady_state_suffix_only_reads_zero_then_exact_append_bytes` uses deterministic byte counters, not elapsed time: stable refresh = `0`; append = exact appended byte count. Fixed header/cursor windows are validation I/O and are not content parsing.

## Deviations from design

- The crate is binary-only, so the checklist's `cargo test --lib` command is inapplicable (`no library targets found`). Focused commands use `cargo test <filter>` and final verification uses the required `cargo test`.
- No dependencies, production build, commits, staged files, delivery actions, provider changes, Waybar changes, or `status.json` schema changes were made.

## Remediation after verify findings

- Implemented Unix/Linux file identity as persisted `dev`/`ino` plus a stable `unix:<dev>:<ino>` index key. Non-Unix keeps the normalized path fallback. Loading a path-keyed/identity-incomplete cache now rejects it and safely bootstraps.
- Rename-with-same-inode keeps the indexed cursor; replacement at the same path with a new inode is discovered as a new identity and removes the old contribution before rebuilding.
- Added explicit coverage for unavailable root, truncation, corrupt/incompatible index, local-day/offset rollover, same model across providers, error/aborted entries, malformed completed JSON cursor advancement, completion of a partial trailing line, nested EIN discovery, and byte-stable JSON/Waybar output.
- Added the injectable final-rename seam in `cache::atomic_save_with_rename`; a forced rename failure preserves the previous complete index and leaves no temporary target visible.
- Replaced the global suffix-byte test counter with thread-local instrumentation so the deterministic byte assertions are not affected by parallel tests.

## TDD Cycle Evidence — remediation

| Cycle | RED evidence | GREEN / triangulation / refactor evidence |
|---|---|---|
| Atomic rename failure | `cargo test failed_final_rename_keeps_the_previous_complete_index_readable` failed to compile because `atomic_save_with_rename` did not exist. | Added the final-rename seam; focused test passes and proves the prior JSON bytes remain parseable with no temp file exposed. |
| Unix stable identity and recovery scenarios | `cargo test unix_inode_identity_preserves_a_renamed_cursor_and_rebuilds_a_replacement` failed to compile because `FileState` lacked `dev`/`ino`. | Added persisted `(dev, ino)`, Unix stable keys, legacy-index invalidation, then `cargo test pi_tokens::tests` passed 14 focused tests for identity, recovery, day rollover, grouping, malformed/partial lines, nested discovery, and privacy/byte-read invariants. |
| JSON/Waybar regression | The new exact JSON fixture initially failed because it omitted the pre-existing serialized `error: null` field. | Corrected the fixture to the established contract; `cargo test json_and_waybar_contracts_remain_byte_stable_without_pi_data` passes. No JSON/Waybar production code changed. |
| Refactor | Parallel test runs exposed cross-test contamination from the global suffix-byte counter. | Replaced it with thread-local test-only instrumentation; focused and full suites pass without timing assertions. |

## Remediation commands run

- `cargo test failed_final_rename_keeps_the_previous_complete_index_readable` — RED then passed.
- `cargo test unix_inode_identity_preserves_a_renamed_cursor_and_rebuilds_a_replacement` — RED then passed through `cargo test pi_tokens::tests` (14 passed).
- `cargo test json_and_waybar_contracts_remain_byte_stable_without_pi_data` — RED fixture correction then passed.
- `rustfmt --config skip_children=true src/pi_tokens.rs src/cache.rs src/output.rs` — passed.
- `cargo test` — passed: 38 tests.
- `cargo clippy -- -D warnings` — passed.
- `git diff --check` — passed.
- `git diff --cached --name-only` — empty; no staged files.

## Remaining work

- None for apply. Route to verify.
