# Tasks — opencode-daily-token-usage

status: ready
blocked_by: none

Forecast de revisión (líneas de producción, sin tests): ~330 LOC concentradas en `src/opencode_tokens.rs` (~250), `Cargo.toml` (~3), `src/cache.rs` (~5), `src/main.rs` (~1) y `src/tui.rs` (~70 para `Update`/`App`/`draw_opencode_tokens`/render). Por debajo del presupuesto de 400 líneas; entra en una sola PR sin necesidad de chained split. Las pruebas (~700 LOC) viven dentro de los mismos `#[cfg(test)] mod` y no cuentan al presupuesto. Si al aplicar el grupo 2+3 la diff de producción se acerca a 350, abrir un commit/work-unit boundary entre bootstrap (R6 inicial) e incremental (R6 watermark) antes de continuar.

## // 001. Dependencia `rusqlite`, descubrimiento y contrato de conexión RO/WAL

- [x] 1.1 Añadir `rusqlite = { version = "0.37", features = ["bundled"] }` a `Cargo.toml` y registrar `mod opencode_tokens;` en `src/main.rs`
  - skills: `ein-discipline`, `work-unit-commits`, `architecture`
  - why: el diseño exige `rusqlite` con `bundled` para JSON1/WAL reproducible; el módulo debe existir antes de poder declarar sus funciones públicas
  - learn: una dependencia `bundled` mete SQLite estático en el binario — fija versión/ABI/JSON1 sin depender de headers del sistema, a cambio de tiempo de compilación y tamaño; declarar el módulo vacío en `main.rs` evita un cambio aislado de wiring al final
  - architecture: `Cargo.toml` declara; `main.rs` solo registra el módulo. Ningún consumidor fuera de `opencode_tokens.rs` debe importar `rusqlite`
  - avoid: usar `rusqlite` contra SQLite del sistema (rompe reproducibilidad de JSON1/WAL) o ejecutar `mod opencode_tokens;` con cuerpo preimplementado antes de tener sus tipos
  - verify: `cargo check` con la nueva dependencia resuelve (dejado a `sdd-apply`/`sdd-verify`); inspección visual de `Cargo.toml` y `src/main.rs`

- [x] 1.2 Crear `src/opencode_tokens.rs` con la función pura `resolve_opencode_db(env: &EnvSnapshot) -> DbResolution` y un helper de URI percent-encoded
  - skills: `ein-discipline`, `architecture`
  - why: R1 obliga a precedencia determinista (`OPENCODE_DB` absoluto, relativo bajo `$XDG_DATA_HOME/opencode`, `:memory:`, XDG/HOME default) y a no buscar canales no declarados
  - learn: la precedencia se modela mejor como tabla de pruebas Given/When/Then que como cascada de `if/else` implícita; una struct `EnvSnapshot { opencode_db, xdg_data_home, home }` permite testear sin variables de entorno reales
  - architecture: `EnvSnapshot` es el único seam de entorno; `resolve_opencode_db` no toca FS. La URI se construye con `file:` + path percent-encoded + `?mode=ro`; sin interpolate crudo
  - avoid: leer `std::env::var` directamente dentro de `resolve_opencode_db` (rompe el seam de TDD) o globs/`read_dir` para "descubrir" canales (prohibido por R1)
  - verify: `cargo test opencode_tokens::tests::resolve_*` (12 casos del escenario R1) — `OPENCODE_DB` absoluto sin sufijos, relativo bajo XDG, `:memory:` → `EphemeralDatabase`, vacío, XDG explícito, HOME explícito, ausencia de HOME, canal no declarado

- [x] 1.3 Crear `tests::fixture_db()` que devuelve un `tempfile::TempDir` con tablas `message`/`part` mínimas y los tres índices sintéticos del mapa
  - skills: `ein-discipline`, `architecture`
  - why: R9 exige pruebas solo contra SQLite temporales; R3/R6/R10 dependen de poder insertar filas y consultar planes deterministamente
  - learn: la fixture debe declarar `message(id PRIMARY KEY, session_id, time_created, time_updated, data)` y `part(id PRIMARY KEY, message_id, session_id, time_created, time_updated, data)` con `CREATE INDEX message(session_id,time_created,id)`, `part(message_id,id)` y `part(session_id)` — replicar los índices oficiales evita que el plan "use el índice" por accidente y desaparezca cuando cambien las estadísticas
  - architecture: la fixture vive solo en `#[cfg(test)] mod tests`; produce un `Connection` rusqlite y un `PathBuf` reutilizable por múltiples tests
  - avoid: usar la base real o copiar fixtures entre tests (rompe aislamiento y privacidad); declarar columnas extras en la fixture que el código pueda llegar a leer
  - verify: `cargo test opencode_tokens::tests::fixture_provises_minimal_schema` — comprueba `sqlite_master` tiene `message`, `part` y los tres índices con las columnas esperadas

- [x] 1.4 Implementar `open_read_only(path: &Path) -> Result<Connection, OpenCodeError>` con `mode=ro`, `busy_timeout=100ms` y `PRAGMA query_only=ON`
  - skills: `ein-discipline`, `performance`, `architecture`
  - why: R2 obliga a abrir con `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`, fijar timeout corto y `query_only=ON`; nunca `immutable=1`, nunca `READ_WRITE`, nunca conexión compartida
  - learn: `query_only=ON` es defensa en profundidad: aunque la URI ya es `mode=ro`, el pragma rechaza DML/DDL accidental incluso si la URI se construyera mal; `busy_timeout` corto evita que un lock ajeno nos ate la TUI
  - architecture: la función devuelve un `OpenCodeError` tipado (`Missing`, `PermissionDenied`, `Busy`, `SchemaIncompatible`, `ReadFailed`) sin rutas ni mensajes SQLite crudos — el caller de TUI nunca ve un `rusqlite::Error`
  - avoid: usar `immutable=1` (puede ignorar WAL), compartir `Connection` entre hilos o hacer `pragma journal_mode`/`checkpoint`/`backup`
  - verify: `cargo test opencode_tokens::tests::connection_*` — DB inexistente → `Missing` ≤100 ms; permisos denegados → `PermissionDenied`; DB en lock → `Busy` sin reintento; WAL con fila committed tras `BEGIN IMMEDIATE` ajeno es visible sin checkpoint

- [x] 1.5 Añadir `cache::opencode_daily_index_file() -> PathBuf` que devuelva `$XDG_CACHE_HOME/lazysubs/opencode-daily-token-index-v1.json` o el equivalente bajo `HOME/.cache`
  - skills: `ein-discipline`, `work-unit-commits`, `architecture`
  - why: R6 fija el nombre exacto del archivo; convive con `pi_daily_index_file` sin colisión
  - learn: separar el path del índice OpenCode del de Pi previene que un fallo de parseo del primero invalide el segundo — cada índice tiene su propio `format`/`version`
  - architecture: `cache.rs` posee únicamente la resolución del archivo y `atomic_save`; `opencode_tokens.rs` lee/escribe ese archivo
  - avoid: reutilizar `pi_daily_index_file` (mezcla semánticas), meter lógica OpenCode dentro de `cache.rs`
  - verify: `cargo test cache::tests::opencode_daily_index_file_uses_xdg_or_home_cache` — con `XDG_CACHE_HOME=/c/lazy` devuelve `/c/lazy/lazysubs/opencode-daily-token-index-v1.json`; sin XDG usa `$HOME/.cache/lazysubs/opencode-daily-token-index-v1.json`

## // 002. Proyección autoritativa `step-finish`, agrupación y validación numérica honesta

- [x] 2.1 Implementar `project_step_finish_rows(conn, after_rowid, snapshot_max, day_window) -> Result<Vec<ProjectedRow>, OpenCodeError>` con la CTE de R3
  - skills: `ein-discipline`, `architecture`
  - why: R3 fija la única proyección permitida — `p.data` y `m.data` nunca se devuelven enteros, solo escalares `json_extract` con guardas `json_valid`
  - learn: usar `CASE WHEN json_valid(p.data) THEN json_extract(p.data, '$.tokens.input') END` evita abortar la consulta cuando una fila tiene `data` corrupto: SQLite devuelve `NULL` y el filtro de validación Rust lo descarta después — un solo malformed JSON no rompe el agregado diario
  - architecture: la consulta usa parámetros `?1..?5` para evitar inyección y poder reusar el `Statement` preparado por scan; el `JOIN` es solo por `p.message_id = m.id` (PK), nunca sobre `data`
  - avoid: hacer `SELECT p.data, m.data` (fuga de contenido), usar `message.data.tokens`/`message.data.cost` (infracómputo multi-step), o filtrar en SQL por `type='step-finish'` sin guardar `json_valid` (aborta en JSON malformado)
  - verify: `cargo test opencode_tokens::tests::projection_*` — multi-step con tres `step-finish` y un resumen de mensaje produce tres filas y `message.data.cost` no entra; filas tool/text/user/JSON inválido se ignoran; la forma del resultado no contiene ninguna clave `data` completa

- [x] 2.2 Modelar `OpenCodeUsageRow { provider, model, input: Option<u64>, output, reasoning, cache_read, cache_write, total: Option<u64>, cost: Option<f64> }` y `aggregate_projected_rows(rows: Vec<ProjectedRow>) -> Result<Vec<OpenCodeUsageRow>, OpenCodeError>`
  - skills: `ein-discipline`, `architecture`
  - why: R5 exige semántica `Option` honesta (`Some(0)` = cero persistido, `None` = ausente en al menos una parte del grupo) y separación de `raz` fuera de `out`
  - learn: el total de grupo se calcula con `effective_total`: si la parte tiene `tokens.total` entero/no-negativo persiste, si no es la suma comprobada de input/output/reasoning/cache read/cache write; categorías se agregan solo si están presentes en TODAS las partes (si una falta → `None` para el grupo, nunca `0` inventado)
  - architecture: el agregador es una función pura testeada con `Vec<ProjectedRow>` construído a mano — sin tocar SQLite en estos tests; overflow `u64` y `f64` no finito/negativo devuelven `Err(OpenCodeError::InvalidUsage)` y no producen parcial
  - avoid: plegar `reasoning` en `output` (falsea categorías), devolver `Some(0)` cuando falta en una parte (miente al usuario), o usar `f64::INFINITY` para "no presente"
  - verify: `cargo test opencode_tokens::tests::aggregate_*` — input 2 + output 3 + reasoning 5 + caches 7/11 + stored total 40 → `raz=5, out=3, total=40`; sin stored total → `total=28` por suma; parte con coste faltante + parte con coste cero → grupo `cost=None`; tokens negativos/fraccionales/NaN/cost infinito → `Err(InvalidUsage)`; provider/model ausente → `Err(InvalidUsage)` sin bucket `unknown`

- [x] 2.3 Cubrir el invariante de privacidad en la proyección y el agregado
  - skills: `ein-discipline`, `architecture`
  - why: R9 prohíbe mensajes, partes, tools, auth o cuentas fuera de SQLite; la superficie debe ser demostrable
  - learn: la fixture inserta filas con `data` que contiene `"content":"SECRET-PROMPT"` y `"cwd":"/private/work"`; los tests assertan que ni el `Vec<OpenCodeUsageRow>` ni el `String` SQL capturado contienen esos strings
  - architecture: el assert es a nivel de API pública (lo que retorna `aggregate_projected_rows`) y a nivel de consulta (stringifying el `Statement` o usando `EXPLAIN` solo contra la CTE)
  - avoid: probar privacidad solo con `assert!(!row.contains(...))` sobre un log interno — la API debe ser el contrato
  - verify: `cargo test opencode_tokens::tests::privacy_*` — `aggregate_projected_rows` con filas sintéticas-secretas devuelve filas sin `SECRET-PROMPT`/`/private/work`/tool args/auth IDs; el SQL ejecutado contiene `json_extract(..., '$.tokens.input')` y nunca `p.data` o `m.data` completos

## // 003. Caché incremental V1, watermark, reconciliación y persistencia atómica

- [x] 3.1 Modelar `OpenCodeIndexV1`, `DbIdentity`, `DayWindow`, `OpenCodePanelState` y serialización/deserialización round-trip con `serde_json`
  - skills: `ein-discipline`, `architecture`, `cognitive-doc-design`
  - why: R6 define el contrato exacto del JSON persistido: `format="lazysubs-opencode-daily"`, `version=1`, `DbIdentity{platform_file_id,path_fingerprint,schema_version,user_version,schema_fingerprint,page_size,watermark_part_id}`, `DayWindow{local_date,start_ms,next_start_ms,start_offset_s,next_offset_s}`, `watermark_rowid`, `seen_part_ids`, `totals`, `last_full_rebuild_ms`
  - learn: `seen_part_ids` debe limitarse al día vigente y borrarse al cambiar de `DayWindow`; el `path_fingerprint` es FNV-1a de 64 bits sobre la ruta canónica y nunca se persiste la ruta en claro
  - architecture: `OpenCodeIndexV1` se serializa con `serde_json` y se guarda con `cache::atomic_save`; el round-trip JSON → struct → JSON debe ser estable (cualquier campo extra se rechaza)
  - avoid: persistir `data` de `message`/`part`, IDs de mensaje, rutas en claro o nombres de proyecto; usar `version=2` antes de tener un caso real que lo pida
  - verify: `cargo test opencode_tokens::tests::index_round_trip` — serializar una instancia y deserializar produce struct idéntica; un JSON con campo extra `prompt` se rechaza; el JSON producido no contiene las cadenas prohibidas

- [x] 3.2 Implementar `cursor_probe(conn) -> CursorProbe { max_rowid, watermark_part_id }` y `query_suffix(conn, after_rowid, snapshot_max, day_window) -> Result<Vec<ProjectedRow>, OpenCodeError>` con las dos constantes `SQL_CURSOR_PROBE` y `SQL_SUFFIX`
  - skills: `ein-discipline`, `performance`, `architecture`
  - why: R6 obliga a un cursor probe antes de cualquier consulta de filas; R10 requiere poder contar invocaciones de las constantes SQL para tests deterministas
  - learn: la probe y la suffix comparten la misma transacción diferida; el probe se ejecuta SIEMPRE, la suffix solo si `max_rowid > watermark`. Si una fila se confirma tras tomar `snapshot_max_rowid`, no entra en este snapshot y entra en el siguiente — el conteo es exacto
  - architecture: `SQL_CURSOR_PROBE` y `SQL_SUFFIX` son `&'static str` a nivel de módulo, expuestas a tests para envolverlas en un contador atómico solo en `cfg(test)`
  - avoid: reescribir la query en cada llamada (rompe el conteo determinista) o saltarse la probe en bootstrap (no se puede saber `max_rowid` de forma barata)
  - verify: `cargo test opencode_tokens::tests::probe_*` — en bootstrap cuenta exactamente 1 probe + 1 suffix (rebuild); en unchanged cuenta 1 probe + 0 suffix; en append de N rowids cuenta 1 probe + 1 suffix con N candidatos visitados

- [x] 3.3 Implementar `load_or_rebuild_index(path, identity) -> OpenCodeIndexV1` con la matriz completa de invalidación de R7
  - skills: `ein-discipline`, `work-unit-commits`, `architecture`
  - why: R7 obliga a invalidar ante cambio de `format`/`version`, identidad de archivo, `schema_version`/`user_version`/`schema_fingerprint`, ausencia o cambio del ID ancla, o `MAX(rowid) < watermark`; un JSON corrupto o un schema DB incompatible debe reconstruirse o devolverse `SchemaIncompatible`
  - learn: cambios normales de WAL (mtime, page_count) NO invalidan por sí solos — se usan como diagnóstico, no como motivo de rebuild. `VACUUM` que cambia file ID/schema/ancla/max fuerza rebuild inmediato; uno que conserva esas señales queda a la reconciliación de 24 h
  - architecture: la función primero lee el JSON con `serde_json`; si falla parse o falta campo crítico, devuelve un `OpenCodeIndexV1::empty()` con el `DayWindow` actual; las comparaciones de identidad usan `metadata()` y `PRAGMA schema_version`, no `mtime`
  - avoid: invalidar por mtime/size (rompe incrementalidad legítima); aceptar JSON parcialmente válido (mezcla totales viejos con cursor nuevo)
  - verify: `cargo test opencode_tokens::tests::invalidation_*` — schema_version DB cambia → rebuild; user_version cambia → rebuild; platform_file_id cambia → rebuild; `MAX(rowid) < watermark` → rebuild; mtime cambia sin más → no rebuild; JSON con campo faltante → bootstrap desde cero

- [x] 3.4 Implementar `collect_opencode_daily(path, cache_path, clock) -> OpenCodePanelState` que orquesta probe + suffix/rebuild + persistencia atómica + reconciliación 24h
  - skills: `ein-discipline`, `performance`, `work-unit-commits`, `architecture`
  - why: R6/R7 convergen aquí: en caché válida hace probe + suffix; en bootstrap/rebuild hace probe + scan hasta `snapshot_max_rowid`; reconcilia si `now >= last_full_rebuild_ms + 24h`; nunca reconcilia en cada tick de 60 s
  - learn: la reconciliación completa se inyecta con un reloj determinista (`clock: impl Fn() -> i64`); un test demuestra un único rebuild en 24 h incluso con N refrescos; `cache::atomic_save` garantiza que un fallo de rename no corrompe el índice previo (test ya existe en `cache::tests`)
  - architecture: la función es el ÚNICO entrypoint público para datos; recibe `path`, `cache_path` y `clock` para ser totalmente determinista en tests; nunca invoca `chrono::Utc::now()` directamente
  - avoid: usar `Instant::now()` o `chrono::Utc::now()` dentro del colector (rompe tests deterministas); persistir antes de validar números (publica parcial inválido); hacer scan completo en cada refresh (prohibido por R7)
  - verify: `cargo test opencode_tokens::tests::collect_*` — bootstrap con R filas → 1 probe + 1 rebuild, R candidatos, índice V1 escrito; unchanged → 1 probe + 0 suffix, 0 filas de `part`; append N → 1 probe + 1 suffix, N candidatos; update/delete raro detectado por cambio de ID ancla o watermark retroactivo → próximo refresh reconstruye; persistencia atómica fallida (inyectada) conserva índice anterior

- [x] 3.5 Implementar `apply_day_rollover(index: &mut OpenCodeIndexV1, new_window: DayWindow)` que vacía `totals` y `seen_part_ids` y conserva watermark seguro
  - skills: `ein-discipline`, `architecture`
  - why: R7 separa dos casos: medianoche abierta (watermark seguro se conserva, totales e IDs se reinician) y medianoche cerrada (sufijo posterior al watermark se procesa con el nuevo `DayWindow`, filas previas se filtran por el rango temporal)
  - learn: el watermark se conserva si y solo si identidad/esquema/ancla siguen válidos; nunca se borra el watermark en un rollover normal — se borra el contenido del día y el sufijo del nuevo día se procesa contra el mismo cursor
  - architecture: la función es pura: recibe índice mutable y nuevo `DayWindow`; produce índice con `totals` vacío, `seen_part_ids` vacío, mismo `watermark_rowid` y `watermark_part_id`
  - avoid: borrar el watermark en rollover (forzaría rebuild diario aunque la DB no haya cambiado) o filtrar por fecha dentro del WHERE antes de la probe (rompe la incrementalidad)
  - verify: `cargo test opencode_tokens::tests::day_rollover_*` — índice de "ayer" con R filas totales + N filas nuevas tras medianoche → totales vacíos + N filas contables; `seen_part_ids` se vacía; `watermark_rowid` se mantiene si identidad válida

## // 004. Panel TUI independiente "OpenCode hoy" y coexistencia con providers/Claude/Pi

- [x] 4.1 Añadir `Update::OpenCodeTokens(OpenCodePanelState)`, campos `App::opencode_tokens` y `App::opencode_scanning`, y `begin_opencode_token_scan()` con guard compare-and-set
  - skills: `ein-discipline`, `architecture`
  - why: R8 obliga a un cuarto canal de update, estado y bandera de scan independientes; la supresión de duplicados sigue el patrón de `begin_pi_token_scan`
  - learn: el guard es `if self.opencode_scanning { return false; } self.opencode_scanning = true; true`; el resultado del worker SIEMPRE limpia el flag (éxito o error recuperable), igual que hace `apply_update` con `tokens_scanning`
  - architecture: el módulo `opencode_tokens` se importa como `crate::opencode_tokens::{self, OpenCodePanelState, OpenCodeUsageRow}`; `tui.rs` no interpreta JSON ni SQLite
  - avoid: condicionar `Status`, Claude o Pi al resultado de OpenCode (rompe R8); usar un solo flag compartido para los tres scans (rompe el patrón de supresión)
  - verify: `cargo test tui::tests::*opencode*` — `begin_opencode_token_scan` retorna `true` la primera vez y `false` mientras el scan está activo; `apply_update(Update::OpenCodeTokens(...))` limpia el flag; un `Update::Status` aplicado mientras `opencode_scanning=true` actualiza `status` sin tocar el flag

- [x] 4.2 Modificar `App::refresh` para lanzar el worker OpenCode sin esperar su respuesta y sin bloquear providers/Claude/Pi
  - skills: `ein-discipline`, `architecture`, `performance`
  - why: R8 exige que un worker lento/fallido no retrase los otros; el patrón ya existe para `pi_tokens::scan_pi_today` — se reusa la forma, no los tipos
  - learn: cada `std::thread::spawn` lleva su propio `tx.clone()`; nunca se comparte `Connection` ni `mpsc::Sender` con estado mutable entre hilos; el `if self.begin_opencode_token_scan() { ... spawn ... }` es la única condición para lanzar el scan
  - architecture: `refresh` llama secuencialmente a `cache::load || providers::collect_all`, luego `begin_token_scan`, luego `begin_pi_token_scan`, luego `begin_opencode_token_scan` — cada uno independiente; si OpenCode está busy, los otros tres completan igual
  - avoid: usar un único `JoinHandle` que espere a los tres (bloquea R8); condicionar el spawn de OpenCode al resultado de providers
  - verify: `cargo test tui::tests::*refresh*` — un test con `opencode_scanning=true` demuestra que `refresh` retorna sin lanzar otro worker OpenCode pero sí lanza/termina provider/Claude/Pi; un test con reloj simulado demuestra que dos `refresh` solapados lanzan UN solo worker OpenCode

- [x] 4.3 Añadir `draw_opencode_tokens` con columnas `provider, modelo, in, out, raz, cache→, cache+, total, coste` y el título "OpenCode hoy"
  - skills: `ein-discipline`, `architecture`, `cognitive-doc-design`
  - why: R8 fija las columnas exactas y el título; la columna `raz` es la única ampliación real respecto a "Pi hoy"; `None` se pinta `—` (igual que `—` no aparece en `Pi hoy` porque Pi siempre tiene totales obligatorios)
  - learn: la altura se calcula como `rows.saturating_add(3)` igual que `pi_section_height`; el bloque usa el mismo `bordered()` con título centrado para mantener legibilidad; los errores se pintan dentro del bloque, nunca como texto rojo fuera de él
  - architecture: `draw_opencode_tokens` se llama desde `App::draw` siempre que haya un panel (incluso `Unavailable`); la sección aparece DESPUÉS de "Pi hoy" y antes del relleno/footer
  - avoid: fusionar filas Pi+OpenCode en una sola tabla (rompe R8), pintar el error fuera del bloque, o usar `fmt_count(0)` para `None` (miente al usuario)
  - verify: `cargo test tui::tests::*opencode_render*` — `Ready(rows)` con métricas mixtas (algunas `Some`, otras `None`) pinta `—` solo donde corresponde; `Empty` pinta "sin uso hoy"; `Unavailable(Missing)` pinta "OpenCode no disponible"; `Stale{rows,reason}` pinta filas + razón; altura correcta

- [x] 4.4 Mapear `OpenCodePanelState` a `OpenCodeUnavailableReason` con normalización de errores sin rutas ni mensajes SQLite crudos
  - skills: `ein-discipline`, `architecture`
  - why: R8 fija los 8 motivos (`Missing`, `PermissionDenied`, `Busy`, `EphemeralDatabase`, `SchemaIncompatible`, `InvalidUsage`, `CacheWriteFailed`, `ReadFailed`) y prohíbe filtrar rutas o el `Display` de `rusqlite::Error` a la TUI
  - learn: la conversión ocurre en `opencode_tokens.rs` (no en `tui.rs`) — `tui.rs` solo recibe el `OpenCodePanelState` ya normalizado; cada motivo tiene una cadena fija en español, decidida por `opencode_tokens`, no por `tui`
  - architecture: `OpenCodePanelState` y `OpenCodeUnavailableReason` se definen en `opencode_tokens.rs` y se reexportan para TUI; ningún `rusqlite::Error` cruza el límite del módulo
  - avoid: usar `format!("{e}")` sobre un `rusqlite::Error` (fuga paths/mensajes); tener un `OpenCodePanelState::Error(String)` con texto libre
  - verify: `cargo test opencode_tokens::tests::*unavailable_*` — un test con cada motivo comprueba que la razón devuelta al panel es una de las 8 variantes exactas y nunca contiene `/`, `rusqlite`, `sqlite`, `pragmas` o un path absoluto

## // 005. Regresión, performance determinista, compatibilidad de salidas y quality gates

- [x] 5.1 Tests de plan de consulta con `EXPLAIN QUERY PLAN` solo contra fixture, demostrando búsqueda por rowid y PK de message en suffix
  - skills: `ein-discipline`, `performance`, `architecture`
  - why: R3/R10 obliga a que el sufijo use `SEARCH p USING INTEGER PRIMARY KEY (rowid>?)` y lookup por PK de `message.id`; un `SCAN part` en steady state es violación del contrato
  - learn: el plan se captura con `PRAGMA case_sensitive_like=ON; EXPLAIN QUERY PLAN <SQL>;` y se parsea buscando substrings deterministas (no regex frágil); se compara con fixture de ~100 filas para que el optimizador elija el plan correcto
  - architecture: el test vive en `opencode_tokens::tests::query_plan_*`; usa `EXPLAIN QUERY PLAN` parametrizado con los mismos placeholders `?1..?5` que el código de producción
  - avoid: hacer `EXPLAIN` contra la DB real (prohibido por R9); assertar sobre el plan con regex complejos que rompen ante cambios de versión de SQLite
  - verify: `cargo test opencode_tokens::tests::query_plan_*` — suffix con 100 filas en fixture muestra `SEARCH p USING INTEGER PRIMARY KEY` y `SEARCH m USING INTEGER PRIMARY KEY`; bootstrap/rebuild muestra `SCAN p` (aceptable solo allí); nunca `SCAN p` en suffix

- [x] 5.2 Tests de no-fuga y no-acceso a auth, contenido y DB real
  - skills: `ein-discipline`, `architecture`
  - why: R9 obliga a que las pruebas no abran `~/.local/share/opencode/opencode.db` ni lean prompts/credenciales; la invariante debe ser demostrable, no implícita
  - learn: la prueba monkeypatchea `std::env::var` solo a través del seam `EnvSnapshot`; un test estático recorre `tests::` y asserta que ningún `test_*` abre un path que contenga `opencode.db`; `cargo test` se ejecuta con `HOME=/tmp/empty` y `XDG_DATA_HOME=/tmp/empty` para garantizar que un eventual `std::env::var` caería en `Missing`
  - architecture: el test se llama `tests_never_open_real_opencode_db` y vive en un archivo separado `tests/no_real_db.rs` para que `cargo test` lo recolecte pero no se cuele en el módulo principal
  - avoid: depender del entorno del desarrollador (rompe CI); usar `assert!(true)` en lugar de una búsqueda real
  - verify: `cargo test --test no_real_db` — bajo `HOME=/tmp/empty XDG_DATA_HOME=/tmp/empty`, ningún test abre un archivo cuyo path contenga `opencode.db`; los datos sintéticos con `SECRET-PROMPT` no aparecen en stdout/stderr de ningún test

- [x] 5.3 Regresiones byte-estables para `output::pretty`, `output::waybar`, `cache::pi_daily_index_file` y panel Pi
  - skills: `ein-discipline`, `architecture`, `cognitive-doc-design`
  - why: R9/R1 prohíbe que OpenCode altere JSON, Waybar o la caché Pi; las pruebas byte-estables ya existen en `output::tests` y `pi_tokens::tests` — basta con que sigan pasando y con un test nuevo que demuestre que añadir `OpenCode hoy` no las toca
  - learn: el test cross-module construye un `Status` con un provider, llama a `output::pretty` y `output::waybar`, y compara contra los strings literales del test existente; ningún `OpenCodePanelState` se serializa a `providers::Status` ni a JSON
  - architecture: `output.rs` no importa `opencode_tokens`; el test vive en `tui::tests::opencode_does_not_leak_into_json_or_waybar` y verifica que la salida de `pretty`/`waybar` es idéntica con y sin panel OpenCode
  - avoid: añadir un campo `opencode: Option<...>` a `providers::Status` (rompe el contrato), exportar un `OpenCodeTotal` por JSON
  - verify: `cargo test output::tests tui::tests::*json_or_waybar* pi_tokens::tests` — todas pasan idénticas antes y después de la PR; comparación byte-a-byte contra strings literales

- [x] 5.4 Quality gates documentados y evidencia de tamaño de binario/dependencia
  - skills: `ein-discipline`, `performance`, `work-unit-commits`
  - why: la SDD exige dejar evidencia de que `rusqlite` bundled no rompe el binario pequeño de lazysubs; el diseño R2 ya acepta mayor compilación/tamaño a cambio de reproducibilidad
  - learn: `cargo build --release` produce un binario `target/release/lazysubs` cuyo tamaño se documenta en `verify-report.md` (no en este artefacto); `cargo tree | grep rusqlite` confirma que solo `rusqlite` y sus deps inmediatas entran, no un ORM
  - architecture: este grupo es documentation/tests, no código nuevo; se documenta el comando y la evidencia esperada
  - avoid: medir rendimiento con wall-clock microbenchmarks (R10 lo prohíbe); usar `cargo bloat` sin baseline
  - verify: `cargo test` completo pasa; `cargo tree --depth 1 | grep -E "rusqlite|chrono|ratatui|serde|ureq|anyhow"` muestra solo esas seis dependencias más sus transitivas (dejado a `sdd-verify`); tamaño del binario release registrado en `verify-report.md`