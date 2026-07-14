## // 000. RESUMEN
La TUI de lazysubs muestra ahora una sección independiente **«OpenCode hoy»** con el consumo local del día por proveedor/modelo, leída de forma privada, incremental y no bloqueante desde la SQLite/WAL de OpenCode. Claude, Codex, providers, Pi, `--json` y `--waybar` conservan sus contratos.

## // 001. QUÉ CAMBIÓ
- `Cargo.toml`, `Cargo.lock`: nueva dependencia `rusqlite = { version = "0.37", features = ["bundled"] }`; el lock fija `libsqlite3-sys 0.35.0` estático.
- `src/opencode_tokens.rs` (nuevo, ~771 LOC prod + ~421 tests): descubrimiento, apertura RO, validación de esquema, proyección `step-finish`, agregación honesta, índice V1, reconciliación 24 h y `OpenCodePanelState`.
- `src/cache.rs`: ruta independiente `opencode_daily_index_file()` reusando `atomic_save`.
- `src/main.rs`: registra `mod opencode_tokens;`.
- `src/tui.rs`: `Update::OpenCodeTokens`, `App::opencode_tokens`, `App::opencode_scanning`, `begin_opencode_token_scan()` con guard compare-and-set, `draw_opencode_tokens` con columna `raz`.
- `src/tokens.rs`: reformateo rustfmt puro (sin cambio funcional).

## // 002. CÓMO FUNCIONA POR DENTRO
- **Descubrimiento (R1)**: `EnvSnapshot` lee `OPENCODE_DB` (absoluto, relativo bajo `$XDG_DATA_HOME/opencode`, `:memory:` → `EphemeralDatabase`, vacío = indefinido); sin override, cae a `$XDG_DATA_HOME/opencode/opencode.db` o `$HOME/.local/share/opencode/opencode.db`. Nunca glob ni canales no declarados.
- **Apertura RO + WAL (R2)**: URI `file:<percent-encoded>?mode=ro` con `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_URI`, `busy_timeout=100 ms` y `PRAGMA query_only=ON`. Nunca `immutable=1`, DDL, checkpoint ni copia. Cada refresh abre su propia conexión dentro de una transacción diferida.
- **Proyección autoritativa (R3)**: CTE sobre `part` (sólo `json_extract` de `p.data`/`m.data` — nunca enteros) + JOIN a `message` por PK. Sólo `part.type='step-finish'` y `message.role='assistant'`. `message.data.tokens`/`cost` quedan fuera (infracómputo multi-step).
- **Agrupación honesta (R5)**: cada métrica es `Option` (`Some(0)` = cero persistido, `None` = ausente en ≥1 parte del grupo). `total` usa `tokens.total` si es entero/no-negativo o, en su defecto, la suma comprobada de input/output/reasoning/cache read/cache write. `reasoning` se conserva como `raz` separado de `out`. Tokens inválidos, coste no finito o provider/model ausente rechazan el snapshot.
- **Índice V1 (R6)**: `cache::atomic_save` escribe `$XDG_CACHE_HOME/lazysubs/opencode-daily-token-index-v1.json` con `DbIdentity` (sin ruta en claro; FNV-1a sobre canónica + `schema_fingerprint`), `DayWindow`, `watermark_rowid`, `seen_part_ids` (IDs opacos sólo del día), `totals` agregados y `last_full_rebuild_ms`. `deny_unknown_fields` rechaza campos extra.
- **Incrementalidad (R6/R7)**: cada refresh ejecuta un cursor probe (`MAX(part.rowid)` + ancla del watermark) antes de cualquier lectura de filas. Si no creció: 1 probe y 0 filas `part`. Crece: suffix `(watermark, max_rowid]` con `SEARCH p USING INTEGER PRIMARY KEY`. Rebuild se dispara por identidad/esquema/ancla/`schema_version`, no por mtime. Reconciliación ≤24 h en `last_full_rebuild_ms + 24h` (reloj inyectable). Bootstrap acepta un único `SCAN part`.
- **Día local (R4)**: `DayWindow::at(clock)` usa `chrono::Local` y límites `[inicio local, inicio siguiente)` para tratar correctamente DST. Al cruzar medianoche se vacían `totals` y `seen_part_ids`; el watermark se conserva si identidad/esquema/ancla siguen válidos.
- **TUI independiente (R8)**: `begin_opencode_token_scan` hace compare-and-set sobre `opencode_scanning` y limpia el flag al recibir `Update::OpenCodeTokens`. `draw_opencode_tokens` pinta **«OpenCode hoy»** tras **«Pi hoy»** con `provider, modelo, in, out, raz, cache→, cache+, total, coste`. Errores se convierten en `Stale` si había filas válidas, o `Unavailable(reason)` en bootstrap. `Status`, Claude y Pi nunca esperan a OpenCode.

## // 003. DECISIONES
- `rusqlite` `bundled` frente a SQLite del sistema: gana reproducibilidad de ABI/JSON1/WAL en CI y entornos sin headers; coste aceptado: binario ≈5,3 MB y compilación mayor.
- Proyección sobre `part.step-finish`, no `message.data.tokens/cost`: granularidad autoritativa y evita infracómputo multi-step.
- `mode=ro` + `query_only`, nunca `immutable=1`: respeta WAL y bloquea DML/DDL accidental.
- `reasoning` visible como `raz` y separado de `out`: oculta/fundir falsearía categorías y diferenciaría indebidamente OpenCode de Pi.
- Reconciliación ≤24 h como cota: más rápido exigiría un scan completo por refresh o un change feed oficial; nunca se hace cada 60 s.
- Índice V1 sin ruta en claro + `deny_unknown_fields`: blindaje contra fugas accidentales y evolución silenciosa.

## // 004. VERIFICACIÓN
- `cargo test` (debug): **51/51 passed**, 0 failed, 0 ignored, 0 filtered (10 nuevos en `opencode_tokens`, 4 nuevos en `tui::tests` para el panel).
- `cargo build --release`: 4 m 41 s. Binario `target/release/lazysubs` 5.474.520 B stripped, MD5 `9d8c8df2645e2eb6a0347133fa46750e`.
- `cargo tree --depth 1`: exactamente las 7 dependencias directas exigidas por R2 (`anyhow`, `chrono`, `ratatui`, `rusqlite`, `serde`, `serde_json`, `ureq`); sin ORM ni crate CLI de SQLite.
- Smoke real del binario: `--help`, `--json` y `--waybar` no contienen `opencode`, `sqlite`, `pragmas`, `SECRET` ni rutas (verificado por grep).
- Cobertura 1:1 de los 13 criterios de aceptación de `scope.md` mapeada a tests en `verify-report.md`; cada criterio R1–R10 ejercita el código de producción contra fixtures o el grafo `App`.

## // 005. PENDIENTE / RIESGOS
- **WIP colgante `1dc6e52`**: intacto en `.git/lost-found`. No se recuperó, no se aplicó, no se eliminó — fuera del alcance de este cambio; decisión previa del orquestador.
- **Reconciliación 24 h**: edits/borrados históricos sin cambio de file id/esquema/ancla pueden permanecer stale hasta el primer refresh debido. Aceptado por R7.
- **Bootstrap inicial caro**: escanea `part` completo (no existe índice temporal). Aceptado por R6; no ocurre cada 60 s.
- **Crash entre persistencia y reset de watermark**: `seen_part_ids` puede sobrevivir al cambio de día; el watermark + `db_identity` fuerzan rebuild en el peor caso.
- **`cargo clippy`/`cargo fmt --check`**: no ejecutados en VERIFY; `openspec/config.yaml` sólo declara `cargo test`.
