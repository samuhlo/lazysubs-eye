# Mapa: uso diario de tokens de OpenCode

status: partial
scope_status: bounded
change: opencode-daily-token-usage
phase: map
skill_resolution: paths-injected
budget_exceeded: true

## Ledger

```yaml
ledger:
  reads:
    - {path: /home/samuhlo/.pi/agent/skills/local/ein-discipline/SKILL.md, lines: 101, estimated_tokens: 1700}
    - {path: /home/samuhlo/.pi/agent/skills/local/cognitive-doc-design/SKILL.md, lines: 52, estimated_tokens: 800}
    - {path: /home/samuhlo/.pi/agent/skills/local/architecture/SKILL.md, lines: 143, estimated_tokens: 3200}
    - {path: /home/samuhlo/.pi/agent/skills/downloaded/performance/SKILL.md, lines: 300, estimated_tokens: 4700}
    - {path: scope.md, lines: 0, estimated_tokens: 0, note: ruta raíz ausente; se leyó la canónica del cambio}
    - {path: openspec/config.yaml, lines: 35, estimated_tokens: 400}
    - {path: "grep:src/**", lines: 100, estimated_tokens: 600}
    - {path: "grep:openspec/changes/opencode-daily-token-usage/**", lines: 100, estimated_tokens: 2600}
    - {path: Cargo.toml, lines: 18, estimated_tokens: 150}
    - {path: README.md, lines: 20, estimated_tokens: 300}
    - {path: openspec/changes/opencode-daily-token-usage/scope.md, lines: 137, estimated_tokens: 2900}
    - {path: src/main.rs, lines: 75, estimated_tokens: 650}
    - {path: src/cache.rs, lines: 107, estimated_tokens: 1500}
    - {path: src/pi_tokens.rs, lines: 783, estimated_tokens: 9300}
    - {path: src/tui.rs, lines: 555, estimated_tokens: 6600}
    - {path: src/output.rs, lines: 122, estimated_tokens: 1500}
    - {path: src/providers.rs, lines: 0, estimated_tokens: 0, note: módulo es un directorio; se leyó mod.rs}
    - {path: src/tokens.rs, lines: 100, estimated_tokens: 1400}
    - {path: "grep:src/providers/**", lines: 25, estimated_tokens: 400}
    - {path: src/providers/mod.rs, lines: 120, estimated_tokens: 1500}
    - {path: "grep:docs/**/*.md (arquitectura/TUI)", lines: 20, estimated_tokens: 500}
    - {path: "grep:Cargo.lock (sqlite)", lines: 0, estimated_tokens: 0}
    - {path: "grep:toolchain/rust-version", lines: 1, estimated_tokens: 50}
    - {path: "grep:seams Pi/cache/TUI", lines: 30, estimated_tokens: 700}
  webfetch_used: false
  budget_consumed: {tokens: 38650, reads: 25}
```

Se detuvo la exploración al superar el presupuesto efectivo de 25.000 tokens. No se hizo webfetch: los hechos oficiales de OpenCode suministrados responden las cuestiones de esquema; tampoco se leyó la base local ni contenido sensible. El mapa siguiente usa esos hechos verificados y los seams de Rust ya leídos.

## Decisión principal

Añadir un collector local aislado, propuesto como `src/opencode_tokens.rs`, que lee SQLite con `rusqlite`, conserva un índice diario agregado propio y entrega un estado de panel independiente a la TUI. No se integra con `providers::Status`, `cache::status.json`, `output::{pretty,waybar}` ni con las sumas de Pi.

La habilidad `performance` se aplica solo a la consulta incremental y concurrencia; sus reglas de rendimiento web no corresponden a este binario Rust. Las demás habilidades aportan disciplina SDD, un diseño mínimo y una documentación escaneable.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado propuesto |
|---|---|---|
| `src/main.rs` | Declara módulos; el modo TUI retorna antes de JSON/Waybar. | Declarar el módulo OpenCode solamente. No cambiar argumentos ni flujos no TUI. |
| `src/cache.rs` | Tiene directorio XDG, `atomic_save` durable y `pi_daily_index_file`. | Añadir una ruta distinta, p. ej. `opencode-daily-token-index-v1.json`, y reutilizar exclusivamente `atomic_save`. |
| `src/pi_tokens.rs` | Patrón probado: clave de día local, índice versionado, acumulado por proveedor/modelo y escritura atómica. | Reusar el patrón conceptual, no el parser JSONL ni sus tipos de coste obligatorios. El índice OpenCode contiene solo agregados y cursor, no filas ni contenido. |
| `src/tui.rs` | `Update`, estado y bandera de scan separados para providers, Claude y Pi; cada scan vive en su hilo y los duplicados se suprimen con una bandera. | Un cuarto flujo y estado OpenCode independiente, con la misma supresión. Nunca condicionar `Status`, Claude o Pi a su resultado. |
| `src/output.rs` | JSON y Waybar consumen solo `providers::Status`; hay pruebas de estabilidad. | Sin cambios deliberados. |
| `Cargo.toml` | No hay SQLite actualmente; edición 2021. | Añadir una dependencia justificada, sin subprocess `sqlite3`. |

Hay deriva documental menor: `docs/ARQUITECTURA.md` describe la tabla de tokens, pero el código es la autoridad y hoy ya tiene una sección Pi independiente en `tui.rs`.

## Integración SQLite y seguridad WAL

### Dependencia

Recomendar `rusqlite` (línea 0.37, bloqueada por `Cargo.lock`) con la feature `bundled`:

- es una API SQLite Rust mantenida que permite parámetros, tipos y errores estructurados; evita parsear la salida de un binario `sqlite3`;
- `bundled` compila SQLite estático: elimina la dependencia de que el equipo/CI tenga headers, una ABI y JSON1 compatibles, y hace reproducible el acceso JSON/WAL;
- coste aceptado: descarga/compilación mayor y binario algo más grande. Es proporcionado para un binario de escritorio pequeño que hasta ahora no declara SQLite;
- la alternativa `rusqlite` contra SQLite del sistema reduce ese coste pero introduce fallos de build/despliegue y variación de versión/opciones de SQLite. No se recomienda salvo que el proyecto adopte explícitamente una política de SQLite de sistema.

No se requieren dependencias de URL ni de base de datos adicionales. La URI se puede construir con codificación percent de la ruta absoluta; no interpolar una ruta sin escapar.

### Apertura exacta

1. Resolver una ruta existente y absoluta; construir `file:<ruta-percent-encoded>?mode=ro`.
2. Abrirla con `rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`. No abrir con CREATE, READ_WRITE ni FULLMUTEX/compartir la conexión entre hilos.
3. En esa conexión privada del worker, fijar `busy_timeout` corto (100 ms como presupuesto inicial) y ejecutar `PRAGMA query_only = ON`. Ambos son ajustes de conexión; no modifican la base. Un `BUSY`/`LOCKED` agotado es estado temporal no disponible, no reintento bloqueante.
4. No usar `immutable=1`: ignoraría WAL y puede mostrar una instantánea incompleta. No hacer `journal_mode`, checkpoint, backup/copia, migración, DDL ni índices.
5. Preparar consultas parametrizadas y cerrar la conexión al terminar el scan. Cada refresh usa su propio worker; no se comparte `Connection`.

`mode=ro` impide crear la base. El acceso normal de solo lectura permite que SQLite lea `opencode.db-wal` y `-shm` de una base activa, siempre que el directorio sea accesible.

## Descubrimiento y estados normales

Precedencia determinista:

1. `OPENCODE_DB` no vacío: ruta explícita de OpenCode, sin añadir sufijos.
2. `XDG_DATA_HOME` no vacío: `$XDG_DATA_HOME/opencode/opencode.db`.
3. `HOME` no vacío: `$HOME/.local/share/opencode/opencode.db`.
4. Sin base resoluble: `Unavailable(Missing)`.

La evidencia oficial suministrada solo garantiza la ruta XDG para canales `prod/latest`. No se deben adivinar rutas de canales de desarrollo, perfiles o formatos JSON heredados: una ruta de canal distinta debe llegar por `OPENCODE_DB`; en otro caso se muestra ausencia recuperable. No se invoca el ejecutable, proceso ni API de OpenCode.

Los errores se normalizan para la TUI, sin rutas ni datos SQLite crudos: `ausente`, `sin permisos`, `ocupada temporalmente`, `esquema incompatible` y `lectura fallida`. Durante un fallo temporal se conservan las filas válidas ya pintadas y se añade el estado de error; en el primer intento se muestra la sección como no disponible, no se aborta la TUI.

## Semántica, privacidad y SQL

La única unidad contable es una fila `part` con `data.type == "step-finish"`. El `message` de asistente unido aporta `providerID` y `modelID`. No usar `message.data.cost` ni `message.data.tokens`: OpenCode los sobrescribe en cada paso y no son autoritativos para una respuesta multi-step.

La proyección permitida —nunca `data` completo— es conceptualmente:

```sql
SELECT
  p.rowid AS part_rowid,
  p.id AS part_id,
  p.time_created AS created_ms,
  json_extract(m.data, '$.providerID') AS provider_id,
  json_extract(m.data, '$.modelID') AS model_id,
  json_extract(p.data, '$.tokens.input') AS input_tokens,
  json_extract(p.data, '$.tokens.output') AS output_tokens,
  json_extract(p.data, '$.tokens.reasoning') AS reasoning_tokens,
  json_extract(p.data, '$.tokens.cache.read') AS cache_read_tokens,
  json_extract(p.data, '$.tokens.cache.write') AS cache_write_tokens,
  json_extract(p.data, '$.tokens.total') AS total_tokens,
  json_extract(p.data, '$.cost') AS cost
FROM part AS p
JOIN message AS m ON m.id = p.message_id
WHERE p.rowid > :after_rowid
  AND p.rowid <= :snapshot_max_rowid
  AND p.time_created >= :day_start_ms
  AND p.time_created < :next_day_start_ms
  AND json_extract(p.data, '$.type') = 'step-finish'
  AND json_extract(m.data, '$.role') = 'assistant';
```

La implementación debe confirmar `role=assistant` contra la forma oficial ya fijada en la fixture; no selecciona `message.data` o `part.data` completos. Si proveedor/modelo falta o un número es inválido (tokens no enteros/no negativos, coste no finito/negativo), el lote se declara incompatible/fallido antes de publicar un agregado parcial engañoso.

`time_created` está en milisegundos Unix. `DayKey` debe guardar fecha local, offset local y los límites `[inicio local, siguiente inicio local)` convertidos a milisegundos con `chrono::Local`; esto trata correctamente las fechas de cambio DST. No usar UTC para decidir «hoy».

Mapeo de presentación:

| Columna OpenCode hoy | Origen autoritativo |
|---|---|
| in | `tokens.input` de cada `step-finish` |
| out | `tokens.output` |
| cache→ | `tokens.cache.read` |
| cache+ | `tokens.cache.write` |
| total | `tokens.total`, solo cuando esté persistido |
| coste | `cost` de `step-finish` |
| reasoning | se conserva fuera de la tabla; Pi no tiene columna equivalente y no se suma artificialmente a entrada/salida |

Cada métrica de un grupo es `Option`: `Some(0)` significa cero persistido; `None` significa que al menos un step-finish del grupo no la persistió. En este último caso la tabla muestra `—`, no una suma parcial ni cero inventado. Los acumulados usan sumas comprobadas (`u64`) y coste `f64` finito/no negativo; overflow o valor no representable causa fallo recuperable del snapshot. Agrupar por `(providerID, modelID)` y ordenar por total cuando exista, con desempate estable proveedor/modelo.

El índice local solo puede persistir: versión, `DayKey`, identidad/metadatos de DB, watermark y totales por pareja proveedor/modelo con sus banderas de presencia. Nunca IDs de mensajes/partes, JSON, prompts, herramientas, credenciales, cuentas ni rutas de proyectos.

## Cursor incremental y plan de consulta

La primera carga de un día debe aceptar un escaneo de `part`: no existe índice temporal y no se puede crear uno. Antes de leer, tomar `MAX(part.rowid)` como límite de instantánea. El bootstrap/rebuild aplica el filtro del día hasta ese límite y guarda su agregado más el watermark. La operación pesada ocurre solo en bootstrap, medianoche o recuperación, nunca cada 60 s.

En un índice válido:

1. Leer `MAX(part.rowid)`; si no creció, no hay consulta de filas.
2. Ejecutar la proyección anterior para `(watermark, max_rowid]`; el filtro temporal y JSON se evalúa sobre ese sufijo.
3. Añadir contribuciones al agregado, mover el watermark a ese máximo y persistir el índice completo con `cache::atomic_save`.

El plan esperado para el sufijo es `SEARCH p USING INTEGER PRIMARY KEY (rowid>?)` (el límite superior acota el rango) y búsqueda PK de `message.id` por cada parte; `time_created` y JSON son filtros residuales. Los índices oficiales `(message_id, id)` y `session_id` en `part`, y `(session_id,time_created,id)` en `message`, no resuelven el filtro global por día y no se deben tocar. La prueba de plan se hace solo contra fixture y comprueba que el steady state no sea `SCAN part`.

El índice se invalida y hace rebuild seguro si cambia la versión del índice, fecha u offset, identidad de archivo (dev/inode Unix; ruta canónica como fallback), `PRAGMA schema_version`, forma mínima de tablas/columnas, o si `MAX(rowid) < watermark`; también ante JSON local corrupto, escritura atómica fallida no recuperable o DB reemplazada. Guardar además page size/page count y metadatos de archivo como señales diagnósticas, no como motivo de rebuild en cada append/WAL.

### Actualizaciones y borrados: límite explícito

OpenCode permite en principio update/delete, pero la creación normal de `step-finish` es append-oriented. El cursor por rowid es exacto para inserciones append, no puede detectar una edición o borrado histórico sin volver a escanear `part` (no hay índice temporal ni journal de cambios que se pueda consultar sin ampliar el coste/alcance).

Política propuesta: mantener el snapshot incremental entre rebuilds; reconciliar por scan diario al bootstrap, al cambio de día y ante los disparadores anteriores. Un refresco manual puede solicitar una reconciliación completa explícita, siempre en el worker y sin bloquear providers; los refresh automáticos no la hacen. La TUI debe identificar el resultado como lectura incremental local y el diseño/UX debe documentar que una edición/borrado raro puede permanecer hasta esa reconciliación. Si el producto exige exactitud inmediata también para updates/deletes, el alcance debe aceptar un scan completo por refresh o una fuente oficial de cambios: no hay una solución simultáneamente exacta, indexada y sin modificar OpenCode.

## TUI independiente

Introducir un tipo de estado explícito, p. ej. `OpenCodePanelState` (`Loading`, `Ready(rows)`, `Unavailable(reason)`, `Stale { rows, reason }`) y una fila `OpenCodeUsageRow` con métricas opcionales. En `tui.rs`:

- añadir `Update::OpenCodeTokens(...)`, `opencode_tokens`, `opencode_scanning` y `begin_opencode_token_scan`; el guard evita scans solapados igual que `begin_pi_token_scan`;
- lanzar su hilo desde `refresh` pero no esperar su respuesta; `Status` sigue actualizándose aunque OpenCode esté leyendo, y viceversa;
- reservar/pintar siempre una sección `OpenCode hoy` después de `Pi hoy`, con las columnas de Pi que aplican: provider, modelo, in, out, cache→, cache+, total, coste;
- pintar cero filas como «sin uso hoy», ausencia como «OpenCode no disponible» y error recuperable sin borrar las filas anteriores;
- usar `—` para `None`, y la misma ayuda de formato de conteo/coste solo cuando haya valor.

No mezclar las filas ni los flags de Pi: comparten estética y patrón de background work, no fuente, tipo de datos, cache ni total. `--json`, `--waybar` y el texto de ayuda no cambian.

## Archivos, símbolos y pruebas para diseño

| Archivo | Símbolos/seam previstos | Pruebas necesarias |
|---|---|---|
| `Cargo.toml` | dependencia `rusqlite` bundled | compilación se deja para apply/verify. |
| `src/main.rs` | `mod opencode_tokens;` | contrato de modos sin cambio. |
| `src/cache.rs` | `opencode_daily_index_file` | ruta independiente y guardado atómico, sin mezclar índice Pi. |
| `src/opencode_tokens.rs` (nuevo) | descubrimiento, apertura RO, validación de esquema, `DayKey`, índice V1, consulta bootstrap/sufijo, agregación y estado | fixture temporal con tablas/índices oficiales mínimos; XDG/OPENCODE_DB; ausente/permisos/ocupada; medianoche/DST; varios provider/model; métricas cero/ausentes; coste; step-finish duplicables; query plan; steady state sin scan; regresión watermark/cache corrupta/reemplazo/schema; privacidad del JSON de índice. |
| `src/tui.rs` | `Update`, `App`, guard de scan, layout, altura y renderer OpenCode | no solape, actualización Status/Pi mientras OpenCode está activo, `Ready/Empty/Unavailable/Stale`, filas separadas y regresión de render Pi. |
| `src/output.rs` | sin cambios | conservar pruebas byte-estables existentes para JSON/Waybar. |

La fixture debe declarar solo `message(id PRIMARY KEY, session_id, time_created, time_updated, data)` y `part(id PRIMARY KEY, message_id, session_id, time_created, time_updated, data)`, más los índices oficiales: `message(session_id,time_created,id)`, `part(message_id,id)` y `part(session_id)`. Sus JSON sintéticos contienen únicamente role/proveedor/modelo y metadatos step-finish; nunca prompts, tools, cuentas o credenciales. Para inspeccionar el plan, usar `EXPLAIN QUERY PLAN` únicamente sobre esa fixture.

## Compatibilidad y límites

- Se conserva la API serializada `providers::Status` y todos sus consumidores.
- Se conserva `PiUsageTotals` (campos obligatorios y costes desglosados) sin forzar a OpenCode a adoptar sus semánticas.
- La base OpenCode no recibe escritura ni un índice auxiliar; el único estado nuevo vive bajo `XDG_CACHE_HOME`/`~/.cache/lazysubs`.
- JSON heredado, canales no prod/latest sin `OPENCODE_DB`, historia, cuota/allowance remoto y salidas JSON/Waybar siguen fuera de alcance.

## Siguiente fase

Pasar a `sdd-design` con este mapa y mantener el trade-off de update/delete como decisión explícita. Antes de aplicar, el diseño debe convertir la política elegida en criterios de aceptación y secuencia RED→GREEN→triangulación; esta fase no ejecutó build ni tests.
