# A. Proposal

## Intent

Añadir a lazysubs una sección TUI independiente, **«OpenCode hoy»**, que agregue por proveedor/modelo el uso y coste del día local leyendo de forma privada, incremental y no bloqueante la SQLite/WAL de OpenCode. La fuente será cada parte autoritativa `step-finish`, no el resumen mutable del mensaje.

## Scope

**Dentro:** descubrimiento oficial de la base, lectura SQLite estrictamente de solo lectura, agregado diario local, caché incremental propia y versionada, reconciliación acotada, estados TUI independientes y pruebas con fixtures SQLite sintéticas.

**Fuera:** cuotas o allowance, API/proceso de OpenCode, autenticación/cuentas, prompts o contenido, proyectos, historial, migración del almacenamiento JSON heredado, modificación de SQLite/WAL, totales combinados y exposición por JSON o Waybar. Claude, Codex, Pi y la caché de providers conservan sus contratos.

## Affected areas

| Archivo | Responsabilidad prevista |
|---|---|
| `Cargo.toml`, `Cargo.lock` | `rusqlite = { version = "0.37", features = ["bundled"] }`; el lock fijará la versión resuelta. |
| `src/main.rs` | Declarar `mod opencode_tokens;`; no cambiar argumentos ni flujos no TUI. |
| `src/cache.rs` | Añadir `opencode_daily_index_file()` y reutilizar `atomic_save`; no mezclar el índice de Pi. |
| `src/opencode_tokens.rs` (nuevo) | Descubrimiento, apertura RO, validación, consulta, agregado, índice V1 y estados de lectura. |
| `src/tui.rs` | Estado, update, guard de scan, worker y tabla **«OpenCode hoy»** independientes. |
| `src/output.rs`, providers, Claude, Codex y Pi | Sin cambios funcionales; solo evidencia de regresión donde ya existan pruebas. |

Símbolos principales: `resolve_opencode_db`, `sqlite_read_only_uri`, `DayWindow`, `DbIdentity`, `OpenCodeIndexV1`, `OpenCodeUsageRow`, `OpenCodePanelState`, `collect_opencode_daily`, `Update::OpenCodeTokens`, `App::opencode_tokens`, `App::opencode_scanning` y `begin_opencode_token_scan`.

## Risks

- Un bootstrap o una reconciliación debe recorrer `part` porque no existe índice global por tiempo; se limita a primera carga, invalidación o una vez cada 24 horas, nunca cada 60 segundos.
- `rowid` detecta inserciones append, pero no updates/deletes históricos. La política normal asume inmutabilidad de `step-finish`; una reconciliación completa cada 24 horas acota la posible obsolescencia.
- SQLite bundled aumenta compilación y tamaño del binario, a cambio de una versión/ABI reproducible con JSON1 y lectura WAL.
- Cambios de esquema, reemplazos o `VACUUM` pueden invalidar cursores; identidad, versión de esquema, ancla de watermark y regresión de `MAX(rowid)` fuerzan reconstrucción segura.
- Datos numéricos corruptos no deben producir parciales engañosos; el snapshot anterior se conserva como stale.

## Rollback

Revertir la dependencia, el módulo y la integración TUI devuelve el comportamiento anterior. Eliminar `opencode-daily-token-index-v1.json` es seguro: solo provoca un bootstrap posterior y nunca afecta la base de OpenCode ni el índice de Pi.

## Success criteria

La TUI muestra agregados correctos del día local por proveedor/modelo y `raz` por separado; los refrescos normales leen solo el sufijo por `rowid`; una SQLite activa en WAL nunca se modifica; errores de OpenCode no bloquean otras fuentes; la caché no contiene contenido sensible; y `cargo test` demuestra los escenarios de esta especificación usando únicamente fixtures temporales.

# B. Spec

## R1. Descubrimiento determinista

El sistema **MUST** resolver `OPENCODE_DB` con la semántica oficial: un valor absoluto se usa tal cual; un valor relativo se resuelve bajo el directorio de datos de OpenCode (`$XDG_DATA_HOME/opencode`, o `$HOME/.local/share/opencode` si XDG no está definido); `:memory:` se declara no disponible porque otro proceso no puede leer esa conexión. Un valor vacío **MUST** equivaler a no definido. Sin override, el sistema **MUST** usar únicamente el default prod/latest `…/opencode/opencode.db`; **MUST NOT** hacer glob, recorrer directorios ni adivinar archivos de canales, perfiles o desarrollo. Un archivo alternativo de canal **MAY** usarse solo si `OPENCODE_DB` lo nombra.

| Escenario | Given | When | Then |
|---|---|---|---|
| Override absoluto | `OPENCODE_DB=/tmp/oc.db` | Se resuelve la fuente | Se usa exactamente `/tmp/oc.db`, sin añadir directorios ni sufijos. |
| Override relativo | `OPENCODE_DB=canal.db` y `XDG_DATA_HOME=/data` | Se resuelve la fuente | Se usa `/data/opencode/canal.db`. |
| Memoria | `OPENCODE_DB=:memory:` | Se inicia el collector | Devuelve `Unavailable(EphemeralDatabase)` sin abrir ni crear archivos. |
| Default XDG/HOME | No hay override | Se resuelve la fuente | Usa `$XDG_DATA_HOME/opencode/opencode.db` o, en su ausencia, `$HOME/.local/share/opencode/opencode.db`. |
| Canal no declarado | No existe el default y hay otros `.db` en el directorio | Se resuelve la fuente | Devuelve ausencia; no busca ni prueba esos archivos. |

## R2. Apertura SQLite privada, RO y WAL-aware

El sistema **MUST** construir una URI `file:<ruta-percent-encoded>?mode=ro`, abrirla con `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`, fijar `busy_timeout` a **100 ms** y `PRAGMA query_only=ON`, y usar una conexión privada por worker. Cada refresh **MUST** leer dentro de una transacción diferida coherente. **MUST NOT** usar `immutable=1`, `CREATE`, `READ_WRITE`, conexión compartida, checkpoint, backup/copia, `journal_mode`, DDL, migraciones o índices. **MUST NOT** consultar auth/account ni llamar al ejecutable, API o proceso de OpenCode.

`rusqlite` con `bundled` **MUST** aportar una SQLite reproducible con JSON1/WAL en CI y equipos sin headers/ABI de sistema. El coste aceptado es mayor compilación y binario. SQLite del sistema se rechaza por variación de ABI, versión y opciones; un subprocess `sqlite3` se rechaza por dependencia externa, quoting/paths, parsing frágil, cancelación y errores no tipados.

| Escenario | Given | When | Then |
|---|---|---|---|
| Ausente o sin permisos | La ruta no existe o no es legible | Se intenta abrir | Se devuelve `Unavailable(Missing|PermissionDenied)` en ≤100 ms de espera SQLite, sin crear la base. |
| Ocupada | SQLite mantiene un lock que impide la lectura | Vence el busy timeout | Se devuelve `Unavailable(Busy)` o `Stale` si había filas previas; no hay retry dentro del ciclo. |
| WAL confirmado | OpenCode confirma un `step-finish` en `-wal` | Comienza después la transacción de lectura | La fila es visible y se agrega sin checkpoint ni copia de WAL/SHM. |
| Commit concurrente | Una fila se confirma después de tomar `snapshot_max_rowid` | Termina el refresh | No entra en ese snapshot y entra en el siguiente; no hay parcial inconsistente. |

## R3. Fuente autoritativa y proyección mínima

El sistema **MUST** contabilizar exclusivamente filas de `part` cuyo JSON válido tenga `type='step-finish'`, unidas por `p.message_id = m.id` a la PK de `message`, cuyo JSON válido tenga `role='assistant'`. Proveedor y modelo **MUST** venir de `message.data.providerID/modelID`. **MUST NOT** usar `message.data.tokens` ni `message.data.cost`, porque el nivel mensaje puede sobrescribirse en respuestas multi-step.

Las únicas columnas JSON proyectadas **MUST** ser escalares de role/type, proveedor/modelo, tokens y coste; la consulta y la API Rust **MUST NOT** retornar `p.data` ni `m.data` completos:

```sql
WITH projected AS (
  SELECT
    p.rowid AS part_rowid,
    p.id AS part_id,
    p.time_created AS created_ms,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.type') END AS part_type,
    CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.role') END AS message_role,
    CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.providerID') END AS provider_id,
    CASE WHEN json_valid(m.data) THEN json_extract(m.data, '$.modelID') END AS model_id,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.input') END AS input_tokens,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.output') END AS output_tokens,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.reasoning') END AS reasoning_tokens,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.cache.read') END AS cache_read_tokens,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.cache.write') END AS cache_write_tokens,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.total') END AS stored_total_tokens,
    CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.cost') END AS cost
  FROM part AS p
  JOIN message AS m ON m.id = p.message_id
  WHERE p.rowid > :after_rowid
    AND p.rowid <= :snapshot_max_rowid
    AND p.time_created >= :day_start_ms
    AND p.time_created < :next_day_start_ms
)
SELECT part_rowid, part_id, created_ms, provider_id, model_id,
       input_tokens, output_tokens, reasoning_tokens,
       cache_read_tokens, cache_write_tokens, stored_total_tokens, cost
FROM projected
WHERE part_type = 'step-finish' AND message_role = 'assistant';
```

El rebuild usa la misma forma con `after_rowid=0`; el refresh normal **MUST** usar el watermark real. Los planes de fixture **MUST** mostrar para el sufijo `SEARCH p USING INTEGER PRIMARY KEY (rowid>? AND rowid<?)` y lookup por el índice/PK de `message.id`; **MUST NOT** mostrar `SCAN part` en steady state. Un scan de `part` solo es aceptable en rebuild.

| Escenario | Given | When | Then |
|---|---|---|---|
| Multi-step | Un mensaje assistant tiene tres `part` step-finish y un resumen de mensaje con solo el último paso | Se agrega | Se cuentan las tres partes una vez; el resumen del mensaje no se lee y no hay infracómputo. |
| Filas no contables | Hay partes tool/text, mensajes user y JSON malformado | Se consulta | Se ignoran sin exponer JSON ni abortar por `json_extract`; el watermark puede avanzar sobre ellas. |
| Privacidad de proyección | La fixture contiene campos sintéticos extra de contenido | Se captura la forma/columnas del resultado | Ningún campo JSON completo, prompt, tool, auth/account o message id sale de SQLite. |

## R4. Día local y agrupación

El sistema **MUST** definir `DayWindow` como fecha local y límites `[inicio del día, inicio del día siguiente)` convertidos a Unix ms mediante `chrono::Local`; **MUST NOT** asumir días UTC ni 24 horas en cambios DST. Las filas **MUST** agruparse por la pareja exacta `(providerID, modelID)` y ordenarse por total descendente cuando exista, con desempate estable por proveedor y modelo.

| Escenario | Given | When | Then |
|---|---|---|---|
| Mismo día/prior day | Hay partes justo dentro de hoy y partes del día local anterior | Se agrega hoy | Solo entran las de `[day_start_ms,next_day_start_ms)`. |
| Separación | El mismo modelo aparece con dos providers y un provider con dos modelos | Se agrega | Se producen tres filas independientes, sin fusionar claves. |
| DST | El día local tiene 23 o 25 horas | Se calcula `DayWindow` | Los límites corresponden a dos medianoches locales consecutivas, no a `start+86_400_000`. |

## R5. Semántica numérica honesta

Cada fila TUI **MUST** conservar por separado `in`, `out`, `raz`, `cache→`, `cache+`, `total` y `coste`. Reasoning **MUST NOT** plegarse en output. Para cada step-finish, un `tokens.total` entero, finito y no negativo **MUST** prevalecer; si falta o es inválido, `effective_total` **MUST** ser la suma comprobada de las categorías presentes y válidas entre input/output/reasoning/cache read/cache write. Si ninguna categoría está presente, total **MUST** ser ausente.

Una categoría agregada **MUST** ser `None` si falta en al menos una parte del grupo; `Some(0)` **MUST** conservar un cero persistido. El total del grupo suma `effective_total`; el coste del grupo **MUST** ser `None` si falta en alguna parte. Tokens **MUST** ser enteros representables en `u64`; coste **MUST** ser `f64` finito y no negativo. Overflow, tipo incorrecto o valores negativos/no finitos en categorías o coste **MUST** rechazar el snapshot antes de publicarlo; un total almacenado inválido **MAY** usar el fallback válido, pero un overflow del fallback rechaza el snapshot. Provider/model ausentes o vacíos **MUST** rechazar el snapshot, no crear un bucket ficticio.

| Escenario | Given | When | Then |
|---|---|---|---|
| Reasoning y total | Una parte tiene input 2, output 3, reasoning 5, caches 7/11 y total almacenado 40 | Se agrega | `raz=5`, `out=3` y `total=40`; reasoning no se suma a output. |
| Fallback | La misma parte no tiene total almacenado | Se agrega | El total es 28 mediante suma comprobada de las cinco categorías. |
| Ausencia y cero | Una parte omite coste/cache write y otra persiste cero | Se agrega el grupo | Coste/cache write muestran `—`; un campo presente cero sigue siendo `0`. |
| Numérico inválido | Aparece token negativo/fraccional, coste NaN/infinito/negativo u overflow | Se refresca | No se publica parcial; se conserva el snapshot anterior como `Stale(InvalidUsage)` o se muestra unavailable en bootstrap. |
| Identidad ausente | Falta providerID o modelID | Se refresca | No se atribuye a `unknown`; el lote falla de forma recuperable. |

## R6. Índice local V1 e incrementalidad

El sistema **MUST** persistir atómicamente mediante `cache::atomic_save` en:

- `$XDG_CACHE_HOME/lazysubs/opencode-daily-token-index-v1.json`, o
- `$HOME/.cache/lazysubs/opencode-daily-token-index-v1.json` sin XDG.

`OpenCodeIndexV1` **MUST** tener `format="lazysubs-opencode-daily"`, `version=1` y únicamente:

```text
DbIdentity {
  platform_file_id, path_fingerprint, schema_version, user_version,
  schema_fingerprint, page_size, watermark_part_id
}
DayWindow { local_date, start_ms, next_start_ms, start_offset_s, next_offset_s }
watermark_rowid: i64
seen_part_ids: set<string>          // solo IDs opacos del día actual
totals: map<(provider_id,model_id), métricas opcionales>
last_full_rebuild_ms: i64
```

`platform_file_id` **MUST** ser `{device,inode}` en Unix o `{volume_serial,file_index}` en Windows. Si la plataforma no expone esos datos, `path_fingerprint` **MUST** ser FNV-1a de 64 bits sobre los bytes de la ruta canónica; es una señal de identidad, no una frontera criptográfica, y nunca se guarda la ruta en claro. `schema_fingerprint` **MUST** ser la cadena normalizada y ordenada de nombres/tipos/PK de las columnas mínimas requeridas en `message` y `part`; no incluye filas ni columnas extra. `seen_part_ids` **MUST** estar acotado a las partes del día actual, deduplicado y borrarse al cambiar de día. Cada total persistido **MUST** contener exactamente `provider_id`, `model_id`, `input`, `output`, `reasoning`, `cache_read`, `cache_write`, `total` y `cost`, con presencia explícita. No se persisten message IDs, JSON, timestamps por fila, prompts, tools, credenciales, cuentas o rutas de proyecto.

Dentro de una misma transacción, el collector **MUST** ejecutar primero un cursor probe equivalente a:

```sql
SELECT bounds.max_rowid,
       (SELECT id FROM part WHERE rowid = :watermark_rowid) AS watermark_part_id
FROM (SELECT COALESCE(MAX(rowid), 0) AS max_rowid FROM part) AS bounds;
```

En caché válida: si `max_rowid == watermark`, **MUST NOT** ejecutar la consulta de filas; si crece, **MUST** consultar solo `(watermark,max_rowid]`, deduplicar por `part.id`, actualizar totales y persistir antes de publicar. En bootstrap/rebuild **MAY** escanear `part` una vez hasta el snapshot max.

| Escenario | Given | When | Then |
|---|---|---|---|
| Primer bootstrap | No existe índice y la fixture tiene R partes | Se recoge hoy | Hay un cursor probe y una consulta rebuild; se agregan solo filas válidas de hoy y se guarda watermark/IDs/totales V1. |
| Base sin cambios | El max rowid coincide con watermark | Llega otro refresh | Hay exactamente 1 data query (cursor probe), 0 consultas de proyección y 0 filas `part` del sufijo procesadas. |
| Sufijo append | Se añaden N rowids contiguos | Llega otro refresh | Hay exactamente 1 cursor probe + 1 consulta suffix; el plan visita solo esos N candidatos y cada parte contable entra una vez. |
| ID duplicado | El seam de agregación reentrega un `part.id` ya visto | Se aplica el lote | Una repetición idéntica se ignora; si el mismo ID trae métricas distintas, el snapshot falla antes de publicar. |
| Caché corrupta/incompatible | El JSON no parsea, format/version no coincide o faltan campos | Se refresca | Se ignora y reconstruye desde la DB; nunca se usan totales parciales. |
| Persistencia fallida | El guardado atómico falla | Se completa la consulta | No se publica un índice imposible de reanudar; se conservan filas previas como stale y la DB no cambia. |

## R7. Medianoche, reemplazo y reconciliación

Al detectar una nueva fecha local, el sistema **MUST** vaciar totals/seen IDs, conservar el watermark solo si identidad/esquema/ancla siguen válidos y procesar el sufijo desde ese watermark con el nuevo `DayWindow`; **MUST NOT** hacer un full scan por cada tick ni exigir un full scan en la medianoche normal.

El sistema **MUST** invalidar y reconstruir ante cambio de versión/formato del índice, identidad de archivo, `schema_version`/`user_version`/fingerprint, ausencia o cambio del ID ancla, o `MAX(rowid) < watermark`. Un esquema DB compatible cambiado **MAY** reconstruirse; uno sin tablas/columnas/PK/JSON1 requeridas **MUST** devolver `SchemaIncompatible` sin consultar uso. Cambios normales de tamaño/mtime/page_count por WAL/checkpoint **MUST NOT** confundirse por sí solos con reemplazo.

La política normal **MUST** considerar inmutables los `step-finish` una vez insertados. Para acotar updates/deletes raros, el sistema **MUST** hacer una reconciliación completa como máximo una vez cada 24 horas y en el primer refresh exitoso donde `now >= last_full_rebuild_ms + 24h`; bootstrap/rebuild reinicia ese reloj. Nunca se reconcilia en cada refresco de 60 s. Hasta entonces, una edición/borrado histórico no detectado por identidad/ancla **MAY** permanecer stale. Un `VACUUM` que cambie file ID, schema, ancla o reduzca max fuerza rebuild inmediato; si conserva todas esas señales pero renumera otros rowids, se trata como el mismo caso raro y queda corregido por la reconciliación ≤24 h. Si la aplicación estuvo cerrada al vencer el plazo, el primer refresh al abrir reconcilia.

| Escenario | Given | When | Then |
|---|---|---|---|
| Medianoche abierta | La app sigue abierta al cambiar la fecha | Llega el primer tick del nuevo día | Totales/seen se reinician, se conserva watermark seguro y solo se procesa el sufijo nuevo. |
| Medianoche cerrada | El índice es de ayer y se crearon partes mientras la app estaba cerrada | Se abre hoy | Se reinicia el día y el sufijo posterior al watermark aporta solo filas de hoy; filas previas se filtran. |
| Reemplazo/VACUUM/regresión | Cambia file id/schema/ancla o max rowid retrocede | Se hace el cursor probe | Se descarta el cursor y se reconstruye un snapshot coherente; no se mezcla DB vieja/nueva. |
| Cambio WAL normal | Solo crece/cambia WAL y la identidad/esquema siguen válidos | Se refresca | Se usa el camino incremental; no se invalida por mtime/page_count. |
| Update/delete raro | Una parte histórica ya contada se edita o borra sin cambiar max/ancla | Se refresca antes y después de 24 h | Puede seguir stale antes; el primer refresh debido reconstruye y corrige, con un único full scan en ese periodo. |
| Versión DB incompatible | Cambian las columnas/PK requeridas | Se valida el esquema | Se conserva el último panel como stale o se muestra unavailable; no hay SQL de uso ni mutación. |

## R8. Estado y concurrencia TUI independientes

`OpenCodePanelState` **MUST** distinguir `Loading`, `Ready(Vec<OpenCodeUsageRow>)`, `Empty`, `Unavailable(reason)` y `Stale { rows, reason }`; `OpenCodeUnavailableReason` **MUST** distinguir `Missing`, `PermissionDenied`, `Busy`, `EphemeralDatabase`, `SchemaIncompatible`, `InvalidUsage`, `CacheWriteFailed` y `ReadFailed`, sin incluir rutas ni mensajes SQLite crudos. `src/tui.rs` **MUST** mantener `Update::OpenCodeTokens`, `App::opencode_tokens` y `App::opencode_scanning` separados de provider, Claude y Pi. `begin_opencode_token_scan` **MUST** hacer compare/set del guard antes de crear el worker y limpiar el guard al recibir resultado; mientras esté activo, refrescos manuales o automáticos **MUST NOT** lanzar otro scan OpenCode.

Providers, Claude, Pi y OpenCode **MUST** publicar por updates independientes y **MUST NOT** esperar, cancelar ni borrar el estado de los otros. La tabla **«OpenCode hoy»** **MUST** aparecer separada después de **«Pi hoy»**, con columnas `provider`, `modelo`, `in`, `out`, `raz`, `cache→`, `cache+`, `total`, `coste`; `None` se muestra `—`. Un error temporal conserva filas previas como stale.

| Escenario | Given | When | Then |
|---|---|---|---|
| No bloqueo | El worker OpenCode está lento/ocupado | Provider, Claude o Pi completan | Sus updates y render se aplican sin esperar a OpenCode. |
| Scan duplicado | Coinciden tick y refresh manual con un worker activo | Ambos piden refresh | Solo existe un scan OpenCode; las demás fuentes mantienen sus propios scans. |
| Fallo con datos previos | Había `Ready(rows)` y la DB queda busy | Llega el resultado | La tabla conserva rows y muestra stale/busy; Pi y providers no cambian. |
| Estados visibles | No hay DB, no hay uso hoy o hay filas | Se renderiza | Se ve respectivamente no disponible, sin uso hoy o la tabla; la interacción no se bloquea. |

## R9. Compatibilidad y privacidad de pruebas

El sistema **MUST NOT** cambiar `providers::Status`, caché de providers, parser/caché Pi, Claude, Codex, `--json`, `--waybar` ni sus formatos. OpenCode **MUST NOT** contribuir a un total combinado. Las pruebas **MUST** usar solo SQLite temporales con `message(id PRIMARY KEY, session_id, time_created, time_updated, data)` y `part(id PRIMARY KEY, message_id, session_id, time_created, time_updated, data)`, más índices sintéticos equivalentes a `message(session_id,time_created,id)`, `part(message_id,id)` y `part(session_id)`. **MUST NOT** abrir la DB real ni contener prompts, credenciales, tools o filas reales.

| Escenario | Given | When | Then |
|---|---|---|---|
| Salidas intactas | Se ejecutan los mismos estados provider/Pi antes y después | Se serializa JSON/Waybar | La salida es byte-equivalente y no contiene OpenCode. |
| Pi intacto | OpenCode está unavailable o ready | Se actualiza/renderiza Pi | Estado, totales y caché Pi son idénticos y la tabla sigue separada. |
| Fixture privada | Se inspeccionan archivos y datos de test | Corren las pruebas | Solo hay role/provider/model y metadatos sintéticos step-finish; ninguna prueba conoce `~/.local/share/opencode/opencode.db`. |

## R10. Seams de TDD y medición

El diseño **MUST** permitir strict TDD sin sleeps ni DB real mediante funciones deterministas: `resolve_opencode_db(env, dirs)`, `DayWindow::at(clock)`, `validate_schema(connection)`, `aggregate_projected_rows(rows)`, `load_or_rebuild_index(path, identity)`, `collect_opencode_daily(path, cache_path, clock)` y un `ScanGate`/canal TUI controlable. Los tests del módulo **MUST** poder contar ejecuciones de las constantes `SQL_CURSOR_PROBE`, `SQL_SUFFIX` y `SQL_REBUILD`, y examinar `EXPLAIN QUERY PLAN` solo contra fixtures.

| Escenario | Given | When | Then |
|---|---|---|---|
| Coste determinista | Una fixture contigua tiene R filas y luego añade N | Bootstrap, unchanged y suffix | Bootstrap: 1 probe+1 rebuild y R candidatos; unchanged: 1+0 y 0; suffix: 1+1 y N, sin `SCAN part` en suffix. |
| Canal determinista | Se controla el worker con canales, sin esperas | Se solapan dos solicitudes | El contador de invocaciones OpenCode queda en 1 mientras provider/Pi pueden completar. |

# C. Decisions

## Decisiones y trade-offs

1. **`rusqlite` 0.37 bundled, no SQLite del sistema.** Gana portabilidad de build, JSON1/WAL uniforme y errores tipados. Se acepta mayor binario/compilación. No se añade ORM: una consulta estrecha es más simple y auditable.
2. **No subprocess `sqlite3`.** Evita dependencia de PATH/versiones, parsing textual, quoting de rutas y control de timeout/proceso. Tampoco se usa OpenCode CLI/API porque se cuelga, amplía permisos y contradice la lectura local privada.
3. **Partes `step-finish`, no resumen de message.** Es la granularidad autoritativa y evita el infracómputo multi-step. Message solo aporta role/provider/model por su PK.
4. **SQLite RO normal, nunca immutable.** `immutable=1` puede ignorar WAL; `mode=ro` + query-only permite la instantánea confirmada sin escribir ni hacer checkpoint.
5. **Watermark + caché diaria acotada.** Es el menor mecanismo que evita recorrer una DB de decenas de MB cada minuto sin modificar índices ajenos. Los IDs vistos se limitan al día vigente.
6. **Inmutabilidad normal + reconciliación cada 24 h.** Exactitud inmediata para updates/deletes exigiría full scan cada refresh o un change feed inexistente. La reconciliación acota la desviación sin violar el presupuesto periódico.
7. **Reasoning visible como `raz`.** Ocultarlo o sumarlo a output falsearía categorías. La columna adicional es una ampliación mínima y explícita de la tabla Pi.
8. **Fallo atómico del snapshot.** Identidad ausente, números inválidos u overflow no generan parciales: se conserva el último estado válido. JSON malformado/no-assistant se filtra porque no puede probarse que sea una unidad contable.
9. **Estado TUI propio.** Se copia el patrón de coordinación de Pi, no sus tipos ni semántica; esto mantiene fallos, caché y refresh desacoplados.

## Boundaries

- `opencode_tokens.rs` posee toda semántica OpenCode, SQL, privacidad, validación, índice y agregado.
- `cache.rs` posee únicamente la resolución del archivo de caché y escritura atómica genérica.
- `tui.rs` posee scheduling, supresión de concurrencia, estado visual y render; no interpreta JSON/SQLite.
- `main.rs` solo registra el módulo.
- `output.rs`, providers, Claude, Codex y Pi no poseen ninguna responsabilidad OpenCode y permanecen sin cambios funcionales.
- `sdd-apply` posee la secuencia RED→GREEN→triangulación→refactor y la ejecución; este documento no implementa ni divide tareas.

## Alternativas rechazadas

- **SQLite del sistema:** menor binario, pero builds/JSON1/ABI no reproducibles.
- **`sqlite3` subprocess:** superficie externa y resultados/timeout frágiles.
- **`immutable=1`, copiar DB/WAL o checkpoint:** instantáneas incorrectas o mutación/contención.
- **API/CLI de OpenCode:** no fiable, no necesaria y fuera de privacidad/alcance.
- **Agregar `message.data.tokens/cost`:** pierde pasos de respuestas multi-step.
- **Índice nuevo en OpenCode:** aceleraría fecha, pero modifica un esquema ajeno.
- **Full scan cada 60 s:** exacto para updates, pero desproporcionado para ~61 MB y contrario al alcance.
- **No reconciliar nunca:** más rápido, pero deja updates/deletes stale sin cota.
- **Fusionar con Pi o JSON/Waybar:** confunde semánticas y rompe contratos existentes.
- **Guardar filas/JSON completos:** simplificaría re-agregado, pero viola privacidad; el índice guarda solo cursor, IDs opacos diarios y totales.

# D. Success Criteria

La aceptación es observable cuando:

- Las pruebas de resolución cubren `OPENCODE_DB` absoluto, relativo, vacío, `:memory:`, XDG/HOME y política sin búsqueda de canales.
- Una fixture WAL confirma que una fila committed se ve con URI RO y que no cambian schema, journal mode ni archivos por acción de lazysubs; missing, permisos y busy producen estados recuperables dentro del timeout.
- Fixtures multi-step, límites locales/DST, provider/model, duplicado, métricas ausentes/cero, reasoning, fallback total, coste y números inválidos producen exactamente la semántica R3–R5.
- Bootstrap, unchanged, suffix, medianoche abierta/cerrada, reemplazo, `VACUUM`/ancla, regresión de rowid, schema/version y caché corrupta cumplen los conteos de data queries/filas de R6 y R10.
- `EXPLAIN QUERY PLAN` de fixture muestra búsqueda por rowid y PK de message en suffix, sin `SCAN part`; el único scan completo aparece en bootstrap/rebuild/reconciliación, nunca en cada refresh de 60 s.
- Un update/delete histórico permanece permitido como stale solo hasta el primer refresh debido a las 24 horas; la prueba usa reloj inyectado y demuestra un único rebuild en ese intervalo.
- El render contiene una tabla independiente **«OpenCode hoy»** con `raz`; un worker lento/fallido no impide updates provider/Claude/Pi y dos solicitudes solapadas crean un solo scan OpenCode.
- Pruebas de regresión demuestran que Pi, Claude, Codex, provider cache, JSON y Waybar no cambian y nunca incorporan OpenCode.
- El índice V1 parseado contiene solo el schema declarado, IDs de parte opacos del día y agregados; no contiene JSON crudo, prompts, tools, auth/account, message IDs ni rutas en claro.
- La verificación ejecutable requerida en `sdd-apply`/`sdd-verify` es `cargo test`; no hay suites integration/E2E, lint, format o coverage configuradas en `openspec/config.yaml` y no se afirma ninguna como ejecutada en esta fase.
