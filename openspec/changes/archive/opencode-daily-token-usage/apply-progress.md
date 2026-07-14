status: complete

# Apply progress — OpenCode daily token usage

## Continuation summary

Se continuó exclusivamente sobre `HEAD` actual (`4c94f8b`). El commit dangling `1dc6e52` se dejó intacto, conforme a la decisión ya resuelta. La comprobación inicial de `cargo test` encontró 43 pruebas verdes, no el RED que describía el encargo; se preservó el trabajo parcial y se añadieron REDs reales para los huecos detectados antes de corregirlos.

## Completed tasks

- Las 21 tareas de `tasks.md` están marcadas completas.
- Se completó el collector SQLite de solo lectura con URI `mode=ro`, `query_only`, timeout de 100 ms, transacción diferida y esquema mínimo validado.
- El índice V1 conserva sólo identidad de DB, cursor, IDs opacos diarios y totales por provider/model; no persiste rutas en claro ni JSON de OpenCode.
- Se completó la trayectoria incremental (probe de cursor, sufijo por `rowid`, reconstrucción/reconciliación y rollover local), además de la tabla TUI aislada y su guard de scan.
- Un fallo posterior con filas OpenCode previas conserva las filas como `Stale`; no afecta a providers, Claude ni Pi.
- Se añadieron fixtures SQLite sintéticas, cobertura de límites diarios, proyección privada, plan de query, esquema, caché, append incremental y render/estado TUI.

## Files changed

- `Cargo.toml`, `Cargo.lock` — `rusqlite` bundled ya presente en la continuación.
- `src/main.rs` — registro del módulo OpenCode ya presente.
- `src/cache.rs` — ruta independiente del índice V1 ya presente.
- `src/opencode_tokens.rs` — collector, caché V1, validación, fixtures y pruebas.
- `src/tui.rs` — panel OpenCode independiente, supresión de scans y conservación stale.
- `openspec/changes/opencode-daily-token-usage/tasks.md` — checklist completado.
- `openspec/changes/opencode-daily-token-usage/apply-progress.md` — esta evidencia acumulada.

## TDD Cycle Evidence

| Slice | RED | GREEN | Triangulation / refactor |
|---|---|---|---|
| Continuación | `cargo test` inicial: 43 verdes; no existía el RED anunciado en el encargo. | Se preservó el código parcial correcto. | Se registró la discrepancia en vez de fabricar un fallo. |
| Estado recuperable TUI | `cargo test tui::tests::opencode_failure_keeps_previous_rows_as_stale` falló: `Unavailable(Busy)` reemplazaba las filas. | `apply_update` transforma un error nuevo en `Stale` si había filas válidas. | Las pruebas de guard y de independencia OpenCode pasan. |
| Forma privada del índice V1 | `cargo test opencode_tokens::tests::index_persists_grouped_totals_without_a_plaintext_path` falló al no existir `totals`. | El índice persiste un mapa `totals`; incorpora identidad/huella de esquema y page size sin ruta en claro. | Round-trip, rechazo de campos extra y ausencia de path se verifican. |
| Rollover diario | El test nuevo no compilaba por faltar `apply_day_rollover`. | La función limpia sólo `totals` e IDs vistos y conserva cursor/ancla. | El collector usa la función y la prueba cubre el cambio de día. |
| Esquema compatible | `cargo test opencode_tokens::tests::schema_validation_rejects_missing_primary_keys` falló: aceptaba tablas sin PK. | La validación exige PK de `id` y las columnas mínimas. | Fixture mínima, proyección diaria y `EXPLAIN QUERY PLAN` pasan. |

## Commands run

- `cargo test` — baseline de continuación: 43 passed.
- `cargo test tui::tests::opencode_failure_keeps_previous_rows_as_stale` — RED y luego GREEN.
- `cargo test opencode_tokens::tests::index_persists_grouped_totals_without_a_plaintext_path` — RED y luego GREEN.
- `cargo test opencode_tokens::tests::day_rollover_keeps_cursor_but_discards_daily_totals_and_seen_ids` — RED de compilación y luego GREEN.
- `cargo test opencode_tokens::tests::schema_validation_rejects_missing_primary_keys` — RED y luego GREEN.
- `cargo fmt` — aplicado tras los ciclos.
- `cargo test opencode_tokens::tests::` y `cargo test tui::tests::opencode_` — pasaron después del refactor.
- `git diff --check` — pasó.

## Deviations

- La tarea 1.3 pedía `tempfile::TempDir`; se usó un directorio temporal aislado con `std` para no introducir una dependencia de desarrollo no aprobada.
- La tarea 5.4 menciona un build release. No se ejecutó: la política de APPLY prohíbe builds de producción y el diseño ya lo reserva para verify. La verificación de APPLY es `cargo test`.
- El nombre interno del entrypoint de colección se mantuvo como `collect_at(path, cache_path, now)` para inyectar el reloj determinista sin ampliar la API pública; cubre la orquestación requerida por 3.4.

## Remaining tasks

Ninguna tarea de APPLY. `sdd-verify` debe ejecutar su validación holística y, si procede, medir build/tamaño fuera de APPLY.
