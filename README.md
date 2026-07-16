# lazysubs-eye

[![CI](https://github.com/samuhlo/lazysubs-eye/actions/workflows/ci.yml/badge.svg)](https://github.com/samuhlo/lazysubs-eye/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/samuhlo/lazysubs-eye)](https://github.com/samuhlo/lazysubs-eye/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Monitor de cuotas y consumo de suscripciones de IA para Linux. Reúne las
ventanas de uso de **Claude Code**, **Codex** y **MiniMax** en Waybar y en una
TUI estilo lazygit, y conserva el historial local de tokens de Claude, Pi y
OpenCode.

Está especialmente pulido para [Omarchy](https://omarchy.org), pero no depende
de él: la TUI funciona en cualquier terminal y la integración automática se
adapta a instalaciones genéricas de Waybar y Hyprland.

```text
 lazysubs-eye · cuotas de IA
╭ ✳ Claude Code ─ pro ──────────────────────────────────────────────────────────╮
│ [✓] 5h             73% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━      → 3h06m │
│ [✓] semana         36% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━      → 5d21h │
╰───────────────────────────────────────────────────────────────────────────────╯
╭ ⬡ Codex ─ plus ───────────────────────────────────────────────────────────────╮
│ [!] semana         80% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━      → 5d23h │
│ Créditos de reinicio disponibles: 4                                          │
╰───────────────────────────────────────────────────────────────────────────────╯
╭ ✳ tokens Claude · hoy ────────────────────────────────────────────────────────╮
│ modelo                         in       out      cache→   cache+       total  │
│ claude-fable-5                 17.7k    30.2k    777.9k   158.7k      984.5k  │
╰───────────────────────────────────────────────────────────────────────────────╯
 q salir  r refrescar  o opciones  j/k scroll                         hace 59s
```

## Qué ofrece

- Cuotas en vivo, tiempo hasta el reset, plan y cuenta de cada provider.
- Salida JSON estable para Waybar, scripts y automatizaciones.
- TUI adaptable con scroll, ayuda, ajustes en vivo y fallbacks sin color/UTF-8.
- Tokens por modelo de Claude, Pi y OpenCode para hoy, semana o mes.
- Historial SQLite, sparklines y gráfica combinada semanal, mensual o por hora.
- Multi-cuenta por provider con orden y visibilidad independientes por superficie.
- Notificaciones de escritorio al cruzar umbrales, con cooldown y escalado.
- Caché rápida y degradación a datos anteriores ante fallos transitorios.
- Diagnósticos accionables mediante `--check`, `doctor` y `--verbose`.
- Persistencia privada y atómica, locks multiproceso y backups con rollback.
- Instalación idempotente en Waybar/Hyprland, con dry-run y sandbox.

Todo el procesamiento del historial se hace en local. Solo se consulta la API
oficial de cada provider con las credenciales que ya utiliza su CLI.

## Inicio rápido

### 1. Instalar

La versión estable para **Linux x86_64** está en
[GitHub Releases](https://github.com/samuhlo/lazysubs-eye/releases/latest) como
binario estático MUSL, acompañado de su SHA-256.

También puedes compilar desde el repositorio:

```bash
git clone https://github.com/samuhlo/lazysubs-eye.git
cd lazysubs-eye
cargo install --path . --locked
```

El repositorio incluye además un [`PKGBUILD`](packaging/aur/PKGBUILD) para
empaquetado Arch Linux.

### 2. Comprobar el entorno

```bash
lazysubs-eye --version
lazysubs-eye doctor
```

Claude y Codex se autodetectan a partir de sus sesiones existentes. Para
MiniMax, define `MINIMAX_API_KEY` o configura `[minimax].api_key`.

### 3. Abrir la TUI

```bash
lazysubs-eye tui
```

### 4. Integrar Waybar

Inspecciona primero el plan y aplícalo cuando estés conforme:

```bash
lazysubs-eye install --dry-run
lazysubs-eye install
```

La instalación localiza el binario real, modifica únicamente los archivos
necesarios, crea backups y recarga Waybar. Es segura al repetirla.

## Fuentes de datos

### Cuotas de suscripción

| Provider | Fuente | Requisito |
|---|---|---|
| Claude Code | `api.anthropic.com/api/oauth/usage` | Sesión iniciada en Claude Code |
| Codex | `codex app-server`, método `account/rateLimits/read` | `codex login` |
| MiniMax | `GET /v1/token_plan/remains` | Subscription Key en config o entorno |

Codex muestra también los créditos de reinicio cuando el servidor los incluye.
lazysubs-eye no renueva tokens OAuth: cada CLI conserva esa responsabilidad.

### Consumo local de tokens

| Panel | Fuente local |
|---|---|
| Claude tokens | `~/.claude/projects/**/*.jsonl` |
| Pi tokens | `~/.pi/agent/sessions/**/*.jsonl` |
| OpenCode tokens | Base SQLite bajo `~/.local/state/opencode` |

Los lectores usan cursores, fingerprints y cutoffs consistentes para evitar
releer todo en cada refresco o contar datos añadidos a mitad de un scan.

## Comandos

```text
lazysubs-eye                 TUI con stdout interactivo; JSON sin TTY
lazysubs-eye tui             abre explícitamente la TUI
lazysubs-eye --waybar        JSON de una línea para Waybar
lazysubs-eye --json          estado completo en JSON
lazysubs-eye --check         resumen y exit code operativo
lazysubs-eye doctor          comprueba configuración y dependencias
lazysubs-eye doctor --json   diagnóstico estructurado
lazysubs-eye --verbose       decisiones de caché/collectors en stderr
lazysubs-eye --no-cache      fuerza una lectura fresca
lazysubs-eye --ttl 120       cambia el TTL para esta ejecución
lazysubs-eye install         integra Waybar y, si existe, Hyprland
lazysubs-eye install --dry-run
lazysubs-eye install --sandbox /tmp/lazysubs-config
lazysubs-eye uninstall       revierte la integración propia
```

`--verbose` no altera stdout, por lo que puede combinarse con `--json` o
`--waybar` sin romper consumidores.

### Exit codes de `--check`

| Código | Significado |
|---:|---|
| `0` | Datos disponibles, frescos y sin umbrales activos |
| `1` | Warning, datos stale o ingesta parcial |
| `2` | Alguna ventana ha alcanzado el umbral crítico |
| `3` | Error operativo, configuración inválida o ningún provider disponible |

Ejemplo para un hook previo a una sesión larga:

```bash
if ! lazysubs-eye --check; then
  echo "Revisa tus cuotas antes de continuar"
fi
```

## Controles de la TUI

| Tecla | Acción |
|---|---|
| `q`, `Esc`, `Ctrl+C` | Salir o cerrar el panel actual |
| `r` | Refresco forzado |
| `j` / `k`, `↓` / `↑` | Scroll o navegación |
| `o` | Abrir ajustes |
| `Espacio`, `Enter` | Activar/desactivar un ajuste |
| `h` / `l`, `←` / `→` | Reducir/aumentar un valor |
| `t`, `Tab` | Ciclar hoy → semana → mes |
| `g` | Abrir/cerrar la gráfica de gasto |
| `v`, `←`, `→` | Cambiar la vista de la gráfica |
| `?` | Mostrar ayuda |

La interfaz recomienda un terminal de al menos 80×24. En tamaños inferiores
muestra una pantalla compacta en vez de romper el layout. Respeta `NO_COLOR`,
`FORCE_COLOR`, `TERM=dumb` y el locale; los estados nunca dependen solo del
color.

## Configuración

El archivo es opcional y vive en
`$XDG_CONFIG_HOME/lazysubs-eye/config.toml` o
`~/.config/lazysubs-eye/config.toml`.

```toml
ttl = 60
warning_at = 80.0
critical_at = 95.0
notifications = true
notification_cooldown = 1800
colors = true
show_account = true

[providers]
claude = true
codex = true
minimax = true

[waybar]
providers = ["claude", "codex"]
percent = true

[waybar.window]
claude = "semana"
codex = "semana"

[tui]
providers = ["claude", "codex", "minimax"]
panels = ["claude_tokens", "pi_tokens", "opencode_tokens"]

[stats]
enabled = true
default_period = "hoy" # hoy | semana | mes
history_days = 90      # 0 conserva todo
sparkline = true

[icons]
claude = "✳"
codex = "⬡"
minimax = "◆"

[minimax]
api_key = "..." # o MINIMAX_API_KEY
# base_url = "https://api.minimaxi.com"
```

Los ajustes guardados desde la TUI conservan comentarios y claves que el panel
no administra. Una configuración inválida se informa en diagnóstico y produce
exit `3` en `--check`; los modos de presentación se degradan a defaults seguros.

### Varias cuentas

Sin bloques `accounts`, cada provider usa su cuenta autodetectada. Para añadir
más cuentas:

```toml
[[accounts.claude]]
name = "personal"

[[accounts.claude]]
name = "trabajo"
credentials = "~/trabajo/.claude/.credentials.json"
icon = "❄"

[[accounts.codex]]
name = "personal"
codex_home = "~/.codex"

[[accounts.minimax]]
name = "personal"
api_key = "..."
```

Las cuentas adicionales reciben ids como `claude:trabajo`. Estos ids se pueden
usar en las listas `waybar.providers` y `tui.providers` para controlar orden y
visibilidad por separado.

## Historial y rendimiento

El primer arranque reconstruye en segundo plano los días que aún existan en las
fuentes. La TUI sigue siendo interactiva, muestra el progreso y puede cerrarse
sin perder días ya confirmados. La próxima ejecución continúa desde el último
día contiguo.

La ingesta de cada fuente y día es transaccional. Un fallo no reemplaza datos
buenos anteriores, y los estados `Partial` y `Failed` quedan visibles para la
TUI y `--check`.

El runtime está diseñado para el polling frecuente de Waybar:

- La caché válida evita llamadas de red; el camino habitual tarda unos pocos ms.
- Las familias de providers se consultan en paralelo con un budget global.
- Los refreshes solapados se agrupan en vez de crear workers duplicados.
- Pi procesa únicamente el sufijo nuevo de cada archivo estable.
- SQLite se recorre en lotes acotados para no cargar historiales enteros en RAM.

Los budgets documentados están en [`perf/baseline.json`](perf/baseline.json) y
se verifican con `scripts/benchmarks/run_budgets.sh`.

## Waybar e instalación segura

`lazysubs-eye install`:

1. Ejecuta un preflight completo antes de escribir.
2. Añade `custom/ai-usage` a Waybar y CSS neutral al tema.
3. Añade la regla flotante solo cuando detecta Hyprland.
4. Delimita su contenido con marcadores propios.
5. Crea backups `.bak.<epoch>` y hace rollback si una fase falla.
6. Recarga Waybar únicamente después de confirmar todos los cambios.

En Linux sin Omarchy busca `config.jsonc` o `config`, usa
`xdg-terminal-exec` o un terminal conocido y omite integraciones que no
apliquen. En Sway/river debes añadir manualmente la regla flotante equivalente.

Para probar sin tocar el host:

```bash
lazysubs-eye install --sandbox /tmp/lazysubs-config --dry-run
lazysubs-eye install --sandbox /tmp/lazysubs-config
```

El sandbox se trata como raíz XDG aislada y nunca recarga servicios reales.

### Integración manual

Si prefieres configurar Waybar a mano:

```jsonc
"custom/ai-usage": {
  "exec": "$HOME/.local/bin/lazysubs-eye --waybar",
  "return-type": "json",
  "interval": 60,
  "signal": 11,
  "on-click": "xdg-terminal-exec lazysubs-eye tui",
  "on-click-right": "$HOME/.local/bin/lazysubs-eye --no-cache --waybar >/dev/null && pkill -RTMIN+11 waybar"
}
```

Clases CSS emitidas: `normal`, `warning`, `critical` y `error`.

## Privacidad y seguridad local

- Las credenciales no se escriben en cachés, índices ni logs.
- Los errores se sanean antes de mostrarse o persistirse.
- Archivos privados: modo `0600`; directorios privados: `0700`.
- Las escrituras usan temp privado, `fsync`, rename atómico y sync del directorio.
- Se rechazan destinos y padres symlink para evitar redirecciones inesperadas.
- `flock` coordina escritores y se verifica el inode antes del commit.
- La base de historial se crea privada antes de abrir SQLite.

Los datos viven en rutas XDG:

| Contenido | Ruta |
|---|---|
| Configuración | `$XDG_CONFIG_HOME/lazysubs-eye/config.toml` |
| Caché de providers | `$XDG_CACHE_HOME/lazysubs-eye/status.json` |
| Estado de notificaciones | `$XDG_CACHE_HOME/lazysubs-eye/notify-state.json` |
| Último error saneado | `$XDG_CACHE_HOME/lazysubs-eye/last-error.json` |
| Historial | `$XDG_STATE_HOME/lazysubs-eye/history.db` |

Sin variables XDG se utilizan los equivalentes bajo `~/.config`, `~/.cache` y
`~/.local/state`.

## Diagnóstico

Empieza por:

```bash
lazysubs-eye doctor
lazysubs-eye --check
lazysubs-eye --verbose --no-cache --json >/tmp/lazysubs-status.json
```

`doctor` revisa configuración, providers, rutas, permisos, binario, base de
historial, índices incrementales, `notify-send` y el último error registrado.
Sus mensajes usan códigos estables `E001`–`E008` y evitan rutas privadas,
credenciales, URLs sensibles y detalles internos de SQLite.

Cuando una consulta fresca falla, lazysubs-eye conserva temporalmente el último
estado bueno como `stale` en lugar de vaciar el panel.

## Compatibilidad

- Release soportado y probado: **x86_64 Linux**, binario estático MUSL.
- Terminal: cualquier emulador con ANSI; color y UTF-8 tienen fallback.
- Waybar: opcional; solo es necesario para la integración en la barra.
- Hyprland/Omarchy: opcionales, con integración automática cuando existen.
- aarch64: todavía no se anuncia como soportado al no tener un job verificado.

## Desarrollo

Requiere Rust estable:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
scripts/check-release-readiness.sh
scripts/benchmarks/run_budgets.sh
```

La CI exige todos los gates anteriores. Los tags construyen el artefacto MUSL,
ejecutan audit RustSec, comprueban la versión, pasan smoke tests sobre el binario
final y publican tarball + SHA-256.

Consulta también:

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — flujo y criterios para pull requests.
- [`SECURITY.md`](SECURITY.md) — reporte responsable de vulnerabilidades.
- [`CHANGELOG.md`](CHANGELOG.md) — cambios por versión.
- [`docs/ARQUITECTURA.md`](docs/ARQUITECTURA.md) — arquitectura y fuentes.
- [`docs/ESTADO.md`](docs/ESTADO.md) — estado y decisiones del proyecto.

## Licencia

[MIT](LICENSE) © Samuel López (`samuhlo`).
