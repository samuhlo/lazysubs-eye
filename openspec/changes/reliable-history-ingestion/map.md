# Mapa: ingesta fiable de historial con estados explícitos

status: partial
scope_status: bounded
change: reliable-history-ingestion
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Ledger (pendiente de exploración)

## Decisión principal

Separar la ingesta de historial en unidades transaccionales por día (no un
backfill completo en una transacción), ejecutar el backfill en un thread
separado, y persistir el estado de cada día en `meta` con estados explícitos
que permitan distinguir vacío real de error recuperable de backfill en curso.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `src/history.rs` `insert_day_stats` | Inserta registros del día y guarda meta; no hay estados explícitos; no hay rollback parcial | Añadir `IngestState` enum y `meta.days`; hacer cada día su propia transacción |
| `src/history.rs` `get_meta`/`set_meta` | Estado en la tabla `meta` de SQLite | Actualizar datos y meta dentro de la misma transacción por día |
| `src/pi_tokens.rs` | Ya tiene cutoff con max_rowid y watermark | Extraer seam reutilizable o adaptar a history.rs |
| `src/opencode_tokens.rs` | Usa max_rowid y watermark | Mismo seam que pi_tokens |
| `src/tui.rs` `App::refresh` | Llama a collectors; no hay canal de progreso de backfill | Añadir BackfillProgress a App; mostrar banner de progreso |
| Backfill actual | Bloqueante en el hilo principal | Thread separado con handle en App |

## Flujo afectado

```
// Antes (P1-4 AUDITORIA)
App::new()
  → load_history()
  → backfill_30_days()   ← BLOQUEA AQUÍ
  → render()             ← no llega hasta que backfill termina

// Después
App::new()
  → load_history()
  → check_backfill_progress()  // Lee meta, determina días pendientes
  → spawn backfill_thread()    // no bloquea
  → render()                   // LLEGA AQUÍ INMEDIATELY

backfill_thread():
  for day in pending_days:
    ingest_single_day(day)     // transacción de datos + meta en éxito
    record_failure(day)        // solo estado, sin reemplazar datos, si falla antes
    notify_progress()          // actualiza BackfillProgress

render():
  if backfill_progress.is_some():
    show_backfill_banner()
```

## Archivos concretos

| Archivo | Rol | Símbolos/seam | Cambio |
|---|---|---|---|
| `src/history.rs` | Ingesta de historial | `insert_day_stats`, `save_meta`, `get_meta`, `IngestState`, `DayFingerprint` | Añadir IngestState enum; transactions por día; fingerprint para idempotencia |
| `src/pi_tokens.rs` | Cutoff existente | `cursor_probe`, `max_rowid` | Extraer seam reutilizable o adaptar |
| `src/opencode_tokens.rs` | Cutoff existente | `cursor_probe`, `max_rowid` | Mismo seam |
| `src/tui.rs` | UI y scheduling | `App`, `BackfillProgress`, `render` | Añadir banner de backfill; thread handle |
| `src/main.rs` | Entry point | — | Sin cambios |

## Puntos de riesgo

1. **Backfill no se completa nunca**: si el thread cae por panic, el progreso
   se pierde. **Mitigación**: meta persiste el progreso cada día; reanudar es
   automático.
2. **Meta inconsistente**: si los datos hacen commit sin su estado, el backfill
   puede repetir o reportar mal el día. **Mitigación**: en éxito, registros y
   meta viven en la misma transacción; los fallos solo registran estado.
3. **Confusión de estados en UI**: si meta dice InProgress pero el thread ya
   terminó, la UI muestra algo incorrecto. **Mitigación**: thread actualiza
   meta al terminar; UI lee atomicamente.
4. **Memory leak del thread handle**: si no se hace join en Drop, el thread
   sigue corriendo. **Mitigación**: App guarda `Option<JoinHandle>` y hace
   join en Drop.

## Estrategia de pruebas

### Unit tests

- Tests de `IngestState` serialization/deserialization.
- Tests de `ingest_single_day`: exitoso, Partial (constraint), rollback.
- Tests de `fingerprint`: same source → same; changed → different.
- Tests de idempotency: double ingest → same result.
- Tests de `backfill` (puro, sin threads): 30 días con fallo en 15.

### Integration tests

- Tests con thread real: App::new retorna antes de que thread termine.
- Tests de progreso: thread actualiza BackfillProgress; UI lee correctamente.
- Tests de reanudación: tras falla en día 15, meta tiene last_day=14;
  se instancia App nuevo; el thread reanuda en 15.

### Regresión tests

- `cargo test` existente pasa sin modificación.
- Comportamiento de los collectors sin cambios.

## Rollback

Revertir los cambios de history.rs y tui.rs. El thread de backfill se elimina.
La DB history.db puede tener las columnas `days` y `fingerprint` nuevas; no
afectan al funcionamiento anterior. Meta nuevo con `days` y `fingerprint` es
compatible hacia atrás (valores por defecto).

## Dependencias con otros paquetes

| Paquete | Relación |
|---------|----------|
| `secure-local-persistence` | Debe garantizar permisos privados de `history.db` y su directorio antes de ampliar la ingesta |
| `adaptive-tui-ux` | El banner de backfill puede interferir con scroll o layout pequeño; coordinar |
| `runtime-performance` | El thread de backfill compite por CPU con otros workers; si hay contención, considerar nice/prioridad |

## Siguiente fase

Pasar a `sdd-design` con este mapa. Convertir los requisitos R1-R6 en tasks
con ciclos RED→GREEN→triangulación. Esta fase no ejecutó build ni tests.
