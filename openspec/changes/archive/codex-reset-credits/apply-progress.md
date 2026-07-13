status: complete

# Apply progress — codex-reset-credits

## Completed tasks

- 1.1–1.4: Added private JSON-RPC serde coverage for positive, zero, missing, `null`, negative, fractional, textual, and out-of-range reset-credit values. Extracted the pure Codex snapshot-to-status mapping and preserved plan/window behavior.
- 2.1–2.4: Added the additive `Option<u64>` model field with serde default/omission semantics; error and Claude statuses explicitly use `None`; old cache-shaped JSON remains readable.
- 3.1–3.4: Added one shared TUI visibility/formatting seam. The row is rendered after the windows and panel height derives from that exact seam.
- 4.1–4.4: Added JSON and Waybar contract tests. Full JSON includes numeric credits only when present; Waybar output remains byte-for-byte equivalent for equivalent statuses.
- 5.1–5.4: Completed the focused regression matrix across Codex parsing/mapping, model serde/cache compatibility, TUI layout, JSON, and Waybar.
- Verification remediation: extracted the deterministic private seam `rate_limits_result_from_response`; it executes the id `2` JSON-RPC error path without a Codex process or credentials, preserves the server error message, and feeds the existing creditless `ProviderStatus::err` conversion.

## Files changed

- `src/providers/mod.rs` — optional serializable status field, error initialization, serde compatibility tests.
- `src/providers/codex.rs` — nested Codex response parsing, pure mapping seam, focused protocol tests.
- `src/providers/claude.rs` — explicit absent value for the unsupported provider.
- `src/tui.rs` — shared conditional row/height seam and layout tests.
- `src/output.rs` — JSON and Waybar non-regression tests; rustfmt applied within the SDD-touched file.
- `src/providers/claude.rs`, `src/providers/codex.rs`, `src/providers/mod.rs`, and `src/tui.rs` — rustfmt applied only within these SDD-touched files; `codex.rs` also contains the JSON-RPC response seam and R6 regression tests.
- `openspec/changes/codex-reset-credits/tasks.md` — all implementation checkboxes completed.
- `openspec/changes/codex-reset-credits/apply-progress.md` — this cumulative completion record.

## Test commands run

- `cargo test providers::codex` — RED: failed because `RateLimitSnapshot.reset_credits` did not exist.
- `cargo test providers::codex` — GREEN: passed after nested serde types and mapping were added.
- `cargo test providers::codex` — TRIANGULATE RED: failed because the pure mapping seam did not exist; then passed after extraction.
- `cargo test providers::` — RED: failed because absent credits serialized as `null`.
- `cargo test providers::` — GREEN/TRIANGULATE: passed after serde omission/default attributes and `Some(3)`/`Some(0)` coverage.
- `cargo test tui::` — RED: failed because the shared row seam did not exist; GREEN/TRIANGULATE passed after the seam drove both rendering and height.
- `cargo test output::` — passed: JSON serialization is supplied by the model serde contract and Waybar deliberately has no new production branch.
- `cargo test` — passed: 12 tests passed, 0 failed.
- `git diff --check` — passed.
- `cargo fmt --check` — not used as a gate; it reports formatting drift in pre-existing untouched files (`src/cache.rs`, `src/main.rs`). No broad formatting refactor was applied.
- `cargo test providers::codex::tests::turns_json_rpc_error_responses_into_creditless_error_statuses` — RED: failed to compile because `rate_limits_result_from_response` did not exist.
- `cargo test providers::codex::tests::turns_json_rpc_error_responses_into_creditless_error_statuses` — GREEN: passed after extracting the private response-interpretation seam.
- `cargo test providers::codex::tests` — TRIANGULATE/REFACTOR: passed 7 tests, including ignored non-target responses and a successful id `2` result, after the scoped refactor.
- `rustfmt src/output.rs src/providers/claude.rs src/providers/codex.rs src/providers/mod.rs src/tui.rs` — applied only to the five SDD-touched Rust files.
- `rustfmt --check src/output.rs src/providers/claude.rs src/providers/codex.rs src/providers/mod.rs src/tui.rs` — passed.
- `cargo test` — final pass: 14 passed, 0 failed.
- `git diff --check` and staged-file check — passed; no staged files.

## TDD Cycle Evidence

| Task group | RED | GREEN | TRIANGULATE | REFACTOR |
| --- | --- | --- | --- | --- |
| 001 Codex collector | `cargo test providers::codex` failed on missing `reset_credits` field | Nested `Option<RateLimitResetCredits>` parses valid values and rejects invalid `u64` inputs | Missing pure mapper failed first; mapping test then passed for `Some(3)`, `Some(0)`, `None`, plan, and both windows | Centralized construction in `provider_status_from_rate_limits`; focused tests pass |
| 002 Model/cache | `cargo test providers::` showed `None` serialized as `null` | Added serde `default` + `skip_serializing_if`; old cache and error tests pass | Verified `Some(3)` and `Some(0)` stay numeric while historical values remain unchanged | Reviewed all `ProviderStatus` literals; Codex, Claude, and error paths are explicit |
| 003 TUI | `cargo test tui::` failed on absent `codex_reset_credits_line` | Added one condition shared by content and height | Verified 5 lines for two windows with `Some(3)`/`Some(0)`, 4 otherwise, including Claude/error | Kept row insertion after window rows and did not alter other panel constraints |
| 004 Output/Waybar | The earlier model-serde RED covered the `pretty` JSON presence boundary; Waybar has intentionally no production branch to add | Model serde makes `pretty` additive; no Waybar code changed | `cargo test output::` compares normal/error Waybar output with and without credits and checks JSON values | No unused output helpers/imports added; cache and main remain untouched |
| 005 Final regression | The preceding focused RED cases cover each boundary in the final matrix | No new fixes were required after the matrix | Final `cargo test` passed all 12 tests | Reviewed scope: only approved source areas and OpenSpec artifacts changed |
| 006 R6 verification remediation | `cargo test providers::codex::tests::turns_json_rpc_error_responses_into_creditless_error_statuses` failed because the response seam did not exist | The same focused test passed after `rate_limits_result_from_response` returned the preserved JSON-RPC error to `ProviderStatus::err` | `cargo test providers::codex::tests` passed for error, non-target id, and id `2` result messages without a process or credentials | Scoped rustfmt touched only the five approved Rust files; focused tests, final `cargo test` (14 passed), `rustfmt --check`, and `git diff --check` passed |

## Deviations from design

None. The cache compatibility test deserializes the same `Status` serde contract used by `cache::load`, avoiding filesystem or `$HOME` dependencies as designed.

## Remaining tasks

None.
