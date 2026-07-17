#!/usr/bin/env bash
set -euo pipefail

# [CI] Accept an artifact path so release smoke tests do not rebuild or depend on a developer binary.
bin=${1:-target/release/lazysubs-eye}
test -x "$bin"
"$bin" --version | grep -Eq '^lazysubs-eye [0-9]+\.[0-9]+\.[0-9]+'
"$bin" --help | grep -q 'EXIT CODES'
# [FLOW] Doctor may report environmental warnings; capture its exit code without bypassing later validation.
set +e
doctor_json=$("$bin" doctor --json)
doctor_code=$?
set -e
test "$doctor_code" -le 1
printf '%s\n' "$doctor_json" | jq -e '.version and (.checks | type == "array")' >/dev/null

# [CI] Isolate cache fixtures so this probe cannot read or mutate the caller's real state.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/cache/lazysubs-eye"
now=$(date +%s)
printf '{"fetched_at":%s,"providers":[]}' "$now" > "$tmp/cache/lazysubs-eye/status.json"
chmod 700 "$tmp/cache/lazysubs-eye"
chmod 600 "$tmp/cache/lazysubs-eye/status.json"
XDG_CACHE_HOME="$tmp/cache" "$bin" --json | jq -e '.providers | type == "array"' >/dev/null
# [FLOW] An empty cached provider list is an expected check failure (exit 3), not a script failure.
set +e
XDG_CACHE_HOME="$tmp/cache" "$bin" --check >/dev/null
code=$?
set -e
test "$code" -eq 3
