# Plan de producto — fases E (traspaso)

Escrito el 2026-07-14 a partir de las anotaciones del usuario. Este documento
es el **plan de trabajo para continuar el desarrollo**; el contexto técnico
completo está en [ARQUITECTURA.md](ARQUITECTURA.md) y el estado/normas del
proyecto en [ESTADO.md](ESTADO.md) — leer ambos antes de empezar (en especial
las "Decisiones de diseño ya tomadas" y la regla de **no atribución a IA en
commits**). Sustituye y absorbe la antigua "Fase D".

Orden recomendado: E1 → E2 → E3. Cada fase termina con el ritual de release
ya establecido: `cargo fmt --check` + `clippy --all-targets -- -D warnings` +
`cargo test`, bump de versión en `Cargo.toml` y `packaging/aur/PKGBUILD`,
commit + tag `vX.Y.0`, esperar el workflow Release, rellenar el sha256 real
en el PKGBUILD, y actualizar el binario del usuario
(`cargo install --path .` + `pkill -RTMIN+11 waybar`).

---

## Fase E1 — Historial de gasto: semana / mes / estadísticas

> **✓ Implementada el 2026-07-14.** Módulo `src/history.rs` (SQLite en
> `~/.local/state/lazysubs-eye/history.db`), backfill one-shot de las tres
> fuentes, ingesta desde main y la TUI, tecla `t`/Tab para ciclar
> hoy/semana/mes, sparkline de 14 días, config `[stats]` y retención. Verificado
> en vivo. Lo de abajo es el diseño original que se siguió.

### Objetivo

Gasto de tokens (y coste donde exista) por semana y mes, por provider y
modelo, con estadísticas en la TUI. Hoy solo existe "tokens de hoy".

### Diseño propuesto

**Almacén**: SQLite en `~/.local/state/lazysubs-eye/history.db`
(`XDG_STATE_HOME`, NO en cache — la cache es borrable). rusqlite ya está en
el árbol (bundled, lo usa opencode_tokens). Esquema:

```sql
CREATE TABLE IF NOT EXISTS daily_usage (
  date        TEXT NOT NULL,   -- YYYY-MM-DD local
  source      TEXT NOT NULL,   -- "claude" | "pi" | "opencode"
  provider    TEXT NOT NULL,   -- provider interno de la fila (pi/opencode traen varios)
  model       TEXT NOT NULL,
  input       INTEGER NOT NULL DEFAULT 0,
  output      INTEGER NOT NULL DEFAULT 0,
  cache_read  INTEGER NOT NULL DEFAULT 0,
  cache_write INTEGER NOT NULL DEFAULT 0,
  reasoning   INTEGER NOT NULL DEFAULT 0,
  total       INTEGER NOT NULL DEFAULT 0,
  cost        REAL    NOT NULL DEFAULT 0,
  PRIMARY KEY (date, source, provider, model)
);
```

**Ingesta**: los tres escáneres de "hoy" (`tokens.rs`, `pi_tokens.rs`,
`opencode_tokens.rs`) ya producen filas por modelo. Tras cada escaneo, upsert
(`INSERT … ON CONFLICT … DO UPDATE`) de las filas de HOY. Los días pasados
quedan congelados — así el historial sobrevive aunque los JSONL/DB de origen
se poden. Un módulo nuevo `src/history.rs` con la conexión, el upsert y las
consultas agregadas; que los escáneres no dependan de él (llamarlo desde
donde se reciben los resultados, p. ej. tui.rs y el camino fresco de main).

**Backfill**: primera ejecución → poblar días pasados desde las fuentes que
aún existan (los JSONL de Claude/Pi tienen días previos; la DB de opencode
es histórica completa). Reutilizar la lógica de parseo existente
parametrizando el día en vez del hardcode "hoy" — ojo: los índices
incrementales actuales (fingerprint/cursor en `~/.cache/lazysubs-eye/`)
están pensados para "hoy"; el backfill puede ser un escaneo completo one-shot
marcado como hecho en una tabla `meta` de la propia DB.

**UI**: en la TUI, una tecla (propuesta: `t` de tiempo, o Tab) que cicla el
periodo de los paneles de tokens: `hoy → semana → mes`. El título del panel
refleja el periodo ("✳ tokens semana"). Estadística extra barata y vistosa:
`Sparkline` de ratatui con el total diario de los últimos N días bajo la
tabla. `--json` puede ganar un bloque `usage_history` opcional (decidir si
por flag `--stats` para no engordar el JSON de waybar).

**Retención**: prune de filas más viejas que `history_days` al abrir la DB.

### Opciones (config + panel `o`)

```toml
[stats]
enabled = true          # false: ni DB ni panel (comportamiento actual)
default_period = "hoy"  # hoy | semana | mes
history_days = 90       # retención; 0 = sin límite
sparkline = true
```

Añadirlas a `Setting`/`setting_row`/`settings_apply` en tui.rs y a
`apply_to_doc` en config.rs (mismo patrón que `notification_cooldown`).

### Verificación

Tests puros del upsert/agregación con DB en tempdir (patrón de
opencode_tokens). Captura tmux de la TUI ciclando periodos. Backfill
verificado contra los datos reales del usuario.

---

## Fase E2 — Cuenta visible y multicuenta por IA

> **✓ Implementada el 2026-07-14.** Paso 1 (cuenta visible, v0.9.0): Claude
> autodetecta el email de `~/.claude.json`; Codex/MiniMax usan alias de config.
> Paso 2 (multicuenta, v0.10.0): `[[accounts.*]]`, collectors parametrizados,
> ids compuestos `claude:trabajo`, providers dinámicos en el panel `o`.
> Verificado en vivo (segunda cuenta Claude que degrada a error sin romper).

### Objetivo

Mostrar qué cuenta y plan se usa en cada provider, y soportar varias cuentas
de la misma IA (p. ej. dos cuentas de Claude).

### Diseño propuesto

**Identidad de cuenta (paso 1, barato)**:
- Claude: `~/.claude.json` tiene `oauthAccount.emailAddress` (verificar clave
  exacta en vivo antes de codificar; NO re-derivar el flujo OAuth, ver regla
  en ARQUITECTURA). El plan ya sale de `subscriptionType`.
- Codex: mirar si `account/read` u otro método del app-server expone email
  (regenerar el esquema con `codex app-server generate-json-schema`); si no,
  leer `~/.codex/auth.json` (puede llevar `email` o un JWT decodificable —
  solo decodificar el payload, sin validar firma).
- MiniMax: no hay identidad en la API → alias definido por el usuario en
  config (`name`).
- Modelo de datos: `ProviderStatus.account: Option<String>` (serde skip si
  None, como `stale_since` — mantener el contrato JSON estable, hay test de
  byte-stability en output.rs que habrá que extender).
- UI: en el título del panel TUI junto al plan (`─ pro · sam@… ─`) y en el
  tooltip de waybar. Truncar emails largos.

**Multicuenta (paso 2)**:

```toml
[[accounts.claude]]
name = "personal"                              # alias visible
credentials = "~/.claude/.credentials.json"    # default si se omite
[[accounts.claude]]
name = "trabajo"
credentials = "~/trabajo/.claude/.credentials.json"

[[accounts.codex]]
name = "personal"
codex_home = "~/.codex"     # se pasa como CODEX_HOME al app-server

[[accounts.minimax]]
name = "personal"
api_key = "..."
```

- Sin `[[accounts.*]]` → comportamiento actual (una cuenta autodetectada).
  `[minimax] api_key` se mantiene como azúcar para una sola cuenta.
- Cada cuenta produce un `ProviderStatus` propio con id compuesto
  `"claude:trabajo"` (y `name` "Claude Code · trabajo"). El id simple
  (`"claude"`) se conserva para la cuenta única/primera, así las listas
  `[waybar]/[tui] providers` y el estado de notificaciones existentes no se
  rompen.
- Los collectors deben parametrizarse: `claude::collect(creds_path)`,
  `codex::collect(codex_home)` (env `CODEX_HOME` al spawn del app-server —
  verificar en vivo que lo respeta), `minimax::collect(api_key, base_url)`.
- Notificaciones y stale-grace ya son por id → funcionan gratis con ids
  compuestos.

### Personalización

- `icon` y `name` por cuenta en `[[accounts.*]]`.
- `show_account = true` global (ocultar la cuenta si no se quiere en
  pantalla).
- Las listas de `[waybar]/[tui] providers` aceptan ids compuestos
  (`"claude:trabajo"`); el panel `o` de la TUI debe construir las filas de
  providers dinámicamente desde la config en vez de la constante `PROVIDERS`
  (hoy es un array fijo de 3 — refactor necesario en tui.rs).

### Verificación

Tests de parseo de `[[accounts.*]]`, de la composición de ids y del fallback
sin accounts. En vivo: el usuario solo tiene una cuenta por IA — probar
multicuenta con un segundo fichero de credenciales falso que degrade a error
(el output nunca rompe por un provider caído).

---

## Fase E3 — Cualquier Linux con waybar (Omarchy primero)

> **✓ Implementada el 2026-07-15 (v0.11.0).** `is_omarchy()` guía los fallbacks;
> config de waybar `config.jsonc` o `config`; CSS neutro (hex) fuera de Omarchy;
> on-click con `xdg-terminal-exec`/terminal conocido; windowrule solo si hay
> `hyprland.conf`; recarga con `pkill -SIGUSR2 waybar` + `systemctl --user
> try-restart`. Round-trip install/uninstall byte a byte verificado en sandbox
> sin Omarchy. README con "Other Linux setups".

### Objetivo

Que `lazysubs-eye install` y el runtime funcionen en cualquier distro con
waybar, degradando con gracia lo específico de Omarchy/Hyprland.

### Diseño propuesto (todo en `src/install.rs` salvo lo indicado)

- **Detección**: `is_omarchy()` = existe `~/.local/share/omarchy` o `omarchy`
  en PATH. Guía todos los fallbacks.
- **Config de waybar**: probar `~/.config/waybar/config.jsonc` y también
  `config` a secas (nombre estándar de waybar fuera de Omarchy).
- **Recarga**: sin omarchy → `pkill -SIGUSR2 waybar` (señal de reload de
  waybar) y si no hay proceso, sugerir arrancarlo; probar también
  `systemctl --user try-restart waybar.service` antes de rendirse.
- **CSS**: fuera de Omarchy no existe `@foreground` (lo define el theme
  import de Omarchy) → generar el bloque con colores hex neutros en vez de
  `alpha(@foreground, …)`. Decidir en install-time según `is_omarchy()`.
- **on-click**: `omarchy-launch-or-focus-tui` → fallback a
  `xdg-terminal-exec lazysubs-eye` (spec freedesktop, lo tienen las distros
  modernas) o, si no existe, el primer terminal conocido
  (`foot|alacritty|kitty|ghostty -e lazysubs-eye`). Sin launch-or-focus
  fuera de Omarchy: abrir sin más es aceptable.
- **Windowrule**: solo si existe `~/.config/hypr/hyprland.conf`. En otros
  compositores (sway, river…) omitir con un mensaje informativo — no
  intentar reglas equivalentes en v1 (documentar en README cómo hacerla a
  mano en sway).
- **Runtime**: el binario ya es agnóstico (musl estático); revisar que
  ningún camino de ejecución (no-install) invoque comandos omarchy.
- **README**: sección "Other Linux setups" + quitar la implicación de que
  Omarchy es requisito (es el target premium, no el mínimo). PKGBUILD:
  quitar/ajustar `optdepends` de hyprland.
- El `uninstall` debe revertir igual de bien ambos sabores (los marcadores
  `lazysubs-eye-begin/end` ya son agnósticos).

### Verificación

El ciclo install/uninstall en sandbox (`XDG_CONFIG_HOME` temporal) tiene
patrón establecido — añadir un caso "sin omarchy" (PATH restringido sin el
comando, config `waybar/config` sin `.jsonc`, sin hyprland.conf) y comprobar
round-trip byte a byte. No hay forma de probar otra distro en vivo aquí;
apoyarse en los tests y pedir feedback de la comunidad (issue template).

---

## Notas transversales para el agente que continúe

- **Idioma**: código/UI en español (decisión de idioma de UI aún abierta,
  ver ESTADO), README público en inglés.
- **Umbral de calidad**: clippy `-D warnings` y fmt en CI; los tests van en
  el propio módulo (`#[cfg(test)]`), nombres en español descriptivos.
- **No tocar**: la semántica documentada de MiniMax (ms, percent invertido,
  status 3), la regla de nunca refrescar tokens OAuth, y el sistema de
  marcadores del install.
- **Config**: cualquier opción nueva necesita las tres patas — struct serde
  con default + `apply_to_doc` (persistencia toml_edit) + fila en el panel
  `o` de la TUI — y su línea comentada en el README y en la plantilla del
  config del usuario.
- **Pendiente externo** (no bloquea): publicar el PKGBUILD en AUR (cuenta
  AUR del usuario) y captura PNG real para el README.
