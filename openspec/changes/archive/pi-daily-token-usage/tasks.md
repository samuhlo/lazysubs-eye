# Tareas: consumo diario de tokens de Pi/EIN

status: ready
blocked_by: none

> Cinco grupos de strict TDD (RED → GREEN → TRIANGULATE → REFACTOR) que cubren
> el módulo nuevo `src/pi_tokens.rs`, los seams de `src/cache.rs` y `src/tui.rs`,
> la declaración de módulo en `src/main.rs`, y la verificación final con
> `cargo test`. Cada sub-tarea cierra un micro-ciclo TDD con tests y código de
> producción en el mismo commit (work-unit-commits). Sin nuevas dependencias.

## // 001. Parser Pi: header v3, envelope assistant, día local y validación numérica

- [x] 1.1 RED — Fixtures y tests del parser mínimo
  - skills: `ein-discipline`, `architecture`
  - why: Fijar el contrato JSONL Pi (header `type:"session"` v3 + envelope `type:"message"` con `message.role:"assistant"`) y los campos válidos antes de tocar producción; los fixtures sintéticos (solo metadatos/uso, sin prompts ni tools) hacen el contrato ejecutable.
  - learn: El header NO aporta uso; la fecha contable sale SIEMPRE de `message.timestamp` en ms convertido a `DateTime<Local>`; los campos `cwd`, `content`, tools y credenciales no se deserializan ni persisten.
  - architecture: Capa pura en `src/pi_tokens.rs` con `parse_pi_line(&str) -> Option<ParsedEntry>` aislada del filesystem y del reloj; primer seam testeable.
  - avoid: Reutilizar structs o `claude_today()` de `tokens.rs` — su contrato (sin `provider`, sin coste, sin id estable, sin día local derivado de timestamp ms) no es compatible.
  - verify: `cargo test --lib parse_pi_line` — tests rojos para: header v3 válido sin uso, envelope assistant con todos los campos, envelope con `role:"user"` rechazado, JSON malformado tolerado.

- [x] 1.2 GREEN — `parse_pi_line` mínimo y `PiUsageTotals` con checked arithmetic
  - skills: `ein-discipline`, `architecture`
  - why: Hacer verde el parser sin expandir superficie; introducir `PiUsageTotals { input, output, cache_read, cache_write, total_tokens, cost_input, cost_output, cost_cache_read, cost_cache_write, cost_total }` como el único agregado de dominio (R3).
  - learn: `u64::checked_add` y `f64::is_finite() && x >= 0.0` son las dos guardas obligatorias — si fallan, la contribución completa se omite sin contaminar el agregado ni marcar el id como visto.
  - architecture: `src/pi_tokens.rs` aloja el dominio Pi; `cost_total` se mantiene como `f64` porque así llega en JSON y el contrato no exige escala decimal.
  - avoid: `unwrap()`, `saturating_add` silencioso o estimación de precios — el contrato es suma estricta de valores registrados.
  - verify: `cargo test --lib pi_tokens::tests` — solo casos del 1.1; el resto sigue rojo y se aborda en 1.3.

- [x] 1.3 TRIANGULATE — Día local, agrupación `(provider, model)`, validación numérica y privacidad
  - skills: `ein-discipline`, `cognitive-doc-design`
  - why: Generalizar el parser con tests adicionales para: timestamp ms fuera del día local, provider/model faltantes o vacíos, contadores negativos rechazados (entrada descartada, no panic), coste `NaN`/`+inf`/`-inf` rechazado, `totalTokens` ausente (no se estima), y entradas con stop reason de error/abortada que SÍ cuentan si el resto del contrato se cumple.
  - learn: El agregador `group_by(usage, day_key) -> BTreeMap<(String,String), PiUsageTotals>` y la función pura `is_countable_entry(&ParsedEntry, day_key) -> bool` blindan R2 y R3 sin tocar disco.
  - architecture: Las funciones puras se inyectan a `scan_pi_today` para mantener separación entre lógica de dominio y seam de I/O; ningún struct persiste `content`, `cwd`, prompts ni resultados de tools.
  - avoid: Calcular día local desde `mtime` o desde el timestamp ISO del sobre — la regla es `message.timestamp` en ms.
  - verify: `cargo test --lib pi_tokens::tests` — casos rojos de 1.1 ya verdes más: timezone offset aplicado, overflow simulado con `u64::MAX`, coste negativo, día anterior excluido, fixture sin `totalTokens` que no rompe el grupo.

- [x] 1.4 REFACTOR — Extraer validador y documentar invariantes en el módulo
  - skills: `ein-discipline`, `cognitive-doc-design`, `work-unit-commits`
  - why: Separar `is_countable_entry` y `merge_totals(checked)` en helpers nombrados mejora legibilidad y deja un solo lugar donde tocar cuando R3 evolucione; ningún test cambia.
  - learn: Una vez verde, refactorizar con confianza: los tests son la red. Doc-inline mínima explica POR QUÉ (id estable obligatorio, coste no se estima), no el QUÉ.
  - architecture: Documentar en cabecera de `src/pi_tokens.rs` el contrato R1–R3 y la política de privacidad (no persistir nada del envelope salvo provider/model/usage/id/timestamp_ms).
  - avoid: Añadir traits genéricos, `serde::Serialize` a tipos intermedios, o `unsafe` — son sobre-abstracciones para el tamaño actual del módulo.
  - verify: `cargo test --lib pi_tokens::tests` y `cargo clippy -- -D warnings` (solo lectura, no se instala nada nuevo) — diff limitado al archivo nuevo, sin regresiones.

## // 002. Índice diario incremental: versión, día, offset seguro, persistencia atómica y dedup por id

- [x] 2.1 RED — Schema `DailyPiIndexV1`, `DayKey` y seam `PiFileStore`
  - skills: `ein-discipline`, `architecture`
  - why: Fijar la forma serializada (R4): `schema_version: 1`, `DayKey { local_date, timezone_offset_seconds }`, `files: BTreeMap<PathBuf, FileState>`, `seen_entries: BTreeMap<String, EntryState>` con `contribution` + `source_paths: BTreeSet<PathBuf>`, y `totals: BTreeMap<(String,String), PiUsageTotals>`. Definir `trait PiFileStore` con métodos `inventory`, `metadata`, `read_window`, `read_suffix`, `atomic_save` para inyectar I/O y reloj.
  - learn: `seen_entries` separa identidad de la contribución; `source_paths` es refcount — una entrada aporta al grupo solo con la primera fuente y se retira al desaparecer la última.
  - architecture: `src/pi_tokens.rs` define los structs y el trait; `src/cache.rs` solo expone ruta (`pi_daily_index_file`) y helper (`atomic_save`); `StdPiFileStore` queda en `pi_tokens.rs` como adaptador a `std::fs`.
  - avoid: Meter el índice en `status.json` — cambiaría el contrato de `load`/`save` y rompería Waybar; evitar también SQLite o crate nueva.
  - verify: `cargo test --lib pi_tokens::tests::index_schema` — rojos para: índice serializa/deserializa v1, `DayKey` distingue offset horario, `FileState` exige `safe_offset ≤ size`, `EntryState` rechaza ids vacíos.

- [x] 2.2 GREEN — `update_pi_index` bootstrap + `atomic_save` (write→sync→rename)
  - skills: `ein-discipline`, `architecture`
  - why: Hacer pasar el bootstrap leyendo cada archivo desde cero, agregando con `merge_totals` y deduplicando por id global; persistir con temp único en el mismo directorio, `fsync`, `rename` atómico, y en Unix `fsync` del directorio (R6).
  - learn: `tempfile::NamedTempFile::persist` no es necesario — un `PathBuf` con sufijo `.tmp-<uuid>` + `File::sync_all()` + `rename` cubren el caso sin nueva dependencia.
  - architecture: `atomic_save(path, bytes)` en `src/cache.rs`; `update_pi_index` en `src/pi_tokens.rs` recibe `PiScanOptions { root, index_path, day_key, store: &dyn PiFileStore }`.
  - avoid: `std::fs::write` directo (no es atómico), o silenciar fallos de persistencia sin mantener el snapshot en memoria usable.
  - verify: `cargo test --lib pi_tokens::tests::bootstrap_atomic` — bootstrap sobre fixture completa produce snapshot correcto; `atomic_save` con directorio inyectado verifica rename; un fallo de rename simulado deja el snapshot en memoria disponible y el archivo anterior intacto.

- [x] 2.3 TRIANGULATE — Suffix read determinista (solo el sufijo nuevo) + dedup contribution map
  - skills: `ein-discipline`, `performance`
  - why: Demostrar con un `RecordingPiFileStore` que, en estado estable (append compatible), las únicas lecturas de contenido son las ventanas de fingerprint (4 KiB previos al offset) y el sufijo desde `safe_offset`; cualquier relectura histórica debe contar hacia un assert de cero. Es la prueba de que la incrementalidad no se rompe con el tiempo.
  - learn: `safe_offset` solo avanza tras una línea terminada en `\n`; la cola parcial se relee sin avanzar cursor (R5/R7). Esto evita pérdida y doble conteo durante una escritura en curso.
  - architecture: `PiFileStore` recibe un `read_window(path, offset_minus_4k, 4k)` que sirve al fingerprint y un `read_suffix(path, safe_offset)` que sirve al append; ambos son contables en el `RecordingPiFileStore`.
  - avoid: Microbenchmarks de wall-clock como correctness gate — los asserts son sobre bytes leídos, no sobre tiempo; los `Instant` solo sirven para logs de diagnóstico.
  - verify: `cargo test --lib pi_tokens::tests::steady_state_suffix_only` — primer ciclo bootstrap lee contenido completo del fixture; segundo ciclo sin cambios NO incrementa bytes_leidos; tercer ciclo con 5 líneas appendeadas lee EXACTAMENTE esos bytes; ningún test asserta tiempo.

- [x] 2.4 REFACTOR — Fingerprint helpers + extraer `merge_file_contribution`
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Aislar `compute_header_fingerprint(&[u8])` y `compute_cursor_window_fingerprint(&[u8])` en helpers nombrados prepara el terreno para 003 (D2 — append-only con validación acotada); `merge_file_contribution` reduce duplicación entre bootstrap y reconcile.
  - learn: Fingerprint sobre los 4 KiB previos al offset detecta reescrituras comunes sin persistir bytes de sesión; el fingerprint NO se usa para contenido, solo para identidad técnica.
  - architecture: Documentar en comentario de cabecera de `src/pi_tokens.rs` por qué el fingerprint existe (R5: detectar truncado/sustitución sin re-leer histórico) y por qué se limita a 4 KiB (cota de I/O en worst case).
  - avoid: Cambiar el schema v1 (rompería compatibilidad aditiva prometida en R4); exponer `PiFileStore` públicamente fuera del crate.
  - verify: `cargo test --lib pi_tokens::tests` — sigue verde; el refactor solo mueve código y deja tests sin tocar.

## // 003. Descubrimiento, recuperación y reconstrucción por cambio de día

- [x] 3.1 RED — Discovery: env root, recursividad, header v3, EIN anidadas
  - skills: `ein-discipline`, `architecture`
  - why: Fijar R1: raíz = `${PI_CODING_AGENT_DIR}/sessions` si la variable existe y no vacía, si no `~/.pi/agent/sessions/`; recorrido recursivo de `.jsonl` (incluido `run-*/session.jsonl`); solo archivos cuya PRIMERA línea completa sea header v3 válido entran al agregado.
  - learn: `PI_CODING_AGENT_DIR` se resuelve en una función pura `resolve_root(env: &dyn Env) -> Option<PathBuf>` testeable; no se autodetectan rutas de `--session-dir` ni se intenta recuperar `--no-session`.
  - architecture: `Env` trait inyecta `get_var` y `home_dir`; `walk_pi_sessions(root, &dyn PiFileStore) -> Vec<PathBuf>` lista candidatos en un solo nivel de indirección; `is_valid_pi_header(&[u8]) -> bool` decide admisión.
  - avoid: Leer `${HOME}` directamente desde el código de dominio, o filtrar por extensión sin verificar header — un `.jsonl` sin header v3 se omite de forma aislada (R7).
  - verify: `cargo test --lib pi_tokens::tests::discovery` — rojos: `PI_CODING_AGENT_DIR` vacío vs ausente vs presente, raíz inexistente devuelve vacío, EIN anidada `run-123/session.jsonl` descubierta, archivo sin header v3 rechazado pero no aborta el resto.

- [x] 3.2 GREEN — Bootstrap + reconcile file-by-file
  - skills: `ein-discipline`, `architecture`
  - why: Cablear `update_pi_index` para que aplique las reglas de R5–R7 por archivo: append compatible → seek al offset y leer sufijo; archivo nuevo → recorrido completo una vez; truncado/sustitución/fingerprint inválido → retirar procedencias y reprocesar; archivo desaparecido en descubrimiento → retirar fuentes.
  - learn: El `RecordingPiFileStore` de 2.3 es ahora el verificador: para cada caso se demuestra que solo se leyeron los bytes esperados.
  - architecture: Tabla de decisión en comentario del módulo; `process_file(path, &mut index, day_key, store)` es la única función que combina fingerprint + seek + dedup + refcount.
  - avoid: Recorrer todo el árbol de nuevo cuando solo cambió un archivo; o tratar el EIN anidado como fuente especial.
  - verify: `cargo test --lib pi_tokens::tests::reconcile` — verdes: append, archivo nuevo, truncado, reemplazo con identidad cambiada, reemplazo con misma longitud pero fingerprint distinto, borrado confirmado, EIN anidada.

- [x] 3.3 TRIANGULATE — Fork/clone dedup, línea parcial, JSON malformado, índice corrupto, medianoche
  - skills: `ein-discipline`, `cognitive-doc-design`
  - why: Cubrir los escenarios B de la sección Spec del diseño que faltan: dos archivos con mismo `id` cuentan una vez (refs doble); cola sin `\n` no avanza cursor y se relee; línea cerrada malformada avanza cursor y se omite; índice ausente/corrupto/v_unknown → bootstrap; `local_date` o `timezone_offset_seconds` cambiados → descartar y reconstruir sin tocar `status.json`.
  - learn: El refcount vía `source_paths` es la única pieza que distingue "copia fork/clone" de "duplicado accidental"; el día local NO se arrastra al cambiar fecha.
  - architecture: `process_file` recibe `day_key` ya validado fuera (un solo lugar decide reset vs append); `rebuild_for_new_day(prev, new_day_key) -> DailyPiIndexV1` reinicia totales e ids.
  - avoid: Fusión del manejo de errores por archivo en un error global (rompe R7); usar `panic!` en presencia de JSON inválido.
  - verify: `cargo test --lib pi_tokens::tests::recovery_and_midnight` — verdes: fork/clone (1 contribución, 2 fuentes), borrado de una copia (refcount baja a 1, grupo intacto), borrado de la última copia (refcount 0, contribución retirada), cola parcial completada, JSON inválido cerrado (cursor avanza), índice v0/corrupto → bootstrap, `DayKey` distinto → reset.

- [x] 3.4 REFACTOR — Extraer `apply_file_event` y aislar el logger diagnóstico no sensible
  - skills: `ein-discipline`, `work-unit-commits`, `cognitive-doc-design`
  - why: Consolidar el flujo reconcile en una sola función por evento de archivo, y centralizar los mensajes de diagnóstico en un helper que jamás emite rutas técnicas ni contenido (R9); esto permite revisar en un solo diff si algo filtra información.
  - learn: Refactor con tests verdes es seguro; cada caso de 3.1–3.3 sigue siendo la red.
  - architecture: `src/cache.rs` añade `pi_daily_index_file() -> PathBuf` y `atomic_save` (separado de `status.json`); `src/main.rs` declara `mod pi_tokens;` sin tocar `cache::load/save`.
  - avoid: Cambiar el contrato de `cache::load/save` (rompería Waybar); exponer rutas internas en errores visibles para el usuario.
  - verify: `cargo test --lib` — todo verde; `git diff --stat` debe mostrar solo `src/pi_tokens.rs`, `src/cache.rs` y `src/main.rs` con menos del budget acumulado restante.

## // 004. TUI independiente: `Update::PiTokens`, estado, flag y renderer `Pi/EIN hoy`

- [x] 4.1 RED — Tests del estado y del dispatcher
  - skills: `ein-discipline`, `architecture`
  - why: Fijar R8: nueva variante `Update::PiTokens(Vec<PiUsageRow>)`, campo `pi_tokens: Vec<PiUsageRow>` y flag `pi_tokens_scanning: bool` en `App`; `Update::Status` y `Update::Tokens` se aplican aunque Pi siga escaneando; `begin_pi_token_scan` rechaza un segundo worker mientras hay uno activo.
  - learn: Replicar el patrón ya probado para tokens Claude: un solo `mpsc`, flag separado, handler que resetea el flag al recibir la variante; independencia lógica = tests pequeños.
  - architecture: `src/tui.rs` añade la variante y el campo sin tocar `Update::Status` ni `Update::Tokens`; `PiUsageRow` viene de `src/pi_tokens.rs` (sin lógica de presentación en el módulo de dominio).
  - avoid: Mover la decisión de "escaneo activo" al render o a `draw`; crear un `Mutex` solo para esto (el `mpsc` ya da el flujo correcto).
  - verify: `cargo test --lib tui::tests::pi_state_independence` — rojos: `Update::Status` se aplica durante `pi_tokens_scanning == true`; segundo `begin_pi_token_scan` devuelve `false` mientras hay worker activo.

- [x] 4.2 GREEN — Spawn del worker Pi y recepción de `Update::PiTokens`
  - skills: `ein-discipline`, `architecture`
  - why: Cablear `App::refresh` para que, tras el bloque ya existente de tokens Claude, llame a `begin_pi_token_scan` y haga spawn de UN worker que invoque `pi_tokens::scan_pi_today()` y envíe `Update::PiTokens`. El worker Pi debe ejecutarse en su propio hilo, sin acoplar a `collect_all()` ni bloquear providers.
  - learn: El triple (provider scan, Claude scan, Pi scan) es ahora tres hilos/flags disjuntos — un fallo lento en uno no bloquea a los otros dos.
  - architecture: `App::begin_pi_token_scan` reusa exactamente la forma de `begin_token_scan`; el handler en el loop principal añade un brazo para `Update::PiTokens`.
  - avoid: Reentradas múltiples (un `refresh(true)` ya activo no debe relanzar Pi); pasar `App` por `Arc<Mutex>` cuando el `mpsc` ya basta.
  - verify: `cargo test --lib tui::tests::pi_state_independence` — verde el caso de 4.1; un test adicional comprueba que `refresh(true)` cuando ya hay un scan Pi NO inicia otro.

- [x] 4.3 TRIANGULATE — Renderer `draw_pi_tokens` con altura independiente y formato neutro de coste
  - skills: `ein-discipline`, `cognitive-doc-design`, `architecture`
  - why: Implementar `draw_pi_tokens` como tabla con columnas provider, model, in, out, cache read, cache write, total y `coste` (sin símbolo de moneda, 4 decimales, recorte si falta ancho). El bloque se renderiza solo si hay filas, su altura es independiente de la tabla Claude y nunca aparece en zonas reservadas a providers.
  - learn: `fmt_count` se reutiliza para los contadores enteros; el coste necesita formato propio (`format!("{:.4}", cost)` con `trim_end_matches('0').trim_end_matches('.')` cuando haga falta); `PiUsageRow` expone `cost_total: f64` ya validado por el dominio.
  - architecture: `Layout::vertical` añade una restricción solo si `!self.pi_tokens.is_empty()`; `draw_pi_tokens` recibe el `Rect` calculado, sin recalcular alturas globales desde dentro.
  - avoid: Fusionar filas Pi con `tokens` (rompería semántica y tests previos); mostrar `cost_total` con `format!("{}", cost)` (puede usar notación científica o dígitos innecesarios).
  - verify: `cargo test --lib tui::tests::pi_render` — verdes: render con 0 filas no añade restricción; render con 2 filas ocupa `rows+3` y la altura no depende de la cantidad de providers; coste `0.0000` se muestra limpio; coste `1234.56789` se recorta a 4 decimales.

- [x] 4.4 REFACTOR — Aislar helpers `fmt_cost` y `pi_section_height` para reutilización y claridad
  - skills: `ein-discipline`, `work-unit-commits`, `cognitive-doc-design`
  - why: `fmt_cost(f64) -> String` y `pi_section_height(usize) -> u16` como helpers nombrados dejan el renderer legible y testeable de forma aislada; ningún test cambia.
  - learn: Refactorizar el renderer tras verde es seguro; los tests de 4.3 son la red.
  - architecture: `src/tui.rs` añade `fmt_cost` como `fn` privada al lado de `fmt_count` (no se mueve a `tokens.rs` para no acoplar dominios); un comentario de cabecera documenta la regla R3 "no símbolo de moneda".
  - avoid: Mover `fmt_cost` a `pi_tokens.rs` (rompe la separación dominio/presentación); cambiar el formato a 2 decimales sin justificación de diseño.
  - verify: `cargo test --lib tui::tests` — todo verde; `cargo build` solo lectura confirma que el binario sigue compilando.

## // 005. Regresión, performance, invariantes y preparación de verificación final

- [x] 5.1 RED — Regresiones explícitas: Claude/Codex/Waybar/`status.json`/TUI Claude intactos
  - skills: `ein-discipline`, `work-unit-commits`, `branch-pr`
  - why: Fijar que los caminos existentes NO cambian: `Status.providers` no incluye Pi; `cache::load/save` siguen leyendo/escribiendo el mismo `status.json`; la tabla Claude conserva su render; los modos `--json` y `--waybar` no llevan datos Pi; `claude_today()` y los collectors de Claude/Codex siguen funcionando sin requerir Pi.
  - learn: La "compatibilidad aditiva" prometida en C se demuestra con tests, no con prosa — un solo test rojo por superficie.
  - architecture: Tests en `src/tui.rs` y (si procede) en un test de integración en `tests/` que ejecute el binario en modo `--json` y `--waybar` con un fixture de `status.json` viejo y verifique bytes exactos.
  - avoid: "Asumir" que nada se rompió; usar `assert!(status.providers.iter().any(|p| p.id == "pi"))` que sería el test contrario al requisito.
  - verify: `cargo test --lib` — rojos: `Status` serializado/deserializado byte-a-byte igual al contrato previo; `output::waybar` con `Status` de prueba produce string idéntico al de `main` antes del cambio.

- [x] 5.2 GREEN — Implementar/ajustar lo necesario para que los rojos de 5.1 pasen sin tocar el contrato previo
  - skills: `ein-discipline`, `architecture`
  - why: Si algún test de regresión falla, la causa suele ser un `Update::*` añadido que reordenó brazos del `match` o un import que introdujo dependencia cruzada. Ajustar lo mínimo (no expandir superficie) hasta verde.
  - learn: La regresión es la red final antes del PR — cualquier "simplificación" oportunista aquí se rechaza.
  - architecture: Mantener límites: `src/pi_tokens.rs` no importa nada de `src/tokens.rs`, `src/tui.rs` ni `src/cache.rs` excepto por los tipos públicos que necesita (`Status`, `fmt_count`).
  - avoid: Refactors "de paso" que mezcle dominios o que reduzcan la cobertura estricta TDD.
  - verify: `cargo test --lib` — verde; `git diff --stat origin/main..HEAD -- . ':(exclude)*.test.*' ':(exclude)**/tests/**'` confirma que la suma de líneas producidas (sin tests) está dentro del budget restante (≤ 400 líneas, medido por `ein-git` antes del PR).

- [x] 5.3 TRIANGULATE — Evidencia determinista de incrementalidad + privacidad + atomicidad en fallo
  - skills: `ein-discipline`, `performance`, `cognitive-doc-design`
  - why: Cerrar los tres invariantes sensibles del diseño:
    1. **Incrementalidad determinista**: con `RecordingPiFileStore`, un ciclo sin cambios lee 0 bytes de contenido y 0 líneas parseadas; un append de N bytes lee exactamente N bytes y parsea exactamente las líneas terminadas en `\n`.
    2. **Privacidad**: snapshot serializado, índice en disco y errores visibles al usuario NO contienen `content`, `cwd`, prompts ni credenciales — assert con `String::contains` sobre campos prohibidos.
    3. **Atomicidad ante fallo**: simular rename fallido deja el snapshot en memoria usable y el `pi-daily-token-index-v1.json` anterior sigue parseable.
  - learn: La prueba de incrementalidad NO mide tiempo — mide BYTES. Esto es un correctness gate, no un benchmark.
  - architecture: `RecordingPiFileStore` evoluciona para contar `bytes_leidos_contenido` y `lineas_parseadas`; `atomic_save` recibe un `RenameFn` inyectable para fallar de forma determinista.
  - avoid: `Instant::now()` en asserts; persistir contenido "solo para diagnóstico"; usar `unwrap` en la ruta de persistencia.
  - verify: `cargo test --lib pi_tokens::tests::invariants` — verdes los tres invariantes; el test de incrementalidad asserta `bytes_leidos_contenido == 0` en steady state y `bytes_leidos_contenido == appended_bytes` tras append.

- [x] 5.4 REFACTOR — Limpieza final, comentarios proporcionales y preparación para `cargo test`
  - skills: `ein-discipline`, `cognitive-doc-design`, `work-unit-commits`, `branch-pr`
  - why: Quitar imports muertos, asegurar que los comentarios explican POR QUÉ (no el QUÉ), confirmar que el binario compila y que el suite completo pasa. Esta tarea NO añade documentación visible para el usuario salvo que el diseño lo pida explícitamente (no lo pide — R8 mantiene la salida Waybar sin cambios y no se solicita README nuevo).
  - learn: Una línea de doc-inline por invariante R1–R9 es suficiente; más es ruido. Cerrar sin PR — `ein-git` decide la entrega según su gate (Review Workload Guard de 400 líneas).
  - architecture: Diff final: `src/pi_tokens.rs` (nuevo), `src/cache.rs` (helpers Pi), `src/main.rs` (declaración), `src/tui.rs` (variante + estado + render). Sin cambios en `src/providers/*`, `src/output.rs`, `src/tokens.rs` (más allá de `fmt_count` ya expuesto).
  - avoid: Tocar docs existentes, README o `EIN.md` — el alcance no lo autoriza; añadir `unsafe`, dependencias nuevas o traits genéricos.
  - verify: `cargo test` (suite completo) verde; `git diff --stat` muestra que la suma de líneas de producción permanece dentro del budget SDD; el ledger de tareas se cierra con referencia a este archivo (`openspec/changes/pi-daily-token-usage/tasks.md`) para `ein-git`.