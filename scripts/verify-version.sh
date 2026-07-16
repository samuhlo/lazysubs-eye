#!/usr/bin/env bash
set -euo pipefail

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n1)
# Un argumento explícito es el contrato de CI; el ref del entorno solo se usa
# en el workflow de tag, donde no se pasa argumento.
tag=${1:-${GITHUB_REF_NAME:-}}
tag=${tag#v}

if [[ -z "$version" ]]; then
  echo "no pude leer la versión de Cargo.toml" >&2
  exit 1
fi
if [[ -n "$tag" && "$tag" != "$version" ]]; then
  echo "tag ($tag) y Cargo.toml ($version) no coinciden" >&2
  exit 1
fi
pkgbuild=${PKGBUILD_PATH:-packaging/aur/PKGBUILD}
if [[ -f "$pkgbuild" ]]; then
  pkgver=$(sed -n 's/^pkgver=//p' "$pkgbuild" | head -n1)
  if [[ -z "$pkgver" || "$pkgver" != "$version" ]]; then
    echo "PKGBUILD ($pkgver) y Cargo.toml ($version) no coinciden" >&2
    exit 1
  fi
fi
echo "versión verificada: $version"
