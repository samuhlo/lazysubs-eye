# Alcance: preparación del proyecto para release

## SCOPE PACKET

```yaml
scope: Establecer gates de calidad en CI, consistencia de versionado,
  smoke tests del artefacto, matriz de compatibilidad, estrategia de
  supply-chain, documentación de release, y gate de beta a RC.
change_name: release-project-readiness
budget_allocated:
  max_tokens: 14000
  max_reads: 15
  max_runtime_ms: 600000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

Release trazable y verificable: cada release tiene gates que pasan,
documentación actualizada y un proceso de feedback de beta. La reproducibilidad
exacta solo se exige si se adopta un entorno reproducible.

## Hechos de partida

- CI existe con fmt, clippy, test en `.github/workflows/ci.yml`.
- Release automation existe con `.github/workflows/release.yml`.
- No hay `cargo audit` en CI.
- README existe pero no tiene screenshots ni troubleshooting.
- SECURITY.md no existe.
- CONTRIBUTING.md no existe.
- CHANGELOG no existe.
- No hay smoke tests del artefacto en CI.

## Criterios de aceptación

1. CI gates pasan: fmt, clippy, test y gate de vulnerabilidades adoptado para RC+.
2. Tag/Cargo/PKGBUILD consistentes.
3. Smoke tests del artefacto en CI.
4. README con quickstart, compatibilidad, privacidad y troubleshooting;
   capturas reales antes de Stable.
5. SECURITY.md existe.
6. CONTRIBUTING.md existe.
7. CHANGELOG existe y sigue Keep a Changelog.
8. Beta feedback gating funciona.

## Fuera de alcance

- Attestation de binarios (SPIKE pendiente para Stable — decisión explícita requerida).
- GUI de release dashboard.
- Automated promotion de Beta a RC.
- Adopción automática de aarch64 en CI (pendiente de decisión SPIKE en RC).
- Adopción automática de cargo-deny (SPIKEpendiente para RC).
- SBOM no adoptado automáticamente; si se descarta, se exige procedencia y checksum.

## Decisiones cerradas para Beta/RC

RC y Stable ejecutan RustSec como gate bloqueante. Beta usa fmt, Clippy y tests.
Los artefactos x86_64-musl incluyen checksum y trazabilidad al tag, pero no se
declaran reproducibles bit a bit. aarch64 queda explícitamente no soportado.
SBOM y attestation se deciden antes de Stable; cargo-deny requiere una decisión
explícita durante RC. Ninguna de esas tres decisiones diferidas se implementa
ni se presume en este change.

## Dependencias de otros paquetes

Este paquete depende de que los paquetes P0/P1/P2 estén resueltos antes
de cada gate. Sin embargo, los gates de este paquete pueden implementarse
en paralelo: el CI, docs, etc. son independientes de la lógica de cada
feature.

## No objetivos

- No implementar un dashboard de release.
- No automatizar la promoción de versiones.
- No agregar telemetry.
