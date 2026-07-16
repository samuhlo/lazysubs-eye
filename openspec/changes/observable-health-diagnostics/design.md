# A. Proposal — diagnóstico de salud y observabilidad

## Intent

Hacer que lazysubs-eye sea diagnósticable sin necesidad de leer código ni usar
debuggers. Añadir: (1) `--check` semántico que distinga unavailable, stale,
partial y not_configured, (2) comando `doctor` que inspeccione el entorno
sin secretos, (3) validación semántica de config, (4) errores accionables
sin mensajes SQLite crudos, (5) documentación de exit codes.

## Spec

### R1. `--check` con estados diferenciados

`lazysubs-eye --check` **MUST** retornar exit codes diferenciados según el estado
real del sistema, no solo "todo bien" o "algo mal":

- **0 OK**: todos los providers están `Ready`, los datos son frescos y ningún
  umbral está en warning o critical.
- **1 warning**: existe un umbral warning o un estado recuperable como `Stale`
  o `Partial`.
- **2 critical**: existe un umbral critical.
- **3 error**: no hay providers configurados, la config es inválida o una
  fuente necesaria está `Unavailable`/`Error` sin datos utilizables.

**Rango: 0-3 SOLAMENTE.** No se introduce exit code 4. Si notify-send falla,
esto se registra en `doctor`, `--verbose` y stderr, pero no cambia el exit code.

El output de `--check` **MUST** incluir, para cada provider, su estado y
desde cuándo lo tiene, sin exponer credenciales ni paths.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Todos ready | Providers configurados y respondiendo | `lazysubs-eye --check` | Exit 0; output: "Claude: ready (2 min ago), Codex: ready (1 min ago)" |
| Uno unavailable | Claude retorna 401 sin datos utilizables | `lazysubs-eye --check` | Exit 3; output: "Claude: reauth required (since 5 min)" |
| Uno stale | Los datos de Claude superan el TTL | `lazysubs-eye --check` | Exit 1; output: "Claude: stale" |
| Umbral critical | Una ventana supera `critical_at` | `lazysubs-eye --check` | Exit 2 con la ventana afectada |
| Ningún provider | No hay `[[providers]]` en config | `lazysubs-eye --check` | Exit 3; output: "No providers configured" |

### R2. Comando `doctor` y `doctor --json`

`lazysubs-eye doctor` **MUST** ejecutar una batería de checks diagnósticos sin
exponer secretos. Los checks incluyen:

1. Config file exists y es parseable.
2. Providers configurados: qué providers, sin keys.
3. Caminos de datos: qué directorios existen, qué permisos tienen.
4. Binario: versión, fecha de compilación.
5. Database health: para Pi y OpenCode, si el cursor/offset es válido.
6. Permisos de archivos de lazysubs-eye.
7. Ultimo error conocido (de `meta`).
8. notify-send: disponibilidad y último fallo (visible, sin exit code 4).

`lazysubs-eye doctor --json` **MUST** devolver JSON estructurado con los mismos
datos para consumo por scripts.

**MUST NOT** exponer en el output: API keys, tokens, paths completos con
nombres de usuario, contenido de mensajes, credenciales.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Sistema sano | Todo funciona | `lazysubs-eye doctor` | Lista de checks en verde; exit 0 |
| Config corrupta | config.toml tiene toml inválido | `lazysubs-eye doctor` | Check de config en rojo con el error de parseo; exit 1 |
| notify-send fallando | notify-send no está en PATH | `lazysubs-eye doctor` | Check de notify-send en amarillo con mensaje; exit sin cambios (0-3 según estado) |
| Doctor JSON | Sistema en cualquier estado | `lazysubs-eye doctor --json` | JSON válido con todos los checks; exit según estado |

### R3. Validación semántica de config

La config **MUST** validarse semánticamente al cargar, no solo parsearse.
Los checks incluyen:

- `base_url` debe ser URL válida (si está presente).
- Thresholds: `warning` < `critical` (si ambos están presentes).
- `ttl` > 0.
- `history_days` > 0.
- Paths en config deben existir o ser creables.

Si la validación falla, **MUST** mostrarse un error accionable:
"Config error in ~/.config/lazysubs-eye/config.toml: base_url is not a valid URL"
— no "parse error at line 42".

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| URL inválida | `base_url = "not-a-url"` | Se carga config | Error: "E002: base_url must be a valid URL; found 'not-a-url'" |
| Thresholds invertidos | `warning = 80, critical = 50` | Se carga config | Error: "E002: warning (80) must be less than critical (50)" |

### R4. Errores accionables sin datos sensibles

Ningún mensaje de error **MUST** contener:
- Paths completos con nombres de usuario (reemplazar por `~`).
- Mensajes SQLite o rusqlite crudos.
- JSON de APIs remotas.
- API keys, tokens o credenciales.

Los errores **MUST** ser accionables: el usuario debe saber qué hacer
para resolverlos. Cada error **MUST** tener un código corto (v.g. `E001`)
y una descripción en español.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| DB no existe | OpenCode DB ausente | Se ejecuta collector | Error: "E041: OpenCode database not found. Set OPENCODE_DB or install OpenCode." |
| Permiso denegado | history.db tiene permisos 000 | Se intenta escribir | Error: "E042: Cannot write to history database. Check permissions on ~/.local/state/lazysubs-eye/" |

### R5. Notificaciones visibles

Las notificaciones de umbral (`notify-send`) **MUST** ser visibles y con
formato legible: título "Límite de AI", cuerpo con provider, umbral y valor
actual. Si `notify-send` no está disponible, **MUST** loguear warning con
`--verbose` y aparecer en `doctor`, pero **NO** fallar ni crear exit code 4.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| notify-send disponible | Se cruza un umbral | Se envía notificación | notify-send se ejecuta con título y cuerpo legibles |
| notify-send no disponible | `notify-send` no está en PATH | Se cruza un umbral | Log verbose: "E043: notify-send not found in PATH; notifications disabled"; visible en doctor |

### R6. `--verbose` y exit codes documentados

`--verbose` **MUST** emitir logs diagnósticos adicionales (no secretos) a stderr:
cada collector iniciado, cada checkpoint de cache, cada decisión de refresh.

Los exit codes **MUST** estar documentados en `--help`:

- 0: todo OK.
- 1: warning, stale o partial.
- 2: umbral critical.
- 3: error operativo, no configurado o config corrupta.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Verbose mode | Todo funciona | `lazysubs-eye --verbose` | stderr tiene logs de cada collector y decisión |
| Help con exit codes | Cualquier estado | `lazysubs-eye --help` | La salida incluye la sección de exit codes (0-3) |

## Decisions

1. **Exit codes no conflictivos con Unix**: se usan 0/1/2/3 (estándar).
   0 es el único "todo bien". **NO se introduce exit code 4.**
2. **Códigos de error cortos (E001...)**: permiten referenciar en docs y
   soporte sin exponer implementation details.
3. **Doctor --json**: la salida JSON no es para humanos, pero debe ser
   estructurada para consumo por scripts.
4. **notify-send failure no crea exit code 4**: se registra en doctor/verbose/stderr.
   La decisión de añadir exit code 4 específico para notify-send requiere
   decisión explícita futura.

## Success Criteria

- `--check` retorna 0/1/2/3 según el estado real.
- `doctor` muestra todos los checks relevantes sin secretos, incluyendo notify-send.
- Config inválida produce error accionable con código E001.
- Ningún error expone paths con nombres de usuario ni credenciales.
- Exit codes documentados en `--help` (0-3, sin 4).
- notify-send failure visible en doctor y `--verbose`, sin exit code nuevo.
- Tests demuestran cada exit code y cada tipo de error.

Compatibility decision: `stats.history_days = 0` remains the documented
"keep forever" policy from the existing history contract; semantic validation
rejects negative values rather than zero. This is the sole intentional
exception to the early `> 0` wording in the task packet.
