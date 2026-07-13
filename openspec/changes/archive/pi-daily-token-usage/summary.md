## // 000. RESUMEN

lazysubs ahora muestra una sección TUI **`Pi/EIN hoy`** con el consumo registrado hoy de las sesiones Pi —incluidos subagentes EIN anidados—, agrupado por `provider`/`model`, leído de forma incremental y sin tocar providers, JSON, Waybar, Claude ni Codex.

## // 001. QUÉ CAMBIÓ

- Nuevo colector local en `src/pi_tokens.rs` que parsea el JSONL oficial de Pi (header v3 + envelope `type:"message"` con `message.role:"assistant"`).
- Índice diario versionado (`DailyPiIndexV1`) en archivo separado bajo XDG cache, persistido de forma atómica (write→sync→rename, con sync del directorio en Unix).
- Sección TUI `Pi/EIN hoy` con renderer y altura independientes de la tabla Claude, alimentada por una variante propia `Update::PiTokens` y un único worker activo (`pi_tokens_scanning`).
- Resto del binario intacto: `Status`, `collect_all()`, `claude_today()`, contratos Waybar/JSON y `status.json` byte-a-byte sin cambios.

## // 002. CÓMO FUNCIONA POR DENTRO

**Raíz y descubrimiento.** `resolve_root` toma `${PI_CODING_AGENT_DIR}/sessions` si existe y no está vacío, si no `~/.pi/agent/sessions/`. `walk` recorre recursivamente todos los `.jsonl`, incluidos `sessions/.../run-*/session.jsonl` de EIN. Un archivo entra solo si su primera línea completa es un header v3 válido (`type:"session"`, `version:3`, id no vacío, timestamp ISO).

**Parser y entrada contable.** `parse_pi_line` exige el sobre `{type:"message", id estable no vacío}` con `message.role:"assistant"`, `provider`, `model`, `message.timestamp` (ms Unix) y un `usage` con `input/output/cacheRead/cacheWrite/totalTokens` + `cost.{input,output,cacheRead,cacheWrite,total}`. El día contable sale **únicamente** de `message.timestamp` convertido a `Local`; entradas con `stopReason` de error/abortado cuentan si el resto del contrato se cumple. Los structs `Deserialize` solo exponen esos campos — `content`, `cwd`, prompts, tools y credenciales nunca se deserializan ni persisten (R9).

**Agregación y dedup.** `merge_totals` usa `checked_add` en los cinco contadores y exige `is_finite() && >= 0.0` en los cinco costes; cualquier overflow o no-finito descarta la contribución completa **antes** de tocar `seen_entries`. La deduplicación es global por `entry.id`: cada entrada registra `contribution + source_paths` (refcount); el grupo recibe la suma al aparecer la primera fuente y solo se resta al desaparecer la última (cubrir forks/clones sin doble conteo).

**Índice diario.** `DailyPiIndexV1 { schema_version: 1, day_key: DayKey { local_date, timezone_offset_seconds }, files, seen_entries, totals }`. Cambia `day_key` (medianoche u offset horario) ⇒ descarte completo y bootstrap sin arrastrar ids ni totales del día anterior. Índice corrupto, schema desconocido o path-keyed legacy sin `(dev, ino)` ⇒ bootstrap seguro; `status.json` nunca se toca.

**Identidad de archivo y ventanas.** En Unix, `stable_file_identity` devuelve `(unix:{dev}:{ino}, Some(dev), Some(ino))` desde `Metadata`; en otras plataformas degrada a `(path:{normalized}, None, None)`. Fingerprint doble: 4 KiB del header inicial hasta `\n` + 4 KiB inmediatamente anteriores a `safe_offset`. Tras validar identidad + header + ventana, un archivo append-only conocido se procesa con `seek(safe_offset)` y se lee solo el sufijo nuevo. Cola sin `\n` no avanza el cursor — se relee íntegra en el siguiente ciclo (R5/R7).

**Recuperación.** Truncado (`size < safe_offset`), sustitución (cambio de identidad/header) o fingerprint incompatible ⇒ `remove_file_sources` retira primero todas las procedencias del archivo y lo reprocesa desde cero. Borrado confirmado en descubrimiento ⇒ retira esas fuentes; el grupo sobrevive si otro fork/clone aún aporta la entrada. Línea cerrada malformada ⇒ omitir y avanzar cursor por `\n`. Raíz ausente/vacía ⇒ snapshot vacío. Ningún error de archivo aborta el agregado global.

**Persistencia atómica.** `cache::atomic_save_with_rename` escribe un temporal único en el mismo directorio, sincroniza, hace `rename` y en Unix sincroniza el directorio; si el rename falla, elimina el temporal y deja el archivo anterior byte-a-byte intacto. El seam inyectable permite testear el fallo sin FS-pseudo-failures.

**TUI independiente.** `Update::PiTokens(Vec<PiUsageRow>)`, `App.pi_tokens` y flag `pi_tokens_scanning` conviven con `refreshing`/`tokens_scanning`. `begin_pi_token_scan` rechaza un segundo worker mientras hay uno activo; `Update::Status` y `Update::Tokens` se aplican aunque Pi siga escaneando. `draw_pi_tokens` solo añade restricción si hay filas, calcula `pi_section_height` propia, muestra columnas provider/model/in/out/cacheRead/cacheWrite/total/coste con `fmt_count` para enteros y `fmt_cost` (`format!("{:.4}")` con trim de ceros/punto, sin símbolo de moneda, etiqueta neutral `coste`).

**Símbolos clave:** `PiUsageTotals`, `PiUsageRow`, `DayKey`, `FileState { dev, ino, header_fingerprint, safe_offset, entry_ids }`, `EntryState { contribution, source_paths }`, `DailyPiIndexV1`, `parse_pi_line`, `merge_totals`, `is_countable_entry`, `add_entry`, `remove_file_sources`, `stable_file_identity`, `index_is_compatible`, `cursor_window`, `process_file`, `walk`, `scan_pi_today`, `update_pi_index`, `atomic_save`, `atomic_save_with_rename`, `pi_daily_index_file`.

## // 003. DECISIONES

- **Módulo separado `pi_tokens.rs`** (no en `tokens.rs` ni como provider): Pi tiene `provider`, coste, id global e incrementalidad propios; se reutiliza solo `fmt_count`.
- **Índice en archivo separado** (`pi-daily-token-index-v1.json`) y no en `status.json` para preservar el contrato serializado de Waybar/JSON y permitir descartar el Pi sin afectar la caché principal.
- **Append-only con validación acotada** (D2): identidad + header + 4 KiB previos al cursor. Se rechaza rehash/reparsear cada archivo modificado completo porque reintroduciría la lectura histórica y rompería la incrementalidad.
- **Persistencia atómica sin nueva dependencia** (D4): temp + sync + rename + `atomic_save_with_rename` inyectable. Se rechazan SQLite y crates nuevas — `std`, `serde`, `serde_json` y `chrono` cubren el caso.
- **Política numérica y visual** (D5): `u64` con aritmética comprobada, `f64` para costes (tal como llegan), total visible a 4 decimales sin divisa, provider+model forman la clave completa de grupo.
- **Un solo productor de estado Pi** (D7): un worker, un `Update::PiTokens`, sin escaneo desde `draw`, sin acoplar a `collect_all()` ni al scan Claude.

## // 004. VERIFICACIÓN

`cargo build` OK · `cargo clippy -- -D warnings` OK (0 warnings, 0 notes) · `rustfmt --config skip_children=true` solo sobre los 4 archivos tocados, OK · `git diff --check` OK · **`cargo test` → 38/38 verde, 0 failed, 0 ignored**. Cobertura de comportamiento verificada por tests dedicados: 14 en `pi_tokens`, 2 nuevos en `cache`, 1 contrato byte-estable en `output`, +1 test `pi_` en `tui`. Determinismo por bytes (no por tiempo): ciclo estable lee 0 bytes del sufijo; append lee exactamente los bytes añadidos. Atomicidad: `failed_final_rename_keeps_the_previous_complete_index_readable` fuerza rename fallido y verifica archivo anterior intacto, parseable, sin temp residual. Privacidad: índice serializado no contiene `content`/`cwd` sintéticos. Waybar/JSON: `src/output.rs` líneas 1-86 byte-a-byte intactas; nuevo test fija el contrato exacto con `error: null`.

## // 005. PENDIENTE / RIESGOS

- **Compatibilidad:** sin cambios en `Status`, `collect_all()`, `claude_today()`, `src/providers/*`, `src/tokens.rs`, `src/output.rs` (producción), `Cargo.toml`/`Cargo.lock`. Sin nueva dependencia. OpenCode queda para un SDD independiente.
- **Path fallback no-Unix** (`#[cfg(not(unix))]`) solo verificado por inspección; el `cargo test` corre en Linux. Lógica directa (`path:{lossy}` + `index_is_compatible` con prefijo `path:`).
- **Race de test contra medianoche local**: `entry(id)` usa `Local::now()` y cada test cachea `DayKey::now()` al inicio; si la suite cae justo en el cambio de día, la entrada generada puede quedar fuera del agregado. No observado en producción — cada `scan_pi_today` recalcula `DayKey::now()` y R8 cubre el rollover.
- **Tamaño del índice sin medir** con los 1.552 ids reales: se rechazó imponer límite artificial antes de tener referencia; tomar la métrica en la primera corrida real.
- **Delivery review size**: producción +696 líneas vs presupuesto 400 (≈+74 %). Es decisión de `ein-git` con `auto-forecast` — debe consultar al usuario entre PR único y split en chained PRs antes del push; no bloquea la verificación funcional. Tests +445 se reportan aparte y no cuentan al budget.
- **`scan_pi_today` no usa `&dyn PiFileStore`** como prometía D6: el seam quedó como helpers puros y el rename inyectable en `cache`. Si en futuro se necesita inyectar fallos finos de `open`/`read`, reintroducir el trait.
- **Sin commit, push, PR, instalación ni movimiento del directorio** en esta fase — esos pasos son del supervisor tras cerrar.