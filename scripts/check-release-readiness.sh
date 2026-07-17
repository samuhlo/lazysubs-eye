#!/usr/bin/env bash
set -euo pipefail

# [CI] Release readiness is a static contract: required workflow gates and public artifacts must exist.
# HARD STOP -> any missing grep target or empty required file fails this script immediately.
grep -Fq 'cargo fmt --check' .github/workflows/ci.yml
grep -Fq 'cargo clippy --all-targets -- -D warnings' .github/workflows/ci.yml
grep -Fq 'cargo test --locked' .github/workflows/ci.yml
grep -Fq 'rustsec/audit-check' .github/workflows/release.yml
grep -Fq 'issues: write' .github/workflows/release.yml
grep -Fq 'checks: write' .github/workflows/release.yml
grep -Fq 'scripts/verify-version.sh' .github/workflows/release.yml
grep -Fq 'scripts/smoke-release.sh' .github/workflows/release.yml
grep -Fq 'cargo build --release --locked --target x86_64-unknown-linux-musl' .github/workflows/release.yml
grep -Fq 'sha256sum' .github/workflows/release.yml
grep -Fq '## // 11\_ COMPATIBILITY' README.md
grep -Fq '## // 09\_ SECURITY_MODEL' README.md
test -s SECURITY.md
test -s CONTRIBUTING.md
test -s CHANGELOG.md
test -s .github/ISSUE_TEMPLATE/beta-feedback.md
test -s .github/labels.yml

# [FLOW] Pass Cargo.toml's version explicitly; this validates the normal release-tag path.
bash scripts/verify-version.sh "v$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"

# [CI] Branch refs must not override an explicit caller tag; this isolates argument precedence.
GITHUB_REF_NAME=main bash scripts/verify-version.sh "v$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"
