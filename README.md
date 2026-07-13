# lazysubs

Monitor de cuotas de suscripciones de IA para Omarchy, al estilo lazygit.
Muestra las ventanas de rate limit (sesión 5h, semanal…) de tus CLIs de IA
en waybar y, próximamente, en una TUI.

## Providers

| Provider | Fuente de datos | Requisito |
|---|---|---|
| Claude Code | endpoint OAuth `api.anthropic.com/api/oauth/usage` con el token de `~/.claude/.credentials.json` | sesión iniciada en Claude Code |
| Codex | JSON-RPC de `codex app-server` (`account/rateLimits/read`) | `codex login` |

Todo local: no se envía nada a terceros, solo se consultan las APIs oficiales
de cada provider con tus propias credenciales. lazysubs **nunca refresca los
tokens** (eso lo hace cada CLI); si un token caduca muestra un aviso de reauth.

## Uso

```
lazysubs            # TUI si stdout es una tty; si no, JSON
lazysubs tui        # TUI explícita (q salir · r refrescar; auto-refresh 60s)
lazysubs --json     # dump JSON completo del estado
lazysubs --waybar   # JSON de una línea para un módulo custom de waybar
lazysubs --no-cache # fuerza consulta fresca
lazysubs --ttl 120  # validez de la cache (segundos, por defecto 60)
```

La TUI usa colores ANSI, así que hereda el tema del terminal (y por tanto el
tema activo de Omarchy) sin configuración. Incluye una tabla con los tokens
consumidos hoy por modelo, sacados de los JSONL de `~/.claude/projects`.

La cache vive en `~/.cache/lazysubs/status.json`.

## Instalación

```
cargo install --path .
```

## Waybar (Fase 2)

```jsonc
"custom/ai-usage": {
  "exec": "$HOME/.local/bin/lazysubs --waybar",
  "return-type": "json",
  "interval": 60,
  "signal": 11,
  "on-click": "omarchy-launch-or-focus-tui lazysubs",
  "on-click-right": "$HOME/.local/bin/lazysubs --no-cache --waybar >/dev/null && pkill -RTMIN+11 waybar"
}
```

Clases CSS emitidas: `normal`, `warning` (≥80 %), `critical` (≥95 %), `error`.
Refresco manual desde cualquier script: `pkill -RTMIN+11 waybar`.
El `on-click` abre (o enfoca) la TUI en una terminal flotante. Requiere esta
regla en `~/.config/hypr/hyprland.conf` para que la ventana flote centrada:

```
windowrule = tag +floating-window, match:class org.omarchy.lazysubs
```

## Documentación

- [docs/ARQUITECTURA.md](docs/ARQUITECTURA.md) — cómo funciona: estructura, fuentes de datos, cache, TUI e integración con el sistema.
- [docs/ESTADO.md](docs/ESTADO.md) — estado del proyecto, decisiones tomadas y plan de la Fase 4 (traspaso para continuar el desarrollo).

## Roadmap

- [x] Fase 1 — core collector + salidas `--json` / `--waybar`
- [x] Fase 2 — integración waybar + ventana flotante en Hyprland
- [x] Fase 3 — TUI (ratatui) con tema del terminal + tokens de hoy por modelo
- [ ] Fase 4 — más providers (gemini, opencode), historial, notificaciones
