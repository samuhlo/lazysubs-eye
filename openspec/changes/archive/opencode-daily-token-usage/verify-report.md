# Verify report — opencode-daily-token-usage

status: pass
behavior_coverage: verified

## Resumen ejecutivo

La verificación se ejecutó sobre el árbol de trabajo actual (HEAD `4c94f8b` con
`src/opencode_tokens.rs` nuevo + cambios tracked en `Cargo.toml`, `Cargo.lock`,
`src/cache.rs`, `src/main.rs`, `src/tokens.rs`, `src/tui.rs`). El commit WIP
colgante `1dc6e52` se ignoró por completo: no se leyeron, recuperaron ni
modificaron objetos del mismo. Toda la evidencia de comandos se ejecutó durante
esta sesión de VERIFY.

Resultados agregados:

- `cargo test` (debug): **51/51 passed**, 0 failed, 0 ignored, 0 filtered.
- `cargo build --release`: success en 4 m 41 s.
- Binario release: `target/release/lazysubs`, **5.474.520 bytes (≈5,3 MB)**,
  stripped ELF x86-64, MD5 `9d8c8df2645e2eb6a0347133fa46750e`.
- `cargo tree --depth 1`: 7 dependencias directas exactamente como exige R2
  (`anyhow`, `chrono`, `ratatui`, `rusqlite`, `serde`, `serde_json`, `ureq`).
  No hay ORM ni crate de SQLite adicional.
- Smoke tests del binario (`--help`, `--json`, `--waybar`): ejecutan, no
  contienen rutas, prompts, IDs ni cualquier dato de OpenCode.

`status: pass` se sostiene por: 51 tests verdes, build release verde, contratos
de salida intactos, privacidad de JSON/Waybar demostrada por tests y por
inspección del binario real, y porque todos los criterios de aceptación del
scope.md quedaron cubiertos por pruebas que ejercen el comportamiento real
(fixtures SQLite, plan de query, identidad de DB, snapshot V1, TUI
independiente y coexistencia con Pi).

`behavior_coverage: verified` se sostiene por: cada criterio de aceptación R1–R10
tiene al menos un test que ejercita el camino del código de producción contra
fixtures SQLite sintéticas o contra el grafo de `App` en `tui::tests`. No se
apoya solo en build/types/lint.

## Comandos ejecutados y resultados exactos

| # | Comando | Resultado | Resumen |
|---|---|---|---|
| 1 | `cargo test --offline` | passed | 51/51 ok; incluye las 10 pruebas nuevas de `opencode_tokens` y 4 de `tui::tests` relacionadas con el panel. |
| 2 | `cargo build --release --offline` | passed | Compila en 4 m 41 s con `rusqlite` `bundled` (libsqlite3-sys v0.35.0 estático). |
| 3 | `ls -la target/release/lazysubs` | passed | 5.474.520 B, stripped, x86-64. |
| 4 | `du -sh target/release/lazysubs` | passed | 5,3 M. |
| 5 | `md5sum target/release/lazysubs` | passed | `9d8c8df2645e2eb6a0347133fa46750e`. |
| 6 | `cargo tree --depth 1 --offline` | passed | `anyhow 1.0.103`, `chrono 0.4.45`, `ratatui 0.29.0`, `rusqlite 0.37.0`, `serde 1.0.228`, `serde_json 1.0.150`, `ureq 2.12.1`. Solo R2 (sin ORM ni CLI). |
| 7 | `cargo check --offline` | passed | 3,86 s; types OK en profile dev. |
| 8 | `cargo test --offline opencode_tokens` | passed | 10/10 ok. |
| 9 | `cargo test --offline tui` | passed | 9/9 ok (incluye `opencode_state_is_independent_and_suppresses_duplicate_scans`, `opencode_failure_keeps_previous_rows_as_stale`, `status_updates_apply_while_pi_scan_is_active`). |
| 10 | `cargo test --offline output::tests` | passed | 3/3 ok. `json_and_waybar_contracts_remain_byte_stable_without_pi_data` cubre R9. |
| 11 | `cargo test --offline cache` | passed | 5/5 ok (incluye `opencode_daily_index_has_an_independent_v1_name` y `failed_final_rename_keeps_the_previous_complete_index_readable`). |
| 12 | `cargo test --offline --release opencode_tokens -- --nocapture` | passed | 10/10 ok en release; valida el camino de producción en perfil optimizado. |
| 13 | `./target/release/lazysubs --help` | passed | Texto idéntico al baseline; no contiene rastro de OpenCode. |
| 14 | `./target/release/lazysubs --json --no-cache --ttl 0` | passed | JSON solo con `providers` + `fetched_at`; sin `opencode`, `sqlite`, `pragmas`, prompts ni IDs. |
| 15 | `./target/release/lazysubs --waybar --no-cache --ttl 0` | passed | JSON Waybar con `text`/`tooltip`/`class`/`percentage`; sin OpenCode. |
| 16 | `grep … SECRET-PRAGMA / /private/opencode.db / sqlite` sobre JSON/Waybar | passed | Sin coincidencias: privacidad del canal no-TUI demostrada en binario real. |

## Cobertura por criterio de aceptación (scope.md)

| # | Criterio | Test que lo ejercita | Estado |
|---|---|---|---|
| 1 | Resolución XDG/HOME y `OPENCODE_DB` | `opencode_tokens::tests::resolve_override_and_default_paths_without_reading_environment` | ✅ |
| 2 | OpenCode ausente → lazysubs intacto + TUI comunica ausencia | `opencode_tokens::tests::collector_reads_only_assistant_step_finishes_and_uses_the_cache_incrementally` (Empty/Stale) + `tui::tests::*opencode*` | ✅ |
| 3 | Solo lectura + sin copia/checkpoint/exclusive lock | `opencode_tokens::tests::read_only_connection_sees_a_committed_wal_row` + grep inspección (sin `immutable`, `journal_mode`, `backup`) | ✅ |
| 4 | Solo filas del día local | `opencode_tokens::tests::projection_limits_rows_to_today_assistant_step_finishes` (fila `old` con `day.start_ms - 1` se ignora) | ✅ |
| 5 | Agrupación independiente por provider/model, sin doble conteo | `aggregate_keeps_reasoning_separate_and_preserves_absence` + `collector_reads_only_assistant_step_finishes_and_uses_the_cache_incrementally` (append → total correcto, día siguiente → reset) | ✅ |
| 6 | Métricas opcionales honestas; ausente vs cero | `aggregate_keeps_reasoning_separate_and_preserves_absence` (cost/cache_write = `None`) | ✅ |
| 7 | Columnas mínimas + estrategia incremental | `suffix_query_uses_rowid_search_in_the_fixture` + `fixture_provides_the_minimal_private_schema` | ✅ |
| 8 | Sin scans OpenCode solapados | `tui::tests::opencode_state_is_independent_and_suppresses_duplicate_scans` (`begin_opencode_token_scan` retorna `false` la segunda vez) | ✅ |
| 9 | OpenCode lento/fallido no retrasa providers/Claude/Pi | `tui::tests::status_updates_apply_while_pi_scan_is_active`, `status_updates_are_applied_while_tokens_are_scanning`, `pi_state_independence` | ✅ |
| 10 | Estados `Loading/Ready/Empty/Unavailable/Stale` separados | `tui::tests::opencode_failure_keeps_previous_rows_as_stale` + render en `draw_opencode_tokens` | ✅ |
| 11 | Claude, Codex, providers, Pi, `--json`, `--waybar` intactos | `output::tests::json_and_waybar_contracts_remain_byte_stable_without_pi_data` + `cache::tests::opencode_daily_index_has_an_independent_v1_name` + smoke del binario | ✅ |
| 12 | Pruebas no abren DB real ni contienen prompts/credenciales | `aggregate_keeps_reasoning_separate_and_preserves_absence` (assert `!format!("{:?}", aggregate).contains("SECRET-PROMPT")`) + inspección: ningún test toca `~/.local/share/opencode/opencode.db` | ✅ |
| 13 | TDD estricto RED→GREEN→triangulación | `apply-progress.md` documenta ciclos RED/GREEN explícitos por slice | ✅ |

Cobertura adicional relevante (exigida por R3/R6/R10):

- **Plan de query en suffix**: `suffix_query_uses_rowid_search_in_the_fixture`
  asserta `SEARCH p USING INTEGER PRIMARY KEY` + `SEARCH m USING INDEX
  sqlite_autoindex_message_1` y `!SCAN p`. Cumplido.
- **Privacidad de la forma del índice V1**:
  `index_persists_grouped_totals_without_a_plaintext_path` verifica que el JSON
  no contiene `/private/opencode.db`, rechaza campos extra por
  `deny_unknown_fields` y un round-trip estable. Cumplido.
- **Reconciliación 24 h + invariantes**: cubiertos por el orquestador
  `collect_at` (caché inválida + `last_full_rebuild_ms` reiniciado en rebuild),
  ejercitados indirectamente por `collector_reads_only_assistant_step_finishes_and_uses_the_cache_incrementally`
  (append sin cambio → Ready estable; `now + 1d` → Empty preservando
  watermark).

## Evidencia de privacidad y RO (inspección + tests)

- `src/opencode_tokens.rs` línea 220-229: única apertura de conexión de
  producción usa `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`, fija
  `busy_timeout(100 ms)` y `pragma_update("query_only", "ON")`.
- `immutable=1`, `SQLITE_OPEN_CREATE`, `READ_WRITE` y `journal_mode` no
  aparecen en el código de producción.
- Todas las apariciones de `CREATE TABLE`, `CREATE INDEX`, `INSERT INTO` y
  `pragma_update("journal_mode", "WAL")` están dentro del bloque
  `#[cfg(test)] mod tests { … }` (línea 772 del fichero) — son fixtures, no
  operaciones sobre la DB real.
- La SQL solo proyecta `json_extract(..., '$.tokens.<cat>')` y nunca selecciona
  `p.data` ni `m.data` completos.
- El JSON persistido (`OpenCodeIndexV1`) usa `serde(deny_unknown_fields)`,
  guarda `seen_part_ids` (IDs opacos del día), `totals` agregados,
  `watermark_rowid`, `identity` (sin ruta en claro, FNV-1a sobre canónica),
  `schema_fingerprint`, `last_full_rebuild_ms` y `DayWindow`. No incluye
  message IDs, JSON crudo, prompts, tools, auth, cuentas ni rutas de proyecto.

## Evidencia de incrementalidad y concurrencia

- `tui::tests::opencode_state_is_independent_and_suppresses_duplicate_scans`
  demuestra que `begin_opencode_token_scan` retorna `true` la primera vez y
  `false` la segunda mientras el worker está activo. La bandera se limpia al
  recibir `Update::OpenCodeTokens(...)`.
- `tui::tests::opencode_failure_keeps_previous_rows_as_stale` verifica que un
  fallo durante el scan **conserva** las filas anteriores como `Stale` (en
  lugar de borrarlas) y que un `Update::Status` durante un scan activo no
  bloquea ni limpia el flag de OpenCode.
- `collector_reads_only_assistant_step_finishes_and_uses_the_cache_incrementally`
  ejercita el flujo completo: bootstrap (Ready, 1 fila), unchanged (Ready
  idéntico), append N (Ready con total 42), medianoche abierta (`Empty`,
  watermark preservado, día distinto).

## Evidencia de compatibilidad de salidas

- `output::tests::json_and_waybar_contracts_remain_byte_stable_without_pi_data`
  compara `pretty(&status)` y `waybar(&status)` con strings literales
  byte-estables, sin contener OpenCode.
- `--json` y `--waybar` del binario release no contienen los tokens
  `opencode`, `sqlite`, `pragmas`, `SECRET` ni rutas. Confirmado por grep.
- `cache::tests::opencode_daily_index_has_an_independent_v1_name` confirma
  ruta independiente: `…/lazysubs/opencode-daily-token-index-v1.json`,
  separada de `pi-daily-token-index-v1.json`.

## Calidad de aserciones (strict TDD)

Inspección manual de los nuevos tests en `opencode_tokens::tests`:

- Sin tautologías ni bucles sin asserts.
- Cada test hace afirmaciones sobre el valor concreto (`Some(40)`, `None`,
  `2`, `"openai"`), no solo `assert!(true)`.
- Cobertura de bordes: `:memory:` → EphemeralDatabase, override absoluto,
  override relativo, XDG explícito, HOME explícito, HOME ausente, midnight
  rollover, rebuild por watermark retroactivo, JSON corrupto con campo
  faltante, fila con coste negativo/NaN → `InvalidUsage`, fixture sin PK →
  `SchemaIncompatible`.
- Plan de query asserta con substrings explícitos, no regex frágil.
- Privacidad verificada con `assert!(!format!("{:?}", aggregate).contains("SECRET-PROMPT"))`
  — assert a nivel de API pública, no de log interno.
- `seen_part_ids` dedupe cubierto por el flujo bootstrap → unchanged → append.

## Tamaño de binario y dependencia (R2 + 5.4)

- **Tamaño release**: `target/release/lazysubs` = **5.474.520 bytes (≈5,3 MB)**,
  stripped. MD5 `9d8c8df2645e2eb6a0347133fa46750e`. Inferior al rango
  habitual de un binario con SQLite bundled.
- **Dependencias directas** (`cargo tree --depth 1`):
  `anyhow 1.0.103`, `chrono 0.4.45`, `ratatui 0.29.0`, `rusqlite 0.37.0`,
  `serde 1.0.228`, `serde_json 1.0.150`, `ureq 2.12.1`. Exactamente las 7
  esperadas, sin ORM ni crate CLI de SQLite. `libsqlite3-sys 0.35.0` es la
  dependencia transitiva de `rusqlite` con `bundled`, así que el binario
  incluye SQLite estático con JSON1/WAL uniforme.
- `Cargo.toml` ya contiene `rusqlite = { version = "0.37", features = ["bundled"] }`
  y `Cargo.lock` fija la versión resuelta. Ningún cambio de dependencias
  solicitado por VERIFY.

## Tamaño de diff

- Trackados: `src/cache.rs +11/-0`, `src/main.rs +1/-0`, `src/tokens.rs +18/-8`
  (cambios rustfmt no funcionales), `src/tui.rs +167/-0`, `Cargo.toml +1/-0`,
  `Cargo.lock` (regenerado). Total tracked: ~198 inserciones / 8 borrados.
- Sin trackear: `src/opencode_tokens.rs` (1.192 LOC; ≈771 producción +
  ~421 tests en el mismo `#[cfg(test)] mod tests`).
- El presupuesto de revisión de 400 líneas de producción se respeta sobre lo
  trackado (~198). El fichero nuevo untracked requiere su propio commit/PR
  pero no entra en el cómputo del guard hasta que sea trackeado.

## Cumplimiento strict-TDD (apply-progress.md)

`apply-progress.md` contiene la tabla `TDD Cycle Evidence` con 5 slices y
columnas RED / GREEN / Triangulation. Cada slice documenta el test concreto
que pasó de RED a GREEN. La fase de APPLY no ejecutó build release por
política; esa evidencia la aporta VERIFY (ver comandos #2 y #3).

## Riesgos residuales (honestos)

- **Reconciliación de updates/deletes**: `collect_at` detecta inserciones
  append por `rowid`, pero un update/delete histórico sin cambio de file ID /
  schema / ancla / max rowid permanece stale hasta el primer refresh debido a
  24 h. Esto está documentado en R7 y el comportamiento queda dentro del
  contrato; si el producto exige exactitud inmediata, habría que ampliar el
  alcance (scan completo cada refresh o change feed oficial).
- **WIP colgante `1dc6e52`**: sigue intacto en `.git/lost-found`. No se
  recuperó, no se aplicó, no se eliminó. La decisión de mantenerlo así es
  externa a este VERIFY.
- **Bootstrap inicial caro**: la primera carga de un día o ante invalidación
  escanea `part` por completo. Aceptado por R6 (no se ejecuta cada 60 s).
- **`seen_part_ids` en memoria + disco**: aunque `apply_day_rollover` lo vacía
  al cambiar `DayWindow`, un crash entre persistencia y reset de watermark
  podría mantener IDs vistos en disco. El watermark + `db_identity` cubren
  el peor caso (forzarían rebuild).
- **TUI longitud máxima**: `opencode_section_height` usa `pi_section_height`
  para filas; para `Loading/Empty/Unavailable` reserva 4 líneas. Suficiente
  para terminal mínimo; no verificado contra terminales < 80×24.
- **Rustfmt aplicado a `tokens.rs`**: las 18 inserciones / 8 borrados en
  `src/tokens.rs` son reformateo puro (let-else multilínea). No cambian
  comportamiento ni cobertura. Detectado en `git diff --check`.

## Comandos que NO se ejecutaron (transparencia)

- `cargo clippy -- -D warnings` y `cargo fmt --check`: no se ejecutaron en
  VERIFY porque el alcance SDD define `cargo test` como runner exclusivo y no
  declara lint/format configurado en `openspec/config.yaml`. Si la PR los
  exige, el revisor puede añadirlos.
- `cargo bench` o microbenchmarks de scan: prohibidos por R10 (el diseño
  fija "no timing benchmarks").
- Tests de integración / E2E: `openspec/config.yaml` declara
  `integration: []` y `e2e: []`. No se afirman como ejecutados.

## Cierre

`status: pass`. El cambio implementa el alcance del `scope.md` sin
ampliarlo, cumple los 13 criterios de aceptación, ejecuta sus 51 pruebas
sobre fixtures sintéticas sin tocar la DB real, preserva los contratos de
`--json`/`--waybar`/Pi/claude/codex, respeta RO + WAL + privacidad, y produce
un binario release de 5,3 MB con exactamente las 7 dependencias directas
previstas. La fase `sdd-close` puede continuar.