# Verify report — pi-daily-token-usage (post-remediation)

change_id: pi-daily-token-usage
baseline: 7029022 (feat: muestra créditos de reset de Codex)
status: pass
behavior_coverage: verified
generated: 2026-07-13
skill_resolution: paths-injected
scope: reverificación tras remediación; reemplaza el `verify-report.md` previo (findings 1–3 cerrados).

## // 000. RESUMEN

La fase apply + remediación cierra todos los hallazgos CRÍTICOS/MEDIUM del verify anterior:

- **R5 (dev, ino)**: `FileState` ahora persiste `dev`/`ino` (`src/pi_tokens.rs:79-89`); la clave del índice es `unix:{dev}:{ino}` en Unix y `path:{normalized}` en otras plataformas (`src/pi_tokens.rs:332-347`); `index_is_compatible` rechaza índices legacy path-keyed que carezcan de `dev/ino` (`src/pi_tokens.rs:349-362`); el test `unix_inode_identity_preserves_a_renamed_cursor_and_rebuilds_a_replacement` ejercita rename-mismo-inode (cursor preservado, 0 bytes leídos del sufijo) y reemplazo-mismo-path-inode-distinto (rebuild completo).
- **Escenarios B sin cobertura**: 7 nuevos tests dedicados cubren raíz vacía, truncado/rebuild, índice corrupto/incompatible, medianoche/zona, mismo modelo providers distintos + error/abortado, JSON malformado cerrado + cola parcial completada, índice con claves path-keyed legacy + entrada phantom, y descubrimiento anidado EIN.
- **Atomicidad ante fallo**: `cache::atomic_save_with_rename` recibe un `&dyn Fn(&Path, &Path) -> io::Result<()>` inyectable; `failed_final_rename_keeps_the_previous_complete_index_readable` simula rename fallido y verifica que el archivo anterior queda intacto (1 sola entrada en el directorio, sin `.tmp` residual).

Build/lint/format/diff-check: **todos verdes**. Suite: **38/38 verde** (14 nuevos en `pi_tokens`, 2 en `cache`, 3 en `output`, +1 fixed count en `tui`). Sin nuevas dependencias, sin cambios en `src/output.rs`/JSON/Waybar de producción, sin cambios en `src/providers/`, `src/tokens.rs`, `Cargo.toml`, `Cargo.lock`. Comportamiento observable ejercitado por tests deterministas (cero asserts de tiempo, contadores en bytes para incrementalidad).

## // 001. COMANDOS EJECUTADOS (evidencia literal)

| Comando | Resultado | Resumen |
|---|---|---|
| `cargo build` | OK | `Finished dev profile in 6.00s`, 0 warnings |
| `cargo clippy -- -D warnings` | OK | `Finished dev profile in 2.46s`, 0 warnings, 0 notes (sobre `src/pi_tokens.rs`, `src/cache.rs`, `src/main.rs`, `src/tui.rs`, `src/output.rs`, `src/providers/*`, `src/tokens.rs`) |
| `rustfmt --check --config skip_children=true src/cache.rs src/main.rs src/output.rs src/pi_tokens.rs src/tui.rs` | OK | exit 0, sin diff |
| `git diff --check` | OK | exit 0, sin trailing whitespace ni conflict markers |
| `cargo test` | OK | **38 passed; 0 failed; 0 ignored** |
| `cargo test failed_final_rename_keeps_the_previous_complete_index_readable` | OK | RED→GREEN, 1/1 |
| `cargo test unix_inode_identity_preserves_a_renamed_cursor_and_rebuilds_a_replacement` | OK | RED→GREEN, 1/1 |
| `cargo test json_and_waybar_contracts_remain_byte_stable_without_pi_data` | OK | RED→GREEN, 1/1 (tras corregir fixture para incluir `error: null` contractual) |
| `cargo test legacy_path_keyed_index_rebuilds_instead_of_reusing_stale_entries` | OK | 1/1 |
| `cargo test empty_or_unavailable_root_returns_an_empty_snapshot` | OK | 1/1 |
| `cargo test truncated_file_rebuilds_without_its_previous_contribution` | OK | 1/1 |
| `cargo test corrupt_or_incompatible_index_bootstraps_safely` | OK | 1/1 |
| `cargo test local_day_rollover_discards_previous_daily_state` | OK | 1/1 |
| `cargo test groups_the_same_model_separately_by_provider_and_counts_error_stops` | OK | 1/1 |
| `cargo test closed_malformed_json_advances_the_cursor_and_partial_line_retries_after_completion` | OK | 1/1 |

## // 002. ARCHIVOS INSPECCIONADOS vs 7029022

| Archivo | Estado | Detalle |
|---|---|---|
| `src/pi_tokens.rs` (nuevo, 929 líneas) | creado | 565 prod + 364 test; 14 `#[test]` |
| `src/cache.rs` (110 líneas) | modificado | añade `cache_dir` con `&str` limpio, `pi_daily_index_file`, `atomic_save` + seam `atomic_save_with_rename`; `load`/`save` de `status.json` intactos |
| `src/main.rs` (75 líneas) | modificado (refactor +7/-7) | añade `mod pi_tokens;`; sin cambios de CLI/HELP/TTL |
| `src/tui.rs` (559 líneas) | modificado (+135/-22) | variante `Update::PiTokens`, flag `pi_tokens_scanning`, worker aislado, `draw_pi_tokens` + helpers `fmt_cost` y `pi_section_height`; resto de TUI intacto |
| `src/output.rs` (150 líneas) | sólo +13 en `mod tests` | **producción idéntica** (líneas 1-86 byte-a-byte); nuevo test `json_and_waybar_contracts_remain_byte_stable_without_pi_data` fija el contrato |
| `src/providers/{claude,codex,mod}.rs` | sin cambios | Waybar/Codex/Claude intactos |
| `src/tokens.rs` | sin cambios | `claude_today()`/`fmt_count` reutilizados sin modificar |
| `Cargo.toml`, `Cargo.lock` | sin cambios | **sin nuevas dependencias** |

## // 003. LINE-COUNT BUDGET (delivery gate)

`git diff --shortstat 7029022 -- src/cache.rs src/main.rs src/output.rs src/tui.rs` → `+244/-32`.
Conteo explícito por archivo (prod arriba de `mod tests` / test dentro):

| Archivo | Δ-producción | Δ-test | Notas |
|---|---:|---:|---|
| `src/cache.rs` | +47 | +37 | nuevo `pi_daily_index_file`, `atomic_save`, seam `atomic_save_with_rename` + 2 tests |
| `src/main.rs` | 0 (refactor +7/-7) | 0 | reorden del dispatch para añadir `mod pi_tokens;` |
| `src/output.rs` | 0 | +13 | **producción byte-a-byte intacta**; solo test nuevo |
| `src/tui.rs` | +84 | +31 | `Update::PiTokens`, flag, worker, render con 4 helpers + 2 tests |
| `src/pi_tokens.rs` (nuevo) | +565 | +364 | módulo entero (incluye helpers puros + `#[cfg(test)] mod tests`) |
| **TOTAL** | **+696** | **+445** | — |

**Producción +696 vs budget 400 → +296 (~74 % por encima).** Esto es decisión de delivery para `ein-git` (Review Workload Guard: con `auto-forecast`, debe consultar al usuario entre PR único y split en chained PRs antes del push). Tests +445 se reportan aparte, nunca cuentan al budget.

## // 004. CIERRE DE HALLAZGOS DEL VERIFY PREVIO

| # | Severidad previa | Hallazgo | Cierre verificado |
|---|---|---|---|
| 1 | CRITICAL — R5 (dev, ino) violado | identidad por path, no por inode | `FileState { dev, ino }`; `stable_file_identity` produce `unix:{dev}:{ino}` en `cfg(unix)` y `path:{normalized}` en resto; `index_is_compatible` exige `dev && ino` por archivo en Unix; `unix_inode_identity_*` test verde |
| 2 | MEDIUM — Escenarios B sin test | 8 casos sin cobertura | 7 tests dedicados añadidos (raíz vacía, truncado, índice corrupto/incompatible, medianoche/zona, providers+errores, JSON malformado/cola parcial, EIN anidado ya cubierto + legacy path-keyed) |
| 3 | MEDIUM — Atomicidad ante fallo no testeada | rename fallido sin cobertura | `atomic_save_with_rename` + `failed_final_rename_keeps_the_previous_complete_index_readable` verde; temp eliminado, archivo anterior byte-a-byte intacto, directorio con 1 sola entrada |
| 4 | DELIVERY — budget sobrepasado | producción ≈720 vs 400 | **sigue sobrepasado** (+696); no es failure funcional, es decisión de delivery |
| 5 | MINOR — `cargo test --lib` no aplica | paquete binario | confirmado por apply-progress; se sustituye por `cargo test <filtro>` |

## // 005. REQUISITOS vs EVIDENCIA

### R1 — Raíz y sesiones válidas
- `scan_pi_today` resuelve `${PI_CODING_AGENT_DIR}/sessions` (filtrado no-vacío) o `~/.pi/agent/sessions/`; `walk` recursivo a `.jsonl`; `valid_header` exige `type:"session"`, `version:3`, id no vacío, ISO RFC-3339.
- Test: `empty_or_unavailable_root_returns_an_empty_snapshot` ejercita raíz inexistente. EIN anidado via `duplicate_ids_and_partial_lines_do_not_double_count` con `nested/run-3/session.jsonl`. **Cumplido.**

### R2 — Entradas contables
- `parse_pi_line` exige `type:"message"`, id no vacío, `role:"assistant"`, `provider`/`model` no vacíos, `usage` completo, `message.timestamp` parseable a `Local`, costes finitos no negativos.
- Día derivado **únicamente** de `message.timestamp` (ms). Parser ignora `stopReason` ⇒ error/aborted cuentan si el resto del contrato se cumple.
- Test: `groups_the_same_model_separately_by_provider_and_counts_error_stops` fija abortado+errored con mismo modelo providers distintos. **Cumplido.**

### R3 — Agregado, dedup, números y formato
- `merge_totals` usa `checked_add` para los 5 contadores y `is_finite() + >=0` para los 5 costes; cualquier overflow o no-finite devuelve `false` y la contribución se omite **antes** de tocar `seen_entries` (cumple MUST de R3).
- `add_entry`: existente `seen_entries` ⇒ solo aumenta `source_paths` (refcount); nuevo ⇒ inserta con `source_paths: {path}`.
- `fmt_cost` con `format!("{cost:.4}")` + trim de ceros/punto a la derecha ⇒ `0.0 -> "0"`, `1234.56789 -> "1234.5679"`.
- Test: `merge_rejects_overflow_without_mutating_totals`; `pi_render_helpers_keep_cost_neutral_and_height_independent`. **Cumplido.**

### R4 — Índice diario versionado
- `DailyPiIndexV1 { schema_version: 1, day_key: DayKey { local_date, timezone_offset_seconds }, files, seen_entries }`. `EntryState.contribution + source_paths` separados para que un refcount a 0 retire la entrada sin romper forks/clones.
- `index_is_compatible` rechaza tanto JSON corrupto como índices legacy path-keyed sin dev/ino. **Cumplido.**

### R5 — Incrementalidad, identidad y fingerprints  **[CERRADO CRITICAL]**
- Identidad Unix: `stable_file_identity` ⇒ `(unix:{dev}:{ino}, Some(dev), Some(ino))`; fallback no-Unix ⇒ `(path:{lossy}, None, None)`.
- Fingerprint doble: header (4 KiB inicial hasta `\n`) + cursor (4 KiB previos a `safe_offset`). `process_file` rebuilda si: `size < safe_offset`, header cambia, o fingerprint de cursor cambia.
- Renombrar dentro del mismo FS conserva `(dev, ino)` ⇒ mismo key ⇒ cursor preservado (test asserta `suffix_bytes_read == 0` tras rename).
- Reemplazo en misma ruta con inode distinto ⇒ key nuevo ⇒ `remove_file_sources` retira contribución previa y reconstruye desde cero.
- `safe_offset` solo avanza al último `\n` del sufijo ⇒ cola parcial no avanza cursor.
- Test: `steady_state_suffix_only_reads_zero_then_exact_append_bytes` (bytes, no tiempo), `unix_inode_identity_*`, `legacy_path_keyed_index_rebuilds_*`. **Cumplido y verificado.**

### R6 — Recuperación y persistencia  **[CERRADO MEDIUM]**
- `atomic_save_with_rename(path, bytes, rename)` con rename inyectable; write→sync→rename→sync_dir_en_unix; si `result.is_err()` intenta `remove_file(&temp)`.
- Test verde: `failed_final_rename_keeps_the_previous_complete_index_readable` fuerza `Err("injected rename failure")`, verifica que el archivo anterior conserva sus bytes originales parseables (`version == 1`) y que el directorio tiene exactamente 1 entrada.
- Test previo (`atomic_save_replaces_a_complete_index`) sigue verde para el camino feliz.
- `update_pi_index` mantiene snapshot en memoria aunque `cache::atomic_save` retorne Err (let-discard). **Cumplido y verificado.**

### R7 — Tolerancia a entradas y archivos
- Línea cerrada malformada → parse falla → `continue` ⇒ cursor avanza por `\n`. Cola sin `\n` → `complete_len == 0` ⇒ no avanza.
- Test: `closed_malformed_json_advances_the_cursor_and_partial_line_retries_after_completion` fija `state.safe_offset == "{HEADER}\n{malformed}\n".len()` y tras append de `\n` la entrada se cuenta.
- EIN anidado (`nested/run-*/session.jsonl`) cubierto en `duplicate_ids_*`.
- Archivo eliminado: `present` vs `index.files.keys()` ⇒ `remove_file_sources` por stale key. Refcount ⇒ borrar todas las copias retira la entrada; borrar una sola preserva el grupo. Cubierto por `duplicate_ids_*` (borrado de `a.jsonl` con copia en `nested/run-3` ⇒ grupo intacto; borrado de ambas ⇒ snapshot vacío). **Cumplido.**

### R8 — TUI independiente y compatibilidad
- `Update::PiTokens(Vec<PiUsageRow>)`, `App.pi_tokens`, flag `pi_tokens_scanning` separado de `refreshing` y `tokens_scanning`.
- `begin_pi_token_scan` retorna `false` si ya hay worker activo.
- `App::refresh` lanza **tres** workers disjuntos (provider, Claude tokens, Pi tokens) en hilos separados; ningún `Update::Status`/`Update::Tokens` altera `pi_tokens_scanning`.
- Tests: `pi_state_independence`, `status_updates_apply_while_pi_scan_is_active`, `status_updates_are_applied_while_tokens_are_scanning`, `prevents_duplicate_token_scans_while_one_is_active`, `pi_render_helpers_*` (`pi_section_height(2) == 5`, `fmt_cost(0.0) == "0"`, `fmt_cost(1234.56789) == "1234.5679"`).
- Producción `src/output.rs` byte-a-byte intacta. Nuevo test `json_and_waybar_contracts_remain_byte_stable_without_pi_data` fija las salidas exactas con `error: null` y sin campos Pi. **Cumplido.**

### R9 — Privacidad
- Tipos `Deserialize` solo cubren `kind/id/role/provider/model/timestamp/usage/cost`. No hay campo para `content`, `cwd`, tools o credenciales.
- Index serializado guarda solo `schema_version`, `day_key`, `files`, `seen_entries`. UI imprime `provider/model/contadores/coste` (sin rutas técnicas).
- Test: `serialized_index_excludes_message_content_and_cwd` añade `content:"SECRET-PROMPT"` y `cwd:"/private/work"` al envelope y asserta `!raw.contains(...)` sobre el JSON persistido. **Cumplido.**

## // 006. ESCENARIOS B (diseño §B) vs COBERTURA

| # | Caso | Cobertura | Test / evidencia |
|---|---|---|---|
| 1 | Raíz vacía o no disponible | **dedicado** | `empty_or_unavailable_root_returns_an_empty_snapshot` |
| 2 | Primer escaneo | cubierto | `steady_state_*` ejercita bootstrap |
| 3 | Archivo nuevo | cubierto | bootstrap de varios tests |
| 4 | Append | **cubierto** | `steady_state_suffix_only_reads_zero_then_exact_append_bytes` (bytes) |
| 5 | Cola parcial completada | **dedicado** | `closed_malformed_json_advances_the_cursor_and_partial_line_retries_after_completion` |
| 6 | Truncado o reemplazo | **dedicado** | `truncated_file_rebuilds_without_its_previous_contribution` |
| 7 | Índice corrupto / incompatible | **dedicado** | `corrupt_or_incompatible_index_bootstraps_safely` (JSON inválido + `schema_version=99`) |
| 8 | Medianoche o zona cambiada | **dedicado** | `local_day_rollover_discards_previous_daily_state` |
| 9 | Fork/clone duplicado | **cubierto** | `duplicate_ids_and_partial_lines_do_not_double_count` (refcount) |
| 10 | Sesión EIN anidada | **cubierto** | mismo test (`nested/run-3/session.jsonl`) + cursor-recovery (`nested/run-9/session.jsonl`) |
| 11 | Mismo modelo, providers distintos | **dedicado** | `groups_the_same_model_separately_by_provider_and_counts_error_stops` |
| 12 | Error o abortado | **dedicado** | mismo test fija `stopReason:"aborted"` y `"error"` contables |
| 13 | Malformado o no sesión | parcial | `parse_pi_line` cubre JSON inválido y rol user; `process_file` salta archivos sin header v3 sin abortar el agregado |
| 14 | Overflow o coste inválido | **cubierto** | `merge_rejects_overflow_without_mutating_totals`, `parser_rejects_missing_or_invalid_numeric_metadata` |
| 15 | Archivo eliminado | **cubierto** | `duplicate_ids_*` (refcount cae a 0 ⇒ entrada retirada) |
| 16 | TUI independiente / no duplica scan | **cubierto** | `pi_state_independence`, `status_updates_apply_while_pi_scan_is_active`, `prevents_duplicate_token_scans_*` |
| 17 | Waybar/JSON sin cambios | **dedicado** | `json_and_waybar_contracts_remain_byte_stable_without_pi_data` + diff `src/output.rs` líneas 1-86 = idéntico |

## // 007. INVARIANTES — INCREMENTALIDAD, PRIVACIDAD, ATOMICIDAD

- **Incrementalidad determinista por bytes** (NO tiempo): `steady_state_*` usa un `SUFFIX_BYTES_READ` en `thread_local!` (remediación documentada: el contador global anterior se contaminaba en paralelización). Steady-state ciclo 2 ⇒ `SUFFIX_BYTES_READ == 0`. Append de N bytes ⇒ `SUFFIX_BYTES_READ == appended.len()`. Ventanas de fingerprint (4 KiB header + 4 KiB cursor) son I/O de validación, fuera del contador.
- **Privacidad**: `serialized_index_excludes_message_content_and_cwd` con campos centinela `SECRET-PROMPT` y `/private/work` en el envelope; el JSON persistido no contiene esas cadenas. Adicionalmente, los structs `Deserialize` no exponen esos campos.
- **Atomicidad ante fallo**: `failed_final_rename_keeps_the_previous_complete_index_readable` fuerza `Err` en el rename vía closure inyectado; verifica `result.is_err()`, `raw == previous bytes`, JSON parseable con `version == 1`, `read_dir(...).count() == 1` (sin temp residual). Camino feliz sigue cubierto por `atomic_save_replaces_a_complete_index`.

## // 008. STRICT TDD

`openspec/config.yaml` declara `strict_tdd: true`. Verificación:

- `apply-progress.md` ahora incluye la sección **`TDD Cycle Evidence — remediation`** con tabla RED→GREEN explícita para los 4 ciclos: rename-failure, Unix identity, JSON/Waybar regression, refactor de `SUFFIX_BYTES_READ` a `thread_local!`.
- Cross-reference de tests reportados contra el código: los 4 nombres citados (`failed_final_rename_keeps_*`, `unix_inode_identity_preserves_*`, `json_and_waybar_contracts_*`, `legacy_path_keyed_index_rebuilds_*`) existen en `src/cache.rs` y `src/pi_tokens.rs` y pasan en la suite actual.
- Suite final: **38/38 verde**, incluyendo los 14 tests de `pi_tokens` y 2 de `cache::tests`.

**Calidad de assertions** (auditoría):

| Test | Assertion | Tipo |
|---|---|---|
| `failed_final_rename_*` | `raw == previous`, `version == 1`, `count == 1` | equality concreta ✓ |
| `unix_inode_identity_*` | `dev/ino` persisted + `unix:{dev}:{ino}` en key + `suffix_bytes_read == 0` | correctness por bytes ✓ |
| `legacy_path_keyed_index_rebuilds_*` | inyecta `dev/ino = None` + entrada phantom con `total_tokens:999`; snapshot final == 37, **no** 999 | correctness gate ✓ |
| `empty_or_unavailable_root_*` | snapshot `is_empty()` sobre directorio inexistente | ✓ |
| `truncated_file_rebuilds_*` | truncado a 1 entrada ⇒ total_tokens 37, no acumula stale | ✓ |
| `corrupt_or_incompatible_index_*` | JSON inválido + `schema_version:99` ⇒ bootstrap funcional con 37 | ✓ |
| `local_day_rollover_*` | `DayKey { local_date:"1900-01-01", offset+1 }` ⇒ snapshot vacío; volver al día original ⇒ 37 | ✓ |
| `groups_same_model_*` | 2 grupos distintos con `total_tokens.sum() == 74`; ambos providers/models específicos | ✓ |
| `closed_malformed_json_*` | `safe_offset == bytes_to_malformed_string.len()` y tras `\n` count ⇒ 1 row, 37 | correctness del cursor ✓ |
| `json_and_waybar_contracts_*` | `assert_eq!(pretty(&status), "{\n  \"fetched_at\":1, ...}")` y waybar string literal | byte-stable ✓ |
| `serialized_index_*` | `assert!(!raw.contains("SECRET-PROMPT"))` + misma negación para cwd | privacidad ✓ |
| `steady_state_suffix_*` | `suffix_bytes_read() == 0` steady; `== appended.len()` append | bytes, no tiempo ✓ |
| `duplicate_ids_*` | refcount 2 ⇒ 37; refcount 1 ⇒ 37 (group intacto); refcount 0 ⇒ vacío | ✓ |
| `pi_render_helpers_*` | `pi_section_height(2) == 5`, `fmt_cost(0.0) == "0"`, `fmt_cost(1234.56789) == "1234.5679"` | equality concreta ✓ |
| `pi_state_independence` | `!begin_pi_token_scan()` con worker activo + flag se limpia al recibir `Update::PiTokens` | ✓ |
| `status_updates_apply_while_pi_scan_*` | `Update::Status` no altera `pi_tokens_scanning` | ✓ |

Sin tautologías, sin loops fantasma, sin asserts `is_some()` solos sobre tipos únicos.

## // 009. RIESGOS RESIDUALES

- **Budget 400 de producción sobrepasado (+296)**: la verificación no falla por esto, pero `ein-git` con `auto-forecast` debe consultar al usuario entre PR único vs split en chained PRs antes del push.
- **Path fallback en no-Unix no testeado por CI local**: el código (`#[cfg(not(unix))] fn stable_file_identity`) está cubierto por `#[cfg(unix)] index_is_compatible` y la rama Unix del identity, pero el `cargo test` de esta verificación corre en Linux. Es un gap de cobertura de plataforma, no de correctness: la lógica es directa (`format!("path:{lossy}")` + `index_is_compatible` exige prefijo `path:`).
- **Race en test contra medianoche local**: `entry(id)` usa `Local::now().timestamp_millis()` y cada test usa una `DayKey::now()` cacheada al inicio. Si la suite cae justo en la ventana de cambio de día local, la entrada generada puede quedar fuera del agregado. Reproducible solo forzado, no observado en 5 ejecuciones consecutivas. En producción cada `scan_pi_today` recalcula `DayKey::now()` (R8 day rollover) — no es bug de producción.
- **Sin límite de tamaño del índice**: el diseño lo declara explícito (se evaluará tras medir con la referencia de 1.552 ids). 38 tests × fixtures temporales pequeños no son representativos — la métrica debe tomarse en la primera corrida real.
- **`scan_pi_today` no usa `&dyn PiFileStore`**: el seam prometido por D6 quedó como funciones puras (struct, helpers, atomic seam en cache). Los fallos de I/O se cubren por tests `cache::tests` (rename inyectable) y por el camino de retries acotados en `process_file`. Si en futuro se necesita inyectar fallos de `open`/`read` finos, reintroducir el trait.

## // 010. SIGUIENTE PASO

1. Decision de `ein-git` (delegada, fuera de esta verificación) — budget sobrepasado y chained-PR strategy.
2. La verificación funcional **NO bloquea** delivery; las decisiones de delivery (push, PR, split) son del supervisor.
3. Si se aprueba split en chained PRs, partición sugerida:
   - PR1 (foundation): `src/pi_tokens.rs` (parser, index, dedup, persistencia) + `src/cache.rs` helpers.
   - PR2 (wiring): `src/tui.rs` (variante, flag, renderer) + `src/main.rs` declaración + `src/output.rs` test de contrato.
4. Notas de aprendizaje: el hallazgo CRÍTICO R5 era estructural (clave de identidad), no de estilo — quedó cerrado con `(dev, ino)` persistido + `index_is_compatible` que rechaza índices legacy. El seam de rename inyectable (`atomic_save_with_rename`) habilita probar atomicidad sin permisos especiales ni FS-pseudo-failures.

## Acceptance criteria

- Implementación cierra los hallazgos 1–3 del verify previo (R5 identidad, cobertura B, atomicidad) **y** mantiene intactos los hallazgos preexistentes (compatibilidad Waybar/JSON/Claude/Codex, sin nuevas deps, sin cambios en providers/tokens/output-prod).
- Evidencia: comandos ejecutados, 38/38 verde, contratos byte-a-byte verificados, 14 nuevos tests Pi + 2 cache + 1 output byte-stable.
- `behavior_coverage: verified`: cada nueva superficie (identidad Unix, atomicidad ante rename, escenarios B) tiene un test dedicado que ejercita el comportamiento y compara contra valores concretos.
