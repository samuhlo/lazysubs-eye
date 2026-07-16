# Alcance: ingesta fiable de historial con estados explícitos

## SCOPE PACKET

```yaml
scope: Hacer que la ingesta de historial desde Claude, Pi y OpenCode sea
  no destructiva, idempotente, transaccional, no bloqueante (backfill en
  background), con cutoff consistente, y con estados explícitos en meta.
  El cambio se enfoca en history.rs y su interacción con los collectors.
change_name: reliable-history-ingestion
budget_allocated:
  max_tokens: 20000
  max_reads: 30
  max_runtime_ms: 1000000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

La ingesta de historial de todos los providers (Claude, Pi, OpenCode) produce
un estado en `meta` que refleja exactamente qué días se ingestaron, cuáles
fallaron y por qué, y cuál es el progreso de un backfill activo. La TUI
nunca se bloquea por backfill; el usuario puede usar la app mientras se
ingresa el historial. Los datos existentes nunca se borran ante un fallo.

## Hechos de partida

Estos hechos están verificados y no necesitan redescubrirse:

- lazysubs-eye es un binario Rust; usa `cargo test` y `strict_tdd: true`.
- `src/history.rs` ya existe y tiene `ensure_table`, `insert_day_stats`,
  `get_meta`, `set_meta`.
- `src/pi_tokens.rs` tiene un patrón de cutoff con watermark para incrementalidad.
- `src/opencode_tokens.rs` también tiene un patrón de cutoff con watermark.
- El backfill actual (P1-4 de la auditoría) es bloqueante: se ejecuta en el
  hilo principal de la TUI o bloquea el primer render.
- No hay estados explícitos de ingesta: history.rs no distingue entre
  "no hay datos" y "falló la ingesta".
- El backfill usa un marcador global y suprime errores; no existe una garantía
  verificable de completitud e idempotencia por fuente.

## Alcance funcional

### No destruir datos ante fallo

- Cada día de backfill se ingesta en su propia transacción SQLite.
- Si la transacción falla, se hace rollback; los días anteriores ya
  commiteados persisten.
- Los días con error parcial se marcan como `Partial` en `meta`.
- Nunca se usa `DELETE` durante backfill; solo `INSERT OR REPLACE` con
  condición de fingerprint.

### Idempotencia

- La ingesta de cada día verifica un fingerprint de los datos fuente
  (rowid max, timestamp, file_id) antes de sobrescribir.
- Si el fingerprint no cambió desde la última ingesta, se omite el día.
- Si cambió, se reemplazan los registros existentes con los nuevos.

### Transacciones

- `BEGIN IMMEDIATE` para adquirir lock de escritura inmediatamente.
- Si `COMMIT` falla, `ROLLBACK` automático.
- La transacción cubre registro + actualización de `meta`.

### Backfill no bloqueante

- El backfill se ejecuta en un `std::thread::spawn` desde `App::new`.
- El primer `App::render` no espera al thread; muestra el estado inmediatamente.
- Si no hay datos, muestra "Backfill en progreso..." con día actual.
- El progreso persiste en `meta` (`backfill_last_day`).
- Si se cierra la TUI, el thread continúa o se hace join en Drop.

### Cutoff consistente

- Reutilizar el seam de cutoff de `pi_tokens.rs`: cursor probe → max_rowid →
  consulta de sufijo.
- Para cada día, tomar `MAX(rowid)` antes de consultar.
- Las filas con rowid mayor al cutoff van en el siguiente scan.

### Estados explícitos en meta

- Enum `IngestState` con variants: `Ingested`, `Partial`, `InProgress`,
  `Pending`, `Skipped`.
- Cada día tiene una entrada en `meta.days: HashMap<NaiveDate, IngestState>`.
- La TUI muestra cada estado con el ícono/color apropiado.

## Criterios de aceptación

1. Backfill de 30 días con fallo en día 15: días 1-14 persisten, día 15 queda
   `Partial` o `Failed`, y días 16-30 continúan procesándose.
2. Reingesta de backfill completo sin duplicados.
3. Primer render TUI < 150ms aunque haya 30 días de backfill pendientes.
4. Progreso de backfill visible y persistente entre ejecuciones.
5. Transacción atómica: si el día 15 falla, día 14 ya commiteado persiste.
6. Estados explícitos: no hay forma de confundir "vacío" con "en progreso".
7. Cutoff coherente: la ingesta no ingiere filas confirmadas después del cutoff.

## Fuera de alcance

- Cambiar el formato de `history.db` de forma no aditiva.
- Implementar GC o compactación de history.db.
- Hacer backfill en streaming (una fila a la vez — por día es suficiente).
- Soporte para providers distintos de Claude, Pi y OpenCode.
- Migración de datos de otros formatos (JSON heredado).

## Investigación acotada para sdd-map

1. ¿Cuál es exactamente el seam de cutoff en `pi_tokens.rs` y
   `opencode_tokens.rs`? ¿Se puede reutilizar directamente?
2. ¿Cuál es el schema actual de `meta` en history.db?
3. ¿Cómo se entera la TUI del progreso de backfill? ¿Qué canal se usa?
4. ¿Dónde se ejecuta el backfill hoy? ¿En `App::new`? ¿En `refresh`?
5. ¿Qué tests existen para history.rs?

## Riesgos y controles

| Riesgo | Control requerido |
|--------|-------------------|
| Backfill bloquea render | Thread de backfill separado; render no espera |
| Idempotencia no funciona y se duplican datos | Tests de doble ingesta; fingerprint único |
| Transacción larga bloquea readers | `BEGIN IMMEDIATE`; transacciones cortas por día |
| Estado de meta inconsistente | Actualizar datos y claves de meta dentro de la misma transacción SQLite |
| Backfill se interrumpe y no se puede reanudar | Progreso en meta; se reanuda desde el último Ingested/Partial |

## Condiciones para pasar a diseño

El mapa debe identificar con evidencia: el seam exacto de cutoff de Pi y
OpenCode que se reutiliza, el schema de meta, el canal de comunicación del
progreso, y los tests existentes de history.rs. Si no se puede demostrar
que el backfill no bloquea el render sin cambios en `App`, el diseño debe
proponer una alternativa.
