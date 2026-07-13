# Mapa: uso diario de tokens de Pi/EIN

status: complete
scope_status: bounded
change: pi-daily-token-usage
phase: map
skill_resolution: paths-injected
budget_source: revisión directa de la tarea
budget_exceeded: false

> Revisión cerrada: se conserva el alcance del mapa anterior y se incorporan los hechos de runtime/sesión y las mediciones locales verificadas entregadas para esta revisión. No se volvió a inspeccionar contenido de sesiones, prompts ni herramientas.

## Resultado y límites

La función debe ser un colector local independiente: recorre recursivamente `~/.pi/agent/sessions/` por defecto (o el directorio alternativo configurado mediante `--session-dir`), indexa entradas `type:"message"` cuyo `message.role == "assistant"`, y entrega a la TUI un agregado del día local por `(provider, model)`. `--no-session` implica que no existe un log recuperable: el colector debe devolver vacío/parcial sin intentar inferir el uso.

No es un provider remoto y no debe modificar `Status`, `collect_all()`, Waybar ni la tabla Claude existente. La primera construcción puede leer los ~176 MB medidos; los ciclos posteriores deben descubrir metadatos de todos los ficheros, pero leer contenido solo desde cursores seguros, y reconstruir únicamente ficheros afectados. Se incluyen los ficheros anidados de subagentes EIN, por ejemplo `run-*/session.jsonl`: tienen el mismo contrato válido de header y entradas, no una fuente especial.

## Superficies verificadas

| Archivo/símbolo | Estado actual | Implicación para Pi |
|---|---|---|
| `src/tokens.rs` / `ModelTokens`, `claude_today()` | Agrega JSONL de Claude por modelo; filtra por mtime y timestamp RFC3339, sin persistencia incremental. | Mantenerlo sin alteración funcional. No reutilizar su modelo: Pi tiene `provider`, coste, `totalTokens` y contrato de timestamp distinto. Sus structs privados y el escaneo directo son un antecedente, no una API. |
| `src/tokens.rs` / `fmt_count(u64)` | Formatea conteos compactos. | Reutilizable para campos enteros Pi si se expone desde el módulo existente o se mueve de forma compatible; el coste necesita formato propio, no conversión a `u64`. |
| `src/tui.rs` / `Update`, `App::refresh`, `begin_token_scan` | Un solo `mpsc` tipado transporta `Status` y `Tokens`; flags separados `refreshing`/`tokens_scanning` y dos hilos evitan que el collector remoto espere el scan Claude. | Añadir una tercera variante/flujo Pi y un flag Pi separado. No condicionar el envío/aplicación de `Update::Status` a que Pi o Claude sigan indexando. El test de hotfix ya fija esa independencia lógica. |
| `src/tui.rs` / `draw`, `draw_tokens` | La tabla Claude se muestra solo si hay filas; calcula su propia altura y usa TUI compacta. | Crear un renderer/sección Pi separado, con título `Pi/EIN hoy`, columnas provider/model, in/out/cache read/cache write/total/coste, y altura independiente. No fusionar filas Pi con `tokens`. |
| `src/tui.rs` tests | Prueban aplicación de estado durante scan de tokens y prevención de scans Claude duplicados. | Extender con la misma propiedad para Pi, además de pruebas puras del agregado fuera de Ratatui. |
| `src/cache.rs` / `cache_file`, `load`, `save` | `status.json` contiene exclusivamente `Status`; escritura directa `std::fs::write`, no atómica; `cache_file()` es privada. | El índice Pi debe vivir en un archivo separado bajo el mismo directorio XDG, para que una caché histórica de `Status` siga deserializando igual. Extraer una utilidad de directorio/ruta o añadir una ruta hermana privada. |
| `src/main.rs` | Declara módulos y los modos no-TUI solo consultan/serializan `Status`. | Declarar el módulo Pi nuevo. No cargar el índice para JSON/Waybar salvo que una decisión posterior lo pida: el alcance exige visibilidad TUI y preserva el contrato de salidas. |
| `src/providers/mod.rs` / `Status` | Tipo serializado de cuotas remotas con compatibilidad aditiva ya comprobada. | No incluir Pi en `Status.providers`: rompería la semántica de cuotas y afectaría JSON/Waybar. |
| `Cargo.toml` | Ya dispone de `serde`, `serde_json`, `chrono`, `anyhow`, `ratatui`. | No se justifica dependencia nueva: `std::fs`, `BufReader`, `Seek`, `OpenOptions`, `rename`, `Metadata`, `HashMap`/`HashSet`, serde y chrono cubren el caso. |

## Contrato JSONL Pi confirmado

- La primera línea de cada fichero válido es `{type:"session", version:3, id, timestamp ISO, cwd, parentSession?}`. Es un header, no una entrada de uso.
- Una entrada contable tiene el sobre `{type:"message", id, parentId, timestamp ISO, message:{...}}`. El `id` estable de este sobre es la clave global de deduplicación.
- Solo se cuenta `message.role == "assistant"`. Sus campos relevantes son `provider`, `model`, `usage` y su `timestamp` Unix en milisegundos.
- `usage` contiene `input`, `output`, `cacheRead`, `cacheWrite`, `totalTokens` y `cost:{input,output,cacheRead,cacheWrite,total}`. La fecha contable se obtiene exclusivamente de `message.timestamp` convertido a `DateTime<Local>` y comparado con el día local; el timestamp ISO del sobre y el mtime no sustituyen esa regla.
- Las sesiones son logs en árbol. `/tree` puede ramificar en el mismo fichero; `/fork` y `/clone` pueden copiar historia a otro fichero. Por ello se deduplica globalmente por `message-entry.id`, nunca por ruta, sesión, offset o `(provider, model)`.
- La deduplicación abarca todos los ficheros hallados recursivamente, incluidos `run-*/session.jsonl` de subagentes EIN.

## Mediciones locales verificadas y privacidad

Una lectura de solo metadatos/uso (sin retener ni emitir prompts o contenido de tools) confirmó:

| Medida | Valor |
|---|---:|
| JSONL de sesión válidos | 713 |
| Tamaño total | 176.341.596 bytes |
| Líneas malformadas en la muestra | 0 |
| Ficheros modificados hoy | 86 |
| Tamaño de esos ficheros | 20.948.770 bytes |
| Entradas assistant con uso hoy | 1.552 |
| IDs estables únicos en la muestra actual | 1.552 |

El agregado de hoy contiene varios providers y modelos, y valores no nulos para input, output, cacheRead, cacheWrite y coste. Estos datos confirman que la agrupación y el almacenamiento de todos los campos de uso son necesarios; no autorizan almacenar contenido de mensajes, prompts, resultados de herramientas, credenciales ni cwd.

## Propuesta concreta de índice diario (nivel mapa)

Persistir un JSON separado y versionado, por ejemplo `pi-daily-token-index-v1.json`, en el directorio XDG de caché ya usado por la aplicación. Su forma propuesta es:

```text
DailyPiIndexV1 {
  schema_version: 1,
  local_date: "YYYY-MM-DD",
  timezone_offset_seconds: i32,
  files: Map<ruta_normalizada, FileState>,
  entries: Map<entry_id, EntryState>,
  groups: Map<(provider, model), UsageTotals>
}

FileState {
  identity: { dev: u64, ino: u64 },       // Linux; diseño decide fallback portable
  path: String,
  size: u64,
  mtime_ns: i128,
  header_fingerprint: String,
  safe_offset: u64,
  entry_ids: Set<entry_id>
}

EntryState {
  contribution: { provider, model, UsageTotals },
  source_paths: Set<ruta_normalizada>
}

UsageTotals {
  input: u64, output: u64, cache_read: u64, cache_write: u64, total_tokens: u64,
  cost_input: f64, cost_output: f64, cost_cache_read: f64,
  cost_cache_write: f64, cost_total: f64
}
```

`entry_ids` permite retirar todas las procedencias de un fichero que se reconstruye o desaparece. `source_paths` es el refcount explícito: la contribución solo entra en `groups` al ver por primera vez su id, y solo sale cuando se retira su última procedencia. Así una copia fork/clone no duplica gasto, y borrar una de varias copias no descuenta gasto todavía. `groups` es derivable de `entries`, pero se persiste para lectura rápida y se reconstruye desde `entries` si falla su validación.

El parser usa structs parciales serde e ignora por defecto campos no necesarios. Una entrada se admite solo con id, provider, model, timestamp ms válido, los cinco contadores enteros no negativos y los cinco costes finitos no negativos. No recalcula precios: suma los valores `cost.*` entregados. El diseño debe usar el coste total agregado para la columna visible y conservar el desglose para consistencia y diagnósticos de caché. Como el contrato no declara divisa, la TUI rotulará el campo neutralmente como `coste` y lo mostrará con precisión fija decidida abajo, sin símbolo de moneda.

## Reglas incrementales y de recuperación

| Situación | Regla de índice |
|---|---|
| Línea completa | Una línea solo está consumida cuando termina en `\n`. Tras parsearla o descartarla por ser malformada/no contable, avanzar `safe_offset` al byte posterior a ese salto. |
| Última línea parcial | No avanzar el cursor ni registrar su id; releerla en el siguiente ciclo. Esto evita pérdida o doble conteo. |
| Append compatible | Si identidad, header fingerprint y tamaño siguen siendo compatibles, hacer `seek(safe_offset)` y procesar únicamente líneas completas del sufijo. Actualizar tamaño, mtime y cursor al final. |
| Nuevo fichero | Validar header v3, crear `FileState` con cursor cero y recorrerlo completo una vez. Incluye cualquier fichero JSONL recursivo, incluidos `run-*/session.jsonl`. |
| Truncado | Si `size < safe_offset`, retirar primero todas sus procedencias mediante `entry_ids`, borrar/recrear su `FileState` y reprocesar desde cero. |
| Sustitución | Si cambian `dev/ino` o el fingerprint de header, retirar las procedencias antiguas y reprocesar el fichero completo, incluso si la longitud nueva es igual o mayor. |
| Mutación in situ no detectable por identidad | Se conserva un fingerprint acotado del header y de la región de cursor para detectar reescrituras comunes. La garantía adicional ante una reescritura histórica con mismo inode, header y tamaño queda fijada como decisión de diseño D2; no se confunde con un hueco de investigación. |
| Fichero eliminado o carrera de borrado | Al descubrir que una ruta indexada ya no existe, retirar sus procedencias y estado. Si desaparece durante apertura/lectura, aislar el error y conservar el último estado hasta que el siguiente descubrimiento confirme su ausencia. |
| Línea JSON malformada cerrada | Omitirla, no registrar contenido ni error sensible, y avanzar el cursor porque la línea está completa. Un fichero defectuoso no aborta el agregado global. |
| Índice ausente, corrupto, versión desconocida o día/zona distintos | Descartar el índice Pi completo y hacer bootstrap seguro. No tocar `status.json`. |
| Medianoche/cambio de zona | Si `local_date` o el offset de zona ya no representa el día actual, descartar el índice diario y reconstruir el agregado para el nuevo día local; no reutilizar IDs/totales del día anterior. |
| Copias de historia | Para cada id, añadir la ruta a `source_paths`; si ya existe, no sumar de nuevo `contribution`. Al reconstruir/eliminar un fichero, retirar solo esa procedencia y restar del grupo únicamente al llegar a cero fuentes. |

Persistencia: serializar primero a un temporal único en el mismo directorio, escribirlo completamente, sincronizar el fichero y renombrarlo sobre el destino final. En Unix, el diseño debe sincronizar también el directorio cuando la plataforma lo permita. Una falla de persistencia deja el resultado en memoria utilizable y fuerza bootstrap/reconciliación posterior; no bloquea el render ni sobrescribe directamente el archivo final.

## Decisiones acotadas para `sdd-design`

No quedan investigaciones abiertas. El diseño debe seleccionar y documentar estas alternativas ya delimitadas:

1. **D1 — identidad fuera de Linux:** usar `(dev, ino)` en Linux y, donde no exista, degradar a ruta + tamaño + mtime + fingerprint, sin dependencia nueva.
2. **D2 — reescritura histórica con mismo inode:** elegir entre (a) aceptar el contrato append-only de Pi tras comprobar identidad/header/cursor, con detección acotada por fingerprint, o (b) rehash/reparsear el fichero modificado completo para máxima exactitud. Debe declarar el coste de I/O; sustitución normal, truncado y header cambiado ya se reconstruyen obligatoriamente.
3. **D3 — coste:** aceptar únicamente números finitos no negativos, sumar el desglose documentado y mostrar `cost_total` neutral, sin símbolo de moneda, con cuatro decimales y recorte visual si falta ancho.
4. **D4 — error de un fichero:** devolver agregado parcial y diagnóstico interno no sensible; no convertir permisos, directorio ausente o carreras en error global de TUI.
5. **D5 — caché grande:** no imponer límite artificial antes de medir. El diseño debe registrar el tamaño real del índice de la referencia de 1.552 ids y, solo si es material, proponer compactación compatible sin sacrificar refcounts.

## Archivos probables y seams de prueba

| Acción probable | Archivo |
|---|---|
| Módulo de descubrimiento, parseo parcial, índice diario, persistencia y agregado Pi con APIs inyectables para reloj, raíz y operaciones de archivos | **crear** `src/pi_tokens.rs` (nombre a confirmar en design) |
| Declarar módulo | `src/main.rs` |
| Ruta XDG compartida y/o helpers de guardado atómico para el índice Pi separado | `src/cache.rs` |
| Estado Pi, variante `Update`, flag de scan, spawn desacoplado, layout y renderer | `src/tui.rs` |
| Tests unitarios junto al módulo Pi y extensión de hotfix TUI | `src/pi_tokens.rs`, `src/tui.rs` |

Los seams deben recibir raíz, fecha/reloj local, directorio temporal y una capa de operaciones de fichero inyectable; no dependen de `$HOME`, reloj real ni sesiones reales. Los fixtures JSONL deben contener solo header/metadatos/uso sintéticos, nunca prompts ni contenido de tool.

| Fixture o seam exacto | Propiedad a fijar antes de aplicar |
|---|---|
| `valid.jsonl` con header v3 y assistants de dos `(provider, model)` | Parseo parcial, conversión de timestamp ms a día local y suma de todos los campos de uso/coste. |
| Árbol `sessions/.../run-123/session.jsonl` | Descubrimiento recursivo e inclusión de subagente EIN. |
| `append.jsonl` escrito en dos pasos | Cursor tras líneas completas, lectura solo del sufijo y relectura de cola sin `\n`. |
| Dos archivos fork/clone con el mismo `entry.id` | Un único gasto y dos `source_paths`. |
| Borrado de una copia y después de la última copia | Refcount: conservar, luego retirar contribución. |
| `truncated.jsonl` y reemplazo de ruta con identidad/header distintos, incluso longitud igual o mayor | Retirada de contribuciones antiguas y reconstrucción correcta. |
| Línea cerrada JSON inválida y entrada con tipos/valores inválidos | Omisión aislada y avance de cursor solo para la línea cerrada. |
| Raíz ausente, permiso denegado y fichero que desaparece durante lectura | Resultado vacío/parcial sin fallo global. |
| Índice v0/corrupto, temporal de escritura fallido y rename inyectado fallido | Bootstrap seguro; el estado en memoria sigue renderizable; no se toca `status.json`. |
| Reloj antes/después de medianoche y offset de zona cambiado | Reset diario y ausencia de arrastre de ids/totales. |
| Prueba de estado TUI con scan Pi lento | `Update::Status` se aplica mientras Pi indexa; no se lanzan scans Pi duplicados; sección Pi tiene altura independiente. |

## Límites de privacidad y tolerancia

El índice no persistirá `content`, prompts, respuestas, argumentos/resultados de tools, credenciales ni cwd. Roles no assistant, entradas sin campos requeridos, JSON malformado y colas parciales se omiten. Directorios vacíos, permisos y errores por fichero se aíslan para que el hilo de render permanezca disponible.

## Siguiente fase

Ejecutar `sdd-design` para elegir D2, concretar las APIs internas y el contrato visual, y convertir estos fixtures en especificación de pruebas antes de implementar.

## Ledger

ledger:
  reads:
    - { path: "/home/samuhlo/.pi/agent/skills/local/ein-discipline/SKILL.md", lines: 101, estimated_tokens: 1350 }
    - { path: "/home/samuhlo/.pi/agent/skills/local/cognitive-doc-design/SKILL.md", lines: 75, estimated_tokens: 750 }
    - { path: "/home/samuhlo/.pi/agent/skills/local/architecture/SKILL.md", lines: 130, estimated_tokens: 2050 }
    - { path: "openspec/changes/pi-daily-token-usage/map.md (revisión)", lines: 177, estimated_tokens: 3600 }
  webfetch_used: false
  budget_consumed: { tokens: 7750, reads: 4 }
