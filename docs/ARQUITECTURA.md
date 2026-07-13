# Arquitectura de lazysubs

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
│   └── codex.rs       # collector de Codex (JSON-RPC a `codex app-server`)
├── tokens.rs          # tokens de hoy por modelo (parseo de JSONL locales de Claude)
├── cache.rs           # cache JSON con TTL en ~/.cache/lazysubs/status.json
├── output.rs          # render --waybar y --json + countdown() + umbrales de color
└── tui.rs             # TUI ratatui (gauges, countdowns, tabla de tokens)
```

### Modos (main.rs)

- Sin args + stdout es tty → **TUI**. Sin tty → `--json`. Así el mismo binario
  sirve para waybar (pipe) y para uso interactivo.
- `tui` / `--tui`, `--json`, `--waybar`, `--no-cache`, `--ttl N` (defecto 60).

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

**Regla crítica**: lazysubs NUNCA refresca el token OAuth — lo hace el propio
Claude Code (caduca cada ~8h). Refrescarlo aquí invalidaría el refresh token
del CLI. Con token caducado o 401 → error "reauth" y ya.

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

### Tokens de hoy (`tokens.rs`)

Escanea `~/.claude/projects/*/*.jsonl` (transcripts de Claude Code), solo
ficheros con mtime de hoy. Filtra entradas `type == "assistant"` con timestamp
de hoy (hora local) y agrega `message.usage` (input/output/cache_read/
cache_creation) por `message.model`. Descarta modelos sintéticos (`<...>`).

## Cache (`cache.rs`)

`~/.cache/lazysubs/status.json` con `fetched_at`; TTL por defecto 60s. La
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
| Binario instalado | `cargo install --path .` → `~/.cargo/bin/lazysubs` |
| Symlink en PATH | `~/.local/bin/lazysubs` → `~/.cargo/bin/lazysubs` (⚠ `~/.cargo/bin` NO está en el PATH del usuario) |
| Módulo waybar | `~/.config/waybar/config.jsonc` → `custom/ai-usage` (primero en `modules-right`) |
| Estilos waybar | `~/.config/waybar/style.css` → `#custom-ai-usage` (10px; colores de la paleta Carbon Vándalo: warning `#FFCA40`, critical `#E04C4C`, error `#D99A6C`) |
| Windowrule | Última línea de `~/.config/hypr/hyprland.conf`: `windowrule = tag +floating-window, match:class org.omarchy.lazysubs` |

El módulo waybar usa `signal: 11` → cualquier script puede forzar el repintado
con `pkill -RTMIN+11 waybar`. El click derecho hace exactamente eso tras un
`--no-cache`. El click izquierdo lanza `omarchy-launch-or-focus-tui lazysubs`,
que abre la TUI en terminal flotante con clase `org.omarchy.lazysubs` (el tag
`floating-window` de Omarchy le da float + centrado + 875x600 de serie) o
enfoca la ventana si ya existe.

Tras editar waybar: `omarchy restart waybar` (no auto-recarga la config; el
CSS sí, por `reload_style_on_change`). Tras editar Hyprland: `hyprctl reload`
+ `hyprctl configerrors`.

## Cómo probar sin abrir ventanas

```bash
cargo build                              # compila sin warnings
./target/debug/lazysubs --no-cache --json  # collectors en vivo
./target/debug/lazysubs --waybar           # línea para waybar

# TUI headless: captura de pantalla textual con tmux
tmux new-session -d -s t -x 100 -y 30 './target/debug/lazysubs tui' \
  && sleep 6 && tmux capture-pane -t t -p; tmux kill-session -t t
```
