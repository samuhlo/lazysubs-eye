# Arquitectura de lazysubs-eye

Monitor de cuotas de suscripciones de IA para Omarchy (Arch + Hyprland + waybar),
inspirado en [CodexBar](https://github.com/steipete/CodexBar) (macOS). Un único
binario Rust con tres modos: TUI estilo lazygit, salida para waybar y dump JSON.

## Estructura del código

```
src/
├── main.rs            # parseo de args y despacho de modos
├── providers/
│   ├── mod.rs         # modelo de datos (Status/ProviderStatus/Window) + collect_all()
│   ├── claude.rs      # collector de Claude Code (HTTP al endpoint OAuth)
│   ├── codex.rs       # collector de Codex (JSON-RPC a `codex app-server`)
│   └── minimax.rs     # collector de MiniMax (token plan; requiere api_key)
├── tokens.rs          # tokens de hoy por modelo (parseo de JSONL locales de Claude)
├── cache.rs           # cache JSON con TTL en ~/.cache/lazysubs-eye/status.json
├── history.rs         # historial de gasto por día (SQLite en XDG_STATE_HOME)
├── config.rs          # config opcional (~/.config/lazysubs-eye/config.toml)
├── notify.rs          # notificaciones de umbral vía notify-send con anti-spam
├── output.rs          # render --waybar y --json + countdown() + umbrales de color
├── install.rs         # subcomandos install/uninstall (waybar + hyprland)
└── tui.rs             # TUI ratatui (gauges, countdowns, tabla de tokens)
```

### Config (config.rs)

TOML opcional en `~/.config/lazysubs-eye/config.toml`: `ttl`, `warning_at`,
`critical_at`, `notifications`, `[providers]` (on/off) e `[icons]`
(overrides). Global vía `config::get()` (OnceLock, una sola lectura); en
tests devuelve siempre los defaults para que la config del usuario no afecte.
Config inválida → aviso por stderr + defaults, nunca rompe el output. Los
umbrales alimentan la clase CSS de waybar, los colores de gauge de la TUI,
`--check` y las notificaciones (única fuente: ya no hay literales duplicados).

### Notificaciones (notify.rs)

Tras cada consulta fresca (waybar corre el binario sin estado cada 60s) se
compara cada ventana con los umbrales y se lanza `notify-send` (urgency
normal/critical) solo al **subir** de nivel. El último nivel notificado por
ventana persiste en `~/.cache/lazysubs-eye/notify-state.json`; se rearma si
la ventana cambia de `resets_at` (reset) o baja del umbral. Un provider en
error conserva su estado previo (no re-notifica al recuperarse). La lógica
está en `plan()` (pura, testada); el envío en `send()`.

### Historial de gasto (history.rs)

Base SQLite en `~/.local/state/lazysubs-eye/history.db` (`XDG_STATE_HOME`, NO en
la cache borrable). Una tabla `daily_usage` con PK `(date, source, provider,
model)` y una tabla `meta`. Cada escaneo de "hoy" hace un **upsert autoritativo**
(`record_day` = delete + insert de las filas de ese día y fuente), así el
historial sobrevive aunque los JSONL/DB de origen se poden. Ingesta desde dos
puntos: el camino fresco de main (`ingest_today`, escanea las tres fuentes en
cada cache-miss de waybar) y la TUI (desde los resultados que ya recibe por
canal, sin re-escanear). La primera vez, `maybe_backfill` puebla los días
pasados desde las fuentes que aún existan (`claude_by_day` / `scan_pi_all_days`
/ `scan_opencode_all_days`, escaneos completos one-shot) y marca `backfill_v1`
en `meta`. Retención: `prune` borra lo más viejo que `history_days` al ingerir.
La capa de base (init/record/query/series/prune/meta) opera sobre `&Connection`
y se testea con `Connection::open_in_memory`; los puntos de entrada abren la
base real y **nunca rompen el flujo** (ante error → vacío). Config en `[stats]`
(`enabled`, `default_period`, `history_days`, `sparkline`). En la TUI, `t`/Tab
cicla el periodo de los paneles de tokens (hoy/semana/mes) y hay un `Sparkline`
del total diario de los últimos 14 días bajo cada panel.

### --check (main.rs)

Para scripts/hooks: imprime una línea por hallazgo y sale con el peor nivel:
0 ok · 1 warning · 2 critical · 3 error de provider (prioridad por máximo,
como la clase de waybar). Usa la cache como el resto de modos.

### install / uninstall (install.rs)

`lazysubs-eye install [--signal N]` integra el módulo en el sistema editando los
configs **por texto** (nunca se reserializa el JSONC: se conservan comentarios
y formato del usuario):

- `~/.config/waybar/config.jsonc`: entrada `"custom/ai-usage"` al principio de
  `modules-right` + definición del módulo tras el array.
- `~/.config/waybar/style.css`: bloque `#custom-ai-usage` neutro (usa
  `alpha(@foreground, …)` del tema activo; warning/critical con hex propios
  porque los temas de Omarchy solo exponen foreground/background).
- `~/.config/hypr/hyprland.conf`: windowrule de la ventana flotante.

Todo lo insertado va delimitado con marcadores `lazysubs-eye-begin`/`lazysubs-eye-end`
(o `// lazysubs-eye` en líneas sueltas); `uninstall` elimina exactamente eso y
restaura el fichero byte a byte (hay test de round-trip). Ambos comandos son
idempotentes, hacen backup `.bak.<epoch>` antes de escribir y recargan con
`omarchy restart waybar` + `hyprctl reload` (+ `configerrors`). Si el config
de waybar no tiene la estructura esperada, se imprime el snippet para
instalación manual en vez de romper nada. Respeta `XDG_CONFIG_HOME` (útil
para probar contra un directorio sandbox).

### Modos (main.rs)

- Sin args + stdout es tty → **TUI**. Sin tty → `--json`. Así el mismo binario
  sirve para waybar (pipe) y para uso interactivo.
- `tui` / `--tui`, `install`, `uninstall`, `--json`, `--waybar`, `--no-cache`,
  `--ttl N` (defecto 60), `--signal N` (defecto 11), `--version`.

### Modelo de datos (providers/mod.rs)

`Status { fetched_at, providers: Vec<ProviderStatus> }`
`ProviderStatus { id, name, icon, plan, windows: Vec<Window>, error }`
`Window { label, used_percent, resets_at: Option<unix_secs>, active }`

Un provider se incluye solo si `available()` detecta sus credenciales
(`~/.claude/.credentials.json` / `~/.codex/auth.json`). Si el collector falla,
se devuelve un `ProviderStatus` con `error` — nunca rompe el output completo.

## Fuentes de datos (verificadas en vivo, no re-derivar)

### Claude Code (`providers/claude.rs`)

`GET https://api.anthropic.com/api/oauth/usage` con:
- `Authorization: Bearer <accessToken>` de `~/.claude/.credentials.json` (clave `claudeAiOauth`)
- `anthropic-beta: oauth-2025-04-20`

La respuesta trae un array `limits` con `{kind, percent, resets_at (RFC3339), is_active, scope}`.
Kinds: `session` (ventana 5h), `weekly_all`, `weekly_scoped` (por modelo, con
`scope.model.display_name`). También trae `extra_usage` y `spend` (sin usar aún).

**Regla crítica**: lazysubs-eye NUNCA refresca el token OAuth — lo hace el propio
Claude Code (caduca cada ~8h). Refrescarlo aquí invalidaría el refresh token
del CLI. Con token caducado o 401 → error "reauth" y ya.

**Cuenta (E2 paso 1)**: la identidad visible sale de `~/.claude.json`
(`oauthAccount.emailAddress`), fichero distinto de las credenciales; solo se lee
el email, nunca los tokens. Va en `ProviderStatus.account` (serde skip si None) y
se pinta junto al plan en la TUI y el tooltip de waybar si `show_account = true`.
Codex y MiniMax no autodetectan cuenta (el email de Codex solo vive en el JWT, y
la API de MiniMax no expone identidad) → usarán el alias de config en multicuenta.

### Codex (`providers/codex.rs`)

Se lanza `codex app-server` (JSON-RPC 2.0, líneas JSON por stdio) y se envía:
1. request `initialize` con `clientInfo`
2. notificación `initialized`
3. request `account/rateLimits/read` (params `{}`)

La respuesta trae `rateLimits.{primary,secondary}` con `usedPercent`,
`windowDurationMins` (10080 = semana), `resetsAt` (unix), más `planType` y
`rateLimitResetCredits` (créditos de "full reset" — sin usar aún, pendiente
de pintar en la TUI). Timeout 15s; el proceso hijo se mata siempre vía guard
`KillOnDrop`. El esquema completo del protocolo se regenera con:
`codex app-server generate-json-schema --out <dir>`.

### MiniMax (`providers/minimax.rs`)

`GET https://api.minimax.io/v1/token_plan/remains` con
`Authorization: Bearer <Subscription Key del token plan>` (config
`[minimax] api_key` o env `MINIMAX_API_KEY`; `base_url` opcional para el host
chino). Semántica verificada en vivo (documentada también en el módulo):
`model_remains[]` por modelo (`general` = LLM del coding plan), tiempos en
**milisegundos**, `*_remaining_percent` es cuota **restante** (se invierte) y
`status == 3` marca ventanas fuera del plan contratado (se omiten).

### Degradación con datos previos (providers/mod.rs)

Si una consulta fresca falla (429 puntual, corte de red), `keep_stale_data()`
rescata los datos del provider desde la cache anterior durante un periodo de
gracia de 30 min (`STALE_GRACE_SECS`), marcándolos con `stale_since` (unix
secs de la consulta buena original — no se encadena la gracia re-guardando).
La UI lo señala: tooltip de waybar y título del panel TUI muestran "datos de
hace Xm". Pasada la gracia, el error se muestra tal cual.

### Tokens de hoy (`tokens.rs`)

Escanea `~/.claude/projects/*/*.jsonl` (transcripts de Claude Code), solo
ficheros con mtime de hoy. Filtra entradas `type == "assistant"` con timestamp
de hoy (hora local) y agrega `message.usage` (input/output/cache_read/
cache_creation) por `message.model`. Descarta modelos sintéticos (`<...>`).

## Cache (`cache.rs`)

`~/.cache/lazysubs-eye/status.json` con `fetched_at`; TTL por defecto 60s. La
ejecución cacheada tarda ~5ms, por eso waybar puede ejecutar el binario cada
60s sin coste. Los countdowns se calculan al renderizar (desde `resets_at`),
nunca se cachean, así que no salen rancios.

## Salidas (`output.rs`)

- `--waybar`: `{text, tooltip, class, percentage}`. `text` = icono + peor
  ventana de cada provider (✳ Claude, ⬡ Codex). Clases: `normal`,
  `warning` (≥80%), `critical` (≥95%), `error` (algún provider falló).
- Umbrales en las constantes `WARNING_AT` / `CRITICAL_AT`.

## TUI (`tui.rs`)

ratatui 0.29 + crossterm (re-exportado como `ratatui::crossterm`). Un panel
por provider con `LineGauge` por ventana (verde/amarillo/rojo según umbral),
countdown a la derecha, tabla de tokens de hoy, y pie con atajos.

- Teclas: `q`/`Esc` salir, `r` refrescar forzando `--no-cache`.
- Auto-refresh cada 60s. El refresh corre en un **hilo aparte** (mpsc channel);
  la UI nunca se bloquea mientras llama a las APIs (~1-3s).
- **Theming**: solo colores ANSI (`Color::Yellow`, etc.) → hereda el tema del
  terminal y por tanto el tema activo de Omarchy sin configuración alguna.

## Integración con el sistema (fuera del repo)

| Qué | Dónde |
|---|---|
| Binario instalado | `cargo install --path .` → `~/.cargo/bin/lazysubs-eye` |
| Symlink en PATH | `~/.local/bin/lazysubs-eye` → `~/.cargo/bin/lazysubs-eye` (⚠ `~/.cargo/bin` NO está en el PATH del usuario) |
| Módulo waybar | `~/.config/waybar/config.jsonc` → `custom/ai-usage` (primero en `modules-right`) |
| Estilos waybar | `~/.config/waybar/style.css` → `#custom-ai-usage` (10px; colores de la paleta Carbon Vándalo: warning `#FFCA40`, critical `#E04C4C`, error `#D99A6C`) |
| Windowrule | Última línea de `~/.config/hypr/hyprland.conf`: `windowrule = tag +floating-window, match:class org.omarchy.lazysubs-eye` |

El módulo waybar usa `signal: 11` → cualquier script puede forzar el repintado
con `pkill -RTMIN+11 waybar`. El click derecho hace exactamente eso tras un
`--no-cache`. El click izquierdo lanza `omarchy-launch-or-focus-tui lazysubs-eye`,
que abre la TUI en terminal flotante con clase `org.omarchy.lazysubs-eye` (el tag
`floating-window` de Omarchy le da float + centrado + 875x600 de serie) o
enfoca la ventana si ya existe.

Tras editar waybar: `omarchy restart waybar` (no auto-recarga la config; el
CSS sí, por `reload_style_on_change`). Tras editar Hyprland: `hyprctl reload`
+ `hyprctl configerrors`.

## Cómo probar sin abrir ventanas

```bash
cargo build                              # compila sin warnings
./target/debug/lazysubs-eye --no-cache --json  # collectors en vivo
./target/debug/lazysubs-eye --waybar           # línea para waybar

# TUI headless: captura de pantalla textual con tmux
tmux new-session -d -s t -x 100 -y 30 './target/debug/lazysubs-eye tui' \
  && sleep 6 && tmux capture-pane -t t -p; tmux kill-session -t t
```
