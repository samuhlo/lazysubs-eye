# Estado del proyecto — 2026-07-13

Traspaso para continuar el desarrollo. Contexto completo del funcionamiento en
[ARQUITECTURA.md](ARQUITECTURA.md).

## Resumen

lazysubs es un clon de CodexBar para Omarchy: muestra las cuotas de las
suscripciones de IA (Claude Code, Codex) en waybar y en una TUI estilo lazygit.
Las **fases 1–3 están completadas, integradas en el sistema del usuario y
verificadas en vivo** con sus cuentas reales (Claude pro, Codex plus).

## Hecho

- **Fase 0 — spike**: fuentes de datos verificadas (endpoint OAuth de Claude,
  JSON-RPC de codex app-server, formato de los JSONL). Detalles exactos en
  ARQUITECTURA.md § Fuentes de datos.
- **Fase 1 — core**: collectors de Claude Code y Codex, cache 60s, salidas
  `--json` y `--waybar`. Binario release de ~2,5 MB.
- **Fase 2 — waybar**: módulo `custom/ai-usage` funcionando en la barra del
  usuario, con tooltip desglosado, clases CSS por umbral, refresco por señal
  (RTMIN+11) y click derecho para forzar consulta fresca.
- **Fase 3 — TUI**: panel por provider con gauges y countdowns, tabla de
  tokens de hoy por modelo, auto-refresh sin bloquear la UI, theming automático
  vía colores ANSI. Click izquierdo del módulo waybar la abre flotante
  (launch-or-focus).

## Estado del repo

- Sin commits todavía — **el usuario decide cuándo commitear**; no commitear
  sin que lo pida.
- `cargo build` limpio, sin warnings. No hay tests automatizados aún.
- Instalado en el sistema: ver tabla "Integración con el sistema" en
  ARQUITECTURA.md (waybar config, style.css, hyprland.conf y symlink en
  `~/.local/bin` — hay backups con timestamp `.bak.<epoch>` de los configs
  tocados).

## Pendiente — Fase 4 (por orden de valor)

1. **Créditos de reset de Codex en la TUI**: ya vienen en la respuesta RPC
   (`rateLimitResetCredits.availableCount`, hoy el usuario tiene 3). Solo hay
   que añadirlos al modelo de datos y pintarlos en el panel de Codex.
2. **Notificaciones**: `notify-send` (mako) al cruzar 80%/95% de una ventana.
   Ojo: el binario se ejecuta cada 60s desde waybar sin estado entre runs —
   guardar el último umbral notificado en la cache para no spamear.
3. **Historial + sparklines**: persistir snapshots (p. ej. en la cache o un
   sqlite/jsonl en `~/.local/state/lazysubs/`) y pintar sparklines de uso en
   la TUI (ratatui tiene widget `Sparkline`).
4. **Provider Gemini CLI**: instalado en `~/.local/bin/gemini`. Sin
   investigar aún — mirar `~/.gemini/` y qué expone su CLI.
5. **Provider opencode**: instalado; hay estado en `~/.local/state/opencode`.
   Sin investigar aún.
6. Ideas menores: coste estimado en la tabla de tokens (precios por modelo),
   desglose por proyecto (el JSONL trae `cwd`), `--check` para scripts/hooks.

## Decisiones de diseño ya tomadas (respetar)

- **Nunca refrescar tokens OAuth** de los CLIs; en 401 → mostrar "reauth".
- Todo local: solo se llama a las APIs oficiales de cada provider con las
  credenciales del propio usuario. Nada de telemetría ni terceros.
- Providers se autodetectan por la existencia de su fichero de credenciales;
  un provider caído degrada a estado `error`, nunca rompe el output.
- TUI con colores ANSI (nada de hex hardcodeado) para heredar el tema del
  terminal. En waybar sí hay hex de la paleta Carbon Vándalo del usuario
  (amarillo `#FFCA40` como acento; regla del tema: nada de cian/azul fríos).
- Umbrales: warning ≥80%, critical ≥95% (constantes en `output.rs`).
- Estética: terminal/lazygit. El usuario quiere las cosas compactas (pidió
  bajar el font-size del módulo waybar a 10px).

## Preferencias del usuario relevantes

- Habla español; UI y docs en español.
- Su tema Omarchy es Carbon Vándalo (suyo, github.com/samuhlo/carbon-vandal).
- Sistema: CachyOS + Omarchy (Hyprland/waybar/alacritty), fish como shell,
  código en `~/Documentos/01_Code/`.
