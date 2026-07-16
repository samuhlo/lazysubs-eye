# Contributing

Use stable Rust and run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
and `cargo test --locked` before opening a pull request. Keep changes scoped,
add regression tests, and do not commit credentials or local provider data.

Clone the repository, install stable Rust, and build with `cargo build --locked`.
Pull requests should explain the user-visible change, link the OpenSpec change
or issue, include tests, and keep generated/local state out of the diff. A
maintainer reviews API compatibility, privacy and the mandatory CI gates before
merge.

For beta feedback, use the beta feedback issue template and include the output
of `lazysubs-eye doctor` with sensitive paths and values removed.

Promotion from Beta to RC is a manual maintainer decision. It requires no open
blocking beta bugs, updated documentation, a passing release checklist and
confirmation from the assigned alpha/beta testers after the documented soak.
CI never promotes a Beta automatically.
