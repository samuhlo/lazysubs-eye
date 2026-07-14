# Estado del proyecto — 2026-07-14

Traspaso para continuar el desarrollo. Contexto completo del funcionamiento en
[ARQUITECTURA.md](ARQUITECTURA.md).

## Resumen

lazysubs es un clon de CodexBar para Omarchy: muestra las cuotas de las
suscripciones de IA (Claude Code, Codex) en waybar y en una TUI estilo lazygit,
más los tokens consumidos hoy por modelo (Claude, Pi, OpenCode). Las fases 1–3
están completadas, integradas en el sistema del usuario y verificadas en vivo
con sus cuentas reales (Claude pro, Codex plus). El objetivo actual es
**lanzarlo como producto para la comunidad Omarchy** (plan de fases A–D abajo).

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
- **Créditos de reset de Codex** en el panel de la TUI (de
  `rateLimitResetCredits`).
- **Tokens diarios de Pi**: parseo incremental de los JSONL de
  `~/.pi/agent/sessions` con índice persistente (fingerprint + offset).
- **Tokens diarios de OpenCode**: lectura de la base SQLite de
  `~/.local/state/opencode` (tablas `part`/`message`), con cursor incremental
  y reconciliación diaria.
- **Fase A de lanzamiento (parcial)**: LICENSE (MIT), `--version`, README en
  inglés con quickstart y muestra de la TUI, docs actualizados.

## Estado del repo

- Historia en `master`; el usuario decide cuándo commitear — **no commitear
  sin que lo pida**.
- `cargo build` limpio, sin warnings. **51 tests** (`cargo test`), todos verdes
  (cubren sobre todo pi_tokens y opencode_tokens).
- Los cambios grandes se especifican con openspec; los specs aplicados están
  en `openspec/changes/archive/`.
- Instalado en el sistema: ver tabla "Integración con el sistema" en
  ARQUITECTURA.md (waybar config, style.css, hyprland.conf y symlink en
  `~/.local/bin` — hay backups con timestamp `.bak.<epoch>` de los configs
  tocados).

## Plan de lanzamiento como producto Omarchy

- **Fase A — base lanzable**: LICENSE ✓, `--version` ✓, README inglés ✓,
  docs al día ✓. Pendiente de la fase: captura PNG real de la TUI y del
  módulo waybar para el README (no hay herramienta de captura instalada;
  de momento hay una muestra en texto).
- **Fase B — instalación en un comando**: subcomando `lazysubs install` /
  `uninstall` (módulo waybar + CSS + windowrule, idempotente, backups
  `.bak.<epoch>`, recarga), CSS por defecto neutro (sin la paleta personal),
  señal RTMIN configurable, CI (fmt+clippy+test), release con binario
  estático y PKGBUILD para AUR.
- **Fase C — producto redondo**: `~/.config/lazysubs/config.toml` (umbrales,
  TTL, providers, iconos), notificaciones 80%/95% vía notify-send con
  anti-spam en la cache, `--check` para scripts.
- **Fase D — v1.x**: providers de cuotas para Gemini CLI y OpenCode,
  historial + sparklines (`~/.local/state/lazysubs/`), coste estimado y
  desglose por proyecto.
- **Decisión abierta**: idioma de la propia UI (hoy en español; el README ya
  está en inglés). Decidir antes del anuncio público.

## Decisiones de diseño ya tomadas (respetar)

- **Nunca refrescar tokens OAuth** de los CLIs; en 401 → mostrar "reauth".
- Todo local: solo se llama a las APIs oficiales de cada provider con las
  credenciales del propio usuario. Nada de telemetría ni terceros.
- Providers se autodetectan por la existencia de su fichero de credenciales;
  un provider caído degrada a estado `error`, nunca rompe el output.
- TUI con colores ANSI (nada de hex hardcodeado) para heredar el tema del
  terminal. En waybar sí hay hex de la paleta Carbon Vándalo del usuario
  (amarillo `#FFCA40` como acento; regla del tema: nada de cian/azul fríos) —
  para el producto, el CSS por defecto debe ser neutro (Fase B).
- Umbrales: warning ≥80%, critical ≥95% (constantes en `output.rs`).
- Estética: terminal/lazygit. El usuario quiere las cosas compactas (pidió
  bajar el font-size del módulo waybar a 10px).

## Preferencias del usuario relevantes

- Habla español; docs internos en español. README público en inglés
  (decidido el 2026-07-14).
- Su tema Omarchy es Carbon Vándalo (suyo, github.com/samuhlo/carbon-vandal).
- Sistema: CachyOS + Omarchy (Hyprland/waybar/alacritty), fish como shell,
  código en `~/Documentos/01_Code/`.
