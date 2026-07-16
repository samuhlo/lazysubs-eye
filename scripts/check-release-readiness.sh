#!/usr/bin/env bash
set -euo pipefail

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
grep -Fq '## Compatibilidad' README.md
grep -Fq '## Privacidad y seguridad local' README.md
test -s SECURITY.md
test -s CONTRIBUTING.md
test -s CHANGELOG.md
test -s .github/ISSUE_TEMPLATE/beta-feedback.md
test -s .github/labels.yml

bash scripts/verify-version.sh "v$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"

# En CI de branch, GITHUB_REF_NAME no debe pisar el tag pasado por el caller.
GITHUB_REF_NAME=main bash scripts/verify-version.sh "v$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)"
