# A. Proposal — ingesta fiable de historial con estados explícitos

## Intent

Hacer que la ingesta de historial desde Claude, Pi y OpenCode sea:
(1) no destructiva — nunca borrar datos existentes ante un fallo de ingesta,
(2) idempotente — ejecutar la ingesta múltiples veces produce el mismo estado,
(3) transaccional por fuente y día,
(4) no bloqueante — el backfill se ejecuta fuera del camino crítico de la TUI,
(5) con cutoff — los datos se leen en un punto consistente dentro de cada scan,
(6) con estados explícitos — cada panel distingue claramente entre vacío real,
   datos aún no ingesados, y error de ingesta.

## Spec

### R1. No destruir datos ante fallo de ingesta

Cuando la ingesta de historial falla (DB corrupta, permisos, parse error,
timeout), el sistema **MUST** conservar los datos existentes de ese día
sin modificarlos. La ingesta **MUST NOT** borrar, truncar ni sobrescribir
ningún registro previamente ingestado. Si la ingesta es parcial (ingesta
algunos días y luego falla), los días ya ingestados **MUST** persistir como
si la ingesta nunca se hubiera intentado.

La única excepción es cuando se detecta corrupción activa o inconsistencia
interna que impide determinar qué datos son válidos. En ese caso,
**MUST** mover el archivo/index a un path `.bak` con timestamp y continuar
con los datos restantes.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Fallo de parse | Hay 15 días de history.db y la ingesta del día 10 encuentra JSON malformado | Se intenta backfill | Días 1-9 y 11-15 quedan intactos; día 10 se marca como `Partial(day=10, reason="ParseError")` |
| Fallo de permisos | History.db tiene permisos 000 durante ingesta | Se intenta escribir | Se retorna error; todos los días existentes quedan intactos; se sugiere `chmod 0600` |
| Ingesta parcial | Se ingestan 20 días y en el día 21 una fuente falla | Continúa el backfill | Días 1-20 persisten; día 21 queda `Partial` o `Failed`; los días posteriores siguen |
| Fallo de disco lleno | Todo listo pero `save` falla | Se intenta persistir | Se retorna error; la DB queda en el último estado consistente |

### R2. Idempotencia y migraciones

Ejecutar la ingesta dos veces con los mismos datos fuente **MUST** producir
el mismo resultado que ejecutarla una vez. La ingesta **MUST** usar la clave
`INSERT OR REPLACE` con una condición de versión o fingerprint que detecte
cambios en los datos fuente antes de sobrescribir.

Las migraciones de esquema de `history.db` **MUST** ser additive: nunca
eliminar columnas ni cambiar tipos de columnas existentes. Las nuevas columnas
se añaden con `ALTER TABLE ADD COLUMN` con valor por defecto. Si una migración
no puede ser additive (v.g. renombrar columna), se crea una tabla nueva y se
migran los datos.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Doble ingesta | Se ingesta el día 5 dos veces con identical source data | Se ejecuta backfill completo | La tabla tiene exactamente los mismos registros que tras una sola ingesta; no hay duplicados |
| Fuente cambió | Se ingestó día 5 con source A; la source ahora es B | Se reingesta día 5 | El registro se actualiza con los nuevos datos; se registra en `meta` que fue overwritten |
| Source retrocedió | Día 5 tenía 10 registros, ahora tiene 8 | Se reingesta | Los 8 registros actuales reemplazan a los 10 anteriores; no hay residuos |
| Schema migration | Se añade columna `reasoning_tokens` a la tabla | Se migra | Los registros antiguos tienen NULL en la nueva columna; no se pierden datos |

### R3. Transacciones y atomicidad

La ingesta válida de cada fuente y día **MUST** ejecutarse dentro de una
transacción SQLite. Si no puede commitear, **MUST** hacer rollback y no
modificar datos ni cursor. Cuando el fallo ocurre antes de escribir datos,
el estado `Partial` o `Failed` se registra en una transacción de estado separada
que no reemplaza los últimos agregados válidos.

Un backfill de N días **MUST** ejecutarse como N transacciones independientes:
si el día K falla, ese día queda marcado como fallido o parcial y el proceso
continúa con K+1..N. Esto conserva progreso útil y permite reintentar únicamente
los días fallidos.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Commit exitoso | Todos los registros del día pasan constraints | Se hace commit | Los registros aparecen en la DB; `meta` se actualiza |
| Violación de constraint | Un registro tiene `total_tokens = -5` | Se intenta commit | Rollback de datos; registros anteriores intactos; se intenta registrar `Failed` sin reemplazar agregados |
| Rollback completo | La transacción hace BEGIN pero falla en INSERT | Se sale del scope | Automatic rollback; la DB no se modifica |

### R4. Backfill fuera del camino crítico

El backfill de días históricos **MUST** ejecutarse en un worker threads
separado que **MUST NOT** bloquear el primer render de la TUI ni los
refreshes normales. El primer render **MUST** mostrar los datos existentes
(si los hay) inmediatamente, sin esperar al backfill. Si no hay datos
existentes, la TUI **MUST** mostrar "Backfill en progreso..." con progreso
observable, sin bloquear la interacción del usuario.

El backfill **MUST** ser cancelable: si el usuario cierra la TUI durante
el backfill, los días ya ingestados persisten. El progreso del backfill
**MUST** persistirse en `meta` (`backfill_last_day`, `backfill_progress`)
para que un backfill interrumpido pueda reanudarse en la próxima ejecución
sin empezar desde cero.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Primera ejecución sin datos | No hay history.db ni datos cache | Se abre TUI | La TUI se renderiza en <150ms mostrando "backfill en progreso"; el worker inicia el backfill en background |
| Ejecución con datos parciales | Hay datos de 10 días pero no del 11-30 | Se abre TUI | La TUI muestra los 10 días existentes; el worker hace backfill del 11-30 en background |
| Backfill interrumpido | El usuario cierra la TUI en el día 15 de 30 | Se cierra TUI | Días 1-14 persisten; `backfill_last_day=14` se graba; la próxima ejecución reanuda en 15 |
| Refresh normal durante backfill | Backfill activo en día 20 de 30 | Se presiona `r` | El refresh normal (día actual) se ejecuta sin esperar al backfill; no se cancela el backfill |

### R5. Cutoff consistente durante lectura

Durante el scan de un día para ingesta, **MUST** tomarse un cutoff consistente
usando el mismo patrón que Pi y OpenCode: `MAX(rowid)` o equivalente antes
de consultar. Los datos confirmados después de tomar el cutoff **MUST**
entrar en el siguiente scan, no en el actual. Esto garantiza instantáneas
consistentes y evita parciales.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Append durante scan | Se toma cutoff max_rowid=100; nuevas filas se insertan con rowid=101,102 | Se termina el scan | Las filas 101 y 102 se ingesarán en el siguiente scan, no en el actual |
| Cutoff Pi/OpenCode | El collector ya usa cutoff para Pi y OpenCode | Se implementa ingesta | Se reutiliza o adapta el mismo seam/patrón de cutoff |
| Reingesta de día | Se reingesta el día 10 con cutoff max_rowid=200 | Se ejecuta | Todas las filas <=200 se ingesan; las >200 se ingesan en siguiente |

### R6. Estados explícitos de ingesta

El sistema **MUST** modelar explícitamente el estado de ingesta de cada día
en `meta` con variants claras:

- `Ingested { day, record_count, last_rowid, ingested_at }`: día completamente
  ingestado.
- `Partial { day, record_count, last_rowid, ingested_at, reason }`: día con
  error recuperable (parse error, constraint violation).
- `InProgress { day, started_at }`: ingesta de este día está activa.
- `Pending { day }`: día pendiente de ingesta.
- `Skipped { day, reason }`: día deliberadamente saltado (fuera de rango,
  no existe source).
- `Failed { day, attempted_at, reason }`: no fue posible obtener o persistir
  datos válidos; el día queda pendiente de reintento.

La TUI **MUST** mostrar estos estados para que el usuario pueda distinguir
entre "no hay historial" y "el historial se está ingestando".

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Día ingestado | Todos los registros del día se ingirieron sin errores | Se completa día | `meta.day_10 = Ingested { record_count: 47, ... }` |
| Día parcial | Se ingestaron 30 registros y luego falló | Se detecta error | `meta.day_10 = Partial { record_count: 30, reason: "ParseError at line 31" }` |
| Backfill activo | Días 11-30 pendientes, se está ingiriendo el 15 | Se abre TUI | Panel muestra "Backfill 15/30 — día 10 ingestado" |
| Error recuperable | Un día tiene JSON malformado en una línea | Se intenta ingesta | Ese día se marca Partial; se continúa con el siguiente |

## Decisions

1. **Transacciones independientes por fuente y día**: se eligen unidades pequeñas
   porque el rollback parcial (días ya commiteados) es mejor que rollback
   total. Un día fallido no debe invalidar 30 días ya ingestados.
2. **Idempotencia por fingerprint, no por sobrescritura ciega**: INSERT OR
   REPLACE con condición de fingerprint detecta si la source cambió. Sobrescribir
   ciegamente perdería la historia de cambios.
3. **Backfill en thread separado con progreso persistente**: no bloquear el
   render es el requisito principal; la presentación del progreso es secundaria.
4. **Estados explícitos en meta, no inferidos por la UI**: la fuente de verdad
   es SQLite; la UI solo refleja estados normalizados.

## Success Criteria

- Ingesta de 30 días en backfill: si el día 15 falla, los días 1-14 persisten.
- Reingesta idempotente: ejecutar dos veces el backfill no produce duplicados.
- Primer render < 150ms incluso durante backfill activo.
- Progreso de backfill persistente entre ejecuciones.
- Estados explícitos en meta: ningún día se reporta como "exitoso" si falló.
- Tests demuestran atomicidad de transacciones.

---

# B. Decisions de diseño adicionales

## Decisiones y trade-offs

1. **Transacción por día, no por backfill completo**: permite progreso parcial
   sin rollback total. El trade-off es que un día puede ingestarse parcialmente y
   el siguiente completo; meta refleja eso con Partial.
2. **Progreso en meta, no en memoria**: si la TUI se cierra durante backfill,
   el progreso se pierde sin persistencia. Meta como journal permite reanudar.
3. **No implementar rollback de Partial**: un día Partial se reintenta
   manualmente o en el siguiente backfill automático; no se hace rollback
   automático porque no sabemos qué causó el fallo.
4. **Reutilizar el patrón de cutoff de Pi/OpenCode**: si Pi ya tiene cutoff
   funcional, reutilizarlo; no reinventar.

## Adaptaciones verificadas durante la implementación

- La huella del día se calcula sobre la identidad de fuente, fecha y contenido
  agregado estable. Esto sustituye la propuesta inicial de tres campos
  (`max_rowid`, `mtime`, `file_id`): esos metadatos siguen protegiendo los
  cursores incrementales de Pi/OpenCode, mientras que la huella de contenido
  evita una reingesta cuando solo cambia el `mtime` y sí detecta cualquier fila
  agregada o modificada.
- La TUI comparte un indicador atómico de cancelación con el worker. `Drop` lo
  activa y el worker lo consulta entre días (la unidad transaccional); cada día
  confirmado y `backfill_progress_v1` ya están en `meta`, por lo que cerrar no
  revierte progreso y la siguiente ejecución reanuda desde el último día
  contiguo confirmado.
- Los parsers que pueden recuperar filas válidas disponen de
  `ingest_partial_day`: datos, huella, procedencia y estado `Partial` se
  confirman en una única transacción con un motivo saneado.
