# Mapa: preparación del proyecto para release

status: partial
scope_status: bounded
change: release-project-readiness
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Decisión principal

Establecer gates obligatorios en CI, consistency de versionado, smoke tests
del artefacto, documentación de release, y proceso de beta feedback.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `.github/workflows/ci.yml` | Tiene fmt, clippy, test | Asegurar gates base; añadir el gate de vulnerabilidades elegido para RC+ |
| `.github/workflows/release.yml` | Tag triggers build + publish | Añadir verify-version; smoke tests |
| README.md | Existe, incompleto | Completar quickstart, compatibilidad, privacidad y troubleshooting; capturas antes de Stable |
| SECURITY.md | No existe | Crear |
| CONTRIBUTING.md | No existe | Crear |
| CHANGELOG.md | No existe | Crear con Keep a Changelog |
| Supply chain avanzada | Sin política cerrada | Decidir SBOM, attestation, advisories y reproducibilidad por fase |
| Sin beta process | No existe | Definir issue template + label + gate |

## Archivos concretos

| Archivo | Cambio |
|---|---|
| `.github/workflows/ci.yml` | Gate de vulnerabilidades elegido para RC+; asegurar gates base |
| `.github/workflows/release.yml` | Verify-version; smoke tests; assets condicionales aprobados |
| `scripts/verify-version.sh` (nuevo) | Verificar tag/Cargo/PKGBUILD consistencia |
| `scripts/generate-sbom.sh` (condicional) | Generar SBOM solo si se adopta |
| `SECURITY.md` (nuevo) | Política de disclosure |
| `CONTRIBUTING.md` (nuevo) | Cómo contribuir |
| `CHANGELOG.md` (nuevo) | Keep a Changelog |
| `README.md` | Completar secciones |
| `.github/ISSUE_TEMPLATE/beta-feedback.md` (nuevo) | Template de feedback |

## Puntos de riesgo

1. **Gates que retrasan releases**: si los gates son demasiado estrictos,
   bloquean el flujo. **Mitigación**: gates mínimos necesarios; Beta
   no requiere todos los gates (audit optional para Beta).
2. **Documentación desactualizada**: si no se mantiene, se convierte en
   ruido. **Mitigación**: el gate de docs verifica que las secciones clave
   existen.
3. **Reproducibilidad difícil**: muchos factores pueden romperla.
   **Mitigación**: documentar requirements; verificar en CI.

## Rollback

Revertir los cambios de CI y scripts; eliminar los nuevos archivos de docs.

## Dependencias con otros paquetes

| Contexto | Dependencia |
|----------|-------------|
| Diseño de gates CI | Sin dependencias — puede avanzar en paralelo |
| Cierre Beta | Depende de secure-local-persistence y safe-system-integration (P0 resueltos) |
| Cierre RC | Depende de todos los P0 y P1 de los paquetes aplicables |

Decisión aplicada: release x86_64-musl con RustSec, checksum SHA-256 y smoke
pre-publicación. aarch64, SBOM, attestation y cargo-deny permanecen fuera hasta
sus gates explícitos de RC/Stable.
| Cierre Stable | Depende de todos los paquetes P0/P1/P2 |

**Nota**: `tasks.md` usa dependencia por fase: el diseño de gates puede avanzar
sin bloqueos; cada gate solo se cierra cuando sus paquetes requeridos están verificados.

## Siguiente fase

Pasar a `sdd-design` con este mapa. Esta fase no ejecutó build ni tests.
