# Mapa: rendimiento medido y optimizado

status: partial
scope_status: bounded
change: runtime-performance
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Decisión principal

Reemplazar el scan O(E²) de Pi con fingerprinting incremental. Medir primero,
ejecutar providers en paralelo con un presupuesto elegido y añadir detección
de regresiones sobre un entorno controlado.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `src/pi_tokens.rs` | O(E²): re-enumera todos los archivos cada refresh | Fingerprint + skip si unchanged |
| `src/providers.rs` `collect_all` | Secuencial | `collect_all_parallel` con threads + budget global |
| No hay baseline ni budgets | No existe | Baseline reproducible, decisión de budgets y gate estable |

## Archivos concretos

| Archivo | Cambio |
|---|---|
| `src/pi_tokens.rs` | `check_fingerprint()`; skip parseo si unchanged |
| `src/providers.rs` | `collect_all_parallel()`; `CollectedResults` |
| `src/scheduler.rs` (nuevo) | `RefreshScheduler` con coalescing |
| `src/streaming.rs` (nuevo) | `query_streaming()` |
| `perf/baseline.json` (nuevo) | Mediciones y presupuestos elegidos |

Baseline aplicado: budget global 8000 ms, batch 128 y tolerancia 10%; el script
`scripts/benchmarks/run_budgets.sh` ejecuta los seams deterministas.
| `.github/workflows/perf.yml` (nuevo) | CI de regressión |

## Puntos de riesgo

1. **Overhead de threads**: spawnear threads para 3 providers tiene overhead.
   **Mitigación**: el overhead es ~1ms; el beneficio de no bloquear es mayor.
2. **Flakiness de tests de rendimiento**: la carga del sistema puede hacer
   que los tests fallen falsamente. **Mitigación**: warm-up runs; tolerate 10%.
3. **Baseline desactualizado**: si no se actualiza tras mejoras, los budgets
   se vuelven obsoletos. **Mitigación**: PR que mejora rendimiento debe
   actualizar el baseline.

## Rollback

Revertir los cambios de cada archivo. Los budgets y CI se eliminan.

## Dependencias con otros paquetes

| Paquete | Relación |
|---------|----------|
| `reliable-history-ingestion` | La optimización del flujo de ingesta de history se beneficia de `IngestState` y el modelo de cutoff unificado |

**Nota**: el diseño puede avanzar en paralelo; la implementación final se optimiza contra el flujo corregido por reliable-history-ingestion.

## Siguiente fase

Pasar a `sdd-design` con este mapa. Esta fase no ejecutó build ni tests.
