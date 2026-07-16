#!/usr/bin/env bash
set -euo pipefail

bin=${1:-target/release/lazysubs-eye}
test -x "$bin"
"$bin" --version | grep -Eq '^lazysubs-eye [0-9]+\.[0-9]+\.[0-9]+'
"$bin" --help | grep -q 'EXIT CODES'
set +e
doctor_json=$("$bin" doctor --json)
doctor_code=$?
set -e
test "$doctor_code" -le 1
printf '%s\n' "$doctor_json" | jq -e '.version and (.checks | type == "array")' >/dev/null

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/cache/lazysubs-eye"
now=$(date +%s)
printf '{"fetched_at":%s,"providers":[]}' "$now" > "$tmp/cache/lazysubs-eye/status.json"
chmod 700 "$tmp/cache/lazysubs-eye"
chmod 600 "$tmp/cache/lazysubs-eye/status.json"
XDG_CACHE_HOME="$tmp/cache" "$bin" --json | jq -e '.providers | type == "array"' >/dev/null
set +e
XDG_CACHE_HOME="$tmp/cache" "$bin" --check >/dev/null
code=$?
set -e
test "$code" -eq 3
