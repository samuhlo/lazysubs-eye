# Estado del proyecto — 2026-07-14

Traspaso para continuar el desarrollo. Contexto completo del funcionamiento en
[ARQUITECTURA.md](ARQUITECTURA.md).

## Resumen

lazysubs-eye es un clon de CodexBar para Omarchy: muestra las cuotas de las
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

- Publicado en `github.com/samuhlo/lazysubs-eye` (rama `main`); release
  `v0.2.0` con binario estático musl. El proyecto se llamó `lazysubs` hasta
  el 2026-07-14 (renombrado porque el nombre estaba pillado); el directorio
  local sigue siendo `~/Documentos/01_Code/lazysubs`.
- El usuario decide cuándo commitear — **no commitear sin que lo pida**.
  **Nada de atribución a Claude/IA en commits ni en el repo**: todo va a
  nombre del usuario (el historial se reescribió el 2026-07-14 para quitar
  los Co-Authored-By).
- `cargo build` limpio, sin warnings. **59 tests** (`cargo test`), todos
  verdes (pi_tokens, opencode_tokens, cache e install).
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
- **Fase B — instalación en un comando** ✓ (2026-07-14): subcomando
  `lazysubs-eye install` / `uninstall` (módulo waybar + CSS neutro + windowrule,
  idempotente, backups `.bak.<epoch>`, marcadores lazysubs-eye-begin/end,
  recarga; ver ARQUITECTURA.md § install), `--signal N` configurable, CI
  (fmt+clippy+test en `.github/workflows/ci.yml`), release con binario
  estático musl al taggear `v*` (`release.yml`) y PKGBUILD de `lazysubs-eye-bin`
  en `packaging/aur/`. Repo creado y release `v0.2.0` publicada (CI y build
  musl en verde; sha256 real ya en el PKGBUILD). Pendiente: publicar el
  PKGBUILD en AUR (requiere cuenta AUR del usuario). El sistema del usuario
  ya está migrado (2026-07-14): binario viejo desinstalado, integración
  manual retirada con backups y `lazysubs-eye install` ejecutado (módulo con
  marcadores, CSS personal del usuario conservado, windowrule nueva).
- **Fase C — producto redondo** ✓ (2026-07-14): `~/.config/lazysubs-eye/
  config.toml` (ttl, umbrales, notifications, providers on/off, iconos;
  módulo `config.rs` con OnceLock), notificaciones de umbral vía notify-send
  con anti-spam persistido en `notify-state.json` (`notify.rs`, lógica pura
  testada + probado end-to-end con un notify-send stub), y `--check` con
  exit codes 0/1/2/3 para scripts. Los umbrales ahora tienen una única
  fuente (config) en waybar, TUI, check y notificaciones.
- **Fases E — plan de producto** (2026-07-14): historial de gasto
  semana/mes con SQLite + estadísticas (E1), cuenta visible y multicuenta
  por IA (E2), y soporte de cualquier Linux con waybar, Omarchy primero
  (E3). **Plan detallado con diseño y decisiones en
  [PLAN-PRODUCTO.md](PLAN-PRODUCTO.md)** — sustituye a la antigua Fase D
  (los providers de Gemini/OpenCode quedan como idea futura sin fase).
- **Fase E1 — historial de gasto** ✓ (2026-07-14): módulo `history.rs`
  (SQLite en `~/.local/state/lazysubs-eye/history.db`), upsert autoritativo
  del día por fuente, ingesta desde el camino fresco de main y desde la TUI,
  backfill one-shot de los días pasados (Claude/Pi/OpenCode) marcado en `meta`,
  retención `history_days` y `prune`. En la TUI: tecla `t`/Tab cicla
  hoy/semana/mes en los paneles de tokens (tabla agregada por modelo con coste)
  y `Sparkline` del total diario (14 días) bajo cada panel. Config `[stats]`
  (enabled/default_period/history_days/sparkline) con las tres patas + panel `o`
  + README. Verificado en vivo contra los datos reales del usuario (backfill de
  ~1 mes en las tres fuentes). Tests puros de la capa SQLite con base en
  memoria. **90 tests** verdes. Pendiente de esta fase: nada (E2/E3 abiertas).
- **Post-lanzamiento (2026-07-14, feedback del usuario)** ✓: fix del texto
  truncado "sin uso hoy" en el panel OpenCode (era una celda de tabla con
  columna de 10 chars; ahora es párrafo), degradación con datos previos ante
  429/fallos puntuales (gracia 30 min, `stale_since`, indicado en tooltip y
  TUI), provider MiniMax (token plan; ver ARQUITECTURA § MiniMax) con
  `[minimax] api_key` en config, y sección Contributing en el README (el
  proyecto se plantea como open source colaborativo para cubrir más
  providers). La config del usuario con su key de MiniMax está en
  `~/.config/lazysubs-eye/config.toml` (0600).
- **Opciones de superficie (2026-07-14)** ✓: `[waybar]`/`[tui]` en config
  con `providers` (visibilidad **y orden** por superficie; los ocultos no
  cuentan para la clase CSS de la barra), `percent = false` (waybar solo
  iconos), `[tui] panels` para los paneles de tokens (apagados ni se
  escanean) y `colors = false` global (sin semáforo; la clase `error` se
  mantiene).
- **Panel de opciones en la TUI (2026-07-14)** ✓: tecla `o` abre un overlay
  con todos los ajustes (toggles + ←/→ para umbrales/ttl). Los cambios se
  aplican en caliente (config global pasó de OnceLock a RwLock;
  `config::get()` ahora devuelve un clon) y se persisten con toml_edit
  conservando comentarios y claves ajenas (`config::persist` +
  `apply_to_doc`, testado). Las tablas nuevas se escriben explícitas
  (`ensure_table`), nunca inline.
- **Cooldown de notificaciones (2026-07-14)** ✓: `notification_cooldown`
  (default 1800s, ajustable en config y en el panel `o` de la TUI, paso
  5 min). Re-notificar el mismo nivel (resets rodantes tipo MiniMax, bajar
  y volver a cruzar) espera el cooldown desde la última notificación de esa
  ventana; escalar a un nivel superior no espera. El estado guarda
  `notified_at` (serde default para estados antiguos).
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
