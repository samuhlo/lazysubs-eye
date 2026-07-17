#!/usr/bin/env bash
set -euo pipefail

# [DATA] Cargo.toml is the version source of truth for every release artifact.
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
# [CI] An explicit argument wins; the environment ref is only the tag-workflow fallback.
tag=${1:-${GITHUB_REF_NAME:-}}
tag=${tag#v}

# HARD STOP -> an unreadable package version makes every comparison untrustworthy.
if [[ -z "$version" ]]; then
  echo "no pude leer la versión de Cargo.toml" >&2
  exit 1
fi
if [[ -n "$tag" && "$tag" != "$version" ]]; then
  echo "tag ($tag) y Cargo.toml ($version) no coinciden" >&2
  exit 1
fi
# [FLOW] The AUR manifest is optional; validate it only when the selected path exists.
pkgbuild=${PKGBUILD_PATH:-packaging/aur/PKGBUILD}
if [[ -f "$pkgbuild" ]]; then
  pkgver=$(sed -n 's/^pkgver=//p' "$pkgbuild" | head -n1)
  if [[ -z "$pkgver" || "$pkgver" != "$version" ]]; then
    echo "PKGBUILD ($pkgver) y Cargo.toml ($version) no coinciden" >&2
    exit 1
  fi
fi
echo "versión verificada: $version"
