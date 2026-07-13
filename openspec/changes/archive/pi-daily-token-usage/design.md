# Diseño: uso diario de tokens de Pi/EIN

## A. Proposal

### Intent

Añadir un colector local e incremental que contabilice el uso registrado hoy en sesiones Pi —incluidos los subagentes EIN—, lo agrupe por `provider`/`model` y lo entregue a una tabla TUI independiente titulada `Pi/EIN hoy`.

### Scope

**Incluye:** sesiones JSONL bajo `${PI_CODING_AGENT_DIR}/sessions` cuando `PI_CODING_AGENT_DIR` esté definido y no vacío, o `~/.pi/agent/sessions/` en otro caso; día natural local; deduplicación global por id de entrada; índice diario incremental; recuperación segura; y presentación TUI.

**No incluye:** OpenCode, historial de siete días, directorios arbitrarios de `--session-dir`, ejecuciones `--no-session`, desglose por proyecto/sesión/agente, cambios de providers o Waybar, backoff/locking de providers ni un rediseño amplio de la TUI.

### Affected areas

| Archivo | Responsabilidad y símbolos previstos |
|---|---|
| `src/pi_tokens.rs` (nuevo) | `PiUsageTotals`, `PiUsageRow`, `PiDailySnapshot`, `DayKey`, `DailyPiIndexV1`, `PiScanOptions`, `PiFileStore`, `StdPiFileStore`, `scan_pi_today` y el núcleo `update_pi_index`; descubrimiento, parseo parcial, deduplicación, agregado e índice. |
| `src/cache.rs` | `cache_dir`, `pi_daily_index_file` y `atomic_save`; ruta XDG hermana de `status.json`, sin cambiar el contrato de `load`/`save` existente. |
| `src/tui.rs` | `Update::PiTokens`, estado `pi_tokens`, flag `pi_tokens_scanning`, `App::begin_pi_token_scan` y `draw_pi_tokens`; actualización y tabla independientes. |
| `src/main.rs` | Declaración `mod pi_tokens`; los modos JSON/Waybar continúan usando solo `Status`. |

La caché será exactamente `$XDG_CACHE_HOME/lazysubs/pi-daily-token-index-v1.json`; si `XDG_CACHE_HOME` no está definido, `~/.cache/lazysubs/pi-daily-token-index-v1.json`.

### Risks

- Una reescritura histórica que conserve ruta, inode, header y la ventana de validación podría eludir la detección; se acepta el contrato append-only de Pi y se cubren truncados, sustituciones y mutaciones cercanas al cursor.
- El índice crecerá con ids y procedencias del día; no se impondrá un límite que pueda romper la deduplicación antes de medir su tamaño con la referencia de 1.552 ids.
- `f64` puede introducir redondeo binario; solo se sumarán costes finitos no negativos y la UI mostrará `cost.total` a cuatro decimales, sin inventar divisa.
- Errores de permisos o carreras pueden producir un agregado parcial; nunca deben bloquear el render ni invalidar contribuciones de otros archivos.

### Rollback

Revertir los cambios de los cuatro módulos y eliminar, si se desea, `pi-daily-token-index-v1.json`. Al estar separado de `status.json`, el rollback no requiere migrar la caché existente ni cambia providers, Claude, Codex o Waybar.

### Success criteria

El primer escaneo produce los totales correctos del día; los siguientes procesan solo archivos nuevos, afectados o sufijos completos; forks/clones no duplican uso; los fallos se aíslan; y la TUI recibe y dibuja `Pi/EIN hoy` sin esperar a providers ni iniciar recorridos duplicados.

## B. Spec

### Requisitos

**R1 — Raíz y sesiones válidas.** El sistema **MUST** resolver la raíz como `${PI_CODING_AGENT_DIR}/sessions` si la variable está definida y no vacía, o como `~/.pi/agent/sessions/` en otro caso. **MUST** recorrer recursivamente archivos `.jsonl`, incluidos `run-*/session.jsonl`. Un archivo **MUST** considerarse sesión Pi solo si su primera línea completa es un objeto con `type:"session"`, `version:3`, id no vacío y timestamp ISO válido; el header no aporta uso. El sistema **MUST NOT** autodetectar `--session-dir` ni intentar recuperar `--no-session`.

**R2 — Entradas contables.** El sistema **MUST** contar únicamente sobres `type:"message"` con id estable no vacío y `message.role:"assistant"`. `message` **MUST** aportar `provider`, `model`, timestamp Unix en milisegundos y `usage.input`, `output`, `cacheRead`, `cacheWrite`, `totalTokens`, más `cost.input`, `output`, `cacheRead`, `cacheWrite` y `total`. El día **MUST** derivarse de `message.timestamp` convertido a la zona local del proceso; ni el timestamp del sobre ni el mtime pueden sustituirlo. Entradas con terminación de error o abortada **MUST** contar si cumplen este contrato.

**R3 — Agregado, deduplicación y números.** El sistema **MUST** agrupar por la pareja exacta `(provider, model)` y **MUST** deduplicar globalmente por id de sobre entre todos los archivos. Cada contador **MUST** ser entero no negativo representable como `u64`; cada coste **MUST** ser finito y no negativo. Las sumas **MUST** ser comprobadas: si cualquier contador desborda o cualquier suma de coste deja de ser finita, la contribución completa **MUST** omitirse sin marcar su id como contabilizado. El sistema **MUST NOT** estimar tokens ni precios. La UI **MUST** mostrar `cost.total` con cuatro decimales, etiqueta neutral `coste` y sin símbolo de moneda.

**R4 — Índice diario versionado.** El sistema **MUST** persistir un `DailyPiIndexV1` separado con `schema_version: 1` y una clave de día `{local_date, timezone_offset_seconds}`. **MUST** contener, por archivo, identidad/fingerprint, tamaño, mtime, último offset posterior a una línea completa y los ids aportados; globalmente, **MUST** contener cada id con su contribución y conjunto de rutas fuente, además de los totales por grupo. Una contribución **MUST** sumarse al aparecer su primera fuente y retirarse solo al desaparecer su última fuente.

**R5 — Incrementalidad y fingerprints.** En Linux, la identidad **MUST** usar `(dev, ino)`; en otras plataformas **MUST** degradar a ruta normalizada, tamaño, mtime y fingerprints. El fingerprint **MUST** cubrir el header técnico y una ventana fija de 4 KiB inmediatamente anterior al offset seguro, sin persistir bytes de sesión. Tras validar identidad, header y ventana, un archivo append-only conocido **MUST** parsearse desde `safe_offset`; la cola sin `\n` **MUST NOT** avanzar el cursor. Los bytes históricos leídos para validación **MUST** quedar acotados por esas ventanas, y el contenido parseado en estado estable **MUST** ser únicamente el sufijo nuevo.

**R6 — Recuperación y persistencia.** Un archivo nuevo **MUST** recorrerse completo una vez. Un archivo truncado, sustituido o con fingerprint incompatible **MUST** retirar primero sus procedencias anteriores y reconstruirse desde cero. Un índice ausente, corrupto, incompatible o de otro día/offset horario **MUST** descartarse y reconstruirse sin tocar `status.json`. El índice **MUST** escribirse en un temporal único del mismo directorio, completarse y sincronizarse antes de `rename`; en Unix el directorio **SHOULD** sincronizarse después. Si guardar falla, el snapshot en memoria **MUST** seguir disponible y el archivo final anterior **MUST NOT** quedar parcialmente sobrescrito.

**R7 — Tolerancia a entradas y archivos.** Una línea cerrada malformada o no contable **MUST** omitirse y avanzar el cursor hasta después de su `\n`; una cola parcial **MUST** releerse al completarse. Un archivo eliminado confirmado **MUST** retirar sus fuentes; si desaparece durante lectura, el error **MUST** aislarse y su estado previo **SHOULD** conservarse hasta la siguiente reconciliación. Una raíz ausente o vacía **MUST** producir estado vacío; ante inaccesibilidad transitoria, el sistema **MAY** conservar el snapshot compatible previo como parcial. Ningún error de archivo **MUST** abortar el agregado válido restante.

**R8 — TUI independiente y compatibilidad.** La TUI **MUST** recibir Pi mediante `Update::PiTokens` y mantener un único scan Pi activo mediante `pi_tokens_scanning`. `Update::Status` y `Update::Tokens` **MUST** aplicarse aunque Pi siga trabajando; el render **MUST NOT** explorar sesiones. `draw_pi_tokens` **MUST** mostrar filas separadas con provider, modelo, entrada, salida, cache read, cache write, total y coste, usando `fmt_count` para contadores. `Status`, `collect_all()`, los collectors y paneles actuales, `status.json`, Claude/Codex y todas las salidas Waybar **MUST** conservar su comportamiento y formato.

**R9 — Privacidad.** El parser **MUST** deserializar solo header técnico, identidad de entrada, rol, provider/model, timestamp y uso. El índice, la UI y los diagnósticos **MUST NOT** guardar ni emitir prompts, respuestas, contenido o argumentos/resultados de tools, credenciales ni cwd. Las rutas técnicas del índice **MUST NOT** mostrarse en la TUI ni en errores de usuario.

### Escenarios Given/When/Then

| Caso | Given | When | Then |
|---|---|---|---|
| Raíz vacía o no disponible | La raíz resuelta no existe, está vacía o no puede leerse | Se solicita el snapshot de hoy | Se devuelve vacío o el último snapshot compatible marcado como parcial, sin fallo global ni intento de `--session-dir`. |
| Primer escaneo | No existe índice y hay sesiones v3 válidas | Se ejecuta el colector | Lee una vez los archivos completos, cuenta solo assistants de hoy y guarda el índice v1 atómicamente. |
| Archivo nuevo | Existe un índice válido y aparece un JSONL con header v3 | Ocurre el siguiente refresco | Recorre completo solo el archivo nuevo y añade sus contribuciones no vistas. |
| Append | Un archivo indexado conserva identidad/fingerprints y recibe líneas completas | Ocurre el siguiente refresco | Valida ventanas acotadas, hace seek al offset seguro y parsea solo el sufijo nuevo. |
| Cola parcial completada | El escaneo anterior terminó ante una línea sin `\n` | Se completa esa línea y vuelve a escanearse | Relee desde el mismo offset, la cuenta una sola vez y avanza tras el nuevo `\n`. |
| Truncado o reemplazo | El tamaño cae bajo el cursor, cambia identidad/header o falla el fingerprint | Se reconcilia el archivo | Retira sus fuentes antiguas y reconstruye únicamente ese archivo desde cero. |
| Índice corrupto | El JSON del índice no parsea, su versión es desconocida o sus invariantes no cuadran | Arranca el colector | Ignora el índice, conserva intacto `status.json` y realiza bootstrap seguro. |
| Medianoche o zona cambiada | La clave guardada no coincide con fecha local y offset actuales | Se refresca después del cambio | Descarta ids/totales diarios y reconstruye el nuevo día sin arrastre. |
| Fork/clone duplicado | Dos archivos contienen el mismo id estable válido | Ambos son descubiertos | Se conserva una contribución con dos fuentes y el gasto se suma una sola vez. |
| Sesión EIN anidada | Existe `sessions/.../run-123/session.jsonl` con header v3 | Se recorre la raíz | Sus assistants participan en el mismo agregado que cualquier sesión Pi. |
| Mismo modelo, providers distintos | Dos entradas usan igual `model` y distinto `provider` | Se agregan | Aparecen en dos grupos y filas diferentes. |
| Error o abortado | Una entrada assistant tiene stop reason de error/aborted y usage válido | Se procesa | Su uso registrado cuenta normalmente; el estado de terminación no lo descarta. |
| Malformado o no sesión | Hay una línea JSON inválida cerrada o un `.jsonl` sin header v3 válido | Se descubre/procesa | La línea o archivo se omite de forma aislada; una línea cerrada procesada permite avanzar el cursor. |
| Overflow o coste inválido | Una contribución desborda `u64`, tiene coste negativo/no finito o vuelve no finita la suma | Se intenta agregar | Se omite completa, no contamina totales ni el conjunto global de ids y se emite solo diagnóstico no sensible. |
| Archivo eliminado | Una ruta indexada deja de existir y la ausencia se confirma en descubrimiento | Se reconcilia el inventario | Se retira esa fuente; la contribución permanece si otro fork/clone aún la contiene. |
| TUI independiente | Un scan Pi sigue lento y llegan estados de provider; además se solicita otro scan Pi | La TUI procesa eventos | Aplica `Update::Status` de inmediato, conserva el render responsivo y no inicia un segundo scan Pi. |
| Waybar sin cambios | Hay datos Pi disponibles | Se ejecuta un modo no TUI/Waybar | La salida mantiene exactamente sus claves, texto, tooltip, clase, porcentaje y umbrales anteriores, sin datos Pi. |

## C. Decisions

### D1. Colector de dominio separado

Pi vivirá en `src/pi_tokens.rs`, no en `tokens.rs` ni como provider remoto. Pi tiene timestamp, coste, deduplicación e incrementalidad propios; compartir solo `fmt_count` evita mezclar contratos. `src/tui.rs` posee scheduling y presentación, `src/cache.rs` posee rutas/escritura atómica y `src/main.rs` solo registra el módulo.

### D2. Append-only con validación acotada

Se elige aceptar el contrato append-only después de validar identidad, header y los 4 KiB anteriores al cursor. Esto mantiene el coste normal ligado al sufijo y detecta sustitución, truncado y reescrituras comunes. Se rechaza rehash/reparsear cada archivo modificado completo: maximiza exactitud ante una mutación histórica excepcional, pero volvería a leer archivos grandes en cada append y contradice el objetivo incremental.

### D3. Índice separado con referencias globales

`DailyPiIndexV1` guardará `files`, `seen_entries` y `totals`. Los conjuntos `source_paths` y `entry_ids` permiten retirar archivos sin romper forks/clones. Los totales son derivados pero se persisten para entrega inmediata; al cargar, cualquier incoherencia estructural obliga a bootstrap. Se rechaza guardar el índice dentro de `status.json` porque alteraría el contrato serializado de providers y Waybar.

### D4. Persistencia mínima y atómica

Solo se persisten día, fingerprints técnicos, cursores, ids, procedencias y números de uso. El guardado temporal + sync + rename protege el último índice válido. Se rechazan SQLite y nuevas crates: `std`, `serde`, `serde_json` y `chrono` ya cubren el caso y una base de datos añadiría complejidad sin dolor presente.

### D5. Política numérica y visual

Los contadores usan `u64` con aritmética comprobada; los costes usan `f64` porque así llegan en JSON y no existe escala decimal contractual. Solo se aceptan valores finitos no negativos; el total visible usa cuatro decimales, sin divisa. Provider y modelo forman la clave completa, aunque compartan nombre de modelo.

### D6. Seams de strict TDD

`scan_pi_today` será la entrada de producción. `update_pi_index` recibirá `PiScanOptions` con raíz, ruta de índice y `DayKey`, más `&dyn PiFileStore`; `StdPiFileStore` conectará `std::fs`. El seam permitirá inyectar inventario, reloj/día, lecturas/seek contabilizados y fallos de open/write/rename sin usar `$HOME`, reloj real ni sesiones reales. Los fixtures contendrán únicamente headers y usage sintéticos. Es la abstracción mínima necesaria para probar recuperación, atomicidad e I/O; no se añade una jerarquía general de repositorios.

### D7. Un único productor de estado Pi

`App::begin_pi_token_scan` inicia como máximo un worker Pi y envía un único `Update::PiTokens`. El refresco de provider y el scan Claude mantienen workers y flags separados. Se rechazan el escaneo desde `draw`, encadenarlo a `collect_all()` y lanzar un segundo recorrido “para la UI”, porque bloquearían o duplicarían trabajo.

### Alternativas y compatibilidad

- No se reutiliza `claude_today()`: su formato, fecha y semántica no contienen provider/coste/id global.
- No se autodetectan rutas arbitrarias ni se añade OpenCode/histórico: son cambios independientes.
- No se impone compactación del índice antes de medir el caso de referencia; cualquier optimización futura deberá conservar los refcounts.
- Los structs serializados existentes no cambian; añadir `Update::PiTokens` es interno a TUI y aditivo.

## D. Success Criteria

- Los casos sintéticos de raíz vacía, bootstrap, archivo nuevo, append, cola parcial, truncado, reemplazo, borrado, corrupción, medianoche, duplicado, sesión EIN, grupos, error/aborted y datos inválidos satisfacen los escenarios de la sección B.
- Una prueba determinista con `PiFileStore` instrumentado demuestra que bootstrap puede leer todo el fixture, un refresco sin cambios no parsea contenido JSONL y un append parsea exactamente desde `safe_offset`; las únicas lecturas históricas permitidas son las ventanas fijas de fingerprint.
- Una prueba de persistencia inyecta fallos de escritura y rename y confirma que el snapshot en memoria sigue disponible y el índice final anterior continúa parseable.
- Una prueba TUI confirma que `Update::Status` se aplica durante un scan Pi lento, que `begin_pi_token_scan` no duplica workers y que la altura/tabla `Pi/EIN hoy` es independiente de Claude.
- Una prueba de privacidad confirma que índice, snapshot y diagnósticos no contienen content, prompts, respuestas, tools, credenciales ni cwd.
- Se registra el tamaño serializado del índice del fixture equivalente a 1.552 ids; el dato sirve de referencia y no constituye un límite artificial.
- La verificación posterior deberá ejecutar `cargo test`; esta fase no ejecuta tests ni build.
- Las pruebas de regresión confirman que `Status`, `collect_all()`, `status.json`, Claude/Codex y las salidas Waybar permanecen byte/semánticamente compatibles según sus contratos actuales.
